use colonist_catan_core::{Action, GameState};
use colonist_catan_search::{
    ActionStats, CudaSimAgentSearchConfig, CudaSimEngine, ExactActionFamily, SearchReport,
    SearchStatistics, belief_domestic_trade_threat, exact_family_for_action, forced_loss_weight,
    posterior_immediate_threat_weight, safer_end_turn_alternative, shared_root_candidates,
    solve_belief_current_turn, solve_exact_belief_excluding,
};
use serde::Serialize;
use serde_json::Value;

use super::{
    ActionReplacementOutput, AuthorityTraceOutput, DecisionAuthority, PrunedRootOutput,
    RankedRootOutput, Request, ResponseDiagnostics, RetainedRootOutput, RootProvenanceOutput,
    action, basic_response_diagnostics, effective_particle_count, exact_family_label,
    exact_mandatory_report, game_states, response, root_exclusion_actions,
};

const GPU_ALGORITHM: &str = "gpu-root-rollout";
pub const NATIVE_GPU_PROTOCOL_VERSION: u32 = 2;
pub const NATIVE_GPU_STATE_SCHEMA_VERSION: u32 = 1;

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
struct RankedGpuRoot {
    action: Action,
    prior: f32,
    legal_weight: f32,
}

#[derive(Clone)]
struct AggregatedRoot {
    action: Action,
    prior: f32,
    availability: u32,
    legal_weight: f32,
    samples: u32,
    errors: u32,
    terminal_outcome: f32,
    victory_margin: f32,
    mean_turn: f32,
    candidate_vp: f32,
    opponent_vp: f32,
}

fn baseline_rollout_metrics(state: &GameState, actor: u8) -> (f32, f32, f32, f32, f32) {
    let candidate_vp = state.players[actor as usize].victory_points() as f32;
    let opponent_vp = state
        .players
        .iter()
        .enumerate()
        .filter(|(player, _)| *player != actor as usize)
        .map(|(_, player)| player.victory_points() as f32)
        .fold(0.0, f32::max);
    let terminal_outcome = match state.winner() {
        Some(winner) if winner == actor => 1.0,
        Some(_) => -1.0,
        None => 0.0,
    };
    (
        terminal_outcome,
        candidate_vp - opponent_vp,
        state.turn as f32,
        candidate_vp,
        opponent_vp,
    )
}

fn truncate_ranked_preserving_end_turn(ranked: &[RankedGpuRoot], cap: usize) -> Vec<RankedGpuRoot> {
    let cap = cap.max(1);
    if ranked.len() <= cap {
        return ranked.to_vec();
    }
    let end_turn = ranked
        .iter()
        .find(|candidate| candidate.action == Action::EndTurn)
        .cloned();
    let mut retained = ranked.iter().take(cap).cloned().collect::<Vec<_>>();
    if let Some(end_turn) = end_turn
        && !retained
            .iter()
            .any(|candidate| candidate.action == Action::EndTurn)
    {
        retained.pop();
        retained.push(end_turn);
    }
    retained
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
            return Err(
                "GPU native search does not accept speculative opponent-turn pondering".into(),
            );
        }

        let particles = game_states(request.state, request.last_rejected_trade)?;
        if particles.is_empty() {
            return Err("GPU native search received no belief particles".into());
        }
        let root_exclusions =
            root_exclusion_actions(&request.root_exclusions, &particles[0].state)?;
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
        if particles.iter().any(|particle| {
            particle.state.actor() != actor || particle.state.observation_hash(actor) != observation
        }) {
            return Err(
                "GPU native search requires one observation-safe root across belief particles"
                    .into(),
            );
        }

        // Honor the live request's directly corresponding GPU search knobs
        // instead of silently running every native decision at the arena's
        // fixed 4 x 16 x 32 campaign budget. `iterations` is treated as a
        // total root-rollout budget, `branchCap` as root width, and
        // `rolloutActions` as continuation length. CPU node/time budgets are
        // intentionally not given fake one-to-one GPU meanings.
        let root_samples = request
            .branch_cap
            .unwrap_or(self.config.root_samples)
            .clamp(2, 24);
        let rollout_steps = request
            .rollout_actions
            .map(usize::from)
            .unwrap_or(self.config.rollout_steps)
            .clamp(24, 160);
        let total_root_rollouts = request
            .iterations
            .map(|value| value as usize)
            .unwrap_or(root_samples.saturating_mul(self.config.rollouts_per_action));
        let rollouts_per_action = total_root_rollouts.div_ceil(root_samples).clamp(8, 96);
        let search_config = CudaSimAgentSearchConfig {
            root_samples,
            rollouts_per_action,
            rollout_steps,
        };

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

        // Root availability is part of the belief state. Do not intersect the
        // particles: an action legal in only some hidden worlds must retain its
        // posterior mass and a no-action baseline in unavailable worlds.
        let mut legal_by_particle = Vec::<Vec<Action>>::with_capacity(particles.len());
        let mut ranked = Vec::<RankedGpuRoot>::new();
        for (particle, weight) in particles.iter().zip(weights.iter().copied()) {
            if weight <= f32::EPSILON {
                legal_by_particle.push(Vec::new());
                continue;
            }
            let legal = particle
                .state
                .legal_actions()
                .into_iter()
                .filter(|candidate| !root_exclusions.contains(candidate))
                .collect::<Vec<_>>();
            let ordered =
                shared_root_candidates(&particle.state, actor, &legal, legal.len().max(1));
            for (candidate, prior) in ordered {
                if let Some(existing) = ranked
                    .iter_mut()
                    .find(|existing| existing.action == candidate)
                {
                    existing.prior += prior * weight;
                    existing.legal_weight += weight;
                } else {
                    ranked.push(RankedGpuRoot {
                        action: candidate,
                        prior: prior * weight,
                        legal_weight: weight,
                    });
                }
            }
            legal_by_particle.push(legal);
        }
        if ranked.is_empty() {
            return Err("GPU native search found no legal root action across the belief".into());
        }
        let prior_mass = ranked
            .iter()
            .map(|candidate| candidate.prior.max(0.0))
            .sum::<f32>()
            .max(f32::EPSILON);
        for candidate in &mut ranked {
            candidate.prior = candidate.prior.max(0.0) / prior_mass;
            candidate.legal_weight = candidate.legal_weight.clamp(0.0, 1.0);
        }
        ranked.sort_by(|left, right| {
            right
                .prior
                .total_cmp(&left.prior)
                .then_with(|| right.legal_weight.total_cmp(&left.legal_weight))
                .then_with(|| format!("{:?}", left.action).cmp(&format!("{:?}", right.action)))
        });

        // Match production MaxN's threat invariant: verified escapes enter the
        // root before the ordinary width cap, so a low-prior blocker cannot be
        // deleted before search has a chance to value it.
        let immediate_threat_weight = posterior_immediate_threat_weight(
            particles
                .iter()
                .map(|particle| (&particle.state, particle.weight)),
            actor,
        );
        let verified_blockers = if immediate_threat_weight > f32::EPSILON {
            ranked
                .iter()
                .filter(|candidate| {
                    forced_loss_weight(
                        particles
                            .iter()
                            .map(|particle| (&particle.state, particle.weight)),
                        actor,
                        &candidate.action,
                    ) + 1e-6
                        < immediate_threat_weight
                })
                .cloned()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let ordinarily_retained =
            truncate_ranked_preserving_end_turn(&ranked, search_config.root_samples);
        let mut pre_trade_retained =
            Vec::<RankedGpuRoot>::with_capacity(search_config.root_samples);
        for candidate in verified_blockers
            .into_iter()
            .chain(ordinarily_retained.into_iter())
        {
            if pre_trade_retained.len() >= search_config.root_samples.max(1) {
                break;
            }
            if !pre_trade_retained
                .iter()
                .any(|existing| existing.action == candidate.action)
            {
                pre_trade_retained.push(candidate);
            }
        }
        let mut pruned_roots = ranked
            .iter()
            .enumerate()
            .filter(|(_, candidate)| {
                !pre_trade_retained
                    .iter()
                    .any(|retained| retained.action == candidate.action)
            })
            .map(|(rank, candidate)| PrunedRootOutput {
                action: action(candidate.action.clone()),
                pre_truncation_rank: Some(rank),
                reason: "branch-truncated",
            })
            .collect::<Vec<_>>();

        let safe = pre_trade_retained
            .iter()
            .filter(|candidate| {
                belief_domestic_trade_threat(
                    particles
                        .iter()
                        .map(|particle| (&particle.state, particle.weight)),
                    &candidate.action,
                )
                .is_none()
            })
            .cloned()
            .collect::<Vec<_>>();
        let retained = if safe.is_empty() {
            pre_trade_retained.clone()
        } else {
            safe
        };
        for candidate in &pre_trade_retained {
            if !retained.iter().any(|safe| safe.action == candidate.action) {
                pruned_roots.push(PrunedRootOutput {
                    action: action(candidate.action.clone()),
                    pre_truncation_rank: ranked
                        .iter()
                        .position(|ranked| ranked.action == candidate.action),
                    reason: "trade-safety",
                });
            }
        }

        let states = particles
            .iter()
            .map(|particle| particle.state.clone())
            .collect::<Vec<_>>();
        self.cuda
            .upload_states(&states)
            .map_err(|error| error.to_string())?;
        let root_actions = retained
            .iter()
            .map(|candidate| candidate.action.clone())
            .collect::<Vec<_>>();
        let root_rows = legal_by_particle
            .iter()
            .map(|legal| {
                root_actions
                    .iter()
                    .filter(|candidate| legal.iter().any(|action| action == *candidate))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let searched = self
            .cuda
            .search_root_actions(
                &root_rows,
                search_config.rollouts_per_action,
                search_config.rollout_steps,
                request.seed.unwrap_or(0x0043_4154_414e),
            )
            .map_err(|error| error.to_string())?;

        let mut aggregated = Vec::<AggregatedRoot>::with_capacity(root_actions.len());
        for (root_index, candidate) in root_actions.iter().enumerate() {
            let retained_root = &retained[root_index];
            let mut root = AggregatedRoot {
                action: candidate.clone(),
                prior: retained_root.prior,
                availability: 0,
                legal_weight: retained_root.legal_weight,
                samples: 0,
                errors: 0,
                terminal_outcome: 0.0,
                victory_margin: 0.0,
                mean_turn: 0.0,
                candidate_vp: 0.0,
                opponent_vp: 0.0,
            };
            for (((row, legal), weight), particle) in searched
                .rows
                .iter()
                .zip(legal_by_particle.iter())
                .zip(weights.iter().copied())
                .zip(particles.iter())
            {
                if let Some(stat) = row.iter().find(|stat| stat.action == *candidate) {
                    root.availability = root.availability.saturating_add(1);
                    root.samples = root.samples.saturating_add(stat.samples);
                    root.errors = root.errors.saturating_add(stat.errors);
                    root.terminal_outcome += stat.net_terminal_outcome() * weight;
                    root.victory_margin += stat.mean_victory_margin() * weight;
                    root.mean_turn += stat.mean_turn * weight;
                    root.candidate_vp += stat.mean_victory_points * weight;
                    root.opponent_vp += stat.mean_best_opponent_victory_points * weight;
                    continue;
                }
                if legal.iter().any(|action| action == candidate) {
                    return Err("GPU native search omitted a legal retained root action".into());
                }
                let (terminal_outcome, victory_margin, mean_turn, candidate_vp, opponent_vp) =
                    baseline_rollout_metrics(&particle.state, actor);
                root.terminal_outcome += terminal_outcome * weight;
                root.victory_margin += victory_margin * weight;
                root.mean_turn += mean_turn * weight;
                root.candidate_vp += candidate_vp * weight;
                root.opponent_vp += opponent_vp * weight;
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
                availability: candidate.availability,
                availability_weight: candidate.legal_weight,
                legal_weight: candidate.legal_weight,
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
        if !tactical.proven
            && let Some(current) = chosen.clone()
        {
            let current_loss = forced_loss_weight(
                particles
                    .iter()
                    .map(|particle| (&particle.state, particle.weight)),
                actor,
                &current,
            );
            if current_loss >= 1.0 - 1e-6
                && let Some(escape) = aggregated
                    .iter()
                    .filter(|candidate| candidate.errors == 0)
                    .filter(|candidate| {
                        forced_loss_weight(
                            particles
                                .iter()
                                .map(|particle| (&particle.state, particle.weight)),
                            actor,
                            &candidate.action,
                        ) <= 1e-6
                    })
                    .max_by(|left, right| {
                        left.terminal_outcome
                            .total_cmp(&right.terminal_outcome)
                            .then_with(|| left.victory_margin.total_cmp(&right.victory_margin))
                            .then_with(|| right.mean_turn.total_cmp(&left.mean_turn))
                            .then_with(|| left.prior.total_cmp(&right.prior))
                    })
            {
                if escape.action != current {
                    safety_replacement = Some(ActionReplacementOutput {
                        from: action(current),
                        to: action(escape.action.clone()),
                    });
                    chosen = Some(escape.action.clone());
                    authority = DecisionAuthority::SafetyOverride;
                }
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
                .map(|(rank, candidate)| RankedRootOutput {
                    action: action(candidate.action.clone()),
                    rank,
                    prior: candidate.prior,
                    planner_value: None,
                    planner_completion_mass: None,
                })
                .collect(),
            retained_roots: retained
                .iter()
                .map(|candidate| RetainedRootOutput {
                    action: action(candidate.action.clone()),
                    pre_truncation_rank: ranked
                        .iter()
                        .position(|ranked_candidate| ranked_candidate.action == candidate.action),
                    prior: candidate.prior,
                    node_budget_per_particle: 0,
                    allocated_nodes: 0,
                    planner_value: None,
                    planner_completion_mass: None,
                })
                .collect(),
            pruned_root_count: pruned_roots.len(),
            pruned_roots,
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
                nodes: total_rollouts as usize * search_config.rollout_steps,
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
