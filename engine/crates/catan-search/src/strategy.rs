use colonist_catan_core::{Action, GameState, Phase};

use crate::mcts::BeliefParticle;
use crate::reachability::{ReachabilityDiagnostic, optimistic_reachability};

pub const STRATEGY_SHADOW_POLICY_VERSION: &str = "adaptive-strategy-shadow-v1";
pub const ADAPTIVE_CANDIDATE_ADMISSION_V1: &str = "adaptive-candidate-admission-v1";
pub const MAX_STRATEGY_CHALLENGERS: usize = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StrategyPolicy {
    #[default]
    Baseline,
    AdaptiveCandidateAdmissionV1,
}

impl StrategyPolicy {
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value {
            None => Ok(Self::Baseline),
            Some(ADAPTIVE_CANDIDATE_ADMISSION_V1) => Ok(Self::AdaptiveCandidateAdmissionV1),
            Some(other) => Err(format!("unknown strategy policy: {other}")),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::AdaptiveCandidateAdmissionV1 => ADAPTIVE_CANDIDATE_ADMISSION_V1,
        }
    }

    pub fn explicit_identity(self) -> Option<&'static str> {
        match self {
            Self::Baseline => None,
            Self::AdaptiveCandidateAdmissionV1 => Some(ADAPTIVE_CANDIDATE_ADMISSION_V1),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StrategyEvidenceTier {
    NecessarySource,
    ContestedOpportunity,
    CurrentTurn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrategyOmissionReason {
    PolicyDisabled,
    AlreadyBaselineRetained,
    ChallengerLimit,
    ProtectedCapacity,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StrategyAdmissionDiagnostic {
    pub root_cap: Option<usize>,
    pub baseline_retained_count: usize,
    pub proposed_count: usize,
    pub distinct_proposed_count: usize,
    pub already_baseline_retained_count: usize,
    pub challenger_candidates_considered: usize,
    pub challengers_selected: usize,
    pub challengers_admitted: usize,
    pub protected_root_count: usize,
    pub omitted_challenger_count: usize,
    pub evaluated_challenger_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrategyId {
    ProductionGrowth,
    ExpansionRace,
    DevelopmentAccess,
    AwardRace,
    CloseoutRecovery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrategyProposalReason {
    CityProduction,
    SettlementProduction,
    SpatialRace,
    FutureDevelopmentVpRequired,
    DevelopmentOptionValue,
    LongestRoadRace,
    LargestArmyRace,
    ImmediateWin,
}

/// Diagnostic taxonomy for why a shadow proposal did not become the baseline
/// search winner. A proposal is evidence to inspect, not an oracle asserting
/// the engine was wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionFailureClass {
    Coverage,
    Valuation,
    Horizon,
    Continuation,
    BeliefModel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrategyContext {
    pub actor: u8,
    pub player_count: u8,
    pub opponent_count: u8,
    pub victory_target: u8,
    pub turn: u16,
    pub response_windows_before_next_turn: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StrategyProposalDiagnostic {
    pub strategy: StrategyId,
    pub action: Action,
    pub reason: StrategyProposalReason,
    pub baseline_rank: Option<usize>,
    /// Whether baseline admission retained this action before Milestone-2 admission.
    pub retained: bool,
    pub evidence_tier: StrategyEvidenceTier,
    pub selected_as_challenger: bool,
    pub admitted: bool,
    pub omission_reason: Option<StrategyOmissionReason>,
    pub displaced_baseline_action: Option<Action>,
    pub entered_common_search: bool,
    pub common_search_rank: Option<usize>,
    pub failure_class: Option<DecisionFailureClass>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StrategyShadowDiagnostics {
    pub policy_version: &'static str,
    /// Present only when an explicit action-changing strategy policy is active.
    pub strategy_policy: Option<&'static str>,
    pub context: StrategyContext,
    pub reachability: ReachabilityDiagnostic,
    pub admission: StrategyAdmissionDiagnostic,
    pub proposals: Vec<StrategyProposalDiagnostic>,
}

fn proposal_failure_class(
    action: &Action,
    ranked_actions: &[Action],
    retained_actions: &[Action],
    search_winner: Option<&Action>,
    requested_depth: u8,
    completed_depth: u8,
    deadline_reached: bool,
) -> Option<DecisionFailureClass> {
    if search_winner == Some(action) {
        return None;
    }
    if !ranked_actions.iter().any(|candidate| candidate == action)
        || !retained_actions.iter().any(|candidate| candidate == action)
    {
        return Some(DecisionFailureClass::Coverage);
    }
    if deadline_reached || completed_depth < requested_depth {
        return Some(DecisionFailureClass::Horizon);
    }
    Some(DecisionFailureClass::Valuation)
}

struct ShadowInputs<'a> {
    ranked_actions: &'a [Action],
    retained_actions: &'a [Action],
    search_winner: Option<&'a Action>,
    requested_depth: u8,
    completed_depth: u8,
    deadline_reached: bool,
}

fn evidence_tier(reason: StrategyProposalReason) -> StrategyEvidenceTier {
    match reason {
        StrategyProposalReason::FutureDevelopmentVpRequired => {
            StrategyEvidenceTier::NecessarySource
        }
        StrategyProposalReason::SpatialRace
        | StrategyProposalReason::LongestRoadRace
        | StrategyProposalReason::LargestArmyRace
        | StrategyProposalReason::ImmediateWin => StrategyEvidenceTier::ContestedOpportunity,
        StrategyProposalReason::CityProduction
        | StrategyProposalReason::SettlementProduction
        | StrategyProposalReason::DevelopmentOptionValue => StrategyEvidenceTier::CurrentTurn,
    }
}

fn push_proposal(
    proposals: &mut Vec<StrategyProposalDiagnostic>,
    strategy: StrategyId,
    action: &Action,
    reason: StrategyProposalReason,
    inputs: &ShadowInputs<'_>,
) {
    if proposals
        .iter()
        .filter(|proposal| proposal.strategy == strategy)
        .count()
        >= 2
        || proposals
            .iter()
            .any(|proposal| proposal.strategy == strategy && proposal.action == *action)
    {
        return;
    }
    proposals.push(StrategyProposalDiagnostic {
        strategy,
        action: action.clone(),
        reason,
        baseline_rank: inputs
            .ranked_actions
            .iter()
            .position(|candidate| candidate == action)
            .map(|rank| rank + 1),
        retained: inputs
            .retained_actions
            .iter()
            .any(|candidate| candidate == action),
        evidence_tier: evidence_tier(reason),
        selected_as_challenger: false,
        admitted: false,
        omission_reason: None,
        displaced_baseline_action: None,
        entered_common_search: false,
        common_search_rank: None,
        failure_class: proposal_failure_class(
            action,
            inputs.ranked_actions,
            inputs.retained_actions,
            inputs.search_winner,
            inputs.requested_depth,
            inputs.completed_depth,
            inputs.deadline_reached,
        ),
    });
}

fn add_production_growth(
    proposals: &mut Vec<StrategyProposalDiagnostic>,
    inputs: &ShadowInputs<'_>,
) {
    if let Some(action) = inputs
        .ranked_actions
        .iter()
        .find(|action| matches!(action, Action::BuildCity { .. }))
    {
        push_proposal(
            proposals,
            StrategyId::ProductionGrowth,
            action,
            StrategyProposalReason::CityProduction,
            inputs,
        );
    }
    if let Some(action) = inputs
        .ranked_actions
        .iter()
        .find(|action| matches!(action, Action::BuildSettlement { .. }))
    {
        push_proposal(
            proposals,
            StrategyId::ProductionGrowth,
            action,
            StrategyProposalReason::SettlementProduction,
            inputs,
        );
    }
}

fn add_expansion_race(
    proposals: &mut Vec<StrategyProposalDiagnostic>,
    inputs: &ShadowInputs<'_>,
    promoted_actions: &[Action],
) {
    for action in inputs.ranked_actions {
        if !promoted_actions.iter().any(|candidate| candidate == action)
            || !matches!(
                action,
                Action::BuildRoad { .. } | Action::BuildSettlement { .. }
            )
        {
            continue;
        }
        push_proposal(
            proposals,
            StrategyId::ExpansionRace,
            action,
            StrategyProposalReason::SpatialRace,
            inputs,
        );
        if proposals
            .iter()
            .filter(|proposal| proposal.strategy == StrategyId::ExpansionRace)
            .count()
            >= 2
        {
            break;
        }
    }
}

fn add_development_access(
    proposals: &mut Vec<StrategyProposalDiagnostic>,
    inputs: &ShadowInputs<'_>,
    reachability: &ReachabilityDiagnostic,
) {
    let Some(action) = inputs
        .ranked_actions
        .iter()
        .find(|action| matches!(action, Action::BuyDevelopment))
    else {
        return;
    };
    let reason = if reachability.future_development_vp_necessary {
        StrategyProposalReason::FutureDevelopmentVpRequired
    } else {
        StrategyProposalReason::DevelopmentOptionValue
    };
    push_proposal(
        proposals,
        StrategyId::DevelopmentAccess,
        action,
        reason,
        inputs,
    );
}

fn add_award_race(
    proposals: &mut Vec<StrategyProposalDiagnostic>,
    inputs: &ShadowInputs<'_>,
    state: &GameState,
    actor: u8,
) {
    let player = &state.players[actor as usize];
    if !player.has_longest_road
        && let Some(action) = inputs.ranked_actions.iter().find(|action| {
            matches!(
                action,
                Action::BuildRoad { .. } | Action::PlayRoadBuilding { .. }
            )
        })
    {
        push_proposal(
            proposals,
            StrategyId::AwardRace,
            action,
            StrategyProposalReason::LongestRoadRace,
            inputs,
        );
    }
    if !player.has_largest_army
        && let Some(action) = inputs
            .ranked_actions
            .iter()
            .find(|action| matches!(action, Action::PlayKnight { .. }))
    {
        push_proposal(
            proposals,
            StrategyId::AwardRace,
            action,
            StrategyProposalReason::LargestArmyRace,
            inputs,
        );
    }
}

fn add_closeout_recovery(
    proposals: &mut Vec<StrategyProposalDiagnostic>,
    inputs: &ShadowInputs<'_>,
    state: &GameState,
    actor: u8,
) {
    for action in inputs.ranked_actions {
        let mut next = state.clone();
        if next.apply(action).is_err() || next.winner() != Some(actor) {
            continue;
        }
        push_proposal(
            proposals,
            StrategyId::CloseoutRecovery,
            action,
            StrategyProposalReason::ImmediateWin,
            inputs,
        );
        if proposals
            .iter()
            .filter(|proposal| proposal.strategy == StrategyId::CloseoutRecovery)
            .count()
            >= 2
        {
            break;
        }
    }
}

/// Generates Milestone-1 strategy evidence without changing root admission,
/// search values, or final action authority.
pub fn shadow_strategy_diagnostics(
    particles: &[BeliefParticle],
    actor: u8,
    ranked_actions: &[Action],
    retained_actions: &[Action],
    promoted_actions: &[Action],
    search_winner: Option<&Action>,
    requested_depth: u8,
    completed_depth: u8,
    deadline_reached: bool,
) -> Option<StrategyShadowDiagnostics> {
    let first = particles.first()?;
    if actor >= first.state.board.num_players || actor as usize >= first.state.players.len() {
        return None;
    }
    let reachability = optimistic_reachability(particles, actor)?;
    let context = StrategyContext {
        actor,
        player_count: first.state.board.num_players,
        opponent_count: first.state.board.num_players.saturating_sub(1),
        victory_target: first.state.victory_target,
        turn: first.state.turn,
        response_windows_before_next_turn: first.state.board.num_players.saturating_sub(1),
    };

    let mut proposals = Vec::new();
    if matches!(first.state.phase, Phase::PreRoll | Phase::Main) {
        let inputs = ShadowInputs {
            ranked_actions,
            retained_actions,
            search_winner,
            requested_depth,
            completed_depth,
            deadline_reached,
        };
        add_production_growth(&mut proposals, &inputs);
        add_expansion_race(&mut proposals, &inputs, promoted_actions);
        add_development_access(&mut proposals, &inputs, &reachability);
        add_award_race(&mut proposals, &inputs, &first.state, actor);
        add_closeout_recovery(&mut proposals, &inputs, &first.state, actor);
    }

    let mut distinct_actions = Vec::<Action>::new();
    let mut baseline_retained_actions = Vec::<Action>::new();
    for proposal in &proposals {
        if !distinct_actions.contains(&proposal.action) {
            distinct_actions.push(proposal.action.clone());
        }
        if proposal.retained && !baseline_retained_actions.contains(&proposal.action) {
            baseline_retained_actions.push(proposal.action.clone());
        }
    }
    for proposal in &mut proposals {
        proposal.entered_common_search = proposal.retained;
        proposal.common_search_rank = proposal
            .retained
            .then(|| {
                retained_actions
                    .iter()
                    .position(|candidate| candidate == &proposal.action)
                    .map(|rank| rank + 1)
            })
            .flatten();
        if proposal.retained {
            proposal.omission_reason = Some(StrategyOmissionReason::AlreadyBaselineRetained);
        } else {
            proposal.omission_reason = Some(StrategyOmissionReason::PolicyDisabled);
        }
    }

    Some(StrategyShadowDiagnostics {
        policy_version: STRATEGY_SHADOW_POLICY_VERSION,
        strategy_policy: None,
        context,
        reachability,
        admission: StrategyAdmissionDiagnostic {
            root_cap: None,
            baseline_retained_count: retained_actions.len(),
            proposed_count: proposals.len(),
            distinct_proposed_count: distinct_actions.len(),
            already_baseline_retained_count: baseline_retained_actions.len(),
            ..StrategyAdmissionDiagnostic::default()
        },
        proposals,
    })
}

/// Builds the same Milestone-1 strategy evidence used by shadow diagnostics,
/// but freezes it before action-changing admission. The baseline roster and
/// ranked action domain are therefore the evidence authority for the decision.
pub(crate) fn strategy_diagnostics_for_admission(
    particles: &[BeliefParticle],
    actor: u8,
    ranked_actions: &[Action],
    baseline_retained_actions: &[Action],
    promoted_actions: &[Action],
    root_cap: usize,
) -> Option<StrategyShadowDiagnostics> {
    let mut diagnostics = shadow_strategy_diagnostics(
        particles,
        actor,
        ranked_actions,
        baseline_retained_actions,
        promoted_actions,
        None,
        0,
        0,
        false,
    )?;
    diagnostics.strategy_policy = Some(ADAPTIVE_CANDIDATE_ADMISSION_V1);
    diagnostics.admission.root_cap = Some(root_cap.max(1));
    diagnostics.admission.baseline_retained_count = baseline_retained_actions.len();
    diagnostics.admission.challenger_candidates_considered = 0;
    diagnostics.admission.challengers_selected = 0;
    diagnostics.admission.challengers_admitted = 0;
    diagnostics.admission.protected_root_count = 0;
    diagnostics.admission.omitted_challenger_count = 0;
    diagnostics.admission.evaluated_challenger_count = 0;
    for proposal in &mut diagnostics.proposals {
        proposal.failure_class = None;
        proposal.entered_common_search = false;
        proposal.common_search_rank = None;
        proposal.selected_as_challenger = false;
        proposal.admitted = false;
        proposal.displaced_baseline_action = None;
        proposal.omission_reason = proposal
            .retained
            .then_some(StrategyOmissionReason::AlreadyBaselineRetained);
    }
    Some(diagnostics)
}

#[derive(Clone, Debug)]
struct StrategyChallenger {
    action: Action,
    evidence_tier: StrategyEvidenceTier,
    baseline_rank: usize,
}

/// Replaces only unprotected baseline roots with at most three distinct
/// evidence-ranked challengers. The configured cap is never enlarged, and the
/// returned roster is frozen for the remainder of this decision.
pub(crate) fn admit_strategy_challengers(
    diagnostics: &mut StrategyShadowDiagnostics,
    ranked: &[(Action, f32)],
    baseline_retained: &[(Action, f32)],
    protected_actions: &[Action],
    cap: usize,
) -> Vec<(Action, f32)> {
    let cap = cap.max(1);
    let mut roster = baseline_retained.to_vec();
    debug_assert!(roster.len() <= cap);
    let baseline_actions = baseline_retained
        .iter()
        .map(|(action, _)| action.clone())
        .collect::<Vec<_>>();

    let mut protected = Vec::<Action>::new();
    for action in protected_actions {
        if baseline_actions.contains(action) && !protected.contains(action) {
            protected.push(action.clone());
        }
    }
    diagnostics.admission.root_cap = Some(cap);
    diagnostics.admission.baseline_retained_count = baseline_retained.len();
    diagnostics.admission.protected_root_count = protected.len();

    let mut challengers = Vec::<StrategyChallenger>::new();
    for proposal in &diagnostics.proposals {
        if proposal.retained {
            continue;
        }
        if let Some(existing) = challengers
            .iter_mut()
            .find(|candidate| candidate.action == proposal.action)
        {
            existing.evidence_tier = existing.evidence_tier.min(proposal.evidence_tier);
            existing.baseline_rank = existing
                .baseline_rank
                .min(proposal.baseline_rank.unwrap_or(usize::MAX));
        } else {
            challengers.push(StrategyChallenger {
                action: proposal.action.clone(),
                evidence_tier: proposal.evidence_tier,
                baseline_rank: proposal.baseline_rank.unwrap_or(usize::MAX),
            });
        }
    }
    challengers.sort_by(|left, right| {
        left.evidence_tier
            .cmp(&right.evidence_tier)
            .then_with(|| left.baseline_rank.cmp(&right.baseline_rank))
            .then_with(|| format!("{:?}", left.action).cmp(&format!("{:?}", right.action)))
    });
    diagnostics.admission.challenger_candidates_considered = challengers.len();

    let selected = challengers
        .iter()
        .take(MAX_STRATEGY_CHALLENGERS)
        .cloned()
        .collect::<Vec<_>>();
    diagnostics.admission.challengers_selected = selected.len();
    for proposal in diagnostics
        .proposals
        .iter_mut()
        .filter(|proposal| !proposal.retained)
    {
        if selected
            .iter()
            .any(|candidate| candidate.action == proposal.action)
        {
            proposal.selected_as_challenger = true;
        } else {
            proposal.omission_reason = Some(StrategyOmissionReason::ChallengerLimit);
        }
    }

    let baseline_rank = |action: &Action| {
        ranked
            .iter()
            .position(|(candidate, _)| candidate == action)
            .map_or(usize::MAX, |rank| rank + 1)
    };
    let mut admitted = 0usize;
    for challenger in selected {
        let Some((_, challenger_prior)) = ranked
            .iter()
            .find(|(candidate, _)| candidate == &challenger.action)
        else {
            for proposal in diagnostics
                .proposals
                .iter_mut()
                .filter(|proposal| proposal.action == challenger.action)
            {
                proposal.omission_reason = Some(StrategyOmissionReason::ProtectedCapacity);
            }
            continue;
        };

        let displaced = if roster.len() < cap {
            roster.push((challenger.action.clone(), *challenger_prior));
            None
        } else {
            let victim = roster
                .iter()
                .enumerate()
                .filter(|(_, (action, _))| {
                    baseline_actions.contains(action) && !protected.contains(action)
                })
                .max_by(|(_, (left, _)), (_, (right, _))| {
                    baseline_rank(left)
                        .cmp(&baseline_rank(right))
                        .then_with(|| format!("{left:?}").cmp(&format!("{right:?}")))
                })
                .map(|(index, (action, _))| (index, action.clone()));
            let Some((index, victim_action)) = victim else {
                for proposal in diagnostics
                    .proposals
                    .iter_mut()
                    .filter(|proposal| proposal.action == challenger.action)
                {
                    proposal.omission_reason = Some(StrategyOmissionReason::ProtectedCapacity);
                }
                continue;
            };
            roster[index] = (challenger.action.clone(), *challenger_prior);
            Some(victim_action)
        };

        admitted += 1;
        for proposal in diagnostics
            .proposals
            .iter_mut()
            .filter(|proposal| proposal.action == challenger.action)
        {
            proposal.admitted = true;
            proposal.omission_reason = None;
            proposal.displaced_baseline_action = displaced.clone();
        }
    }

    diagnostics.admission.challengers_admitted = admitted;
    diagnostics.admission.omitted_challenger_count = diagnostics
        .admission
        .challenger_candidates_considered
        .saturating_sub(admitted);
    debug_assert!(admitted <= MAX_STRATEGY_CHALLENGERS);
    debug_assert!(roster.len() <= cap);
    roster
}

pub(crate) fn finalize_strategy_diagnostics(
    diagnostics: &mut StrategyShadowDiagnostics,
    searched_actions: &[Action],
    search_winner: Option<&Action>,
    requested_depth: u8,
    completed_depth: u8,
    deadline_reached: bool,
) {
    let mut evaluated_challengers = Vec::<Action>::new();
    for proposal in &mut diagnostics.proposals {
        proposal.entered_common_search = searched_actions.contains(&proposal.action);
        proposal.common_search_rank = searched_actions
            .iter()
            .position(|candidate| candidate == &proposal.action)
            .map(|rank| rank + 1);
        debug_assert!(
            !proposal.admitted || proposal.entered_common_search,
            "admitted strategy challenger must enter common search"
        );
        proposal.failure_class = if search_winner == Some(&proposal.action) {
            None
        } else if !proposal.entered_common_search {
            Some(DecisionFailureClass::Coverage)
        } else if deadline_reached || completed_depth < requested_depth {
            Some(DecisionFailureClass::Horizon)
        } else {
            Some(DecisionFailureClass::Valuation)
        };
        if proposal.admitted
            && proposal.entered_common_search
            && !evaluated_challengers.contains(&proposal.action)
        {
            evaluated_challengers.push(proposal.action.clone());
        }
    }
    diagnostics.admission.evaluated_challenger_count = evaluated_challengers.len();
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{Action, DevCard, GameState, Phase};

    use super::*;

    fn particle(mut state: GameState) -> BeliefParticle {
        state.phase = Phase::Main;
        state.current_player = 0;
        BeliefParticle { state, weight: 1.0 }
    }

    #[test]
    fn two_player_context_reports_one_opponent_response_window() {
        let state = GameState::standard(31, 2);
        let diagnostics = shadow_strategy_diagnostics(
            &[particle(state)],
            0,
            &[Action::EndTurn],
            &[Action::EndTurn],
            &[],
            Some(&Action::EndTurn),
            3,
            3,
            false,
        )
        .unwrap();
        assert_eq!(diagnostics.context.player_count, 2);
        assert_eq!(diagnostics.context.opponent_count, 1);
        assert_eq!(diagnostics.context.response_windows_before_next_turn, 1);
    }

    #[test]
    fn development_shadow_marks_structurally_required_vp_access() {
        let mut state = GameState::standard(37, 2);
        state.victory_target = 15;
        state.players[0].public_victory_points = 10;
        state.players[0].settlements_left = 0;
        state.players[0].cities_left = 0;
        state.players[0].has_longest_road = true;
        state.players[0].has_largest_army = true;
        state.development_deck[DevCard::VictoryPoint.index()] = 5;
        let diagnostics = shadow_strategy_diagnostics(
            &[particle(state)],
            0,
            &[Action::BuyDevelopment, Action::EndTurn],
            &[Action::EndTurn],
            &[],
            Some(&Action::EndTurn),
            3,
            3,
            false,
        )
        .unwrap();
        let proposal = diagnostics
            .proposals
            .iter()
            .find(|proposal| proposal.strategy == StrategyId::DevelopmentAccess)
            .unwrap();
        assert_eq!(
            proposal.reason,
            StrategyProposalReason::FutureDevelopmentVpRequired
        );
        assert_eq!(proposal.failure_class, Some(DecisionFailureClass::Coverage));
    }

    #[test]
    fn retained_losing_shadow_is_valuation_only_after_completed_horizon() {
        let state = GameState::standard(41, 2);
        let diagnostics = shadow_strategy_diagnostics(
            &[particle(state)],
            0,
            &[Action::BuyDevelopment, Action::EndTurn],
            &[Action::BuyDevelopment, Action::EndTurn],
            &[],
            Some(&Action::EndTurn),
            3,
            3,
            false,
        )
        .unwrap();
        let proposal = diagnostics
            .proposals
            .iter()
            .find(|proposal| proposal.strategy == StrategyId::DevelopmentAccess)
            .unwrap();
        assert_eq!(
            proposal.failure_class,
            Some(DecisionFailureClass::Valuation)
        );
    }

    #[test]
    fn incomplete_search_marks_retained_shadow_as_horizon_limited() {
        let state = GameState::standard(43, 2);
        let diagnostics = shadow_strategy_diagnostics(
            &[particle(state)],
            0,
            &[Action::BuyDevelopment, Action::EndTurn],
            &[Action::BuyDevelopment, Action::EndTurn],
            &[],
            Some(&Action::EndTurn),
            4,
            2,
            true,
        )
        .unwrap();
        let proposal = diagnostics
            .proposals
            .iter()
            .find(|proposal| proposal.strategy == StrategyId::DevelopmentAccess)
            .unwrap();
        assert_eq!(proposal.failure_class, Some(DecisionFailureClass::Horizon));
    }

    #[test]
    fn strategy_policy_missing_is_baseline_and_unknown_fails_closed() {
        assert_eq!(
            StrategyPolicy::parse(None).unwrap(),
            StrategyPolicy::Baseline
        );
        assert_eq!(
            StrategyPolicy::parse(Some(ADAPTIVE_CANDIDATE_ADMISSION_V1)).unwrap(),
            StrategyPolicy::AdaptiveCandidateAdmissionV1
        );
        assert!(StrategyPolicy::parse(Some("adaptive-candidate-admission-v2")).is_err());
    }

    #[test]
    fn admission_deduplicates_shared_support_and_preserves_cap_and_protected_roots() {
        let particles = [particle(GameState::standard(47, 2))];
        let road = Action::BuildRoad { edge: 0 };
        let city = Action::BuildCity { vertex: 0 };
        let settlement = Action::BuildSettlement { vertex: 1 };
        let ranked = vec![
            (Action::EndTurn, 0.30),
            (city.clone(), 0.25),
            (settlement.clone(), 0.20),
            (road.clone(), 0.15),
            (Action::BuyDevelopment, 0.10),
        ];
        let ranked_actions = ranked
            .iter()
            .map(|(action, _)| action.clone())
            .collect::<Vec<_>>();
        let baseline = vec![
            (Action::EndTurn, 0.30),
            (city.clone(), 0.25),
            (settlement.clone(), 0.20),
        ];
        let baseline_actions = baseline
            .iter()
            .map(|(action, _)| action.clone())
            .collect::<Vec<_>>();
        let mut diagnostics = strategy_diagnostics_for_admission(
            &particles,
            0,
            &ranked_actions,
            &baseline_actions,
            std::slice::from_ref(&road),
            3,
        )
        .unwrap();
        let roster = admit_strategy_challengers(
            &mut diagnostics,
            &ranked,
            &baseline,
            &[Action::EndTurn, city.clone()],
            3,
        );

        assert_eq!(roster.len(), 3);
        assert!(roster.iter().any(|(action, _)| action == &Action::EndTurn));
        assert!(roster.iter().any(|(action, _)| action == &city));
        assert_eq!(
            roster.iter().filter(|(action, _)| action == &road).count(),
            1
        );
        assert_eq!(diagnostics.admission.challengers_admitted, 1);
        let road_support = diagnostics
            .proposals
            .iter()
            .filter(|proposal| proposal.action == road)
            .collect::<Vec<_>>();
        assert!(road_support.len() >= 2);
        assert!(road_support.iter().all(|proposal| proposal.admitted));
        assert!(
            road_support
                .iter()
                .all(|proposal| proposal.displaced_baseline_action.as_ref() == Some(&settlement))
        );
        drop(road_support);

        let searched = roster
            .iter()
            .map(|(action, _)| action.clone())
            .collect::<Vec<_>>();
        finalize_strategy_diagnostics(&mut diagnostics, &searched, Some(&road), 3, 3, false);
        assert_eq!(diagnostics.admission.evaluated_challenger_count, 1);
        assert!(
            diagnostics
                .proposals
                .iter()
                .filter(|p| p.action == road)
                .all(|p| { p.entered_common_search && p.failure_class.is_none() })
        );
    }

    #[test]
    fn admission_never_selects_more_than_three_distinct_challengers() {
        let particles = [particle(GameState::standard(51, 2))];
        let road = Action::BuildRoad { edge: 0 };
        let city = Action::BuildCity { vertex: 0 };
        let settlement = Action::BuildSettlement { vertex: 1 };
        let knight = Action::PlayKnight {
            hex: 0,
            victim: Some(1),
        };
        let ranked = vec![
            (Action::EndTurn, 0.30),
            (city, 0.20),
            (settlement, 0.18),
            (road.clone(), 0.16),
            (Action::BuyDevelopment, 0.10),
            (knight, 0.06),
        ];
        let ranked_actions = ranked
            .iter()
            .map(|(action, _)| action.clone())
            .collect::<Vec<_>>();
        let baseline = vec![ranked[0].clone()];
        let baseline_actions = vec![Action::EndTurn];
        let mut diagnostics = strategy_diagnostics_for_admission(
            &particles,
            0,
            &ranked_actions,
            &baseline_actions,
            std::slice::from_ref(&road),
            4,
        )
        .unwrap();
        let roster = admit_strategy_challengers(
            &mut diagnostics,
            &ranked,
            &baseline,
            &baseline_actions,
            4,
        );

        assert!(diagnostics.admission.challenger_candidates_considered > 3);
        assert_eq!(diagnostics.admission.challengers_selected, 3);
        assert_eq!(diagnostics.admission.challengers_admitted, 3);
        assert_eq!(roster.len(), 4);
    }

    #[test]
    fn necessary_source_evidence_outranks_baseline_order() {
        let mut state = GameState::standard(53, 2);
        state.victory_target = 15;
        state.players[0].public_victory_points = 10;
        state.players[0].settlements_left = 0;
        state.players[0].cities_left = 0;
        state.players[0].has_longest_road = true;
        state.players[0].has_largest_army = true;
        state.development_deck[DevCard::VictoryPoint.index()] = 5;
        let particles = [particle(state)];
        let leader = Action::BuildRoad { edge: 0 };
        let city = Action::BuildCity { vertex: 0 };
        let settlement = Action::BuildSettlement { vertex: 1 };
        let ranked = vec![
            (leader.clone(), 0.30),
            (Action::EndTurn, 0.25),
            (city.clone(), 0.20),
            (Action::BuyDevelopment, 0.15),
            (settlement.clone(), 0.10),
        ];
        let ranked_actions = ranked
            .iter()
            .map(|(action, _)| action.clone())
            .collect::<Vec<_>>();
        let baseline = ranked[..3].to_vec();
        let baseline_actions = baseline
            .iter()
            .map(|(action, _)| action.clone())
            .collect::<Vec<_>>();
        let mut diagnostics = strategy_diagnostics_for_admission(
            &particles,
            0,
            &ranked_actions,
            &baseline_actions,
            &[],
            3,
        )
        .unwrap();
        let roster = admit_strategy_challengers(
            &mut diagnostics,
            &ranked,
            &baseline,
            &[leader.clone(), Action::EndTurn],
            3,
        );

        assert!(
            roster
                .iter()
                .any(|(action, _)| action == &Action::BuyDevelopment)
        );
        assert!(!roster.iter().any(|(action, _)| action == &city));
        let development = diagnostics
            .proposals
            .iter()
            .find(|proposal| proposal.action == Action::BuyDevelopment)
            .unwrap();
        assert_eq!(
            development.evidence_tier,
            StrategyEvidenceTier::NecessarySource
        );
        assert!(development.admitted);
        let lower_ranked_current_turn = diagnostics
            .proposals
            .iter()
            .find(|proposal| proposal.action == settlement)
            .unwrap();
        assert_eq!(
            lower_ranked_current_turn.evidence_tier,
            StrategyEvidenceTier::CurrentTurn
        );
        assert!(!lower_ranked_current_turn.admitted);
    }

    #[test]
    fn protected_capacity_omits_challengers_without_growing_roster() {
        let particles = [particle(GameState::standard(59, 2))];
        let road = Action::BuildRoad { edge: 0 };
        let ranked = vec![
            (Action::EndTurn, 0.4),
            (Action::BuildCity { vertex: 0 }, 0.3),
            (road.clone(), 0.2),
            (Action::BuyDevelopment, 0.1),
        ];
        let ranked_actions = ranked
            .iter()
            .map(|(action, _)| action.clone())
            .collect::<Vec<_>>();
        let baseline = ranked[..2].to_vec();
        let baseline_actions = baseline
            .iter()
            .map(|(action, _)| action.clone())
            .collect::<Vec<_>>();
        let mut diagnostics = strategy_diagnostics_for_admission(
            &particles,
            0,
            &ranked_actions,
            &baseline_actions,
            std::slice::from_ref(&road),
            2,
        )
        .unwrap();
        let roster =
            admit_strategy_challengers(&mut diagnostics, &ranked, &baseline, &baseline_actions, 2);

        assert_eq!(roster, baseline);
        assert_eq!(diagnostics.admission.challengers_admitted, 0);
        assert!(
            diagnostics
                .proposals
                .iter()
                .filter(|proposal| proposal.selected_as_challenger)
                .all(|proposal| proposal.omission_reason
                    == Some(StrategyOmissionReason::ProtectedCapacity))
        );
    }

    #[test]
    fn admission_evidence_is_invariant_to_hidden_world_order() {
        let mut left = GameState::standard(61, 2);
        left.phase = Phase::Main;
        left.current_player = 0;
        left.players[1].resources = [3, 0, 0, 0, 0];
        let mut right = left.clone();
        right.players[1].resources = [0, 3, 0, 0, 0];
        let first = BeliefParticle {
            state: left,
            weight: 0.5,
        };
        let second = BeliefParticle {
            state: right,
            weight: 0.5,
        };
        let ranked = [
            Action::EndTurn,
            Action::BuildRoad { edge: 0 },
            Action::BuyDevelopment,
        ];
        let baseline = [Action::EndTurn];
        let promoted = [Action::BuildRoad { edge: 0 }];
        let forward = strategy_diagnostics_for_admission(
            &[first.clone(), second.clone()],
            0,
            &ranked,
            &baseline,
            &promoted,
            2,
        )
        .unwrap();
        let reverse = strategy_diagnostics_for_admission(
            &[second, first],
            0,
            &ranked,
            &baseline,
            &promoted,
            2,
        )
        .unwrap();
        assert_eq!(forward, reverse);
    }
}
