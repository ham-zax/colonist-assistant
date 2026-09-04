use colonist_catan_core::{Action, GameState};
use colonist_catan_search::{
    ActionStats, BeliefParticle, CudaSimEngine, CudaSimError, CudaSimRootActionStats,
    DEVELOPMENT_EXACT_FAMILIES, ExactActionFamily, ExactDecisionResult, HARD_VETO_POSTERIOR,
    IntroducedRoadFragility, RoadCutContinuationAssessment, SearchReport, SearchStatistics,
    admit_promoted_roots, apply_closeout_root_impacts, belief_domestic_trade_assessment,
    belief_road_cut_continuation_assessment, belief_root_closeout_plans,
    compute_spatial_root_impacts, exact_family_for_action, forced_loss_weight,
    posterior_immediate_threat_weight, safer_end_turn_alternative, shared_root_candidates,
    solve_belief_current_turn_timed, solve_exact_belief_excluding_controlled,
};
use serde::Serialize;
use serde_json::Value;

use super::{
    ActionReplacementOutput, AuthorityTraceOutput, DecisionAuthority, DecisionClock,
    HorizonEscalationOutput, PrunedRootOutput, RankedRootOutput, Request, ResponseDiagnostics,
    RetainedRootOutput, RootCausalEvidenceOutput, RootProvenanceOutput, action,
    basic_response_diagnostics, domestic_trade_threat_label, effective_particle_count,
    exact_family_label, exact_mandatory_report_controlled, game_states,
    introduced_road_fragility_output, response, road_cut_continuation_output,
    root_exclusion_actions, root_promotion_reason, weighted_policy_report_for_actions_controlled,
};

const GPU_ALGORITHM: &str = "gpu-root-rollout";
const HORIZON_ESCALATION_MIN_UNRESOLVED_CUT_MASS: f32 = 0.20;
const HORIZON_ESCALATION_MIN_STEPS: usize = 192;
const HORIZON_ESCALATION_MAX_STEPS: usize = 768;
const HORIZON_ESCALATION_MAX_STAGES: usize = 3;
const HORIZON_ESCALATION_MIN_REMAINING_MS: u32 = 40;
const HORIZON_ESCALATION_MIN_SAMPLES_PER_ROOT: usize = 16;
const HORIZON_ESCALATION_MAX_SAMPLES_PER_ROOT: usize = 32;
pub const NATIVE_GPU_PROTOCOL_VERSION: u32 = 6;
pub const NATIVE_GPU_STATE_SCHEMA_VERSION: u32 = 3;

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
}

#[derive(Clone)]
struct RootFragilityAssessment {
    action: Action,
    introduced_road_fragility: IntroducedRoadFragility,
    road_cut_continuation: RoadCutContinuationAssessment,
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
    terminal_rate: f32,
    terminal_variance: f32,
    victory_margin: f32,
    victory_margin_variance: f32,
    mean_turn: f32,
    candidate_vp: f32,
    candidate_vp_variance: f32,
    opponent_vp: f32,
    opponent_vp_variance: f32,
}

#[derive(Clone, Default)]
struct RootSampleMoments {
    samples: u32,
    errors: u32,
    terminal_sum: f64,
    terminal_square_sum: f64,
    turn_sum: f64,
    margin_sum: f64,
    margin_square_sum: f64,
    candidate_vp_sum: f64,
    candidate_vp_square_sum: f64,
    opponent_vp_sum: f64,
    opponent_vp_square_sum: f64,
}

impl RootSampleMoments {
    fn add(&mut self, stat: &CudaSimRootActionStats) {
        let valid = stat.samples.saturating_sub(stat.errors);
        self.samples = self.samples.saturating_add(stat.samples);
        self.errors = self.errors.saturating_add(stat.errors);
        if valid == 0 {
            return;
        }
        let valid_f64 = f64::from(valid);
        let losses = stat.terminal_samples.saturating_sub(stat.wins);
        self.terminal_sum += f64::from(stat.wins) - f64::from(losses);
        self.terminal_square_sum += f64::from(stat.terminal_samples);
        self.turn_sum += f64::from(stat.mean_turn) * valid_f64;
        self.margin_sum += f64::from(stat.mean_victory_margin()) * valid_f64;
        self.margin_square_sum += f64::from(stat.mean_victory_margin_squared) * valid_f64;
        self.candidate_vp_sum += f64::from(stat.mean_victory_points) * valid_f64;
        self.candidate_vp_square_sum += f64::from(stat.mean_victory_points_squared) * valid_f64;
        self.opponent_vp_sum += f64::from(stat.mean_best_opponent_victory_points) * valid_f64;
        self.opponent_vp_square_sum +=
            f64::from(stat.mean_best_opponent_victory_points_squared) * valid_f64;
    }

    fn valid_samples(&self) -> u32 {
        self.samples.saturating_sub(self.errors)
    }
}

fn variance(mean: f32, second_moment: f32) -> f32 {
    (second_moment - mean * mean).max(0.0)
}

fn confidence_width(variance: f32, samples: u32) -> f32 {
    if samples == 0 {
        return f32::INFINITY;
    }
    1.96 * (variance / samples as f32).max(0.0).sqrt()
}

fn mix_sampling_seed(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn allocate_systematic_counts(
    total: usize,
    weighted: &[(usize, f32)],
    seed: u64,
) -> Vec<(usize, usize)> {
    if total == 0 || weighted.is_empty() {
        return Vec::new();
    }
    let positive = weighted
        .iter()
        .copied()
        .filter(|(_, weight)| *weight > f32::EPSILON)
        .collect::<Vec<_>>();
    if positive.is_empty() {
        return Vec::new();
    }
    let mass = positive
        .iter()
        .map(|(_, weight)| f64::from(weight.max(0.0)))
        .sum::<f64>()
        .max(f64::EPSILON);
    let unit = ((mix_sampling_seed(seed) >> 11) as f64) / ((1u64 << 53) as f64);
    let step = 1.0 / total as f64;
    let start = unit * step;
    let mut counts = vec![0usize; positive.len()];
    let mut slot = 0usize;
    let mut cumulative = f64::from(positive[0].1.max(0.0)) / mass;
    for sample in 0..total {
        let target = start + sample as f64 * step;
        while slot + 1 < positive.len() && target >= cumulative {
            slot += 1;
            cumulative += f64::from(positive[slot].1.max(0.0)) / mass;
        }
        counts[slot] += 1;
    }
    positive
        .into_iter()
        .zip(counts)
        .filter_map(|((index, _), count)| (count > 0).then_some((index, count)))
        .collect()
}

fn equal_budget(total: usize, active: &[usize]) -> Vec<(usize, usize)> {
    if total == 0 || active.is_empty() {
        return Vec::new();
    }
    let base = total / active.len();
    let remainder = total % active.len();
    active
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(position, root)| {
            let count = base + usize::from(position < remainder);
            (count > 0).then_some((root, count))
        })
        .collect()
}

fn weighted_gap(report: &SearchReport, actor: usize) -> f32 {
    let Some(first) = report.actions.first() else {
        return 0.0;
    };
    let Some(second) = report.actions.get(1) else {
        return 1.0;
    };
    (first.value[actor] - second.value[actor]).max(0.0)
}

fn weighted_robust_gap(report: &SearchReport, actor: usize) -> f32 {
    let Some(first) = report.actions.first() else {
        return 0.0;
    };
    let Some(second) = report.actions.get(1) else {
        return 1.0;
    };
    (first.lower_confidence_value[actor] - second.value[actor]).max(0.0)
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

fn aggregate_root(
    candidate: &RankedGpuRoot,
    samples: &RootSampleMoments,
    particles: &[BeliefParticle],
    legal_by_particle: &[Vec<Action>],
    weights: &[f32],
    actor: u8,
) -> AggregatedRoot {
    let valid = samples.valid_samples();
    let valid_f32 = valid.max(1) as f32;
    let legal_terminal = samples.terminal_sum as f32 / valid_f32;
    // Terminal outcomes are {-1, 0, +1}, so their second moment is exactly
    // the fraction of valid rollouts that actually reached a terminal state.
    let legal_terminal_rate = samples.terminal_square_sum as f32 / valid_f32;
    let legal_terminal_second = legal_terminal_rate;
    let legal_turn = samples.turn_sum as f32 / valid_f32;
    let legal_margin = samples.margin_sum as f32 / valid_f32;
    let legal_margin_second = samples.margin_square_sum as f32 / valid_f32;
    let legal_candidate_vp = samples.candidate_vp_sum as f32 / valid_f32;
    let legal_candidate_vp_second = samples.candidate_vp_square_sum as f32 / valid_f32;
    let legal_opponent_vp = samples.opponent_vp_sum as f32 / valid_f32;
    let legal_opponent_vp_second = samples.opponent_vp_square_sum as f32 / valid_f32;

    let mut terminal_outcome = candidate.legal_weight * legal_terminal;
    let mut terminal_rate = candidate.legal_weight * legal_terminal_rate;
    let mut terminal_second = candidate.legal_weight * legal_terminal_second;
    let mut victory_margin = candidate.legal_weight * legal_margin;
    let mut victory_margin_second = candidate.legal_weight * legal_margin_second;
    let mut mean_turn = candidate.legal_weight * legal_turn;
    let mut candidate_vp = candidate.legal_weight * legal_candidate_vp;
    let mut candidate_vp_second = candidate.legal_weight * legal_candidate_vp_second;
    let mut opponent_vp = candidate.legal_weight * legal_opponent_vp;
    let mut opponent_vp_second = candidate.legal_weight * legal_opponent_vp_second;
    let mut availability = 0u32;

    for ((particle, legal), weight) in particles
        .iter()
        .zip(legal_by_particle.iter())
        .zip(weights.iter().copied())
    {
        if legal.iter().any(|action| action == &candidate.action) {
            availability = availability.saturating_add(1);
            continue;
        }
        let (terminal, margin, turn, own_vp, best_opponent_vp) =
            baseline_rollout_metrics(&particle.state, actor);
        terminal_outcome += terminal * weight;
        terminal_rate += terminal.abs() * weight;
        terminal_second += terminal * terminal * weight;
        victory_margin += margin * weight;
        victory_margin_second += margin * margin * weight;
        mean_turn += turn * weight;
        candidate_vp += own_vp * weight;
        candidate_vp_second += own_vp * own_vp * weight;
        opponent_vp += best_opponent_vp * weight;
        opponent_vp_second += best_opponent_vp * best_opponent_vp * weight;
    }

    AggregatedRoot {
        action: candidate.action.clone(),
        prior: candidate.prior,
        availability,
        legal_weight: candidate.legal_weight,
        samples: samples.samples,
        errors: samples.errors,
        terminal_outcome,
        terminal_rate,
        terminal_variance: variance(terminal_outcome, terminal_second),
        victory_margin,
        victory_margin_variance: variance(victory_margin, victory_margin_second),
        mean_turn,
        candidate_vp,
        candidate_vp_variance: variance(candidate_vp, candidate_vp_second),
        opponent_vp,
        opponent_vp_variance: variance(opponent_vp, opponent_vp_second),
    }
}

fn compare_roots(left: &AggregatedRoot, right: &AggregatedRoot) -> std::cmp::Ordering {
    left.terminal_outcome
        .total_cmp(&right.terminal_outcome)
        .then_with(|| left.victory_margin.total_cmp(&right.victory_margin))
        .then_with(|| right.mean_turn.total_cmp(&left.mean_turn))
        .then_with(|| left.prior.total_cmp(&right.prior))
}

fn compare_escalated_point_estimates(
    left: &AggregatedRoot,
    right: &AggregatedRoot,
) -> std::cmp::Ordering {
    left.victory_margin
        .total_cmp(&right.victory_margin)
        .then_with(|| left.terminal_outcome.total_cmp(&right.terminal_outcome))
        .then_with(|| right.mean_turn.total_cmp(&left.mean_turn))
        .then_with(|| left.prior.total_cmp(&right.prior))
}

fn escalated_root_order(indices: &[usize], roots: &[AggregatedRoot]) -> Vec<usize> {
    if indices.len() <= 1 {
        return indices.to_vec();
    }

    let best_terminal_lower = indices
        .iter()
        .map(|index| {
            let root = &roots[*index];
            root.terminal_outcome - confidence_width(root.terminal_variance, root.samples)
        })
        .max_by(f32::total_cmp)
        .expect("non-empty escalation set has a terminal lower bound");
    let is_terminal_contender = |index: usize| {
        let root = &roots[index];
        root.terminal_outcome + confidence_width(root.terminal_variance, root.samples) + 1e-6
            >= best_terminal_lower
    };
    let best_margin_lower = indices
        .iter()
        .copied()
        .filter(|index| is_terminal_contender(*index))
        .map(|index| {
            let root = &roots[index];
            root.victory_margin - confidence_width(root.victory_margin_variance, root.samples)
        })
        .max_by(f32::total_cmp)
        .expect("terminal contender set is non-empty");
    let is_margin_contender = |index: usize| {
        if !is_terminal_contender(index) {
            return false;
        }
        let root = &roots[index];
        root.victory_margin + confidence_width(root.victory_margin_variance, root.samples) + 1e-6
            >= best_margin_lower
    };
    let tier = |index: usize| {
        if is_margin_contender(index) {
            2u8
        } else if is_terminal_contender(index) {
            1u8
        } else {
            0u8
        }
    };

    let mut order = indices.to_vec();
    order.sort_by(|left, right| {
        let left_tier = tier(*left);
        let right_tier = tier(*right);
        left_tier
            .cmp(&right_tier)
            .then_with(|| {
                if left_tier == 0 {
                    compare_roots(&roots[*left], &roots[*right])
                } else {
                    compare_escalated_point_estimates(&roots[*left], &roots[*right])
                }
            })
            .reverse()
    });
    order
}

fn compare_surviving_roots(
    left: &AggregatedRoot,
    right: &AggregatedRoot,
    prefer_prior: bool,
) -> std::cmp::Ordering {
    if !prefer_prior {
        return compare_roots(left, right);
    }
    left.prior
        .total_cmp(&right.prior)
        .then_with(|| compare_roots(left, right))
}

fn racing_contenders(active: &[usize], roots: &[AggregatedRoot]) -> Vec<usize> {
    if active.len() <= 1 {
        return active.to_vec();
    }
    let best = active
        .iter()
        .copied()
        .max_by(|left, right| compare_roots(&roots[*left], &roots[*right]))
        .expect("non-empty active root set has a best root");
    let best_root = &roots[best];
    let best_terminal_lower = best_root.terminal_outcome
        - confidence_width(best_root.terminal_variance, best_root.samples);
    let best_margin_lower = best_root.victory_margin
        - confidence_width(best_root.victory_margin_variance, best_root.samples);
    let mut contenders = active
        .iter()
        .copied()
        .filter(|index| {
            let root = &roots[*index];
            let terminal_upper =
                root.terminal_outcome + confidence_width(root.terminal_variance, root.samples);
            if terminal_upper + 1e-6 < best_terminal_lower {
                return false;
            }
            let terminal_overlap = (root.terminal_outcome - best_root.terminal_outcome).abs()
                <= confidence_width(root.terminal_variance, root.samples)
                    + confidence_width(best_root.terminal_variance, best_root.samples);
            if !terminal_overlap {
                return true;
            }
            root.victory_margin
                + confidence_width(root.victory_margin_variance, root.samples)
                + 1e-6
                >= best_margin_lower
        })
        .collect::<Vec<_>>();
    if !contenders.contains(&best) {
        contenders.push(best);
    }
    if contenders.len() > 2 {
        contenders.sort_by(|left, right| {
            let left_root = &roots[*left];
            let right_root = &roots[*right];
            let left_terminal = left_root.terminal_outcome
                - confidence_width(left_root.terminal_variance, left_root.samples);
            let right_terminal = right_root.terminal_outcome
                - confidence_width(right_root.terminal_variance, right_root.samples);
            right_terminal
                .total_cmp(&left_terminal)
                .then_with(|| {
                    let left_margin = left_root.victory_margin
                        - confidence_width(left_root.victory_margin_variance, left_root.samples);
                    let right_margin = right_root.victory_margin
                        - confidence_width(right_root.victory_margin_variance, right_root.samples);
                    right_margin.total_cmp(&left_margin)
                })
                .then_with(|| right_root.prior.total_cmp(&left_root.prior))
        });
        contenders.truncate(contenders.len().div_ceil(2).max(2));
    }
    contenders
}

fn horizon_escalation_schedule(initial_horizon: usize) -> Vec<usize> {
    let mut horizons = Vec::with_capacity(HORIZON_ESCALATION_MAX_STAGES);
    let mut next = initial_horizon
        .saturating_mul(2)
        .max(HORIZON_ESCALATION_MIN_STEPS)
        .min(HORIZON_ESCALATION_MAX_STEPS);
    while horizons.len() < HORIZON_ESCALATION_MAX_STAGES {
        if horizons.last().copied() == Some(next) {
            break;
        }
        horizons.push(next);
        if next >= HORIZON_ESCALATION_MAX_STEPS {
            break;
        }
        next = next.saturating_mul(2).min(HORIZON_ESCALATION_MAX_STEPS);
    }
    horizons
}

fn horizon_escalation_trigger(
    winner: &AggregatedRoot,
    evidence: &RootFragilityAssessment,
) -> Option<f32> {
    if evidence.introduced_road_fragility.award_vp_exposure == 0
        || evidence.road_cut_continuation.award_loss_posterior <= f32::EPSILON
    {
        return None;
    }
    let unresolved_cut_mass = evidence.road_cut_continuation.award_loss_posterior
        * (1.0 - winner.terminal_rate.clamp(0.0, 1.0));
    (unresolved_cut_mass + 1e-6 >= HORIZON_ESCALATION_MIN_UNRESOLVED_CUT_MASS)
        .then_some(unresolved_cut_mass)
}

fn horizon_escalation_contenders(
    provisional_winner: usize,
    shallow_racers: &[usize],
    retained: &[RankedGpuRoot],
    evidence: &RootFragilityAssessment,
) -> Vec<usize> {
    let mut contenders = Vec::with_capacity(shallow_racers.len() + 4);
    contenders.push(provisional_winner);
    for index in shallow_racers.iter().copied() {
        if !contenders.contains(&index) {
            contenders.push(index);
        }
    }

    let exposed_vertices = evidence
        .introduced_road_fragility
        .critical_vertices
        .iter()
        .filter(|cut| cut.award_loss || cut.additional_road_loss > 0)
        .map(|cut| cut.vertex)
        .collect::<Vec<_>>();
    let approach_edges = evidence
        .road_cut_continuation
        .continuations
        .iter()
        .filter(|continuation| continuation.posterior > f32::EPSILON)
        .flat_map(|continuation| continuation.approach_edges.iter().copied())
        .collect::<Vec<_>>();

    for (index, candidate) in retained.iter().enumerate() {
        let causal_protector = match &candidate.action {
            Action::BuildRoad { edge } => approach_edges.contains(edge),
            Action::BuildSettlement { vertex } => exposed_vertices.contains(vertex),
            _ => false,
        };
        if causal_protector && !contenders.contains(&index) {
            contenders.push(index);
        }
    }
    contenders
}

impl NativeGpuSearchEngine {
    pub fn new() -> Result<Self, String> {
        let cuda = CudaSimEngine::new().map_err(|error| error.to_string())?;
        Ok(Self { cuda })
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
        self.analyze_json_controlled(value, || false)
    }

    pub fn analyze_json_controlled<F>(
        &mut self,
        value: Value,
        should_cancel: F,
    ) -> Result<Value, String>
    where
        F: Fn() -> bool,
    {
        let request: Request = serde_json::from_value(value).map_err(|error| error.to_string())?;
        if matches!(request.mode.as_deref(), Some("weighted")) {
            return Err("GPU native search is reserved for the Strategist engine".into());
        }
        if request.ponder.unwrap_or(false) {
            return Err(
                "GPU native search does not accept speculative opponent-turn pondering".into(),
            );
        }
        let effort = request.resolved_effort();
        let decision_clock = DecisionClock::start(effort.decision_time_ms);
        if should_cancel() {
            return Err("GPU native search cancelled".into());
        }

        let particles = game_states(request.state, request.last_rejected_trade)?;
        if particles.is_empty() {
            return Err("GPU native search received no belief particles".into());
        }
        let root_exclusions =
            root_exclusion_actions(&request.root_exclusions, &particles[0].state)?;
        match exact_mandatory_report_controlled(&particles, &root_exclusions, || {
            should_cancel() || decision_clock.remaining_ms() == 0
        }) {
            Ok(Some(report)) => {
                return serde_json::to_value(response(
                    report,
                    particles.len(),
                    GPU_ALGORITHM,
                    DecisionAuthority::ExactMandatory,
                    basic_response_diagnostics(
                        particles.len(),
                        DecisionAuthority::ExactMandatory,
                        effort,
                    ),
                ))
                .map_err(|error| error.to_string());
            }
            Ok(None) => {}
            Err(()) if should_cancel() => return Err("GPU native search cancelled".into()),
            Err(()) => {
                return Err("GPU native decision deadline expired during exact arbitration".into());
            }
        }

        let actor = particles[0].state.actor();
        let tactical_particles = particles
            .iter()
            .map(|particle| (&particle.state, particle.weight))
            .collect::<Vec<_>>();
        let tactical_budget_ms = decision_clock
            .remaining_ms()
            .min((effort.decision_time_ms / 3).max(1));
        let tactical = solve_belief_current_turn_timed(
            &tactical_particles,
            effort.tactical.max_depth,
            effort.tactical.node_budget,
            tactical_budget_ms,
        );
        if tactical.proven {
            let total_weight = particles
                .iter()
                .map(|particle| particle.weight.max(0.0))
                .sum::<f32>()
                .max(f32::EPSILON);
            let root_value = particles.iter().fold([0.0; 4], |mut total, particle| {
                let weight = particle.weight.max(0.0) / total_weight;
                let evaluated = colonist_catan_search::evaluate(&particle.state);
                for player in 0..4 {
                    total[player] += evaluated[player] * weight;
                }
                total
            });
            let report = SearchReport {
                chosen: tactical.principal_line.first().cloned(),
                root_value,
                actions: Vec::new(),
                tactical: tactical.clone(),
                exact: ExactDecisionResult::default(),
                statistics: SearchStatistics {
                    iterations: 0,
                    nodes: tactical.nodes as usize,
                    deepest_decision_depth: 0,
                    rollouts: 0,
                    effective_particle_count: effective_particle_count(&particles),
                    deadline_reached: decision_clock.remaining_ms() == 0,
                },
            };
            return serde_json::to_value(response(
                report,
                particles.len(),
                GPU_ALGORITHM,
                DecisionAuthority::TacticalProven,
                basic_response_diagnostics(
                    particles.len(),
                    DecisionAuthority::TacticalProven,
                    effort,
                ),
            ))
            .map_err(|error| error.to_string());
        }

        let observation = particles[0].state.observation_hash(actor);
        if particles.iter().any(|particle| {
            particle.state.actor() != actor || particle.state.observation_hash(actor) != observation
        }) {
            return Err(
                "GPU native search requires one observation-safe root across belief particles"
                    .into(),
            );
        }

        let rollout_budget = effort.gpu.rollout_budget as usize;
        let root_cap = effort.gpu.root_cap.min(rollout_budget.max(1));
        let rollout_steps = effort.gpu.rollout_steps as usize;
        if decision_clock.remaining_ms() == 0 {
            return Err("GPU native decision deadline expired before strategic preparation".into());
        }

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
        let mut pruned_roots = Vec::<PrunedRootOutput>::new();
        let mut exact_family_results = Vec::<(ExactActionFamily, ExactDecisionResult)>::new();
        for family in DEVELOPMENT_EXACT_FAMILIES {
            if should_cancel() {
                return Err("GPU native search cancelled".into());
            }
            if decision_clock.remaining_ms() == 0 {
                break;
            }
            let family_members = ranked
                .iter()
                .enumerate()
                .filter(|(_, candidate)| exact_family_for_action(&candidate.action) == Some(family))
                .map(|(rank, candidate)| (rank, candidate.clone()))
                .collect::<Vec<_>>();
            if family_members.is_empty() {
                continue;
            }
            let Some(exact) = solve_exact_belief_excluding_controlled(
                &particles,
                family,
                &root_exclusions,
                || should_cancel() || decision_clock.remaining_ms() == 0,
            ) else {
                if should_cancel() {
                    return Err("GPU native search cancelled".into());
                }
                break;
            };
            let Some(representative) = exact.chosen.clone() else {
                continue;
            };
            let family_prior = family_members
                .iter()
                .map(|(_, candidate)| candidate.prior.max(0.0))
                .sum::<f32>();
            let legal_weight = exact
                .actions
                .iter()
                .find(|candidate| candidate.action == representative)
                .map_or_else(
                    || {
                        family_members
                            .iter()
                            .find(|(_, candidate)| candidate.action == representative)
                            .map_or(0.0, |(_, candidate)| candidate.legal_weight)
                    },
                    |candidate| candidate.legal_weight,
                );
            for (rank, candidate) in &family_members {
                if candidate.action != representative {
                    pruned_roots.push(PrunedRootOutput {
                        action: action(candidate.action.clone()),
                        pre_truncation_rank: Some(*rank + 1),
                        reason: "exact-family-collapsed",
                    });
                }
            }
            ranked.retain(|candidate| exact_family_for_action(&candidate.action) != Some(family));
            ranked.push(RankedGpuRoot {
                action: representative,
                prior: family_prior,
                legal_weight,
            });
            exact_family_results.push((family, exact));
        }
        ranked.sort_by(|left, right| {
            right
                .prior
                .total_cmp(&left.prior)
                .then_with(|| right.legal_weight.total_cmp(&left.legal_weight))
                .then_with(|| format!("{:?}", left.action).cmp(&format!("{:?}", right.action)))
        });
        let ranked_root_count = ranked.len();

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
                .filter_map(|candidate| {
                    let residual_loss = forced_loss_weight(
                        particles
                            .iter()
                            .map(|particle| (&particle.state, particle.weight)),
                        actor,
                        &candidate.action,
                    );
                    (residual_loss + 1e-6 < immediate_threat_weight)
                        .then(|| (candidate.action.clone(), residual_loss))
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let ranked_tuples: Vec<(Action, f32)> =
            ranked.iter().map(|r| (r.action.clone(), r.prior)).collect();
        let root_actions_list: Vec<Action> = ranked.iter().map(|r| r.action.clone()).collect();
        let planner_nodes = (effort.cpu.nodes_per_depth_wave / 12).clamp(300, 4_000);
        let closeout_plans = belief_root_closeout_plans(&particles, actor, planner_nodes)
            .into_iter()
            .filter(|plan| root_actions_list.contains(&plan.first_action))
            .collect::<Vec<_>>();
        let spatial_impact_report = particles.first().map(|first| {
            let mut report = compute_spatial_root_impacts(&first.state, actor, &root_actions_list);
            apply_closeout_root_impacts(&mut report, &closeout_plans);
            report
        });
        let promoted_spatial_actions: Vec<Action> = spatial_impact_report
            .as_ref()
            .map(|report| {
                report
                    .actions
                    .iter()
                    .filter(|impact| impact.promotion.is_some())
                    .map(|impact| impact.action.clone())
                    .collect()
            })
            .unwrap_or_default();
        let admitted_without_promotions =
            admit_promoted_roots(&ranked_tuples, &verified_blockers, &[], root_cap);
        let admitted = admit_promoted_roots(
            &ranked_tuples,
            &verified_blockers,
            &promoted_spatial_actions,
            root_cap,
        );
        let admitted_by_promotion = admitted
            .iter()
            .filter(|(action, _)| {
                !admitted_without_promotions
                    .iter()
                    .any(|(ordinary, _)| ordinary == action)
            })
            .map(|(action, _)| action.clone())
            .collect::<Vec<_>>();
        let trade_assessments = admitted
            .iter()
            .map(|(candidate_action, _)| {
                (
                    candidate_action.clone(),
                    belief_domestic_trade_assessment(
                        particles
                            .iter()
                            .map(|particle| (&particle.state, particle.weight)),
                        candidate_action,
                    ),
                )
            })
            .collect::<Vec<_>>();
        let fragility_assessments = ranked
            .iter()
            .map(|candidate| {
                let introduced_road_fragility = spatial_impact_report
                    .as_ref()
                    .and_then(|report| {
                        report
                            .actions
                            .iter()
                            .find(|impact| impact.action == candidate.action)
                    })
                    .map(|impact| impact.introduced_road_fragility.clone())
                    .unwrap_or_default();
                let exposed_vertices = introduced_road_fragility
                    .critical_vertices
                    .iter()
                    .map(|cut| cut.vertex)
                    .collect::<Vec<_>>();
                let road_cut_continuation = belief_road_cut_continuation_assessment(
                    particles
                        .iter()
                        .map(|particle| (&particle.state, particle.weight)),
                    actor,
                    &candidate.action,
                    &exposed_vertices,
                );
                RootFragilityAssessment {
                    action: candidate.action.clone(),
                    introduced_road_fragility,
                    road_cut_continuation,
                }
            })
            .collect::<Vec<_>>();
        let root_evidence = ranked
            .iter()
            .map(|candidate| {
                let impact = spatial_impact_report.as_ref().and_then(|report| {
                    report
                        .actions
                        .iter()
                        .find(|impact| impact.action == candidate.action)
                });
                let trade = trade_assessments
                    .iter()
                    .find(|(action, _)| action == &candidate.action)
                    .map_or_else(Default::default, |(_, assessment)| *assessment);
                let fragility = fragility_assessments
                    .iter()
                    .find(|assessment| assessment.action == candidate.action)
                    .expect("every ranked root has a fragility assessment");
                let has_introduced_fragility = !fragility
                    .introduced_road_fragility
                    .critical_vertices
                    .is_empty();
                RootCausalEvidenceOutput {
                    action: action(candidate.action.clone()),
                    promotion_reason: impact
                        .and_then(|impact| impact.promotion)
                        .map(root_promotion_reason),
                    introduced_road_fragility: has_introduced_fragility.then(|| {
                        introduced_road_fragility_output(&fragility.introduced_road_fragility)
                    }),
                    road_cut_continuation: has_introduced_fragility
                        .then(|| road_cut_continuation_output(&fragility.road_cut_continuation)),
                    admitted_by_promotion: impact.is_some_and(|impact| {
                        impact.promotion.is_some()
                            && admitted
                                .iter()
                                .any(|(action, _)| action == &candidate.action)
                            && !admitted_without_promotions
                                .iter()
                                .any(|(action, _)| action == &candidate.action)
                    }),
                    closeout_gain: impact.map_or(0.0, |impact| impact.closeout_gain),
                    response_windows: impact.and_then(|impact| impact.response_windows),
                    decisive_completion_mass: impact
                        .map_or(0.0, |impact| impact.decisive_completion_mass),
                    trade_threat: trade.threat.map(domestic_trade_threat_label),
                    trade_risk_posterior: trade.posterior,
                    dirty_monopoly_posterior: trade.dirty_monopoly_posterior,
                    trade_hard_veto_posterior: trade.hard_veto_posterior,
                    trade_hard_veto: trade.hard_veto,
                }
            })
            .collect::<Vec<_>>();
        let mut pre_trade_retained = Vec::with_capacity(admitted.len());
        for (candidate_action, _) in &admitted {
            if let Some(r) = ranked.iter().find(|r| r.action == *candidate_action) {
                pre_trade_retained.push(r.clone());
            }
        }
        pruned_roots.extend(
            ranked
                .iter()
                .enumerate()
                .filter(|(_, candidate)| {
                    !pre_trade_retained
                        .iter()
                        .any(|retained| retained.action == candidate.action)
                })
                .map(|(rank, candidate)| PrunedRootOutput {
                    action: action(candidate.action.clone()),
                    pre_truncation_rank: Some(rank + 1),
                    reason: "branch-truncated",
                }),
        );

        let safe = pre_trade_retained
            .iter()
            .filter(|candidate| {
                trade_assessments
                    .iter()
                    .find(|(action, _)| action == &candidate.action)
                    .is_none_or(|(_, assessment)| !assessment.hard_veto)
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
                        .position(|ranked| ranked.action == candidate.action)
                        .map(|rank| rank + 1),
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
        if root_actions.is_empty() {
            return Err("GPU native search retained no strategic root actions".into());
        }
        if decision_clock.remaining_ms() == 0 {
            return Err("GPU native decision deadline expired before strategic sampling".into());
        }
        let prepass = match weighted_policy_report_for_actions_controlled(
            &particles,
            &root_exclusions,
            Some(&root_actions),
            || should_cancel() || decision_clock.remaining_ms() == 0,
        ) {
            Ok(report) => report,
            Err(()) if should_cancel() => return Err("GPU native search cancelled".into()),
            Err(()) => {
                return Err("GPU native decision deadline expired during strategic prepass".into());
            }
        };
        let prepass_gap = weighted_gap(&prepass, actor as usize);
        let prepass_robust_gap = weighted_robust_gap(&prepass, actor as usize);

        let mut moments = vec![RootSampleMoments::default(); root_actions.len()];
        let mut active = (0..root_actions.len()).collect::<Vec<_>>();
        let mut remaining_rollouts = rollout_budget;
        let effective_particles = effective_particle_count(&particles);
        let prepass_disagrees =
            prepass.chosen.as_ref() != retained.first().map(|root| &root.action);
        let ambiguous = prepass_gap < 0.06
            || prepass_robust_gap < 0.02
            || effective_particles >= 32.0
            || immediate_threat_weight > f32::EPSILON
            || prepass_disagrees;
        let mut samples_per_active_root = if ambiguous { 8usize } else { 4usize };
        let base_seed = request.seed.unwrap_or(0x0043_4154_414e);
        let mut phase = 0u64;
        while remaining_rollouts > 0 && !active.is_empty() {
            if should_cancel() {
                return Err("GPU native search cancelled".into());
            }
            if decision_clock.remaining_ms() == 0 {
                break;
            }
            let phase_total = remaining_rollouts.min(
                active
                    .len()
                    .saturating_mul(samples_per_active_root)
                    .max(active.len()),
            );
            let root_budgets = equal_budget(phase_total, &active);
            let mut root_rows = vec![Vec::<Action>::new(); particles.len()];
            for (root_index, budget) in root_budgets {
                let eligible = legal_by_particle
                    .iter()
                    .enumerate()
                    .filter(|(_, legal)| {
                        legal
                            .iter()
                            .any(|action| action == &root_actions[root_index])
                    })
                    .map(|(particle_index, _)| (particle_index, weights[particle_index]))
                    .collect::<Vec<_>>();
                let sampling_seed = base_seed
                    ^ (root_index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
                    ^ phase.wrapping_mul(0xa409_3822_299f_31d0);
                for (particle_index, count) in
                    allocate_systematic_counts(budget, &eligible, sampling_seed)
                {
                    root_rows[particle_index]
                        .extend(std::iter::repeat_n(root_actions[root_index].clone(), count));
                }
            }
            let phase_rollouts = root_rows.iter().map(Vec::len).sum::<usize>();
            if phase_rollouts == 0 {
                break;
            }
            let phase_seed = base_seed ^ phase.wrapping_mul(0x9e37_79b9_7f4a_7c15);
            let searched = match self.cuda.search_root_actions_controlled(
                &root_rows,
                1,
                rollout_steps,
                phase_seed,
                || should_cancel() || decision_clock.remaining_ms() == 0,
            ) {
                Ok(searched) => searched,
                Err(CudaSimError::Cancelled) if should_cancel() => {
                    return Err("GPU native search cancelled".into());
                }
                Err(CudaSimError::Cancelled) if decision_clock.remaining_ms() == 0 => break,
                Err(error) => return Err(error.to_string()),
            };
            for row in &searched.rows {
                for stat in row {
                    if let Some(root_index) = root_actions
                        .iter()
                        .position(|candidate| candidate == &stat.action)
                    {
                        moments[root_index].add(stat);
                    }
                }
            }
            remaining_rollouts = remaining_rollouts.saturating_sub(phase_rollouts);
            let snapshot = retained
                .iter()
                .enumerate()
                .map(|(index, candidate)| {
                    aggregate_root(
                        candidate,
                        &moments[index],
                        &particles,
                        &legal_by_particle,
                        &weights,
                        actor,
                    )
                })
                .collect::<Vec<_>>();
            let next_active = racing_contenders(&active, &snapshot);
            active = next_active;
            if active.len() <= 1 {
                break;
            }
            samples_per_active_root = samples_per_active_root.saturating_mul(2).min(128);
            phase = phase.wrapping_add(1);
        }

        let shallow_aggregated = retained
            .iter()
            .enumerate()
            .map(|(index, candidate)| {
                aggregate_root(
                    candidate,
                    &moments[index],
                    &particles,
                    &legal_by_particle,
                    &weights,
                    actor,
                )
            })
            .collect::<Vec<_>>();
        if shallow_aggregated
            .iter()
            .all(|candidate| candidate.samples == 0)
        {
            return Err("GPU native decision deadline expired before any rollout completed".into());
        }
        let mut shallow_final_root_order = active
            .iter()
            .copied()
            .filter(|index| {
                let candidate = &shallow_aggregated[*index];
                candidate.errors == 0 && candidate.samples > 0
            })
            .collect::<Vec<_>>();
        let pairwise_unresolved =
            shallow_final_root_order
                .iter()
                .enumerate()
                .all(|(position, left)| {
                    shallow_final_root_order[position + 1..]
                        .iter()
                        .all(|right| {
                            racing_contenders(&[*left, *right], &shallow_aggregated).len() == 2
                        })
                });
        // Priors only stabilize a wholly ordinary survivor set after every final pair has been
        // checked by the racer's own confidence rule. Mandatory blockers, promotion-only
        // admissions, exact families, and EndTurn keep rollout-first final arbitration.
        let prefer_prior_for_survivors = pairwise_unresolved
            && shallow_final_root_order.iter().all(|index| {
                let candidate = &shallow_aggregated[*index];
                candidate.action != Action::EndTurn
                    && exact_family_for_action(&candidate.action).is_none()
                    && !admitted_by_promotion.contains(&candidate.action)
                    && !verified_blockers
                        .iter()
                        .any(|(action, _)| action == &candidate.action)
            });
        let shallow_chosen_index = shallow_final_root_order
            .iter()
            .copied()
            .max_by(|left, right| {
                compare_surviving_roots(
                    &shallow_aggregated[*left],
                    &shallow_aggregated[*right],
                    prefer_prior_for_survivors,
                )
            })
            .ok_or_else(|| {
                "GPU native search had no error-free surviving root candidate".to_string()
            })?;
        shallow_final_root_order.sort_by(|left, right| {
            compare_surviving_roots(
                &shallow_aggregated[*left],
                &shallow_aggregated[*right],
                prefer_prior_for_survivors,
            )
            .reverse()
        });

        let mut aggregated = shallow_aggregated.clone();
        let mut final_root_order = shallow_final_root_order.clone();
        let mut chosen_root = shallow_aggregated[shallow_chosen_index].clone();
        let mut final_evaluation_horizons = vec![rollout_steps as u16; retained.len()];
        let mut horizon_escalation = None;
        let mut total_rollout_nodes = moments
            .iter()
            .map(|moment| moment.samples as usize * rollout_steps)
            .sum::<usize>();
        let mut total_executed_rollouts = moments.iter().map(|moment| moment.samples).sum::<u32>();

        if let Some(fragility) = fragility_assessments
            .iter()
            .find(|assessment| assessment.action == chosen_root.action)
            && let Some(unresolved_cut_mass) = horizon_escalation_trigger(&chosen_root, fragility)
        {
            let escalation_indices = horizon_escalation_contenders(
                shallow_chosen_index,
                &shallow_final_root_order,
                &retained,
                fragility,
            );
            let escalation_samples_per_root =
                (rollout_budget / escalation_indices.len().max(1) / 4).clamp(
                    HORIZON_ESCALATION_MIN_SAMPLES_PER_ROOT,
                    HORIZON_ESCALATION_MAX_SAMPLES_PER_ROOT,
                );
            let mut attempted_horizons = Vec::new();
            let mut completed_horizon = None;
            let mut deepest_stage = None::<Vec<(usize, AggregatedRoot)>>;
            let mut deadline_limited = false;

            for (stage, horizon) in horizon_escalation_schedule(rollout_steps)
                .into_iter()
                .enumerate()
            {
                if should_cancel() {
                    return Err("GPU native search cancelled".into());
                }
                if decision_clock.remaining_ms() < HORIZON_ESCALATION_MIN_REMAINING_MS {
                    deadline_limited = true;
                    break;
                }
                attempted_horizons.push(horizon as u16);
                let mut stage_moments = vec![RootSampleMoments::default(); root_actions.len()];
                let mut root_rows = vec![Vec::<Action>::new(); particles.len()];
                for root_index in escalation_indices.iter().copied() {
                    let eligible = legal_by_particle
                        .iter()
                        .enumerate()
                        .filter(|(_, legal)| {
                            legal
                                .iter()
                                .any(|action| action == &root_actions[root_index])
                        })
                        .map(|(particle_index, _)| (particle_index, weights[particle_index]))
                        .collect::<Vec<_>>();
                    let sampling_seed = base_seed
                        ^ 0x243f_6a88_85a3_08d3
                        ^ (root_index as u64 + 1).wrapping_mul(0x1319_8a2e_0370_7344)
                        ^ (horizon as u64).wrapping_mul(0xa409_3822_299f_31d0)
                        ^ stage as u64;
                    for (particle_index, count) in allocate_systematic_counts(
                        escalation_samples_per_root,
                        &eligible,
                        sampling_seed,
                    ) {
                        root_rows[particle_index]
                            .extend(std::iter::repeat_n(root_actions[root_index].clone(), count));
                    }
                }
                let stage_rollouts = root_rows.iter().map(Vec::len).sum::<usize>();
                if stage_rollouts == 0 {
                    break;
                }
                let stage_seed = base_seed
                    ^ 0x082e_fa98_ec4e_6c89
                    ^ (horizon as u64).wrapping_mul(0x4528_21e6_38d0_1377)
                    ^ stage as u64;
                let searched = match self.cuda.search_root_actions_controlled(
                    &root_rows,
                    1,
                    horizon,
                    stage_seed,
                    || should_cancel() || decision_clock.remaining_ms() == 0,
                ) {
                    Ok(searched) => searched,
                    Err(CudaSimError::Cancelled) if should_cancel() => {
                        return Err("GPU native search cancelled".into());
                    }
                    Err(CudaSimError::Cancelled) if decision_clock.remaining_ms() == 0 => {
                        deadline_limited = true;
                        break;
                    }
                    Err(error) => return Err(error.to_string()),
                };
                for row in &searched.rows {
                    for stat in row {
                        if let Some(root_index) = root_actions
                            .iter()
                            .position(|candidate| candidate == &stat.action)
                        {
                            stage_moments[root_index].add(stat);
                        }
                    }
                }
                let stage_aggregated = escalation_indices
                    .iter()
                    .copied()
                    .filter_map(|root_index| {
                        let aggregate = aggregate_root(
                            &retained[root_index],
                            &stage_moments[root_index],
                            &particles,
                            &legal_by_particle,
                            &weights,
                            actor,
                        );
                        (aggregate.samples > 0 && aggregate.errors == 0)
                            .then_some((root_index, aggregate))
                    })
                    .collect::<Vec<_>>();
                if stage_aggregated.len() != escalation_indices.len() {
                    deadline_limited = decision_clock.remaining_ms() == 0;
                    break;
                }
                total_executed_rollouts = total_executed_rollouts.saturating_add(
                    stage_aggregated
                        .iter()
                        .map(|(_, aggregate)| aggregate.samples)
                        .sum::<u32>(),
                );
                total_rollout_nodes = total_rollout_nodes.saturating_add(
                    stage_aggregated
                        .iter()
                        .map(|(_, aggregate)| aggregate.samples as usize * horizon)
                        .sum::<usize>(),
                );
                completed_horizon = Some(horizon as u16);
                deepest_stage = Some(stage_aggregated);
            }

            let mut final_winner = None;
            if let Some(stage_aggregated) = deepest_stage {
                for (index, aggregate) in stage_aggregated {
                    aggregated[index] = aggregate;
                    final_evaluation_horizons[index] =
                        completed_horizon.unwrap_or(rollout_steps as u16);
                }
                final_root_order = escalation_indices
                    .iter()
                    .copied()
                    .filter(|index| {
                        let candidate = &aggregated[*index];
                        candidate.errors == 0 && candidate.samples > 0
                    })
                    .collect::<Vec<_>>();
                final_root_order = escalated_root_order(&final_root_order, &aggregated);
                if let Some(winner_index) = final_root_order.first().copied() {
                    chosen_root = aggregated[winner_index].clone();
                    final_winner = Some(action(chosen_root.action.clone()));
                }
            }

            horizon_escalation = Some(HorizonEscalationOutput {
                reason: "fragile-award-low-terminal-completion",
                provisional_winner: action(shallow_aggregated[shallow_chosen_index].action.clone()),
                initial_horizon: rollout_steps as u16,
                unresolved_cut_mass,
                roots: escalation_indices
                    .iter()
                    .map(|index| action(retained[*index].action.clone()))
                    .collect(),
                attempted_horizons,
                completed_horizon,
                final_winner,
                deadline_limited,
            });
        }

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
        let root_lower_values = |candidate: &AggregatedRoot| {
            let mut value = root_values(candidate);
            value[actor as usize] = (candidate.candidate_vp
                - confidence_width(candidate.candidate_vp_variance, candidate.samples))
            .max(0.0);
            let opponent_lower = (candidate.opponent_vp
                - confidence_width(candidate.opponent_vp_variance, candidate.samples))
            .max(0.0);
            for (player, slot) in value.iter_mut().enumerate().take(player_count) {
                if player != actor as usize {
                    *slot = opponent_lower;
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
                lower_confidence_value: root_lower_values(candidate),
            })
            .collect::<Vec<_>>();

        let mut exact = ExactDecisionResult::default();
        let mut authority = DecisionAuthority::GpuRootRollout;
        let initial_authority = authority;
        let mut chosen = Some(chosen_root.action.clone());
        let mut exact_family = None;
        let mut exact_family_replacement = None;
        let mut safety_replacement = None;

        if let Some(family) = chosen.as_ref().and_then(exact_family_for_action) {
            exact_family = Some(exact_family_label(family));
            if let Some((_, cached)) = exact_family_results
                .iter()
                .find(|(cached_family, _)| *cached_family == family)
            {
                exact = cached.clone();
            }
            let before = chosen.clone();
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
        if let Some(current) = chosen.clone() {
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
        let total_rollouts = total_executed_rollouts;
        let mut root_provenance = RootProvenanceOutput {
            ranked_root_count,
            ranked_roots: ranked
                .iter()
                .enumerate()
                .map(|(rank, candidate)| RankedRootOutput {
                    action: action(candidate.action.clone()),
                    rank: rank + 1,
                    prior: candidate.prior,
                    planner_value: None,
                    planner_completion_mass: None,
                    planner_decisive_completion_mass: None,
                    planner_response_windows: None,
                })
                .collect(),
            retained_roots: retained
                .iter()
                .enumerate()
                .map(|(index, candidate)| {
                    let aggregate = &aggregated[index];
                    let terminal_width =
                        confidence_width(aggregate.terminal_variance, aggregate.samples);
                    let margin_width =
                        confidence_width(aggregate.victory_margin_variance, aggregate.samples);
                    RetainedRootOutput {
                        action: action(candidate.action.clone()),
                        pre_truncation_rank: ranked
                            .iter()
                            .position(|ranked_candidate| {
                                ranked_candidate.action == candidate.action
                            })
                            .map(|rank| rank + 1),
                        prior: candidate.prior,
                        node_budget_per_particle: 0,
                        allocated_nodes: 0,
                        planner_value: None,
                        planner_completion_mass: None,
                        planner_decisive_completion_mass: None,
                        planner_response_windows: None,
                        final_rank: final_root_order
                            .iter()
                            .position(|candidate_index| *candidate_index == index)
                            .map(|rank| rank + 1),
                        final_evaluation_horizon: Some(final_evaluation_horizons[index]),
                        initial_terminal_outcome: (final_evaluation_horizons[index]
                            != rollout_steps as u16)
                            .then_some(shallow_aggregated[index].terminal_outcome),
                        initial_terminal_rate: (final_evaluation_horizons[index]
                            != rollout_steps as u16)
                            .then_some(shallow_aggregated[index].terminal_rate),
                        initial_victory_margin: (final_evaluation_horizons[index]
                            != rollout_steps as u16)
                            .then_some(shallow_aggregated[index].victory_margin),
                        terminal_outcome: Some(aggregate.terminal_outcome),
                        terminal_rate: Some(aggregate.terminal_rate),
                        terminal_lower_bound: Some(aggregate.terminal_outcome - terminal_width),
                        terminal_upper_bound: Some(aggregate.terminal_outcome + terminal_width),
                        victory_margin: Some(aggregate.victory_margin),
                        victory_margin_lower_bound: Some(aggregate.victory_margin - margin_width),
                        victory_margin_upper_bound: Some(aggregate.victory_margin + margin_width),
                        mean_turn: Some(aggregate.mean_turn),
                    }
                })
                .collect(),
            pruned_root_count: pruned_roots.len(),
            pruned_roots,
            root_evidence,
            horizon_escalation,
            trade_hard_veto_threshold: HARD_VETO_POSTERIOR,
            search_winner: Some(action(chosen_root.action.clone())),
            exact_family_replacement: None,
            safety_replacement: None,
        };
        root_provenance.exact_family_replacement = exact_family_replacement.clone();
        root_provenance.safety_replacement = safety_replacement.clone();
        let diagnostics = ResponseDiagnostics {
            rust_posterior_particles: particles.len(),
            rust_search_particles: particles.len(),
            effective_effort: effort,
            search_stages: None,
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
                nodes: total_rollout_nodes,
                deepest_decision_depth: 0,
                rollouts: total_rollouts,
                effective_particle_count: effective_particle_count(&particles),
                deadline_reached: decision_clock.remaining_ms() == 0,
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

#[cfg(test)]
#[path = "native_gpu_d95_tests.rs"]
mod d95_tests;
