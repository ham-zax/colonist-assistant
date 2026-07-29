use std::collections::HashMap;

use colonist_catan_core::{Action, GameState, Phase};

use crate::eval::{public_strategic_utility, strategic_utility};
use crate::policy::normalize_priors;

#[derive(Clone, Debug)]
pub struct OpeningActionValue {
    pub action: Action,
    pub value: f32,
}

#[derive(Clone, Debug)]
pub struct OpeningReport {
    pub chosen: Option<Action>,
    pub actions: Vec<OpeningActionValue>,
    pub nodes: u32,
    pub completed_setups: u32,
    pub complete: bool,
}

#[derive(Clone, Debug)]
pub struct OpeningConfig {
    pub maximum_nodes: u32,
    pub root_width: usize,
    pub opponent_width: usize,
}

impl Default for OpeningConfig {
    fn default() -> Self {
        Self {
            maximum_nodes: 18_000,
            root_width: 12,
            opponent_width: 3,
        }
    }
}

struct OpeningSolver {
    root: u8,
    config: OpeningConfig,
    nodes: u32,
    node_limit: u32,
    aborted: bool,
    completed_setups: u32,
    memo: HashMap<u64, f32>,
}

impl OpeningSolver {
    fn value(&self, state: &GameState) -> f32 {
        let own = strategic_utility(state, self.root);
        let rival = (0..state.board.num_players)
            .filter(|player| *player != self.root)
            .map(|player| public_strategic_utility(state, player))
            .fold(f32::NEG_INFINITY, f32::max);
        own - rival.max(0.0) * 0.28
    }

    fn visit(&mut self, state: &GameState) -> f32 {
        if self.nodes >= self.node_limit {
            self.aborted = true;
            return self.value(state);
        }
        self.nodes += 1;
        if !matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            self.completed_setups += 1;
            return self.value(state);
        }
        if let Some(value) = self.memo.get(&state.state_hash()) {
            return *value;
        }
        let actor = state.actor();
        let legal = state.legal_actions();
        let ranked = normalize_priors(state, &legal, actor);
        let result = if actor == self.root {
            ranked
                .into_iter()
                .take(self.config.root_width)
                .filter_map(|(action, _)| {
                    let mut next = state.clone();
                    next.apply(&action).ok()?;
                    Some(self.visit(&next))
                })
                .fold(f32::NEG_INFINITY, f32::max)
        } else {
            let candidates = ranked
                .into_iter()
                .take(self.config.opponent_width)
                .collect::<Vec<_>>();
            let total = candidates
                .iter()
                .map(|(_, prior)| *prior)
                .sum::<f32>()
                .max(f32::EPSILON);
            candidates
                .into_iter()
                .filter_map(|(action, prior)| {
                    let mut next = state.clone();
                    next.apply(&action).ok()?;
                    Some(self.visit(&next) * prior / total)
                })
                .sum::<f32>()
        };
        let result = if result.is_finite() {
            result
        } else {
            self.value(state)
        };
        self.memo.insert(state.state_hash(), result);
        result
    }
}

/// Joint snake-order placement solver. A first settlement is valued through
/// simulated intervening placements, the best surviving second settlement,
/// starting resources, and both anchored road directions.
pub fn solve_opening(state: &GameState, root: u8, config: OpeningConfig) -> OpeningReport {
    if !matches!(
        state.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    ) {
        return OpeningReport {
            chosen: None,
            actions: Vec::new(),
            nodes: 0,
            completed_setups: 0,
            complete: true,
        };
    }
    let ranked = normalize_priors(state, &state.legal_actions(), state.actor());
    let mut solver = OpeningSolver {
        root,
        config,
        nodes: 0,
        node_limit: 0,
        aborted: false,
        completed_setups: 0,
        memo: HashMap::new(),
    };
    let root_candidates = ranked
        .into_iter()
        .take(solver.config.root_width)
        .collect::<Vec<_>>();
    let per_root_budget = (solver.config.maximum_nodes / root_candidates.len().max(1) as u32)
        .max(64)
        .min(solver.config.maximum_nodes);
    let mut actions = Vec::new();
    for (action, _) in root_candidates {
        if solver.nodes >= solver.config.maximum_nodes {
            break;
        }
        solver.node_limit = solver
            .nodes
            .saturating_add(per_root_budget)
            .min(solver.config.maximum_nodes);
        let mut next = state.clone();
        if next.apply(&action).is_err() {
            continue;
        }
        actions.push(OpeningActionValue {
            action,
            value: solver.visit(&next),
        });
    }
    actions.sort_by(|left, right| right.value.total_cmp(&left.value));
    OpeningReport {
        chosen: actions.first().map(|candidate| candidate.action.clone()),
        actions,
        nodes: solver.nodes,
        completed_setups: solver.completed_setups,
        complete: !solver.aborted,
    }
}

pub(crate) fn opening_adjusted_priors(
    state: &GameState,
    ranked: &mut [(Action, f32)],
    maximum_nodes: u32,
) {
    if !matches!(
        state.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    ) {
        return;
    }
    let report = solve_opening(
        state,
        state.actor(),
        OpeningConfig {
            maximum_nodes,
            root_width: ranked.len().min(16),
            ..OpeningConfig::default()
        },
    );
    let Some(minimum) = report
        .actions
        .iter()
        .map(|candidate| candidate.value)
        .reduce(f32::min)
    else {
        return;
    };
    let Some(maximum) = report
        .actions
        .iter()
        .map(|candidate| candidate.value)
        .reduce(f32::max)
    else {
        return;
    };
    for (action, prior) in ranked.iter_mut() {
        if let Some(candidate) = report
            .actions
            .iter()
            .find(|candidate| candidate.action == *action)
        {
            let normalized = if maximum > minimum {
                (candidate.value - minimum) / (maximum - minimum)
            } else {
                0.5
            };
            *prior = 0.04 + *prior * 0.18 + normalized * 0.78;
        }
    }
    let total = ranked
        .iter()
        .map(|(_, prior)| *prior)
        .sum::<f32>()
        .max(0.001);
    for (_, prior) in ranked.iter_mut() {
        *prior /= total;
    }
    ranked.sort_by(|left, right| right.1.total_cmp(&left.1));
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{Action, GameState, Phase};

    use super::{OpeningConfig, solve_opening};

    #[test]
    fn second_road_is_anchored_to_the_second_settlement() {
        let mut state = GameState::standard(91, 4);
        while state.setup_step < 4 {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        assert_eq!(state.current_player, 3);
        let settlement = state.legal_actions()[0].clone();
        state.apply(&settlement).unwrap();
        let Phase::SetupRoad { settlement } = state.phase else {
            panic!("expected anchored setup road");
        };
        let report = solve_opening(
            &state,
            3,
            OpeningConfig {
                maximum_nodes: 1_000,
                ..OpeningConfig::default()
            },
        );
        let Some(Action::PlaceRoad { edge }) = report.chosen else {
            panic!("opening solver must return a road");
        };
        assert!(
            state.board.edges[edge as usize]
                .vertices
                .contains(&settlement),
        );
    }

    #[test]
    fn first_settlement_is_scored_through_complete_snake_order_pairs() {
        let state = GameState::standard(97, 3);
        assert_eq!(state.phase, Phase::SetupSettlement);
        let report = solve_opening(
            &state,
            state.actor(),
            OpeningConfig {
                maximum_nodes: 40_000,
                root_width: 8,
                opponent_width: 2,
            },
        );
        assert!(matches!(
            report.chosen,
            Some(Action::PlaceSettlement { .. })
        ));
        assert!(
            report.completed_setups > 0,
            "the first placement must be evaluated after the second settlement, starting cards, and both roads",
        );
    }
}
