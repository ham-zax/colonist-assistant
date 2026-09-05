use std::collections::HashMap;

use colonist_catan_core::{Action, GameState, NodeKind, Phase};

use crate::deadline::CooperativeDeadline;
use crate::policy::actor_proposal_actions;

#[derive(Clone, Debug)]
pub struct TacticalResult {
    pub win_probability: f32,
    pub lower_bound: f32,
    pub principal_line: Vec<Action>,
    pub nodes: u32,
    pub proven: bool,
}

#[derive(Default)]
struct Solver {
    root_player: u8,
    maximum_depth: u8,
    maximum_nodes: u32,
    nodes: u32,
    aborted: bool,
    observation_safe: bool,
    deadline: Option<CooperativeDeadline>,
    memo: HashMap<(u64, u8), (f32, Vec<Action>)>,
}

impl Solver {
    fn visit(&mut self, state: &GameState, depth: u8) -> (f32, Vec<Action>) {
        if self.nodes >= self.maximum_nodes
            || self.deadline.as_ref().is_some_and(CooperativeDeadline::has_elapsed)
        {
            self.aborted = true;
            return (0.0, Vec::new());
        }
        self.nodes += 1;
        if let Some(winner) = state.winner() {
            return (f32::from(winner == self.root_player), Vec::new());
        }
        if depth >= self.maximum_depth
            || (state.current_player != self.root_player
                && !matches!(state.node_kind(), NodeKind::Chance))
        {
            return (0.0, Vec::new());
        }
        if let Some(value) = self.memo.get(&(state.state_hash(), depth)) {
            return value.clone();
        }
        let exact_actions = state.legal_actions();
        if exact_actions.is_empty() {
            return (0.0, Vec::new());
        }
        let result = match state.node_kind() {
            NodeKind::Terminal => (0.0, Vec::new()),
            NodeKind::Chance => {
                let total = exact_actions
                    .iter()
                    .map(|action| state.chance_weight(action) as f32)
                    .sum::<f32>();
                let mut expected = 0.0;
                let mut principal = Vec::new();
                let mut principal_mass = -1.0;
                for action in exact_actions {
                    let weight = state.chance_weight(&action) as f32;
                    let mut next = state.clone();
                    next.apply(&action)
                        .expect("legal chance outcome must transition");
                    let (value, tail) = self.visit(&next, depth + 1);
                    expected += value * weight / total;
                    let mass = value * weight;
                    if mass > principal_mass {
                        principal_mass = mass;
                        principal = std::iter::once(action).chain(tail).collect();
                    }
                }
                (expected, principal)
            }
            NodeKind::Decision { actor } => {
                let maximize = actor == self.root_player;
                let mut best = if maximize { -1.0 } else { 2.0 };
                let mut principal = Vec::new();
                let decision_actions = if self.observation_safe {
                    actor_proposal_actions(state)
                } else {
                    exact_actions
                };
                for action in decision_actions {
                    if matches!(action, Action::EndTurn) {
                        continue;
                    }
                    let mut next = state.clone();
                    next.apply(&action)
                        .expect("legal tactical action must transition");
                    let (value, tail) = self.visit(&next, depth + 1);
                    let improves = if maximize { value > best } else { value < best };
                    if improves {
                        best = value;
                        principal = std::iter::once(action).chain(tail).collect();
                    }
                    if (maximize && best >= 1.0) || (!maximize && best <= 0.0) {
                        break;
                    }
                }
                if !(0.0..=1.0).contains(&best) {
                    (0.0, Vec::new())
                } else {
                    (best, principal)
                }
            }
        };
        // Cache the executable continuation as well as its value. Returning an
        // empty tail on a transposition could turn a proven win into advice
        // with no legal first click.
        self.memo
            .insert((state.state_hash(), depth), result.clone());
        result
    }
}

fn solve_current_turn_with_deadline(
    state: &GameState,
    maximum_depth: u8,
    maximum_nodes: u32,
    observation_safe: bool,
    deadline: Option<CooperativeDeadline>,
) -> TacticalResult {
    if !matches!(state.phase, Phase::PreRoll | Phase::Main) {
        return TacticalResult {
            win_probability: 0.0,
            lower_bound: 0.0,
            principal_line: Vec::new(),
            nodes: 0,
            proven: false,
        };
    }
    let mut solver = Solver {
        root_player: state.actor(),
        maximum_depth,
        maximum_nodes,
        observation_safe,
        deadline,
        ..Solver::default()
    };
    let (win_probability, principal_line) = solver.visit(state, 0);
    TacticalResult {
        win_probability,
        lower_bound: if solver.aborted { 0.0 } else { win_probability },
        proven: !solver.aborted && win_probability >= 1.0 - f32::EPSILON,
        principal_line,
        nodes: solver.nodes,
    }
}

pub fn solve_current_turn(
    state: &GameState,
    maximum_depth: u8,
    maximum_nodes: u32,
) -> TacticalResult {
    solve_current_turn_with_deadline(state, maximum_depth, maximum_nodes, false, None)
}

/// A tactical line is called proven only when the same executable continuation
/// wins in every materially weighted hidden world and no per-world proof
/// exhausted its bound. Requiring the complete line is deliberately
/// conservative: agreement on only the first click can still hide
/// incompatible later choices (strategy fusion).
pub fn solve_belief_current_turn(
    particles: &[(&GameState, f32)],
    maximum_depth: u8,
    maximum_nodes: u32,
) -> TacticalResult {
    solve_belief_current_turn_with_deadline(particles, maximum_depth, maximum_nodes, None)
}

pub fn solve_belief_current_turn_timed(
    particles: &[(&GameState, f32)],
    maximum_depth: u8,
    maximum_nodes: u32,
    time_budget_ms: u32,
) -> TacticalResult {
    if time_budget_ms == 0 {
        return TacticalResult {
            win_probability: 0.0,
            lower_bound: 0.0,
            principal_line: Vec::new(),
            nodes: 0,
            proven: false,
        };
    }
    solve_belief_current_turn_with_deadline(
        particles,
        maximum_depth,
        maximum_nodes,
        Some(CooperativeDeadline::start(time_budget_ms)),
    )
}

fn solve_belief_current_turn_with_deadline(
    particles: &[(&GameState, f32)],
    maximum_depth: u8,
    maximum_nodes: u32,
    deadline: Option<CooperativeDeadline>,
) -> TacticalResult {
    if particles.is_empty() {
        return TacticalResult {
            win_probability: 0.0,
            lower_bound: 0.0,
            principal_line: Vec::new(),
            nodes: 0,
            proven: false,
        };
    }
    let total_weight = particles
        .iter()
        .map(|(_, weight)| weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let nodes_per_particle = (maximum_nodes / particles.len() as u32).max(1);
    let mut expected = 0.0;
    let mut lower_bound = 0.0;
    let mut nodes = 0;
    let mut common_first: Option<Action> = None;
    let mut common_line = Vec::new();
    let mut proven = true;
    for (state, raw_weight) in particles {
        let weight = raw_weight.max(0.0) / total_weight;
        if weight <= 1e-6 {
            continue;
        }
        let result = solve_current_turn_with_deadline(
            state,
            maximum_depth,
            nodes_per_particle,
            true,
            deadline.clone(),
        );
        expected += result.win_probability * weight;
        lower_bound += result.lower_bound * weight;
        nodes += result.nodes;
        let first = result.principal_line.first().cloned();
        if !result.proven || first.is_none() {
            proven = false;
        }
        match &common_first {
            None => {
                common_first = first;
                common_line = result.principal_line;
            }
            Some(expected_action)
                if first.as_ref() == Some(expected_action)
                    && result.principal_line == common_line => {}
            Some(_) => proven = false,
        }
    }
    if !proven {
        common_line.clear();
    }
    TacticalResult {
        win_probability: expected,
        lower_bound,
        principal_line: common_line,
        nodes,
        proven,
    }
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{GameState, Phase};

    use super::solve_belief_current_turn;

    #[test]
    fn belief_tactical_solver_respects_one_global_node_budget() {
        let mut worlds = Vec::new();
        for hidden_lumber in 0..32_u8 {
            let mut state = GameState::standard(301 + u64::from(hidden_lumber), 4);
            state.phase = Phase::Main;
            state.current_player = 0;
            state.players[0].resources = [2, 2, 2, 2, 3];
            state.players[1].resources = [hidden_lumber % 4, 0, 0, 0, 0];
            worlds.push(state);
        }
        let particles = worlds
            .iter()
            .map(|state| (state, 1.0 / worlds.len() as f32))
            .collect::<Vec<_>>();

        let result = solve_belief_current_turn(&particles, 8, 64);

        assert!(result.nodes <= 64);
    }
}
