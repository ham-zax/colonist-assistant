use colonist_catan_core::{Action, GameState, Phase};

use crate::mcts::BeliefParticle;
use crate::reachability::{ReachabilityDiagnostic, optimistic_reachability};

pub const STRATEGY_SHADOW_POLICY_VERSION: &str = "adaptive-strategy-shadow-v1";

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
    pub retained: bool,
    pub failure_class: Option<DecisionFailureClass>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StrategyShadowDiagnostics {
    pub policy_version: &'static str,
    pub context: StrategyContext,
    pub reachability: ReachabilityDiagnostic,
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

    Some(StrategyShadowDiagnostics {
        policy_version: STRATEGY_SHADOW_POLICY_VERSION,
        context,
        reachability,
        proposals,
    })
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
}
