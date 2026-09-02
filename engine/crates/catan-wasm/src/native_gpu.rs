use colonist_catan_core::Action;
use colonist_catan_search::{
    ActionStats, CudaSimAgentSearchConfig, CudaSimEngine, ExactActionFamily,
    SearchReport, SearchStatistics, belief_domestic_trade_threat, exact_family_for_action,
    safer_end_turn_alternative, shared_root_candidates, solve_belief_current_turn,
    solve_exact_belief_excluding,
};
use serde::Serialize;
use serde_json::Value;

use super::{
    ActionReplacementOutput, AuthorityTraceOutput, DecisionAuthority, PrunedRootOutput, Request,
    ResponseDiagnostics, RetainedRootOutput, RootProvenanceOutput, RankedRootOutput, action,
    basic_response_diagnostics, effective_particle_count, exact_family_label,
    exact_mandatory_report, game_states, response, root_exclusion_actions,
};

const GPU_ALGORITHM: &str = "gpu-root-rollout";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeGpuDeviceIdentity {
    pub backend: &'static str,
    pub ordinal: usize,
    pub name: String,
    pub compute_capability: [i32; 2],
}

pub struct NativeGpuSearchEngine {
    cuda: CudaSimEngine,
    config: CudaSimAgentSearchConfig,
}

#[derive(Clone)]
struct AggregatedRoot {
    action: Action,
    prior: f32,
    samples: u32,
    errors: u32,
    terminal_outcome: f32,
    victory_margin: f32,
    mean_turn: f32,
    candidate_vp: f32,
    opponent_vp: f32,
}

impl NativeGpuSearchEngine {
    pub fn new() -> Result<Self, String> {
        let cuda = CudaSimEngine::new().map_err(|error| error.to_string())?;
        Ok(Self {
            cuda,
            config: CudaSimAgentSearchConfig::default(),
        })
    }

    pub fn device_identity(&self) -> NativeGpuDeviceIdentity {
        let identity = self.cuda.device_identity();
        NativeGpuDeviceIdentity {
            backend: identity.backend,
            ordinal: identity.ordinal,
            name: identity.name.clone(),
            compute_capability: [identity.compute_capability.0, identity.compute_capability.1],
        }
    }

    pub fn analyze_json(&mut self, value: Value) -> Result<Value, String> {
        let request: Request = serde_json::from_value(value).map_err(|error| error.to_string())?;
        if matches!(request.mode.as_deref(), Some("weighted")) {
            return Err("GPU native search is reserved for the Strategist engine".into());
        }
        if request.ponder.unwrap_or(false) {
            return Err("GPU native search does not accept speculative opponent-turn pondering".into());
        }

        let particles = game_states(request.state, request.last_rejected_trade)?;
        if particles.is_empty() {
            return Err("GPU native search received no belief particles".into());
        }
        let root_exclusions = root_exclusion_actions(&request.root_exclusions, &particles[0].state)?;
        if let Some(report) = exact_mandatory_report(&particles, &root_exclusions) {
            return serde_json::to_value(response(
                report,
                particles.len(),
                GPU_ALGORITHM,
                DecisionAuthority::ExactMandatory,
                basic_response_diagnostics(particles.len(), DecisionAuthority::ExactMandatory),
            ))
            .map_err(|error| error.to_string());
        }

        let actor = particles[0].state.actor();
        let observation = particles[0].state.observation_hash(actor);
        if particles
            .iter()
            .any(|particle| particle.state.actor() != actor || particle.state.observation_hash(actor) != observation)
        {
            return Err("GPU native search requires one observation-safe root across belief particles".into());
        }

        let universal_actions = particles[0]
            .state
            .legal_actions()
            .into_iter()
            .filter(|candidate| !root_exclusions.contains(candidate))
            .filter(|candidate| {
                particles.iter().all(|particle| {
                    particle
                        .state
                        .legal_actions()
                        .into_iter()
                        .any(|legal| legal == *candidate)
                })
            })
            .collect::<Vec<_>>();
        if universal_actions.is_empty() {
            return Err("GPU native search found no universally legal root action".into());
        }

        let ranked = shared_root_candidates(
            &particles[0].state,
            actor,
            &universal_actions,
            self.config.root_samples,
        );
        if ranked.is_empty() {
            return Err("GPU native search produced no root candidates".into());
        }
        let safe = ranked
            .iter()
            .filter(|(candidate, _)| {
                belief_domestic_trade_threat(
                    particles
                        .iter()
                        .map(|particle| (&particle.state, particle.weight)),
                    candidate,
                )
                .is_none()
            })
            .cloned()
            .collect::<Vec<_>>();
        let retained = if safe.is_empty() { ranked.clone() } else { safe };
        let pruned_trade_roots = if retained.len() == ranked.len() {
            Vec::new()
        } else {
            ranked
                .iter()
                .enumerate()
                .filter(|(_, (candidate, _))| {
                    !retained.iter().any(|(safe, _)| safe == candidate)
                })
                .map(|(rank, (candidate, _))| PrunedRootOutput {
                    action: action(candidate.clone()),
                    pre_truncation_rank: Some(rank),
                    reason: "trade-safety",
                })
                .collect::<Vec<_>>()
        };

        let states = particles
            .iter()
            .map(|particle| particle.state.clone())
            .collect::<Vec<_>>();
        self.cuda
            .upload_states(&states)
            .map_err(|error| error.to_string())?;
        let root_actions = retained
            .iter()
            .map(|(candidate, _)| candidate.clone())
            .collect::<Vec<_>>();
        let root_rows = (0..particles.len())
            .map(|_| root_actions.clone())
            .collect::<Vec<_>>();
        let searched = self
            .cuda
            .search_root_actions(
                &root_rows,
                self.config.rollouts_per_action,
                self.config.rollout_steps,
                request.seed.unwrap_or(0x0043_4154_414e),
            )
            .map_err(|error| error.to_string())?;

        let total_weight = particles
            .iter()
            .map(|particle| particle.weight.max(0.0))
            .sum::<f32>();
        let uniform_weight = 1.0 / particles.len() as f32;
        let weights = particles
            .iter()
            .map(|particle| {
                if total_weight > f32::EPSILON {
                    particle.weight.max(0.0) / total_weight
                } else {
                    uniform_weight
                }
            })
            .collect::<Vec<_>>();

        let mut aggregated = Vec::<AggregatedRoot>::with_capacity(root_actions.len());
        for (root_index, candidate) in root_actions.iter().enumerate() {
            let mut root = AggregatedRoot {
                action: candidate.clone(),
                prior: retained[root_index].1,
                samples: 0,
                errors: 0,
                terminal_outcome: 0.0,
                victory_margin: 0.0,
                mean_turn: 0.0,
                candidate_vp: 0.0,
                opponent_vp: 0.0,
            };
            for ((row, weight), _) in searched
                .rows
                .iter()
                .zip(weights.iter().copied())
                .zip(particles.iter())
            {
                let Some(stat) = row.get(root_index) else {
                    return Err("GPU native search returned a malformed root matrix".into());
                };
                if stat.action != *candidate {
                    return Err("GPU native search changed root action ordering".into());
                }
                root.samples = root.samples.saturating_add(stat.samples);
                root.errors = root.errors.saturating_add(stat.errors);
                root.terminal_outcome += stat.net_terminal_outcome() * weight;
                root.victory_margin += stat.mean_victory_margin() * weight;
                root.mean_turn += stat.mean_turn * weight;
                root.candidate_vp += stat.mean_victory_points * weight;
                root.opponent_vp += stat.mean_best_opponent_victory_points * weight;
            }
            aggregated.push(root);
        }
        let chosen_root = aggregated
            .iter()
            .filter(|candidate| candidate.errors == 0)
            .max_by(|left, right| {
                left.terminal_outcome
                    .total_cmp(&right.terminal_outcome)
                    .then_with(|| left.victory_margin.total_cmp(&right.victory_margin))
                    .then_with(|| right.mean_turn.total_cmp(&left.mean_turn))
                    .then_with(|| left.prior.total_cmp(&right.prior))
            })
            .cloned()
            .ok_or_else(|| "GPU native search had no error-free root candidate".to_string())?;

        let player_count = particles[0].state.board.num_players as usize;
        let root_values = |candidate: &AggregatedRoot| {
            let mut value = [0.0; 4];
            value[actor as usize] = candidate.candidate_vp.max(0.0);
            for (player, slot) in value.iter_mut().enumerate().take(player_count) {
                if player != actor as usize {
                    *slot = candidate.opponent_vp.max(0.0);
                }
            }
            value
        };
        let actions = aggregated
            .iter()
            .map(|candidate| ActionStats {
                action: candidate.action.clone(),
                visits: candidate.samples,
                availability: particles.len().min(u32::MAX as usize) as u32,
                availability_weight: 1.0,
                legal_weight: 1.0,
                prior: candidate.prior,
                value: root_values(candidate),
                lower_confidence_value: root_values(candidate),
            })
            .collect::<Vec<_>>();

        let tactical_particles = particles
            .iter()
            .map(|particle| (&particle.state, particle.weight))
            .collect::<Vec<_>>();
        let tactical = solve_belief_current_turn(
            &tactical_particles,
            request.tactical_depth.unwrap_or(14).clamp(4, 32),
            request.tactical_nodes.unwrap_or(900).clamp(100, 100_000),
        );
        let mut exact = solve_exact_belief_excluding(
            &particles,
            ExactActionFamily::Mandatory,
            &root_exclusions,
        );
        let mut authority = if tactical.proven {
            DecisionAuthority::TacticalProven
        } else {
            DecisionAuthority::GpuRootRollout
        };
        let initial_authority = authority;
        let mut chosen = if tactical.proven {
            tactical.principal_line.first().cloned()
        } else {
            Some(chosen_root.action.clone())
        };
        let mut exact_family = None;
        let mut exact_family_replacement = None;
        let mut safety_replacement = None;

        if !tactical.proven
            && let Some(family) = chosen.as_ref().and_then(exact_family_for_action)
        {
            exact_family = Some(exact_family_label(family));
            let before = chosen.clone();
            exact = solve_exact_belief_excluding(&particles, family, &root_exclusions);
            if let Some(exact_chosen) = exact.chosen.clone() {
                if before.as_ref() != Some(&exact_chosen)
                    && let Some(previous) = before
                {
                    exact_family_replacement = Some(ActionReplacementOutput {
                        from: action(previous),
                        to: action(exact_chosen.clone()),
                    });
                }
                chosen = Some(exact_chosen);
                authority = DecisionAuthority::ExactFamily;
            }
        }
        if chosen == Some(Action::EndTurn)
            && let Some(safer) = safer_end_turn_alternative(
                &particles[0].state,
                actor as usize,
                &actions,
                Some(&particles),
            )
        {
            if safer != Action::EndTurn {
                safety_replacement = Some(ActionReplacementOutput {
                    from: action(Action::EndTurn),
                    to: action(safer.clone()),
                });
            }
            chosen = Some(safer);
            authority = DecisionAuthority::SafetyOverride;
        }

        let root_value = actions
            .iter()
            .find(|candidate| Some(&candidate.action) == chosen.as_ref())
            .map(|candidate| candidate.value)
            .unwrap_or_else(|| root_values(&chosen_root));
        let total_rollouts = aggregated
            .iter()
            .map(|candidate| candidate.samples)
            .sum::<u32>();
        let mut root_provenance = RootProvenanceOutput {
            ranked_root_count: ranked.len(),
            ranked_roots: ranked
                .iter()
                .enumerate()
                .map(|(rank, (candidate, prior))| RankedRootOutput {
                    action: action(candidate.clone()),
                    rank,
                    prior: *prior,
                    planner_value: None,
                    planner_completion_mass: None,
                })
                .collect(),
            retained_roots: retained
                .iter()
                .enumerate()
                .map(|(_rank, (candidate, prior))| RetainedRootOutput {
                    action: action(candidate.clone()),
                    pre_truncation_rank: ranked
                        .iter()
                        .position(|(ranked_candidate, _)| ranked_candidate == candidate),
                    prior: *prior,
                    node_budget_per_particle: 0,
                    allocated_nodes: 0,
                    planner_value: None,
                    planner_completion_mass: None,
                })
                .collect(),
            pruned_root_count: pruned_trade_roots.len(),
            pruned_roots: pruned_trade_roots,
            exact_family_replacement: None,
            safety_replacement: None,
        };
        root_provenance.exact_family_replacement = exact_family_replacement.clone();
        root_provenance.safety_replacement = safety_replacement.clone();
        let diagnostics = ResponseDiagnostics {
            rust_posterior_particles: particles.len(),
            rust_search_particles: particles.len(),
            root_provenance,
            authority_trace: AuthorityTraceOutput {
                initial_authority,
                exact_family,
                exact_family_replacement,
                safety_replacement,
            },
        };
        let report = SearchReport {
            chosen,
            root_value,
            actions,
            tactical,
            exact,
            statistics: SearchStatistics {
                iterations: total_rollouts,
                nodes: total_rollouts as usize * self.config.rollout_steps,
                deepest_decision_depth: 0,
                rollouts: total_rollouts,
                effective_particle_count: effective_particle_count(&particles),
                deadline_reached: false,
            },
        };
        serde_json::to_value(response(
            report,
            particles.len(),
            GPU_ALGORITHM,
            authority,
            diagnostics,
        ))
        .map_err(|error| error.to_string())
    }
}
