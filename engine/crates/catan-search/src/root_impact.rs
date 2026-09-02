//! Root-level strategic consequence aggregation and candidate action promotion.
//!
//! Evaluates the topological and tactical consequences of candidate root actions
//! before width truncation. Promotes actions that prevent or inflict material
//! road cuts, protect Longest Road, or defend critical expansion corridors.
//!
//! Promotion guarantees search width coverage; it does NOT inject synthetic
//! utility bonuses into tree search or leaf evaluation.

use colonist_catan_core::{Action, Building, GameState};

use crate::planner::TurnPlan;
use crate::resilience::{RoadResilience, analyze_road_resilience};

/// Closeout may affect coverage only when planner endpoints are effectively a
/// strategic tie. One public VP is ~7.4 utility in `eval`, so this band is
/// deliberately much smaller than a material score/build difference.
const CLOSEOUT_COMPARABLE_VALUE_DELTA: f32 = 0.20;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoadImpactDelta {
    pub longest_road_loss_prevented: i8,
    pub longest_road_loss_inflicted: i8,
    pub award_vp_swing: i8,
    pub expansion_portfolio_delta: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootPromotionReason {
    RoadAwardProtection,
    CriticalExpansionProtection,
    OpponentRouteCut,
    CloseoutCompression,
}

#[derive(Clone, Debug)]
pub struct RootStrategicImpact {
    pub action: Action,
    pub road_delta: RoadImpactDelta,
    /// Normalized reduction in opponent response windows among materially
    /// comparable planner endpoints. Diagnostic/coverage evidence only.
    pub closeout_gain: f32,
    pub response_windows: Option<f32>,
    pub decisive_completion_mass: f32,
    pub promotion: Option<RootPromotionReason>,
}

#[derive(Clone, Debug)]
pub struct RootImpactReport {
    pub baseline_road: RoadResilience,
    pub actions: Vec<RootStrategicImpact>,
}

/// Computes the public-board road resilience impact for each candidate action.
///
/// Baseline road topology is computed once and shared across all candidate checks.
pub fn compute_spatial_root_impacts(
    state: &GameState,
    actor: u8,
    actions: &[Action],
) -> RootImpactReport {
    let baseline_road = analyze_road_resilience(state, actor);
    let mut opponent_resiliences = Vec::with_capacity(state.board.num_players as usize);
    for opp in 0..state.board.num_players {
        if opp != actor {
            opponent_resiliences.push((opp, analyze_road_resilience(state, opp)));
        }
    }

    let mut impacts = Vec::with_capacity(actions.len());

    for action in actions {
        let mut road_delta = RoadImpactDelta::default();
        let mut promotion = None;

        match action {
            Action::BuildSettlement { vertex } | Action::PlaceSettlement { vertex } => {
                let mut probe = state.clone();
                probe.buildings[*vertex as usize] = Some(Building::Settlement(actor));
                probe.update_longest_road();

                // 1. Check the actual actor-specific settlement consequence for each
                // opponent whose route or expansion portfolio touches this vertex.
                for &(opp, ref opp_res) in &opponent_resiliences {
                    if let Some(cut) = opp_res
                        .critical_vertices
                        .iter()
                        .find(|c| c.vertex == *vertex)
                    {
                        let road_loss = state
                            .longest_road_length(opp)
                            .saturating_sub(probe.longest_road_length(opp));
                        if road_loss > 0 {
                            road_delta.longest_road_loss_inflicted =
                                road_delta.longest_road_loss_inflicted.max(road_loss as i8);
                            promotion = Some(RootPromotionReason::OpponentRouteCut);
                        }
                        if state.longest_road_holder == Some(opp)
                            && probe.longest_road_holder != Some(opp)
                        {
                            if probe.longest_road_holder == Some(actor) {
                                road_delta.award_vp_swing = 2;
                            }
                            promotion = Some(RootPromotionReason::OpponentRouteCut);
                        }
                        if cut.expansion_loss > 0.35 {
                            road_delta.expansion_portfolio_delta =
                                road_delta.expansion_portfolio_delta.max(cut.expansion_loss);
                            if promotion.is_none() {
                                promotion = Some(RootPromotionReason::OpponentRouteCut);
                            }
                        }
                    }
                }

                // 2. Check if building this settlement protects actor's own road network from cuts
                if let Some(own_cut) = baseline_road
                    .critical_vertices
                    .iter()
                    .find(|c| c.vertex == *vertex)
                {
                    road_delta.longest_road_loss_prevented = own_cut.road_loss as i8;
                    if own_cut.award_loss {
                        road_delta.award_vp_swing = 2;
                        promotion = Some(RootPromotionReason::RoadAwardProtection);
                    } else if own_cut.road_loss > 0 && promotion.is_none() {
                        promotion = Some(RootPromotionReason::RoadAwardProtection);
                    }
                }
            }

            Action::BuildRoad { edge } | Action::PlaceRoad { edge } => {
                // 1. Check if building this road claims Longest Road
                let mut probe = state.clone();
                probe.roads[*edge as usize] = Some(actor);
                probe.update_longest_road();

                if state.longest_road_holder != Some(actor)
                    && probe.longest_road_holder == Some(actor)
                {
                    road_delta.award_vp_swing = 2;
                    promotion = Some(RootPromotionReason::RoadAwardProtection);
                }

                // 2. Check if building this road establishes a bypass for an existing cut
                if baseline_road.maximum_longest_road_loss > 0 {
                    let after_res = analyze_road_resilience(&probe, actor);
                    if after_res.maximum_longest_road_loss < baseline_road.maximum_longest_road_loss
                    {
                        road_delta.longest_road_loss_prevented = (baseline_road
                            .maximum_longest_road_loss
                            - after_res.maximum_longest_road_loss)
                            as i8;
                        if promotion.is_none() {
                            promotion = Some(RootPromotionReason::RoadAwardProtection);
                        }
                    }
                }

                // 3. Check if building this road protects actor's own critical edge
                if let Some(own_cut) = baseline_road
                    .critical_edges
                    .iter()
                    .find(|c| c.edge == *edge)
                {
                    if own_cut.award_loss {
                        road_delta.award_vp_swing = 2;
                        promotion = Some(RootPromotionReason::RoadAwardProtection);
                    }
                    if own_cut.expansion_loss > 0.35 {
                        road_delta.expansion_portfolio_delta = road_delta
                            .expansion_portfolio_delta
                            .max(own_cut.expansion_loss);
                        if promotion.is_none() {
                            promotion = Some(RootPromotionReason::CriticalExpansionProtection);
                        }
                    }
                }

                // 4. Claiming an opponent-critical empty edge can deny expansion,
                // but it cannot shorten the opponent's already-built road. Any
                // actual Longest Road transfer caused by this root was handled by
                // the actor-specific probe above.
                for &(_opp, ref opp_res) in &opponent_resiliences {
                    if let Some(cut_edge) = opp_res.critical_edges.iter().find(|c| c.edge == *edge)
                        && cut_edge.expansion_loss > 0.35
                    {
                        road_delta.expansion_portfolio_delta = road_delta
                            .expansion_portfolio_delta
                            .max(cut_edge.expansion_loss);
                        if promotion.is_none() {
                            promotion = Some(RootPromotionReason::OpponentRouteCut);
                        }
                    }
                }
            }

            _ => {}
        }

        impacts.push(RootStrategicImpact {
            action: action.clone(),
            road_delta,
            closeout_gain: 0.0,
            response_windows: None,
            decisive_completion_mass: 0.0,
            promotion,
        });
    }

    RootImpactReport {
        baseline_road,
        actions: impacts,
    }
}

/// Attach bounded closeout evidence to an already-computed root-impact report.
/// This function never changes planner/search utility. `CloseoutCompression`
/// can only preserve a materially comparable decisive root through width
/// truncation; ordinary MaxN/GPU comparison still decides whether it wins.
pub fn apply_closeout_root_impacts(report: &mut RootImpactReport, plans: &[TurnPlan]) {
    let covered = plans
        .iter()
        .filter(|plan| plan.completion_mass > f32::EPSILON && plan.response_windows.is_some())
        .collect::<Vec<_>>();
    let Some(best_value) = covered.iter().map(|plan| plan.value).reduce(f32::max) else {
        return;
    };
    let comparable = covered
        .iter()
        .copied()
        .filter(|plan| plan.value + CLOSEOUT_COMPARABLE_VALUE_DELTA >= best_value)
        .collect::<Vec<_>>();
    let maximum_windows = comparable
        .iter()
        .filter_map(|plan| plan.response_windows)
        .fold(0.0_f32, f32::max);

    for impact in &mut report.actions {
        let Some(plan) = plans.iter().find(|plan| plan.first_action == impact.action) else {
            continue;
        };
        impact.response_windows = plan.response_windows;
        impact.decisive_completion_mass = plan.decisive_completion_mass.clamp(0.0, 1.0);
        if plan.value + CLOSEOUT_COMPARABLE_VALUE_DELTA < best_value
            || impact.decisive_completion_mass <= f32::EPSILON
        {
            continue;
        }
        let Some(windows) = plan.response_windows else {
            continue;
        };
        if maximum_windows <= f32::EPSILON || windows >= maximum_windows {
            continue;
        }
        impact.closeout_gain = ((maximum_windows - windows) / maximum_windows).clamp(0.0, 1.0)
            * impact.decisive_completion_mass;
        if impact.closeout_gain > f32::EPSILON && impact.promotion.is_none() {
            impact.promotion = Some(RootPromotionReason::CloseoutCompression);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colonist_catan_core::GameState;

    #[test]
    fn opponent_cut_action_is_promoted() {
        let mut state = GameState::standard(1, 3);
        // Build 5 roads for opponent (player 1)
        let e0 = 0usize;
        let [_v0, v1] = state.board.edges[e0].vertices;
        state.roads[e0] = Some(1);

        let e1 = *state.board.vertices[v1 as usize]
            .adjacent_edges
            .iter()
            .find(|&&e| e != e0 as u8)
            .unwrap() as usize;
        state.roads[e1] = Some(1);
        let [_, v2] = state.board.edges[e1].vertices;
        let v2 = if v2 == v1 {
            state.board.edges[e1].vertices[0]
        } else {
            v2
        };

        let e2 = *state.board.vertices[v2 as usize]
            .adjacent_edges
            .iter()
            .find(|&&e| e != e1 as u8)
            .unwrap() as usize;
        state.roads[e2] = Some(1);
        let [_, v3] = state.board.edges[e2].vertices;
        let v3 = if v3 == v2 {
            state.board.edges[e2].vertices[0]
        } else {
            v3
        };

        let e3 = *state.board.vertices[v3 as usize]
            .adjacent_edges
            .iter()
            .find(|&&e| e != e2 as u8)
            .unwrap() as usize;
        state.roads[e3] = Some(1);
        let [_, v4] = state.board.edges[e3].vertices;
        let v4 = if v4 == v3 {
            state.board.edges[e3].vertices[0]
        } else {
            v4
        };

        let e4 = *state.board.vertices[v4 as usize]
            .adjacent_edges
            .iter()
            .find(|&&e| e != e3 as u8)
            .unwrap() as usize;
        state.roads[e4] = Some(1);

        state.update_longest_road();
        assert_eq!(state.longest_road_holder, Some(1));

        // Candidate action for player 0: BuildSettlement on v2 (cutting opponent 1)
        let cut_action = Action::BuildSettlement { vertex: v2 };
        let other_action = Action::Roll;
        let report = compute_spatial_root_impacts(&state, 0, &[cut_action.clone(), other_action]);

        let cut_impact = report
            .actions
            .iter()
            .find(|i| i.action == cut_action)
            .unwrap();
        assert_eq!(
            cut_impact.promotion,
            Some(RootPromotionReason::OpponentRouteCut)
        );
        assert!(cut_impact.road_delta.longest_road_loss_inflicted > 0);
    }

    #[test]
    fn comparable_same_turn_closeout_gets_coverage_promotion() {
        let state = GameState::standard(421, 3);
        let fast = Action::Roll;
        let slow = Action::EndTurn;
        let mut report = compute_spatial_root_impacts(&state, 0, &[fast.clone(), slow.clone()]);
        let plans = vec![
            TurnPlan {
                first_action: fast.clone(),
                actions: vec![fast.clone()],
                value: 10.0,
                nodes: 0,
                completion_mass: 1.0,
                decisive_completion_mass: 1.0,
                response_windows: Some(0.0),
            },
            TurnPlan {
                first_action: slow.clone(),
                actions: vec![slow],
                value: 10.1,
                nodes: 0,
                completion_mass: 1.0,
                decisive_completion_mass: 0.0,
                response_windows: Some(2.0),
            },
        ];
        apply_closeout_root_impacts(&mut report, &plans);

        let impact = report
            .actions
            .iter()
            .find(|impact| impact.action == fast)
            .unwrap();
        assert_eq!(
            impact.promotion,
            Some(RootPromotionReason::CloseoutCompression)
        );
        assert_eq!(impact.response_windows, Some(0.0));
        assert!((impact.closeout_gain - 1.0).abs() < 1e-6);
    }

    #[test]
    fn materially_weaker_same_turn_line_gets_no_closeout_promotion() {
        let state = GameState::standard(423, 3);
        let fast = Action::Roll;
        let slow = Action::EndTurn;
        let mut report = compute_spatial_root_impacts(&state, 0, &[fast.clone(), slow.clone()]);
        let plans = vec![
            TurnPlan {
                first_action: fast.clone(),
                actions: vec![fast.clone()],
                value: 9.0,
                nodes: 0,
                completion_mass: 1.0,
                decisive_completion_mass: 1.0,
                response_windows: Some(0.0),
            },
            TurnPlan {
                first_action: slow.clone(),
                actions: vec![slow],
                value: 10.0,
                nodes: 0,
                completion_mass: 1.0,
                decisive_completion_mass: 0.0,
                response_windows: Some(2.0),
            },
        ];
        apply_closeout_root_impacts(&mut report, &plans);

        let impact = report
            .actions
            .iter()
            .find(|impact| impact.action == fast)
            .unwrap();
        assert_eq!(impact.promotion, None);
        assert_eq!(impact.closeout_gain, 0.0);
    }

    #[test]
    fn negative_control_vanity_branch_is_not_promoted() {
        let mut state = GameState::standard(1, 3);
        // Player 0 has a road
        state.roads[0] = Some(0);
        state.update_longest_road();

        // An edge that doesn't claim award or cut anyone
        let road_action = Action::BuildRoad { edge: 1 };
        let report = compute_spatial_root_impacts(&state, 0, &[road_action.clone()]);
        let impact = report
            .actions
            .iter()
            .find(|i| i.action == road_action)
            .unwrap();
        assert_eq!(
            impact.promotion, None,
            "vanity road branch must not receive synthetic promotion"
        );
    }

    #[test]
    fn own_critical_expansion_edge_is_promoted() {
        let mut state = GameState::standard(910300003, 3);
        for edge in [32usize, 33, 34] {
            state.roads[edge] = Some(0);
            state.players[0].roads_left -= 1;
        }
        for edge in 0usize..=4 {
            state.roads[edge] = Some(1);
            state.players[1].roads_left -= 1;
        }
        state.buildings[26] = Some(Building::Settlement(0));
        state.players[0].settlements_left -= 1;
        state.buildings[0] = Some(Building::Settlement(1));
        state.players[1].settlements_left -= 1;
        state.update_longest_road();

        let road_action = Action::BuildRoad { edge: 31 };
        let baseline = analyze_road_resilience(&state, 0);
        let critical = baseline
            .critical_edges
            .iter()
            .find(|cut| cut.edge == 31)
            .expect("fixture must expose an actor-critical expansion edge");
        assert!(critical.expansion_loss > 0.35);

        let report = compute_spatial_root_impacts(&state, 0, &[road_action.clone()]);
        let impact = report
            .actions
            .iter()
            .find(|candidate| candidate.action == road_action)
            .unwrap();
        assert_eq!(
            impact.promotion,
            Some(RootPromotionReason::CriticalExpansionProtection)
        );
        assert!(impact.road_delta.expansion_portfolio_delta > 0.35);
    }
}
