//! Root-level strategic consequence aggregation and candidate action promotion.
//!
//! Evaluates the topological and tactical consequences of candidate root actions
//! before width truncation. Promotes actions that prevent or inflict material
//! road cuts, protect Longest Road, or defend critical expansion corridors.
//!
//! Promotion guarantees search width coverage; it does NOT inject synthetic
//! utility bonuses into tree search or leaf evaluation.

use colonist_catan_core::{Action, Building, GameState};

use crate::resilience::{analyze_road_resilience, RoadResilience};

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
}

#[derive(Clone, Debug)]
pub struct RootStrategicImpact {
    pub action: Action,
    pub road_delta: RoadImpactDelta,
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
                // 1. Check if building this settlement cuts any opponent's road network
                for &(_opp, ref opp_res) in &opponent_resiliences {
                    if let Some(cut) = opp_res.critical_vertices.iter().find(|c| c.vertex == *vertex) {
                        if cut.road_loss > 0 {
                            road_delta.longest_road_loss_inflicted =
                                road_delta.longest_road_loss_inflicted.max(cut.road_loss as i8);
                            promotion = Some(RootPromotionReason::OpponentRouteCut);
                        }
                        if cut.award_loss {
                            let mut probe = state.clone();
                            probe.buildings[*vertex as usize] = Some(Building::Settlement(actor));
                            probe.update_longest_road();
                            if probe.longest_road_holder == Some(actor) {
                                road_delta.award_vp_swing = 2;
                            }
                            promotion = Some(RootPromotionReason::OpponentRouteCut);
                        }
                        if cut.expansion_loss > 0.35 {
                            road_delta.expansion_portfolio_delta =
                                road_delta.expansion_portfolio_delta.max(cut.expansion_loss);
                            if promotion.is_none() {
                                promotion = Some(RootPromotionReason::CriticalExpansionProtection);
                            }
                        }
                    }
                }

                // 2. Check if building this settlement protects actor's own road network from cuts
                if let Some(own_cut) = baseline_road.critical_vertices.iter().find(|c| c.vertex == *vertex) {
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

                if state.longest_road_holder != Some(actor) && probe.longest_road_holder == Some(actor) {
                    road_delta.award_vp_swing = 2;
                    promotion = Some(RootPromotionReason::RoadAwardProtection);
                }

                // 2. Check if building this road establishes a bypass for an existing cut
                if baseline_road.maximum_longest_road_loss > 0 {
                    let after_res = analyze_road_resilience(&probe, actor);
                    if after_res.maximum_longest_road_loss < baseline_road.maximum_longest_road_loss {
                        road_delta.longest_road_loss_prevented =
                            (baseline_road.maximum_longest_road_loss - after_res.maximum_longest_road_loss) as i8;
                        if promotion.is_none() {
                            promotion = Some(RootPromotionReason::RoadAwardProtection);
                        }
                    }
                }

                // 3. Check if building this road denies a critical edge to an opponent
                for &(_opp, ref opp_res) in &opponent_resiliences {
                    if let Some(cut_edge) = opp_res.critical_edges.iter().find(|c| c.edge == *edge) {
                        if cut_edge.award_loss {
                            road_delta.award_vp_swing = 2;
                            promotion = Some(RootPromotionReason::OpponentRouteCut);
                        }
                        if cut_edge.expansion_loss > 0.35 {
                            road_delta.expansion_portfolio_delta =
                                road_delta.expansion_portfolio_delta.max(cut_edge.expansion_loss);
                            if promotion.is_none() {
                                promotion = Some(RootPromotionReason::CriticalExpansionProtection);
                            }
                        }
                    }
                }
            }

            _ => {}
        }

        impacts.push(RootStrategicImpact {
            action: action.clone(),
            road_delta,
            promotion,
        });
    }

    RootImpactReport {
        baseline_road,
        actions: impacts,
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
        let v2 = if v2 == v1 { state.board.edges[e1].vertices[0] } else { v2 };

        let e2 = *state.board.vertices[v2 as usize]
            .adjacent_edges
            .iter()
            .find(|&&e| e != e1 as u8)
            .unwrap() as usize;
        state.roads[e2] = Some(1);
        let [_, v3] = state.board.edges[e2].vertices;
        let v3 = if v3 == v2 { state.board.edges[e2].vertices[0] } else { v3 };

        let e3 = *state.board.vertices[v3 as usize]
            .adjacent_edges
            .iter()
            .find(|&&e| e != e2 as u8)
            .unwrap() as usize;
        state.roads[e3] = Some(1);
        let [_, v4] = state.board.edges[e3].vertices;
        let v4 = if v4 == v3 { state.board.edges[e3].vertices[0] } else { v4 };

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

        let cut_impact = report.actions.iter().find(|i| i.action == cut_action).unwrap();
        assert_eq!(cut_impact.promotion, Some(RootPromotionReason::OpponentRouteCut));
        assert!(cut_impact.road_delta.longest_road_loss_inflicted > 0);
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
        let impact = report.actions.iter().find(|i| i.action == road_action).unwrap();
        assert_eq!(impact.promotion, None, "vanity road branch must not receive synthetic promotion");
    }
}
