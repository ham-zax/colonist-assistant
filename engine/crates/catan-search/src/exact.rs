use colonist_catan_core::{Action, GameState, NodeKind, Phase};

use crate::eval::evaluate;
use crate::mcts::BeliefParticle;
use crate::planner::{TurnPlanConfig, plan_current_turn};
use crate::policy::trade_acceptance_probability;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExactActionFamily {
    Mandatory,
    Monopoly,
    YearOfPlenty,
    RoadBuilding,
}

pub fn exact_family_for_action(action: &Action) -> Option<ExactActionFamily> {
    match action {
        Action::PlayMonopoly { .. } => Some(ExactActionFamily::Monopoly),
        Action::PlayYearOfPlenty { .. } => Some(ExactActionFamily::YearOfPlenty),
        Action::PlayRoadBuilding { .. } => Some(ExactActionFamily::RoadBuilding),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct ExactActionValue {
    pub action: Action,
    pub value: [f32; 4],
    pub lower_bound: [f32; 4],
    pub legal_weight: f32,
    pub decision_score: f32,
    pub lower_score: f32,
}

#[derive(Clone, Debug, Default)]
pub struct ExactDecisionResult {
    pub applicable: bool,
    pub chosen: Option<Action>,
    pub actions: Vec<ExactActionValue>,
    pub worlds: usize,
}

fn matches_family(state: &GameState, action: &Action, family: ExactActionFamily) -> bool {
    match family {
        ExactActionFamily::Mandatory => match state.phase {
            Phase::Discard => matches!(action, Action::Discard { .. }),
            Phase::MoveRobber => matches!(action, Action::MoveRobber { .. }),
            Phase::TradeResponses => matches!(
                action,
                Action::RespondTrade { .. }
                    | Action::CounterTrade { .. }
                    | Action::ConfirmTrade { .. }
                    | Action::CancelTrade
            ),
            _ => false,
        },
        ExactActionFamily::Monopoly => matches!(action, Action::PlayMonopoly { .. }),
        ExactActionFamily::YearOfPlenty => {
            matches!(action, Action::PlayYearOfPlenty { .. })
        }
        ExactActionFamily::RoadBuilding => {
            matches!(action, Action::PlayRoadBuilding { .. })
        }
    }
}

/// Resolve the small forced chance tail following a deadline-sensitive action.
///
/// Moving the robber is followed by a weighted steal outcome. Evaluating the
/// board before that chance transition systematically undervalues victims with
/// useful cards, so this routine enumerates the public action's complete forced
/// tail before evaluating it. It intentionally stops at the next player choice.
fn forced_tail_value(state: &GameState, depth: u8) -> [f32; 4] {
    if depth >= 5 {
        return evaluate(state);
    }
    match state.node_kind() {
        NodeKind::Chance => {
            let actions = state.legal_actions();
            if actions.is_empty() {
                return evaluate(state);
            }
            let total = actions
                .iter()
                .map(|action| state.chance_weight(action) as f32)
                .sum::<f32>()
                .max(f32::EPSILON);
            let mut result = [0.0; 4];
            for action in actions {
                let probability = state.chance_weight(&action) as f32 / total;
                let mut next = state.clone();
                if next.apply(&action).is_err() {
                    continue;
                }
                let child = forced_tail_value(&next, depth + 1);
                for player in 0..4 {
                    result[player] += child[player] * probability;
                }
            }
            result
        }
        NodeKind::Decision { actor } if state.phase == Phase::TradeResponses => {
            let actions = state.legal_actions();
            if actions.is_empty() {
                return evaluate(state);
            }
            let responses_complete = actions
                .iter()
                .any(|action| matches!(action, Action::ConfirmTrade { .. } | Action::CancelTrade));
            if responses_complete {
                return actions
                    .into_iter()
                    .filter_map(|action| {
                        let mut next = state.clone();
                        next.apply(&action).ok()?;
                        Some(forced_tail_value(&next, depth + 1))
                    })
                    .max_by(|left, right| left[actor as usize].total_cmp(&right[actor as usize]))
                    .unwrap_or_else(|| evaluate(state));
            }
            let accept = trade_acceptance_probability(state, actor);
            let counters = actions
                .iter()
                .filter(|action| matches!(action, Action::CounterTrade { .. }))
                .count();
            let counter_mass = if counters > 0 {
                (1.0 - accept) * 0.18
            } else {
                0.0
            };
            let mut result = [0.0; 4];
            let mut mass = 0.0;
            for action in actions {
                let probability = match action {
                    Action::RespondTrade { accept: true } => accept,
                    Action::RespondTrade { accept: false } => 1.0 - accept - counter_mass,
                    Action::CounterTrade { .. } => counter_mass / counters.max(1) as f32,
                    _ => 0.0,
                };
                if probability <= 0.0 {
                    continue;
                }
                let mut next = state.clone();
                if next.apply(&action).is_err() {
                    continue;
                }
                let child = forced_tail_value(&next, depth + 1);
                for player in 0..4 {
                    result[player] += child[player] * probability;
                }
                mass += probability;
            }
            if mass > 0.0 {
                result.map(|value| value / mass)
            } else {
                evaluate(state)
            }
        }
        NodeKind::Decision { .. } | NodeKind::Terminal => evaluate(state),
    }
}

fn action_value(state: &GameState, action: &Action) -> [f32; 4] {
    let mut next = state.clone();
    if next.apply(action).is_err() {
        return evaluate(state);
    }
    forced_tail_value(&next, 0)
}

fn family_score(
    state: &GameState,
    action: &Action,
    family: ExactActionFamily,
    actor: usize,
) -> f32 {
    let mut next = state.clone();
    if next.apply(action).is_err() {
        return evaluate(state)[actor];
    }
    if family == ExactActionFamily::Mandatory {
        if matches!(action, Action::ConfirmTrade { .. } | Action::CancelTrade)
            && next.current_player as usize == actor
            && next.phase == Phase::Main
        {
            // The creator's partner choice is not a protocol-only preference:
            // it must price the build that the accepted trade enables and the
            // strategic benefit given to that specific opponent.
            let own_plan = plan_current_turn(
                &next,
                TurnPlanConfig {
                    maximum_actions: 10,
                    maximum_nodes: 180,
                    branch_cap: 12,
                    root_cap: 20,
                },
            )
            .first()
            .map_or_else(
                || crate::eval::strategic_utility(&next, actor as u8),
                |plan| plan.value,
            );
            if let Action::ConfirmTrade { partner } = action {
                let opponent_gain = (crate::eval::strategic_utility(&next, *partner)
                    - crate::eval::strategic_utility(state, *partner))
                .max(0.0);
                let points_remaining = state
                    .victory_target
                    .saturating_sub(state.players[*partner as usize].public_victory_points);
                let threat_multiplier = match points_remaining {
                    0 | 1 => 2.4,
                    2 => 1.65,
                    3 => 1.15,
                    _ => 0.72,
                };
                return own_plan - opponent_gain * threat_multiplier;
            }
            return own_plan;
        }
        return forced_tail_value(&next, 0)[actor];
    }
    if matches!(
        family,
        ExactActionFamily::Monopoly
            | ExactActionFamily::YearOfPlenty
            | ExactActionFamily::RoadBuilding
    ) && next.current_player as usize == actor
        && matches!(next.phase, Phase::PreRoll | Phase::Main)
    {
        return plan_current_turn(
            &next,
            TurnPlanConfig {
                maximum_actions: 10,
                maximum_nodes: 180,
                branch_cap: 12,
                root_cap: 20,
            },
        )
        .first()
        .map_or_else(
            || crate::eval::strategic_utility(&next, actor as u8),
            |plan| plan.value,
        );
    }
    evaluate(&next)[actor]
}

/// Exhaustively ranks every action in a compact parameter family over the full
/// weighted belief. Missing/unavailable worlds retain their baseline value
/// instead of disappearing from the denominator.
///
/// This is used authoritatively for discard, robber/victim, and trade-response
/// deadlines. The same routine is exposed for exact Monopoly, Year of Plenty,
/// and Road Building parameter selection and regression tests.
pub fn solve_exact_belief(
    particles: &[BeliefParticle],
    family: ExactActionFamily,
) -> ExactDecisionResult {
    let Some(first) = particles.first() else {
        return ExactDecisionResult::default();
    };
    let actor = first.state.actor() as usize;
    // Parameter legality can itself depend on hidden state (for example a
    // counteroffer requesting a card the sender may or may not hold, or Year
    // of Plenty with a non-public bank). Build the observable candidate union
    // across the posterior and score every candidate over the full mass.
    let mut candidates = particles
        .iter()
        .flat_map(|particle| {
            particle
                .state
                .legal_actions()
                .into_iter()
                .filter(move |action| matches_family(&particle.state, action, family))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|action| format!("{action:?}"));
    candidates.dedup();
    if candidates.is_empty() {
        return ExactDecisionResult::default();
    }

    let total_weight = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let mut values = Vec::with_capacity(candidates.len());
    for action in candidates {
        let mut expected = [0.0; 4];
        let mut lower = [1.0_f32; 4];
        let mut legal_mass = 0.0;
        let mut decision_score = 0.0;
        let mut lower_score = f32::INFINITY;
        for particle in particles {
            let weight = particle.weight.max(0.0) / total_weight;
            if weight <= 1e-8 {
                continue;
            }
            let legal = particle.state.legal_actions().contains(&action);
            let value = if legal {
                legal_mass += weight;
                action_value(&particle.state, &action)
            } else {
                evaluate(&particle.state)
            };
            let score = if legal {
                family_score(&particle.state, &action, family, actor)
            } else {
                evaluate(&particle.state)[actor]
            };
            decision_score += score * weight;
            lower_score = lower_score.min(score);
            for player in 0..4 {
                expected[player] += value[player] * weight;
                lower[player] = lower[player].min(value[player]);
            }
        }
        values.push(ExactActionValue {
            action,
            value: expected,
            lower_bound: lower,
            legal_weight: legal_mass,
            decision_score,
            lower_score,
        });
    }
    values.sort_by(|left, right| {
        let left_score = left.decision_score * 0.82 + left.lower_score * 0.18;
        let right_score = right.decision_score * 0.82 + right.lower_score * 0.18;
        right_score
            .total_cmp(&left_score)
            .then_with(|| right.legal_weight.total_cmp(&left.legal_weight))
            .then_with(|| format!("{:?}", left.action).cmp(&format!("{:?}", right.action)))
    });
    ExactDecisionResult {
        applicable: true,
        chosen: values.first().map(|candidate| candidate.action.clone()),
        actions: values,
        worlds: particles.len(),
    }
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{Action, Building, GameState, Phase, Resource};

    use super::{ExactActionFamily, solve_exact_belief};
    use crate::mcts::BeliefParticle;

    fn particle(state: GameState) -> Vec<BeliefParticle> {
        vec![BeliefParticle { state, weight: 1.0 }]
    }

    #[test]
    fn discard_enumerates_every_legal_vector_and_returns_exact_count() {
        let mut state = GameState::standard(17, 3);
        state.phase = Phase::Discard;
        state.current_player = 0;
        state.discard_cursor = 0;
        state.players[0].resources = [4, 3, 2, 2, 1];
        state.discard_remaining[0] = 6;
        let legal_count = state.legal_actions().len();
        let result = solve_exact_belief(&particle(state), ExactActionFamily::Mandatory);
        let Action::Discard { cards } = result.chosen.expect("an exact discard") else {
            panic!("expected discard");
        };
        assert_eq!(cards.iter().sum::<u8>(), 6);
        assert_eq!(result.actions.len(), legal_count);
    }

    #[test]
    fn robber_never_selects_the_current_hex_and_values_the_steal_tail() {
        let mut state = GameState::standard(19, 3);
        state.phase = Phase::MoveRobber;
        state.current_player = 0;
        state.players[1].resources[Resource::Grain.index()] = 3;
        let current = state.robber_hex;
        let result = solve_exact_belief(&particle(state), ExactActionFamily::Mandatory);
        let Action::MoveRobber { hex, .. } = result.chosen.expect("an exact robber move") else {
            panic!("expected robber move");
        };
        assert_ne!(hex, current);
        assert!(result.actions.iter().all(|candidate| {
            !matches!(candidate.action, Action::MoveRobber { hex, .. } if hex == current)
        }));
    }

    #[test]
    fn development_parameter_families_are_fully_enumerated() {
        let mut state = GameState::standard(23, 3);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.players[0].development[3] = 1;
        let result = solve_exact_belief(&particle(state.clone()), ExactActionFamily::YearOfPlenty);
        let expected = state
            .legal_actions()
            .iter()
            .filter(|action| matches!(action, Action::PlayYearOfPlenty { .. }))
            .count();
        assert_eq!(result.actions.len(), expected);
        assert!(matches!(
            result.chosen,
            Some(Action::PlayYearOfPlenty { .. })
        ));
    }

    #[test]
    fn monopoly_uses_weighted_hidden_hands_instead_of_particle_zero() {
        let mut grain_world = GameState::standard(29, 3);
        while matches!(
            grain_world.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = grain_world.legal_actions()[0].clone();
            grain_world.apply(&action).unwrap();
        }
        grain_world.phase = Phase::Main;
        grain_world.current_player = 0;
        grain_world.players[0].development[4] = 1;
        grain_world.players[1].resources = [0, 0, 0, 7, 0];
        grain_world.players[2].resources = [0, 0, 0, 5, 0];
        let mut ore_world = grain_world.clone();
        ore_world.players[1].resources = [0, 0, 0, 0, 7];
        ore_world.players[2].resources = [0, 0, 0, 0, 5];
        let result = solve_exact_belief(
            &[
                BeliefParticle {
                    state: ore_world,
                    weight: 0.1,
                },
                BeliefParticle {
                    state: grain_world,
                    weight: 0.9,
                },
            ],
            ExactActionFamily::Monopoly,
        );
        assert_eq!(
            result.chosen,
            Some(Action::PlayMonopoly {
                resource: Resource::Grain,
            }),
        );
    }

    #[test]
    fn year_of_plenty_completes_a_city_instead_of_collecting_static_favorites() {
        let mut state = GameState::standard(31, 3);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].development[3] = 1;
        state.players[0].resources = [0, 0, 0, 0, 3];
        let result = solve_exact_belief(&particle(state), ExactActionFamily::YearOfPlenty);
        assert!(matches!(
            result.chosen,
            Some(Action::PlayYearOfPlenty {
                first: Resource::Grain,
                second: Resource::Grain,
            })
        ));
    }

    #[test]
    fn first_particle_win_is_not_a_belief_proof() {
        let mut winning = GameState::standard(37, 3);
        while matches!(
            winning.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = winning.legal_actions()[0].clone();
            winning.apply(&action).unwrap();
        }
        winning.phase = Phase::Main;
        winning.current_player = 0;
        winning.players[0].public_victory_points = 9;
        winning.players[0].resources = [1, 1, 1, 1, 0];
        let mut losing = winning.clone();
        losing.players[0].resources = [0, 0, 0, 0, 0];
        let result = crate::tactical::solve_belief_current_turn(
            &[(&winning, 0.5), (&losing, 0.5)],
            12,
            4_000,
        );
        assert!(!result.proven);
        assert!(result.principal_line.is_empty());
    }

    #[test]
    fn exact_parameter_candidates_union_hidden_bank_worlds() {
        let mut first = GameState::standard(39, 3);
        while matches!(
            first.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = first.legal_actions()[0].clone();
            first.apply(&action).unwrap();
        }
        first.phase = Phase::Main;
        first.current_player = 0;
        first.players[0].development[3] = 1;
        first.bank_is_public = false;
        first.bank = [18, 18, 18, 0, 1];
        let first_count = first
            .legal_actions()
            .iter()
            .filter(|action| matches!(action, Action::PlayYearOfPlenty { .. }))
            .count();
        let mut second = first.clone();
        second.bank = [18, 18, 18, 1, 0];
        assert_eq!(first.observation_hash(0), second.observation_hash(0));

        let result = solve_exact_belief(
            &[
                BeliefParticle {
                    state: first,
                    weight: 0.5,
                },
                BeliefParticle {
                    state: second,
                    weight: 0.5,
                },
            ],
            ExactActionFamily::YearOfPlenty,
        );
        assert!(result.actions.len() > first_count);
        assert!(
            result
                .actions
                .iter()
                .any(|candidate| candidate.legal_weight < 1.0),
        );
    }

    #[test]
    fn incoming_trade_response_scores_the_confirmed_transfer_not_the_pending_flag() {
        let mut state = GameState::standard(41, 3);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [0, 1, 0, 0, 0];
        state.players[1].resources = [3, 0, 1, 1, 0];
        state
            .apply(&Action::OfferTrade {
                recipients: 1 << 1,
                give: [0, 1, 0, 0, 0],
                receive: [1, 0, 0, 0, 0],
            })
            .unwrap();
        assert_eq!(state.actor(), 1);
        let result = solve_exact_belief(&particle(state), ExactActionFamily::Mandatory);
        assert!(!matches!(
            result.chosen,
            Some(Action::RespondTrade { accept: false })
        ));
    }

    #[test]
    fn completed_offer_selects_the_safer_acceptor_instead_of_the_first() {
        let mut state = GameState::standard(43, 3);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [0, 1, 0, 2, 2];
        state.players[1].resources = [1, 0, 1, 1, 1];
        state.players[1].public_victory_points = 8;
        state.players[2].resources = [0, 0, 0, 0, 1];
        state
            .apply(&Action::OfferTrade {
                recipients: (1 << 1) | (1 << 2),
                give: [0, 1, 0, 0, 0],
                receive: [0, 0, 0, 0, 1],
            })
            .unwrap();
        state.apply(&Action::RespondTrade { accept: true }).unwrap();
        state.apply(&Action::RespondTrade { accept: true }).unwrap();
        assert_eq!(state.actor(), 0);
        let result = solve_exact_belief(&particle(state), ExactActionFamily::Mandatory);
        assert_eq!(result.chosen, Some(Action::ConfirmTrade { partner: 2 }));
    }

    #[test]
    fn road_building_claims_a_two_point_longest_road_swing() {
        fn path(
            state: &GameState,
            vertex: u8,
            used: &mut Vec<u8>,
            target: usize,
        ) -> Option<Vec<u8>> {
            if used.len() == target {
                return Some(used.clone());
            }
            for edge in &state.board.vertices[vertex as usize].adjacent_edges {
                if used.contains(edge) {
                    continue;
                }
                let [left, right] = state.board.edges[*edge as usize].vertices;
                let next = if left == vertex { right } else { left };
                used.push(*edge);
                if let Some(found) = path(state, next, used, target) {
                    return Some(found);
                }
                used.pop();
            }
            None
        }

        let mut state = GameState::standard(47, 3);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.buildings.fill(None);
        state.roads.fill(None);
        let route = (0..state.board.vertices.len() as u8)
            .find_map(|vertex| {
                path(&state, vertex, &mut Vec::new(), 5).map(|route| (vertex, route))
            })
            .expect("the standard board contains a five-edge route");
        state.buildings[route.0 as usize] = Some(Building::Settlement(0));
        for edge in &route.1[..3] {
            state.roads[*edge as usize] = Some(0);
        }
        state.players[0].roads_left = 12;
        state.players[0].settlements_left = 4;
        state.players[0].development[2] = 1;
        state.players[0].bought_development[2] = 0;

        let result = solve_exact_belief(&particle(state.clone()), ExactActionFamily::RoadBuilding);
        let chosen = result.chosen.expect("an exact Road Building pair");
        let mut next = state;
        next.apply(&chosen).expect("the selected pair stays legal");
        assert_eq!(
            next.longest_road_holder,
            Some(0),
            "Road Building must price the immediate two-point trophy swing",
        );
        assert!(next.longest_road_length(0) >= 5);
    }
}
