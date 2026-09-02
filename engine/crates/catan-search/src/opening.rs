use std::collections::HashMap;

use colonist_catan_core::{
    Action, CITY_COST, DEVELOPMENT_COST, GameState, Phase, ROAD_COST, ResourceHand,
    SETTLEMENT_COST, SplitMix64,
};

use crate::deadline::CooperativeDeadline;
use crate::eval::{evaluate, production_pips, vertex_value};
use crate::policy::{choose_rollout_action, normalize_priors};

const NUMBER_PIPS: [f32; 13] = [
    0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 0.0, 5.0, 4.0, 3.0, 2.0, 1.0,
];
const OPENING_BUILD_COSTS: [(ResourceHand, f32); 4] = [
    (ROAD_COST, 1.10),
    (SETTLEMENT_COST, 1.0),
    (CITY_COST, 0.82),
    (DEVELOPMENT_COST, 0.92),
];

/// Setup-specific endpoint value derived from the strongest published
/// JSettlers opening ablation: reward production and build coverage, while
/// explicitly penalizing repeated roll numbers and putting both settlements
/// on the same hex.
fn opening_position_bonus(state: &GameState, player: u8, exact_hand: bool) -> f32 {
    let production = production_pips(state, player);
    let mut number_uses = [0u8; 13];
    let mut hex_uses = vec![0u8; state.board.hexes.len()];
    let mut owned_buildings = 0u8;
    for (vertex, building) in state.buildings.iter().enumerate() {
        if !building.is_some_and(|building| building.player() == player) {
            continue;
        }
        owned_buildings += 1;
        for hex in &state.board.vertices[vertex].adjacent_hexes {
            let tile = &state.board.hexes[*hex as usize];
            if tile.resource.is_none() || tile.number as usize >= NUMBER_PIPS.len() {
                continue;
            }
            number_uses[tile.number as usize] += 1;
            hex_uses[*hex as usize] += 1;
        }
    }
    if owned_buildings == 0 {
        return 0.0;
    }
    let unique_strike_ways = number_uses
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .map(|(number, _)| NUMBER_PIPS[number])
        .sum::<f32>();
    let duplicate_number_exposure = number_uses
        .iter()
        .enumerate()
        .map(|(number, count)| count.saturating_sub(1) as f32 * NUMBER_PIPS[number])
        .sum::<f32>();
    let shared_hex_exposure = hex_uses
        .iter()
        .enumerate()
        .map(|(hex, count)| {
            count.saturating_sub(1) as f32 * NUMBER_PIPS[state.board.hexes[hex].number as usize]
        })
        .sum::<f32>();
    let resource_diversity = production.iter().filter(|pips| **pips > 0.0).count() as f32;
    let total_production = production.iter().sum::<f32>();
    let hand = if exact_hand {
        state.players[player as usize].resources
    } else {
        [0; 5]
    };
    let build_access = OPENING_BUILD_COSTS
        .iter()
        .map(|(cost, importance)| {
            let slowest_resource_rolls = cost
                .iter()
                .enumerate()
                .filter(|(_, required)| **required > 0)
                .map(|(resource, required)| {
                    let missing = required.saturating_sub(hand[resource]) as f32;
                    missing * 36.0 / (production[resource] + 0.55)
                })
                .fold(0.0_f32, f32::max);
            importance / (1.0 + slowest_resource_rolls / 18.0)
        })
        .sum::<f32>();

    total_production * 0.055
        + unique_strike_ways * 0.085
        + resource_diversity * 0.30
        + build_access * 0.82
        - duplicate_number_exposure * 0.19
        - shared_hex_exposure * 0.38
}

/// Values where the opening roads leave the player after one to three
/// additional roads. Ranking the top sites gives a portfolio signal instead of
/// only the single best fragile route.
fn opening_road_reach(state: &GameState, player: u8) -> f32 {
    let mut site_values = vec![None::<f32>; state.board.vertices.len()];
    let mut frontier = state
        .board
        .edges
        .iter()
        .enumerate()
        .filter(|(edge, _)| state.roads[*edge] == Some(player))
        .flat_map(|(_, edge)| edge.vertices)
        .collect::<Vec<_>>();
    frontier.sort_unstable();
    frontier.dedup();
    let mut distance = vec![u8::MAX; state.board.vertices.len()];
    for vertex in &frontier {
        distance[*vertex as usize] = 0;
    }
    for depth in 1u8..=3 {
        let mut next_frontier = Vec::new();
        for vertex in &frontier {
            if state.buildings[*vertex as usize].is_some_and(|building| building.player() != player)
            {
                continue;
            }
            for next_edge in &state.board.vertices[*vertex as usize].adjacent_edges {
                if state.roads[*next_edge as usize].is_some() {
                    continue;
                }
                let candidate = state.board.edges[*next_edge as usize]
                    .vertices
                    .into_iter()
                    .find(|other| other != vertex)
                    .unwrap_or(*vertex);
                if distance[candidate as usize] <= depth {
                    continue;
                }
                distance[candidate as usize] = depth;
                if !state.buildings[candidate as usize]
                    .is_some_and(|building| building.player() != player)
                {
                    next_frontier.push(candidate);
                }
                if state.buildings[candidate as usize].is_some()
                    || state.board.vertices[candidate as usize]
                        .adjacent_vertices
                        .iter()
                        .any(|neighbor| state.buildings[*neighbor as usize].is_some())
                {
                    continue;
                }
                let distance_penalty = 1.0 + (depth as f32 - 1.0) * 0.42;
                site_values[candidate as usize] =
                    Some(vertex_value(state, candidate, player) / distance_penalty);
            }
        }
        next_frontier.sort_unstable();
        next_frontier.dedup();
        frontier = next_frontier;
    }
    let mut site_values = site_values.into_iter().flatten().collect::<Vec<_>>();
    site_values.sort_by(|left, right| right.total_cmp(left));
    let best = site_values.first().copied().unwrap_or(0.0);
    let second = site_values.get(1).copied().unwrap_or(0.0);
    let third = site_values.get(2).copied().unwrap_or(0.0);
    best * 0.34 + second * 0.16 + third * 0.08
}

fn board_resource_scarcity(state: &GameState) -> [f32; 5] {
    let mut pips = [0.0_f32; 5];
    for hex in &state.board.hexes {
        let Some(resource) = hex.resource else {
            continue;
        };
        if (hex.number as usize) < NUMBER_PIPS.len() {
            pips[resource.index()] += NUMBER_PIPS[hex.number as usize];
        }
    }
    let mean = pips.iter().sum::<f32>() / 5.0;
    pips.map(|value| (mean - value).max(0.0))
}

fn opening_port_option_value(state: &GameState, player: u8) -> f32 {
    let production = production_pips(state, player);
    let total_production = production.iter().sum::<f32>();
    if total_production <= f32::EPSILON {
        return 0.0;
    }
    state
        .trade_ratios(player)
        .iter()
        .enumerate()
        .map(|(resource, ratio)| {
            let production_share = production[resource] / total_production;
            (4 - *ratio) as f32 * production_share
        })
        .sum::<f32>()
        * 0.11
}

fn opening_robber_concentration(state: &GameState, player: u8) -> f32 {
    let production = production_pips(state, player);
    let total = production.iter().sum::<f32>().max(0.01);
    let peak = production.iter().copied().fold(0.0_f32, f32::max);
    peak / total
}

fn opening_position_value(state: &GameState, player: u8, exact_hand: bool) -> f32 {
    let production = production_pips(state, player);
    let scarcity = board_resource_scarcity(state);
    let scarcity_alignment = production
        .iter()
        .zip(scarcity)
        .map(|(pips, scarce)| *pips * scarce * 0.012)
        .sum::<f32>();
    state.players[player as usize].public_victory_points as f32 * 1.8
        + opening_position_bonus(state, player, exact_hand)
        + opening_road_reach(state, player)
        + opening_port_option_value(state, player)
        + scarcity_alignment
        - opening_robber_concentration(state, player) * 0.22
}

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
    pub deadline_reached: bool,
}

#[derive(Clone, Debug)]
pub struct OpeningConfig {
    pub maximum_nodes: u32,
    pub root_width: usize,
    pub opponent_width: usize,
    pub time_budget_ms: u32,
    /// When true, opposing seats maximize their own opening value instead of
    /// returning a prior-weighted average of a few candidates.
    pub opponent_maximizes: bool,
    /// Short post-draft heuristic rollouts blended into the leaf.
    pub rollout_horizon: u8,
    pub rollout_count: u8,
}

impl Default for OpeningConfig {
    fn default() -> Self {
        Self {
            maximum_nodes: 18_000,
            root_width: 12,
            opponent_width: 4,
            time_budget_ms: 0,
            opponent_maximizes: true,
            // Keep rollouts off by default in the live path until a held-out
            // opening oracle shows positive regret reduction. Callers that want
            // post-draft samples set these explicitly.
            rollout_horizon: 16,
            rollout_count: 0,
        }
    }
}

struct OpeningSolver {
    root: u8,
    config: OpeningConfig,
    nodes: u32,
    node_limit: u32,
    aborted: bool,
    deadline_reached: bool,
    completed_setups: u32,
    memo: HashMap<u64, f32>,
    deadline: CooperativeDeadline,
}

impl OpeningSolver {
    fn value(&self, state: &GameState) -> f32 {
        let static_value = {
            let own = opening_position_value(state, self.root, true);
            let rival = (0..state.board.num_players)
                .filter(|player| *player != self.root)
                .map(|player| opening_position_value(state, player, false))
                .fold(f32::NEG_INFINITY, f32::max);
            own - rival.max(0.0) * 0.34
        };
        if matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) || self.config.rollout_count == 0
            || self.config.rollout_horizon == 0
            // Static first-click scoring and tiny budgets stay on the closed
            // form. Rollouts are reserved for completed deep draft leaves.
            || self.nodes < 32
            || self.node_limit.saturating_sub(self.nodes) < 64
        {
            return static_value;
        }
        let rollout = self.rollout_leaf(state);
        static_value * 0.68 + rollout * 0.32
    }

    fn rollout_leaf(&self, state: &GameState) -> f32 {
        let mut total = 0.0_f32;
        let count = self.config.rollout_count.max(1);
        for sample in 0..count {
            let mut cursor = state.clone();
            let mut rng = SplitMix64::new(
                state.state_hash() ^ ((sample as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)),
            );
            let mut steps = 0u8;
            while steps < self.config.rollout_horizon && !cursor.is_terminal() {
                let actions = cursor.legal_actions();
                if actions.is_empty() {
                    break;
                }
                let action = choose_rollout_action(&cursor, &actions, &mut rng);
                if cursor.apply(&action).is_err() {
                    break;
                }
                steps = steps.saturating_add(1);
            }
            let values = evaluate(&cursor);
            total += values[self.root as usize];
        }
        total / count as f32
    }

    fn visit(&mut self, state: &GameState) -> f32 {
        if self.nodes >= self.node_limit {
            self.aborted = true;
            return self.value(state);
        }
        if self.deadline.expired_at_checkpoint(self.nodes, 8) {
            self.aborted = true;
            self.deadline_reached = true;
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
            let mut best = f32::NEG_INFINITY;
            for (action, _) in ranked.into_iter().take(self.config.root_width) {
                if self.nodes >= self.node_limit {
                    self.aborted = true;
                    break;
                }
                let mut next = state.clone();
                if next.apply(&action).is_ok() {
                    best = best.max(self.visit(&next));
                    if self.deadline_reached {
                        return self.value(state);
                    }
                }
            }
            best
        } else if self.config.opponent_maximizes {
            // Opponents greedily maximize their own setup-aware leaf features
            // over a pruned candidate set, then the draft continues. This is
            // closer to independent seat play than a prior-weighted mixture.
            let mut best_action = None;
            let mut best_opponent_score = f32::NEG_INFINITY;
            for (action, _) in ranked.into_iter().take(self.config.opponent_width) {
                if self.nodes >= self.node_limit {
                    self.aborted = true;
                    break;
                }
                let mut next = state.clone();
                if next.apply(&action).is_ok() {
                    let opponent_score = opening_position_value(&next, actor, false);
                    if opponent_score > best_opponent_score {
                        best_opponent_score = opponent_score;
                        best_action = Some(action);
                    }
                }
            }
            if let Some(action) = best_action {
                let mut next = state.clone();
                if next.apply(&action).is_ok() {
                    self.visit(&next)
                } else {
                    self.value(state)
                }
            } else {
                self.value(state)
            }
        } else {
            let candidates = ranked
                .into_iter()
                .take(self.config.opponent_width)
                .collect::<Vec<_>>();
            let mut weighted = 0.0;
            let mut explored_mass = 0.0;
            for (action, prior) in candidates {
                if self.nodes >= self.node_limit {
                    self.aborted = true;
                    break;
                }
                let mut next = state.clone();
                if next.apply(&action).is_ok() {
                    weighted += self.visit(&next) * prior;
                    if self.deadline_reached {
                        return self.value(state);
                    }
                    explored_mass += prior;
                }
            }
            weighted / explored_mass.max(f32::EPSILON)
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
            deadline_reached: false,
        };
    }
    let deadline = CooperativeDeadline::start(config.time_budget_ms);
    let ranked = normalize_priors(state, &state.legal_actions(), state.actor());
    let mut solver = OpeningSolver {
        root,
        config,
        nodes: 0,
        node_limit: 0,
        aborted: false,
        deadline_reached: false,
        completed_setups: 0,
        memo: HashMap::new(),
        deadline,
    };
    // Score every legal first click with the setup leaf, then spend the deep
    // draft budget preferentially on the strongest static candidates. A flat
    // equal split across ~54 intersections left each root with too little
    // snake-draft depth under the live deadline.
    let mut static_actions = ranked
        .iter()
        .filter_map(|(action, prior)| {
            let mut next = state.clone();
            next.apply(action).ok()?;
            Some((action.clone(), *prior, solver.value(&next)))
        })
        .collect::<Vec<_>>();
    static_actions.sort_by(|left, right| {
        right
            .2
            .total_cmp(&left.2)
            .then_with(|| right.1.total_cmp(&left.1))
    });
    let deep_count = static_actions.len().min(solver.config.root_width.max(12));
    let budgets =
        crate::policy::allocate_root_node_budgets(deep_count, solver.config.maximum_nodes);
    let mut actions = Vec::new();
    for (index, (action, _, static_value)) in static_actions.into_iter().enumerate() {
        let mut next = state.clone();
        if next.apply(&action).is_err() {
            continue;
        }
        let value = if index >= deep_count || solver.deadline.has_elapsed() {
            if solver.deadline.has_elapsed() {
                solver.aborted = true;
                solver.deadline_reached = true;
            }
            static_value
        } else {
            let per_root_budget = budgets.get(index).copied().unwrap_or(1).max(1);
            solver.node_limit = solver
                .nodes
                .saturating_add(per_root_budget)
                .min(solver.config.maximum_nodes);
            if solver.nodes < solver.config.maximum_nodes {
                let completed_before = solver.completed_setups;
                let deep = solver.visit(&next);
                if solver.deadline_reached {
                    // Preserve a deep value only when THIS candidate completed
                    // at least one draft leaf. A global completed_setups counter
                    // would incorrectly keep partial values after earlier
                    // candidates finished.
                    let candidate_completed = solver.completed_setups > completed_before;
                    if deep.is_finite() && candidate_completed {
                        deep
                    } else {
                        static_value
                    }
                } else {
                    deep
                }
            } else {
                solver.aborted = true;
                static_value
            }
        };
        actions.push(OpeningActionValue { action, value });
    }
    if solver.deadline.has_elapsed() {
        solver.aborted = true;
        solver.deadline_reached = true;
    }
    actions.sort_by(|left, right| right.value.total_cmp(&left.value));
    OpeningReport {
        chosen: actions.first().map(|candidate| candidate.action.clone()),
        actions,
        nodes: solver.nodes,
        completed_setups: solver.completed_setups,
        complete: !solver.aborted,
        deadline_reached: solver.deadline_reached,
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
    use std::sync::Arc;

    use colonist_catan_core::{Action, Building, GameState, Phase};

    use super::{OpeningConfig, opening_position_bonus, solve_opening};

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
        let legal_root_actions = state.legal_actions().len();
        let report = solve_opening(
            &state,
            state.actor(),
            OpeningConfig {
                maximum_nodes: 40_000,
                root_width: 8,
                opponent_width: 2,
                time_budget_ms: 0,
                rollout_count: 0,
                ..OpeningConfig::default()
            },
        );
        assert!(matches!(
            report.chosen,
            Some(Action::PlaceSettlement { .. })
        ));
        assert_eq!(report.actions.len(), legal_root_actions);
        assert!(
            report.completed_setups > 0,
            "the first placement must be evaluated after the second settlement, starting cards, and both roads",
        );
    }

    #[test]
    fn timed_opening_keeps_every_legal_root_placement_after_deadline() {
        let state = GameState::standard(99, 4);
        let legal = state.legal_actions();
        let report = solve_opening(
            &state,
            state.actor(),
            OpeningConfig {
                maximum_nodes: 250_000,
                root_width: 32,
                opponent_width: 8,
                time_budget_ms: 1,
                rollout_count: 0,
                ..OpeningConfig::default()
            },
        );

        assert!(report.deadline_reached);
        assert_eq!(report.actions.len(), legal.len());
        assert!(
            report
                .chosen
                .as_ref()
                .is_some_and(|action| legal.contains(action))
        );
        assert!(report.nodes < 250_000);
    }

    #[test]
    fn opening_value_prefers_roll_diversity_at_equal_pip_strength() {
        let mut diverse = GameState::standard(101, 4);
        diverse.buildings.fill(None);
        let (first, second) = diverse
            .board
            .vertices
            .iter()
            .enumerate()
            .find_map(|(first_index, first)| {
                diverse
                    .board
                    .vertices
                    .iter()
                    .enumerate()
                    .find(|(_, second)| {
                        first
                            .adjacent_hexes
                            .iter()
                            .all(|hex| !second.adjacent_hexes.contains(hex))
                    })
                    .map(|(second_index, _)| (first_index, second_index))
            })
            .expect("standard board has disjoint opening vertices");
        diverse.buildings[first] = Some(Building::Settlement(0));
        diverse.buildings[second] = Some(Building::Settlement(0));
        let first_hexes = diverse.board.vertices[first].adjacent_hexes.clone();
        let second_hexes = diverse.board.vertices[second].adjacent_hexes.clone();
        let board = Arc::make_mut(&mut diverse.board);
        for hex in &first_hexes {
            board.hexes[*hex as usize].number = 6;
        }
        for hex in &second_hexes {
            board.hexes[*hex as usize].number = 8;
        }
        let mut duplicated = diverse.clone();
        let board = Arc::make_mut(&mut duplicated.board);
        for hex in &second_hexes {
            board.hexes[*hex as usize].number = 6;
        }

        assert!(
            opening_position_bonus(&diverse, 0, false)
                > opening_position_bonus(&duplicated, 0, false),
            "equal-pip settlements on different roll numbers must beat duplicated exposure"
        );
    }
}
