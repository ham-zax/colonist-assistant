use std::collections::HashMap;

use colonist_catan_core::{
    Action, CITY_COST, DEVELOPMENT_COST, GameState, Phase, ROAD_COST, ResourceHand,
    SETTLEMENT_COST, SplitMix64,
};

use crate::deadline::CooperativeDeadline;
use crate::economy::{build_conversion_efficiency, build_eta_rolls};
use crate::eval::{evaluate, expansion_option_value, production_pips, vertex_value};
use crate::policy::{choose_rollout_action, normalize_priors};

const NUMBER_PIPS: [f32; 13] = [
    0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 0.0, 5.0, 4.0, 3.0, 2.0, 1.0,
];
const OPENING_BUILD_COSTS: [(ResourceHand, f32); 4] = [
    // Both setup roads are free and route value is priced separately by the
    // expansion portfolio. Giving a third, paid road the old top weight
    // double-counted route access and rewarded starts that could build roads
    // immediately but could not close a settlement/city economy.
    (ROAD_COST, 0.45),
    (SETTLEMENT_COST, 1.0),
    (CITY_COST, 0.82),
    (DEVELOPMENT_COST, 0.92),
];
#[derive(Clone, Copy, Debug)]
struct OpeningBuildEconomy {
    /// Deterministic whole-card production proxy, not a first-passage expectation.
    eta_rolls: [f32; 4],
    /// Existing opening importance weights applied to the complete-build ETAs.
    weighted_access: f32,
    /// Long-run card efficiency of self-funding the same builds. This keeps a
    /// high-throughput port engine viable while distinguishing it from an
    /// economy that must burn four cards for every missing build resource.
    weighted_efficiency: f32,
    /// Best legal maritime ratio for each sold resource (4:1 / 3:1 / 2:1).
    maritime_ratios: ResourceHand,
}

fn opening_build_eta_rolls(
    production: &[f32; 5],
    hand: &ResourceHand,
    maritime_ratios: &ResourceHand,
    cost: &ResourceHand,
) -> f32 {
    build_eta_rolls(production, hand, maritime_ratios, cost)
}

fn opening_build_economy_from_inputs(
    production: &[f32; 5],
    hand: &ResourceHand,
    maritime_ratios: &ResourceHand,
) -> OpeningBuildEconomy {
    let eta_rolls = OPENING_BUILD_COSTS
        .map(|(cost, _)| opening_build_eta_rolls(production, hand, maritime_ratios, &cost));
    let weighted_access = eta_rolls
        .iter()
        .zip(OPENING_BUILD_COSTS)
        .map(|(eta, (_, importance))| importance / (1.0 + *eta / 18.0))
        .sum::<f32>();
    let total_importance = OPENING_BUILD_COSTS
        .iter()
        .map(|(_, importance)| *importance)
        .sum::<f32>();
    let weighted_efficiency = OPENING_BUILD_COSTS
        .iter()
        .map(|(cost, importance)| {
            importance * build_conversion_efficiency(production, maritime_ratios, cost)
        })
        .sum::<f32>()
        / total_importance.max(f32::EPSILON);
    OpeningBuildEconomy {
        eta_rolls,
        weighted_access,
        weighted_efficiency,
        maritime_ratios: *maritime_ratios,
    }
}

fn opening_build_economy(state: &GameState, player: u8) -> OpeningBuildEconomy {
    let production = production_pips(state, player);
    // Setup resource cards are deterministic public consequences of each
    // player's second settlement. Role identity must not erase them.
    let hand = state.players[player as usize].resources;
    // `trade_ratios` already implements the best applicable maritime ratio per
    // sold resource. This model intentionally stops at deterministic/self-
    // controlled acquisition: domestic/player trades are not assigned a
    // speculative rate, while bank/port conversion remains fully represented.
    let ratios = state.trade_ratios(player);
    opening_build_economy_from_inputs(&production, &hand, &ratios)
}

/// Setup-specific endpoint value. Expected production and conversion-aware
/// complete-build access are the main economic terms. Resource/color diversity
/// and roll-number distribution remain only residual flexibility/variance
/// signals; they are deliberately too small to substitute for build economics.
/// Shared-hex exposure remains a separate correlated-blocking/robber risk term.
fn opening_position_bonus(state: &GameState, player: u8) -> f32 {
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
    let settlement_resource_diversity = production
        .iter()
        .zip(SETTLEMENT_COST)
        .filter(|(pips, required)| **pips > 0.0 && *required > 0)
        .count() as f32;
    let ore_access = f32::from(production[4] > 0.0);
    let total_production = production.iter().sum::<f32>();
    let build_economy = opening_build_economy(state, player);

    total_production * 0.055
        + unique_strike_ways * 0.030
        + settlement_resource_diversity * 0.10
        + ore_access * 0.04
        + build_economy.weighted_access * 0.82
        + build_economy.weighted_efficiency * 0.30
        - duplicate_number_exposure * 0.04
        - shared_hex_exposure * 0.38
}

/// Cheap partial-setup ordering prior for where the free opening roads leave
/// the player after one to three additional roads. Completed setup leaves use
/// the shared paid-expansion ETA/race model instead.
fn opening_setup_reach_prior(state: &GameState, player: u8) -> f32 {
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

fn opening_robber_concentration(state: &GameState, player: u8) -> f32 {
    let production = production_pips(state, player);
    let total = production.iter().sum::<f32>().max(0.01);
    let peak = production.iter().copied().fold(0.0_f32, f32::max);
    peak / total
}

fn opening_expansion_value(state: &GameState, player: u8) -> f32 {
    let expansion = expansion_option_value(state, player);
    if expansion.vertex.is_none() {
        return 0.0;
    }

    // The shared expansion owner already prices the complete road-plus-
    // settlement cost and race arrival. Applying a second ETA multiplier here
    // double-counts affordability and lets changes in one heuristic owner drift
    // away from ordinary strategic road evaluation.
    expansion.value * 0.32 + expansion.portfolio_value * 0.22
}

fn opening_position_value(state: &GameState, player: u8) -> f32 {
    let production = production_pips(state, player);
    let scarcity = board_resource_scarcity(state);
    let scarcity_alignment = production
        .iter()
        .zip(scarcity)
        .map(|(pips, scarce)| *pips * scarce * 0.012)
        .sum::<f32>();
    let expansion_value = if matches!(
        state.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    ) {
        opening_setup_reach_prior(state, player)
    } else {
        opening_expansion_value(state, player)
    };
    state.players[player as usize].public_victory_points as f32 * 1.8
        + opening_position_bonus(state, player)
        + expansion_value
        + scarcity_alignment
        - opening_robber_concentration(state, player) * 0.22
}

#[derive(Clone, Copy, Debug)]
pub struct OpeningEvidence {
    pub production_pips: [f32; 5],
    pub starting_hand: ResourceHand,
    pub maritime_ratios: ResourceHand,
    pub build_eta_rolls: [f32; 4],
    pub settlement_vertices: [Option<u8>; 2],
    pub road_edges: [Option<u8>; 2],
    pub victory_term: f32,
    pub production_diversity_term: f32,
    pub build_access_term: f32,
    pub conversion_efficiency_term: f32,
    /// Counterfactual component delta with the same production/hand and 4:1
    /// everywhere. It is already inside the build terms, not an added bonus.
    pub port_build_gain: f32,
    pub expansion_term: f32,
    pub scarcity_term: f32,
    pub concentration_penalty: f32,
    pub own_value: f32,
    pub rival_value: f32,
    pub rival_weight: f32,
}

fn opening_evidence(state: &GameState, root: u8) -> OpeningEvidence {
    let production = production_pips(state, root);
    let economy = opening_build_economy(state, root);
    let hand = state.players[usize::from(root)].resources;
    let without_port = opening_build_economy_from_inputs(&production, &hand, &[4; 5]);
    let build_access_term = economy.weighted_access * 0.82;
    let conversion_efficiency_term = economy.weighted_efficiency * 0.30;
    let mut settlement_vertices = [None; 2];
    for (slot, (vertex, _)) in settlement_vertices.iter_mut().zip(state.buildings.iter().enumerate()
        .filter(|(_, piece)| piece.is_some_and(|piece| piece.player() == root))) {
        *slot = Some(vertex as u8);
    }
    let mut road_edges = [None; 2];
    for (slot, (edge, _)) in road_edges.iter_mut().zip(state.roads.iter().enumerate()
        .filter(|(_, owner)| **owner == Some(root))) {
        *slot = Some(edge as u8);
    }
    OpeningEvidence {
        production_pips: production,
        starting_hand: hand,
        maritime_ratios: economy.maritime_ratios,
        build_eta_rolls: economy.eta_rolls,
        settlement_vertices,
        road_edges,
        victory_term: f32::from(state.players[usize::from(root)].public_victory_points) * 1.8,
        production_diversity_term: opening_position_bonus(state, root) - build_access_term - conversion_efficiency_term,
        build_access_term,
        conversion_efficiency_term,
        port_build_gain: build_access_term + conversion_efficiency_term
            - (without_port.weighted_access * 0.82 + without_port.weighted_efficiency * 0.30),
        expansion_term: opening_expansion_value(state, root),
        scarcity_term: production.iter().zip(board_resource_scarcity(state))
            .map(|(pips, scarce)| pips * scarce * 0.012).sum(),
        concentration_penalty: opening_robber_concentration(state, root) * 0.22,
        own_value: opening_position_value(state, root),
        rival_value: (0..state.board.num_players).filter(|player| *player != root)
            .map(|player| opening_position_value(state, player)).fold(0.0, f32::max),
        rival_weight: if state.board.num_players == 2 { 1.0 } else { 0.34 },
    }
}

#[derive(Clone, Debug)]
pub struct OpeningActionValue {
    pub action: Action,
    pub value: f32,
    /// True only when this value's selected continuation reached the end of setup.
    pub endpoint_complete: bool,
    /// Whether this value is comparable for the final opening decision. Partial
    /// setup values remain in the report for diagnostics but cannot outrank a
    /// completion-backed root once any such root exists.
    pub authoritative: bool,
    pub node_budget: u32,
    pub visited_nodes: u32,
    pub evidence: Option<OpeningEvidence>,
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

#[derive(Clone, Copy, Debug)]
struct OpeningVisitValue {
    value: f32,
    endpoint_complete: bool,
    evidence: Option<OpeningEvidence>,
}

fn opening_visit_is_better(candidate: OpeningVisitValue, incumbent: OpeningVisitValue) -> bool {
    (candidate.endpoint_complete && !incumbent.endpoint_complete)
        || (candidate.endpoint_complete == incumbent.endpoint_complete
            && candidate.value > incumbent.value)
}

struct OpeningSolver {
    root: u8,
    config: OpeningConfig,
    nodes: u32,
    node_limit: u32,
    aborted: bool,
    deadline_reached: bool,
    completed_setups: u32,
    budget_cutoffs: u32,
    memo: HashMap<u64, OpeningVisitValue>,
    deadline: CooperativeDeadline,
}

impl OpeningSolver {
    fn static_value(&self, state: &GameState) -> f32 {
        let own = opening_position_value(state, self.root);
        let rival = (0..state.board.num_players)
            .filter(|player| *player != self.root)
            .map(|player| opening_position_value(state, player))
            .fold(f32::NEG_INFINITY, f32::max);
        let rival_weight = if state.board.num_players == 2 {
            1.0
        } else {
            0.34
        };
        own - rival.max(0.0) * rival_weight
    }

    fn value(&self, state: &GameState) -> f32 {
        let static_value = self.static_value(state);
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

    fn scout_completion_value(&self, state: &GameState) -> f32 {
        let mut cursor = state.clone();
        while matches!(
            cursor.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            if self.deadline.has_elapsed() {
                return self.static_value(state);
            }
            let actor = cursor.actor();
            let legal = cursor.legal_actions();
            let width = if actor == self.root {
                self.config.root_width
            } else {
                self.config.opponent_width
            };
            let Some((action, _)) = normalize_priors(&cursor, &legal, actor)
                .into_iter()
                .take(width)
                .next()
            else {
                break;
            };
            if cursor.apply(&action).is_err() {
                break;
            }
        }
        self.static_value(&cursor)
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

    fn visit(&mut self, state: &GameState) -> OpeningVisitValue {
        let endpoint_complete = !matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        );
        if self.nodes >= self.node_limit {
            self.aborted = true;
            self.budget_cutoffs = self.budget_cutoffs.saturating_add(1);
            return OpeningVisitValue {
                value: self.value(state),
                endpoint_complete,
                evidence: endpoint_complete.then(|| opening_evidence(state, self.root)),
            };
        }
        if self.deadline.expired_at_checkpoint(self.nodes, 8) {
            self.aborted = true;
            self.budget_cutoffs = self.budget_cutoffs.saturating_add(1);
            self.deadline_reached = true;
            return OpeningVisitValue {
                value: self.value(state),
                endpoint_complete,
                evidence: endpoint_complete.then(|| opening_evidence(state, self.root)),
            };
        }
        self.nodes += 1;
        if endpoint_complete {
            self.completed_setups += 1;
            return OpeningVisitValue {
                value: self.value(state),
                endpoint_complete: true,
                evidence: Some(opening_evidence(state, self.root)),
            };
        }
        if let Some(value) = self.memo.get(&state.state_hash()) {
            return *value;
        }
        let before_cutoffs = self.budget_cutoffs;
        let actor = state.actor();
        let legal = state.legal_actions();
        let mut ranked = normalize_priors(state, &legal, actor);
        if actor == self.root && state.phase == Phase::SetupSettlement {
            // A second settlement is a portfolio decision. Rank it through a
            // cheap policy-led complete snake-draft scout rather than a one-ply
            // score that can bury the strongest completed portfolio behind many
            // locally attractive settlements.
            let mut candidates = ranked
                .into_iter()
                .filter_map(|(action, prior)| {
                    let mut next = state.clone();
                    next.apply(&action).ok()?;
                    Some((action, prior, self.static_value(&next)))
                })
                .collect::<Vec<_>>();
            candidates.sort_by(|left, right| {
                right
                    .2
                    .total_cmp(&left.2)
                    .then_with(|| right.1.total_cmp(&left.1))
            });
            candidates.truncate(self.config.root_width);
            let mut candidates = candidates
                .into_iter()
                .map(|(action, prior, _)| {
                    let mut next = state.clone();
                    let score = if next.apply(&action).is_ok() {
                        self.scout_completion_value(&next)
                    } else {
                        f32::NEG_INFINITY
                    };
                    (action, prior, score)
                })
                .collect::<Vec<_>>();
            candidates.sort_by(|left, right| {
                right
                    .2
                    .total_cmp(&left.2)
                    .then_with(|| right.1.total_cmp(&left.1))
            });
            ranked = candidates
                .into_iter()
                .map(|(action, prior, _)| (action, prior))
                .collect();
        }
        let result = if actor == self.root {
            let mut best = OpeningVisitValue {
                value: f32::NEG_INFINITY,
                endpoint_complete: false,
                evidence: None,
            };
            let candidates = ranked.into_iter().take(self.config.root_width).collect::<Vec<_>>();
            let parent_limit = self.node_limit;
            let candidate_count = candidates.len();
            for (index, (action, _)) in candidates.into_iter().enumerate() {
                if self.nodes >= parent_limit {
                    self.aborted = true;
            self.budget_cutoffs = self.budget_cutoffs.saturating_add(1);
                    break;
                }
                let mut next = state.clone();
                if next.apply(&action).is_ok() {
                    if matches!(state.phase, Phase::SetupRoad { .. }) {
                        // Each legal road must reach a portfolio continuation.
                        // A depth-first first road must not exhaust all setup
                        // work before the alternative directions are examined.
                        let remaining = parent_limit.saturating_sub(self.nodes);
                        let slice = remaining / (candidate_count - index) as u32;
                        self.node_limit = self.nodes.saturating_add(slice).min(parent_limit);
                    }
                    let candidate = self.visit(&next);
                    self.node_limit = parent_limit;
                    if opening_visit_is_better(candidate, best) {
                        best = candidate;
                    }
                    if self.deadline_reached {
                        return OpeningVisitValue {
                            value: self.value(state),
                            endpoint_complete: false,
                evidence: None,
                        };
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
            self.budget_cutoffs = self.budget_cutoffs.saturating_add(1);
                    break;
                }
                let mut next = state.clone();
                if next.apply(&action).is_ok() {
                    let opponent_score = opening_position_value(&next, actor);
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
                    OpeningVisitValue {
                        value: self.value(state),
                        endpoint_complete: false,
                evidence: None,
                    }
                }
            } else {
                OpeningVisitValue {
                    value: self.value(state),
                    endpoint_complete: false,
                evidence: None,
                }
            }
        } else {
            let candidates = ranked
                .into_iter()
                .take(self.config.opponent_width)
                .collect::<Vec<_>>();
            let mut weighted = 0.0;
            let mut explored_mass = 0.0;
            let mut all_contributions_complete = true;
            for (action, prior) in candidates {
                if self.nodes >= self.node_limit {
                    self.aborted = true;
            self.budget_cutoffs = self.budget_cutoffs.saturating_add(1);
                    break;
                }
                let mut next = state.clone();
                if next.apply(&action).is_ok() {
                    let candidate = self.visit(&next);
                    weighted += candidate.value * prior;
                    all_contributions_complete &= candidate.endpoint_complete;
                    if self.deadline_reached {
                        return OpeningVisitValue {
                            value: self.value(state),
                            endpoint_complete: false,
                evidence: None,
                        };
                    }
                    explored_mass += prior;
                }
            }
            OpeningVisitValue {
                value: weighted / explored_mass.max(f32::EPSILON),
                endpoint_complete: explored_mass > 0.0 && all_contributions_complete,
                evidence: None,
            }
        };
        let result = if result.value.is_finite() {
            result
        } else {
            OpeningVisitValue {
                value: self.value(state),
                endpoint_complete: false,
                evidence: None,
            }
        };
        // A partial value depends on the remaining per-root budget. Caching it
        // under only the public state hash lets an early, narrow root poison a
        // later root that reaches the same state with enough budget to finish.
        // Reaching one endpoint is not proof the subtree was fully searched.
        // Cache only nodes whose complete expansion saw no budget cutoff;
        // stochastic rollout gates also depend on remaining budget.
        if result.endpoint_complete && before_cutoffs == self.budget_cutoffs && self.config.rollout_count == 0 {
            self.memo.insert(state.state_hash(), result);
        }
        result
    }
}

fn opening_root_node_budgets(
    root_count: usize,
    maximum_nodes: u32,
    remaining_setup_pairs: u8,
) -> Vec<u32> {
    if remaining_setup_pairs <= 3 && root_count > 0 {
        let base = maximum_nodes / root_count as u32;
        let remainder = maximum_nodes % root_count as u32;
        (0..root_count)
            .map(|index| base + u32::from((index as u32) < remainder))
            .collect()
    } else {
        crate::policy::allocate_root_node_budgets(root_count, maximum_nodes)
    }
}

/// Joint snake-order placement solver. A first settlement is valued through
/// simulated intervening placements, the best surviving second settlement,
/// starting resources, and both anchored road directions.
pub fn solve_opening(state: &GameState, root: u8, config: OpeningConfig) -> OpeningReport {
    solve_opening_excluding(state, root, config, &[])
}

pub(crate) fn solve_opening_excluding(
    state: &GameState, root: u8, config: OpeningConfig, root_exclusions: &[Action],
) -> OpeningReport {
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
    let legal = state.legal_actions().into_iter().filter(|action| !root_exclusions.contains(action)).collect::<Vec<_>>();
    let ranked = normalize_priors(state, &legal, state.actor());
    let mut solver = OpeningSolver {
        root,
        config,
        nodes: 0,
        node_limit: 0,
        aborted: false,
        deadline_reached: false,
        completed_setups: 0,
        budget_cutoffs: 0,
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
    let remaining_setup_pairs = state
        .board
        .num_players
        .saturating_mul(2)
        .saturating_sub(state.setup_step);
    let deep_count = if remaining_setup_pairs <= 3 {
        static_actions.len()
    } else {
        static_actions.len().min(solver.config.root_width.max(12))
    };
    let budgets = opening_root_node_budgets(
        deep_count,
        solver.config.maximum_nodes,
        remaining_setup_pairs,
    );
    let mut actions = Vec::new();
    for (index, (action, _, static_value)) in static_actions.into_iter().enumerate() {
        let mut next = state.clone();
        if next.apply(&action).is_err() {
            continue;
        }
        let before_nodes = solver.nodes;
        let node_budget = budgets.get(index).copied().unwrap_or(0);
        let (value, endpoint_complete, evidence) = if index >= deep_count || solver.deadline.has_elapsed() {
            if solver.deadline.has_elapsed() {
                solver.aborted = true;
                solver.deadline_reached = true;
            }
            (static_value, false, None)
        } else {
            let per_root_budget = budgets.get(index).copied().unwrap_or(1).max(1);
            solver.node_limit = solver
                .nodes
                .saturating_add(per_root_budget)
                .min(solver.config.maximum_nodes);
            if solver.nodes < solver.config.maximum_nodes {
                let deep = solver.visit(&next);
                if deep.value.is_finite() && deep.endpoint_complete {
                    (deep.value, true, deep.evidence)
                } else {
                    (static_value, false, None)
                }
            } else {
                solver.aborted = true;
                (static_value, false, None)
            }
        };
        actions.push(OpeningActionValue {
            action,
            value,
            endpoint_complete,
            authoritative: false,
            node_budget,
            visited_nodes: solver.nodes - before_nodes,
            evidence,
        });
    }
    if solver.deadline.has_elapsed() {
        solver.aborted = true;
        solver.deadline_reached = true;
    }
    // Partial setup scores are ordering priors, not comparable endpoint values.
    // Once any root's returned value is backed by a completed snake-draft endpoint,
    // only completion-backed values may become authoritative. Keep every legal root
    // in the report so budget/completion transitions remain observable instead of
    // silently changing the candidate set.
    let has_completed_root = actions.iter().any(|candidate| candidate.endpoint_complete);
    for candidate in &mut actions {
        candidate.authoritative = !has_completed_root || candidate.endpoint_complete;
    }
    actions.sort_by(|left, right| {
        right
            .authoritative
            .cmp(&left.authoritative)
            .then_with(|| right.value.total_cmp(&left.value))
    });
    let chosen = actions
        .iter()
        .find(|candidate| candidate.authoritative)
        .map(|candidate| candidate.action.clone());
    OpeningReport {
        chosen,
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
        .filter(|candidate| candidate.authoritative)
        .map(|candidate| candidate.value)
        .reduce(f32::min)
    else {
        return;
    };
    let Some(maximum) = report
        .actions
        .iter()
        .filter(|candidate| candidate.authoritative)
        .map(|candidate| candidate.value)
        .reduce(f32::max)
    else {
        return;
    };
    for (action, prior) in ranked.iter_mut() {
        if let Some(candidate) = report
            .actions
            .iter()
            .find(|candidate| candidate.authoritative && candidate.action == *action)
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
#[path = "opening_recorded_tests.rs"]
mod recorded_tests;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use colonist_catan_core::{Action, Building, GameState, Phase, SETTLEMENT_COST};

    use super::{
        OpeningConfig, OpeningVisitValue, opening_position_bonus, opening_position_value,
        opening_visit_is_better, solve_opening,
    };

    #[test]
    fn completion_backed_child_outranks_a_higher_partial_value() {
        let completed = OpeningVisitValue {
            value: 0.4,
            endpoint_complete: true,
                evidence: None,
        };
        let partial = OpeningVisitValue {
            value: 0.9,
            endpoint_complete: false,
                evidence: None,
        };

        assert!(opening_visit_is_better(completed, partial));
        assert!(!opening_visit_is_better(partial, completed));
    }

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
    fn completed_opening_expansion_prices_rival_affordability() {
        let mut state = GameState::standard(103, 3);
        state.phase = Phase::PreRoll;
        state.current_player = 1;
        state.buildings.fill(None);
        state.roads.fill(None);
        let (target, own_edge, rival_edge) = state
            .board
            .vertices
            .iter()
            .enumerate()
            .find_map(|(vertex, candidate)| {
                (candidate.adjacent_edges.len() >= 2).then_some((
                    vertex,
                    candidate.adjacent_edges[0],
                    candidate.adjacent_edges[1],
                ))
            })
            .expect("standard topology has a contested expansion vertex");
        state.roads[own_edge as usize] = Some(0);
        state.roads[rival_edge as usize] = Some(1);
        state.players[0].roads_left = 14;
        state.players[1].roads_left = 14;
        state.players[0].settlements_left = 5;
        state.players[1].settlements_left = 5;
        state.players[0].resources = SETTLEMENT_COST;
        state.players[1].resources = SETTLEMENT_COST;

        let target_hexes = state.board.vertices[target].adjacent_hexes.clone();
        let board = Arc::make_mut(&mut state.board);
        for tile in &mut board.hexes {
            if tile.resource.is_some() {
                tile.number = 2;
            }
        }
        for hex in target_hexes {
            if board.hexes[hex as usize].resource.is_some() {
                board.hexes[hex as usize].number = 6;
            }
        }

        let funded_rival = opening_position_value(&state, 0);
        state.players[1].resources = [0; 5];
        let starved_rival = opening_position_value(&state, 0);

        assert!(
            starved_rival > funded_rival + 0.05,
            "completed opening value must discount expansion sites that a funded rival can win before a resource-starved rival",
        );
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
            opening_position_bonus(&diverse, 0) > opening_position_bonus(&duplicated, 0),
            "equal-pip settlements on different roll numbers must beat duplicated exposure"
        );
    }
}
