//! Bounded topology-aware Longest Road race comparison for Road Building.
//!
//! Static length gap (`target - current`) is not a completion cost: joining two
//! networks can add several already-owned roads at once, while opponent
//! buildings and piece limits can make a short gap unreachable. This module
//! prices Road Building pairs by concrete legal paid-road completion paths on
//! a frozen board instead.
//!
//! The search clones the post-free-pair state, funds road costs artificially,
//! and runs a level-order (BFS) expansion over native `BuildRoad` transitions.
//! Connectivity, opponent-building blocks, piece inventory, and the award
//! itself all go through `colonist-catan-core`, so a witnessed route is a
//! real completion path. Only funding is artificial, and only inside this
//! search; affordability is priced separately through the existing ETA
//! machinery on the real hand.
//!
//! Budget semantics are explicit: every candidate receives its own independent
//! node budget (equal exploration, so a late joining pair is never starved by
//! lexical order), results memoize per public topology across particles and
//! swapped free-pair orders, and an exhausted search reports `Unknown` (worth
//! zero bonus) rather than a fake impossible.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};

use colonist_catan_core::{Action, GameState, Phase};

use crate::deadline::search_cancel_requested;
use crate::economy::build_eta_rolls;
use crate::eval::production_pips;

/// Live node budget per distinct post-free-pair topology.
///
/// Each node is one native `BuildRoad` transition attempt plus the award check.
/// Budgets are per candidate, never consumed from a shared pool, so every
/// pair receives equal exploration. 2048 witnesses the D235-class three-paid
/// completions (~1500 nodes); deeper races stay `Unknown` and price neutrally
/// rather than as fake impossibles.
pub(crate) const LIVE_RACE_NODE_BUDGET: usize = 2048;
/// Live cap on paid roads in a witnessed completion route.
pub(crate) const LIVE_RACE_MAX_PAID: u8 = 4;
/// Bound on memoized race outcomes (cleared, not grown, past this).
const RACE_CACHE_LIMIT: usize = 1024;
/// Cooperative cancellation is checked at least this often.
const CANCEL_CHECK_INTERVAL: u64 = 16;

#[derive(Clone, Copy, Debug)]
pub(crate) struct RaceConfig {
    pub(crate) node_budget: usize,
    pub(crate) max_paid: u8,
}

impl RaceConfig {
    pub(crate) fn live() -> Self {
        Self {
            node_budget: LIVE_RACE_NODE_BUDGET,
            max_paid: LIVE_RACE_MAX_PAID,
        }
    }

    #[cfg(test)]
    pub(crate) fn test() -> Self {
        Self {
            node_budget: 60_000,
            max_paid: 6,
        }
    }
}

/// A witnessed paid-road completion of the Longest Road award.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaidCompletion {
    /// Actual paid roads on the witnessed route (the completion cost).
    pub(crate) paid_roads: u8,
    /// Placed edges in an order that is sequentially legal.
    pub(crate) route: Vec<u8>,
    /// Own longest-road length once the route is placed.
    pub(crate) achieved_length: u8,
    /// Search nodes consumed to witness this route (0 on a cache hit).
    pub(crate) nodes_used: u64,
}

/// Bounded outcome of the completion search.
#[derive(Clone, Debug)]
pub(crate) enum RaceOutcome {
    Found(PaidCompletion),
    /// The search hit its node budget, depth cap, or a stop request before
    /// proving or disproving a completion. Must price as neutral, never as
    /// impossible.
    Unknown {
        #[cfg_attr(not(test), allow(dead_code))] // diagnostic node accounting
        nodes_used: u64,
    },
    /// Every legal completion within the depth cap was enumerated without
    /// taking the award.
    Exhausted {
        #[cfg_attr(not(test), allow(dead_code))] // diagnostic node accounting
        nodes_used: u64,
    },
}

impl RaceOutcome {
    #[cfg_attr(not(test), allow(dead_code))] // used by route profilers
    pub(crate) fn nodes_used(&self) -> u64 {
        match self {
            Self::Found(completion) => completion.nodes_used,
            Self::Unknown { nodes_used } | Self::Exhausted { nodes_used } => *nodes_used,
        }
    }
}

/// Compact FNV-1a identity of the road-sensitive board graph: player count,
/// edge-vertex incidence, and per-vertex edge adjacency. Hexes, numbers, and
/// ports never affect `BuildRoad` continuation, so they are excluded; hidden
/// hands and decks are excluded so belief particles share cache entries.
fn board_road_identity(board: &colonist_catan_core::Board) -> u64 {
    fn byte(hash: &mut u64, value: u8) {
        *hash ^= value as u64;
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    fn sized(hash: &mut u64, value: u64) {
        for chunk in value.to_le_bytes() {
            byte(hash, chunk);
        }
    }
    let mut hash = 0xcbf2_9ce4_8422_2325;
    byte(&mut hash, board.num_players);
    sized(&mut hash, board.edges.len() as u64);
    sized(&mut hash, board.vertices.len() as u64);
    for edge in &board.edges {
        byte(&mut hash, edge.vertices[0]);
        byte(&mut hash, edge.vertices[1]);
    }
    for vertex in &board.vertices {
        sized(&mut hash, vertex.adjacent_edges.len() as u64);
        for edge in &vertex.adjacent_edges {
            byte(&mut hash, *edge);
        }
    }
    hash
}

/// Cache identity over exactly the public rule-sensitive inputs the completion
/// search reads: graph, ownership arrays, piece inventory, and award holder.
/// Hidden hands/decks are excluded so particles share entries; phase, turn,
/// and funding are excluded because the search overrides them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct RaceKey {
    player: u8,
    board: u64,
    roads: Vec<u8>,
    buildings: Vec<u8>,
    roads_left: u8,
    longest_road_holder: Option<u8>,
    node_budget: usize,
    max_paid: u8,
}

#[derive(Clone, Debug)]
enum CachedRace {
    Found {
        paid_roads: u8,
        route: Vec<u8>,
        achieved_length: u8,
    },
    Unknown,
    Exhausted,
}

thread_local! {
    static RACE_CACHE: RefCell<HashMap<RaceKey, CachedRace>> =
        RefCell::new(HashMap::new());
}

fn race_key(state: &GameState, player: u8, config: &RaceConfig) -> RaceKey {
    RaceKey {
        player,
        board: board_road_identity(&state.board),
        roads: state
            .roads
            .iter()
            .map(|owner| owner.unwrap_or(u8::MAX))
            .collect(),
        buildings: state
            .buildings
            .iter()
            .map(|building| match building {
                None => u8::MAX,
                Some(piece) => {
                    piece.player()
                        + 4 * u8::from(matches!(piece, colonist_catan_core::Building::City(_)))
                }
            })
            .collect(),
        roads_left: state.players[player as usize].roads_left,
        longest_road_holder: state.longest_road_holder,
        node_budget: config.node_budget,
        max_paid: config.max_paid,
    }
}

#[cfg(test)]
pub(crate) fn cache_len_for_test() -> usize {
    RACE_CACHE.with(|cache| cache.borrow().len())
}

#[cfg(test)]
pub(crate) fn clear_cache_for_test() {
    RACE_CACHE.with(|cache| cache.borrow_mut().clear());
}

fn lookup_cached(key: &RaceKey) -> Option<RaceOutcome> {
    RACE_CACHE.with(|cache| {
        cache.borrow().get(key).map(|cached| match cached {
            CachedRace::Found {
                paid_roads,
                route,
                achieved_length,
            } => RaceOutcome::Found(PaidCompletion {
                paid_roads: *paid_roads,
                route: route.clone(),
                achieved_length: *achieved_length,
                nodes_used: 0,
            }),
            CachedRace::Unknown => RaceOutcome::Unknown { nodes_used: 0 },
            CachedRace::Exhausted => RaceOutcome::Exhausted { nodes_used: 0 },
        })
    })
}

/// Minimal paid-road completion of the Longest Road award on a frozen board.
///
/// `state` is the post-free-pair position. The board never changes except for
/// the actor's own paid roads: opponent buildings, occupied edges, and piece
/// inventory stay authoritative. Road funding is artificial (the scratch hand
/// is topped up) so the search measures topology, not the current wallet;
/// affordability is priced by [`future_award_bonus`] on the real hand.
///
/// Level-order expansion guarantees a found route uses the fewest paid roads
/// within the depth cap; within that cheapest level the longest achieved
/// trail wins, so a joining route that reaches further with the same number
/// of paid roads outranks a shorter one. A depth cap below the remaining
/// pieces reports `Unknown` when nothing is witnessed; only piece exhaustion
/// or a drained frontier reports `Exhausted`.
pub(crate) fn min_paid_completion(
    state: &GameState,
    player: u8,
    config: &RaceConfig,
    should_stop: &mut dyn FnMut() -> bool,
) -> RaceOutcome {
    let key = race_key(state, player, config);
    if let Some(hit) = lookup_cached(&key) {
        return hit;
    }
    let mut aborted = false;
    let outcome = min_paid_completion_uncached(state, player, config, should_stop, &mut aborted);
    // A stop request is transient deadline state, not a property of the
    // topology; never let it poison later decisions through the cache.
    // Node-budget Unknowns are deterministic for a fixed key (fixed expansion
    // order, membership-only visited set), so they are safe to memoize.
    if !aborted {
        RACE_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() >= RACE_CACHE_LIMIT {
                cache.clear();
            }
            let cached = match &outcome {
                RaceOutcome::Found(completion) => CachedRace::Found {
                    paid_roads: completion.paid_roads,
                    route: completion.route.clone(),
                    achieved_length: completion.achieved_length,
                },
                RaceOutcome::Unknown { .. } => CachedRace::Unknown,
                RaceOutcome::Exhausted { .. } => CachedRace::Exhausted,
            };
            cache.insert(key, cached);
        });
    }
    outcome
}

fn min_paid_completion_uncached(
    state: &GameState,
    player: u8,
    config: &RaceConfig,
    should_stop: &mut dyn FnMut() -> bool,
    aborted: &mut bool,
) -> RaceOutcome {
    if state.longest_road_holder == Some(player) {
        return RaceOutcome::Found(PaidCompletion {
            paid_roads: 0,
            route: Vec::new(),
            achieved_length: state.longest_road_length(player),
            nodes_used: 0,
        });
    }
    let roads_left = state.players[player as usize].roads_left;
    if roads_left == 0 {
        return RaceOutcome::Exhausted { nodes_used: 0 };
    }
    // A depth cap below the remaining pieces cannot prove impossibility: an
    // award unwitnessed within the cap is Unknown, not Exhausted. Only piece
    // exhaustion or a drained frontier is Exhausted.
    let piece_limited = config.max_paid >= roads_left;
    let depth_cap = config.max_paid.min(roads_left);
    if depth_cap == 0 {
        return RaceOutcome::Unknown { nodes_used: 0 };
    }
    let mut base = state.clone();
    base.current_player = player;
    base.phase = Phase::Main;
    base.free_roads = 0;
    // Artificial funding for the topology search only. Piece inventory,
    // connectivity, and the award check below stay native.
    base.players[player as usize].resources = [15, 15, 0, 0, 0];

    let mut visited: HashSet<Vec<u8>> = HashSet::new();
    visited.insert(Vec::new());
    let mut frontier: VecDeque<(GameState, Vec<u8>)> = VecDeque::from([(base, Vec::new())]);
    let mut nodes_used: u64 = 0;
    let mut stopped = || should_stop() || search_cancel_requested();
    for _paid in 1..=depth_cap {
        if stopped() {
            *aborted = true;
            return RaceOutcome::Unknown { nodes_used };
        }
        let level_size = frontier.len();
        if level_size == 0 {
            return RaceOutcome::Exhausted { nodes_used };
        }
        let mut best: Option<(Vec<u8>, u8)> = None;
        let mut next_frontier: VecDeque<(GameState, Vec<u8>)> = VecDeque::new();
        for _ in 0..level_size {
            let Some((node, placed)) = frontier.pop_front() else {
                break;
            };
            for edge in buildable_frontier(&node, player) {
                if nodes_used >= config.node_budget as u64 {
                    return RaceOutcome::Unknown { nodes_used };
                }
                nodes_used += 1;
                if nodes_used.is_multiple_of(CANCEL_CHECK_INTERVAL) && stopped() {
                    *aborted = true;
                    return RaceOutcome::Unknown { nodes_used };
                }
                let mut child = node.clone();
                if child.apply(&Action::BuildRoad { edge }).is_err() {
                    continue;
                }
                let mut route = placed.clone();
                route.push(edge);
                let mut dedup = route.clone();
                dedup.sort_unstable();
                if !visited.insert(dedup) {
                    continue;
                }
                if child.longest_road_holder == Some(player) {
                    let achieved = child.longest_road_length(player);
                    let better = best.as_ref().is_none_or(|(_, length)| achieved > *length);
                    if better {
                        best = Some((route, achieved));
                    }
                } else {
                    next_frontier.push_back((child, route));
                }
            }
        }
        // The whole level is scanned before returning so the cheapest cost
        // keeps the longest witnessed trail, not the first edge order hit.
        if let Some((route, achieved)) = best {
            return RaceOutcome::Found(PaidCompletion {
                paid_roads: route.len() as u8,
                route,
                achieved_length: achieved,
                nodes_used,
            });
        }
        frontier = next_frontier;
    }
    if piece_limited {
        RaceOutcome::Exhausted { nodes_used }
    } else {
        RaceOutcome::Unknown { nodes_used }
    }
}

/// Unoccupied edges connected to `player`'s network.
///
/// This enumerates exactly the edges the native road-connectivity rule
/// accepts: an unoccupied edge with an endpoint holding an own building, or
/// a building-free endpoint touching an own road. Vertices carrying an
/// opponent building never connect, so they contribute no candidates even
/// when own roads meet there. Every placement still goes through native
/// [`Action::BuildRoad`], which re-validates connectivity.
fn buildable_frontier(node: &GameState, player: u8) -> Vec<u8> {
    let mut seen = vec![false; node.board.edges.len()];
    let mut out = Vec::new();
    let push = |vertex: usize, node: &GameState, seen: &mut [bool], out: &mut Vec<u8>| {
        for edge in &node.board.vertices[vertex].adjacent_edges {
            let index = *edge as usize;
            if !seen[index] && node.roads[index].is_none() {
                seen[index] = true;
                out.push(*edge);
            }
        }
    };
    for (edge, owner) in node.roads.iter().enumerate() {
        if *owner != Some(player) {
            continue;
        }
        for vertex in node.board.edges[edge].vertices {
            if node.buildings[vertex as usize].is_some_and(|piece| piece.player() != player) {
                continue;
            }
            push(vertex as usize, node, &mut seen, &mut out);
        }
    }
    for (vertex, building) in node.buildings.iter().enumerate() {
        if building.is_some_and(|piece| piece.player() == player) {
            push(vertex, node, &mut seen, &mut out);
        }
    }
    out
}

fn sigmoid(value: f32) -> f32 {
    1.0 / (1.0 + (-value).exp())
}

/// Observation-safe ETA for a rival to fund `roads` roads.
///
/// Rival card identities are hidden, so the rival hand is estimated from the
/// public resource total spread over the rival production mix (the same
/// public-only inputs the expansion arrival model uses), then priced with the
/// shared [`build_eta_rolls`] machinery. Returns infinity when the rival
/// lacks the pieces or the funding path.
fn public_rival_road_eta(state: &GameState, rival: u8, roads: u8) -> f32 {
    if roads == 0 {
        return 0.0;
    }
    if roads > state.players[rival as usize].roads_left {
        return f32::INFINITY;
    }
    let production = production_pips(state, rival);
    let total = production.iter().sum::<f32>();
    let public_cards = state.players[rival as usize].resource_total() as f32;
    let hand: [u8; 5] = std::array::from_fn(|resource| {
        let share = (production[resource] + 1.0) / (total + 5.0);
        (public_cards * share).floor().clamp(0.0, 99.0) as u8
    });
    build_eta_rolls(
        &production,
        &hand,
        &state.trade_ratios(rival),
        &[roads, roads, 0, 0, 0],
    )
}

/// Discounted future-award value of a witnessed paid completion, in the same
/// raw utility units as `post_action_development_score`.
///
/// Returns zero when the award is already held (the endpoint prices the
/// immediate +2VP exactly once), when no completion is witnessed, or when the
/// route is unfundable. Otherwise the award is discounted by the actor's own
/// funding ETA and by the fastest rival's ability to extend past the
/// witnessed trail: reaching further with the same paid cost forces the rival
/// to fund more roads to retake the trophy.
pub(crate) fn future_award_bonus(
    next: &GameState,
    player: u8,
    config: &RaceConfig,
    should_stop: &mut dyn FnMut() -> bool,
) -> f32 {
    if next.longest_road_holder == Some(player) || next.winner().is_some() {
        return 0.0;
    }
    let completion = match min_paid_completion(next, player, config, should_stop) {
        RaceOutcome::Found(completion) if completion.paid_roads > 0 => completion,
        _ => return 0.0,
    };
    price_completion(next, player, &completion)
}

fn price_completion(next: &GameState, player: u8, completion: &PaidCompletion) -> f32 {
    let roads = completion.paid_roads;
    let production = production_pips(next, player);
    let own_eta = build_eta_rolls(
        &production,
        &next.players[player as usize].resources,
        &next.trade_ratios(player),
        &[roads, roads, 0, 0, 0],
    );
    if !own_eta.is_finite() {
        return 0.0;
    }
    // The holder sticks on ties, so a rival must strictly exceed the
    // witnessed trail to retake the trophy.
    let best_other = (0..next.board.num_players)
        .filter(|candidate| *candidate != player)
        .map(|candidate| next.longest_road_length(candidate))
        .max()
        .unwrap_or(0);
    let retake_roads = completion
        .achieved_length
        .saturating_add(1)
        .saturating_sub(best_other)
        .max(1);
    let rival_eta = (0..next.board.num_players)
        .filter(|candidate| *candidate != player)
        .map(|candidate| public_rival_road_eta(next, candidate, retake_roads))
        .fold(f32::INFINITY, f32::min);
    let eta_access = 1.0 / (1.0 + own_eta / 18.0);
    let survival = sigmoid((rival_eta - own_eta) / 12.0);
    let points_to_win =
        next.victory_target
            .saturating_sub(next.players[player as usize].victory_points()) as f32;
    let urgency = 1.0 + (4.0 - points_to_win).max(0.0) * 0.18;
    3.2 * urgency * eta_access * survival
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sea8653_fixture;

    fn apply_pair(state: &GameState, first: &str, second: &str) -> GameState {
        let mut next = state.clone();
        next.apply(&Action::PlayRoadBuilding {
            first: sea8653_fixture::edge(first),
            second: Some(sea8653_fixture::edge(second)),
        })
        .unwrap();
        next
    }

    fn assert_route_witnessed(state: &GameState, player: u8, completion: &PaidCompletion) {
        let mut replay = state.clone();
        replay.current_player = player;
        replay.phase = Phase::Main;
        replay.free_roads = 0;
        replay.players[player as usize].resources = [15, 15, 0, 0, 0];
        for edge in &completion.route {
            replay
                .apply(&Action::BuildRoad { edge: *edge })
                .expect("witnessed route replays through native transitions");
        }
        assert_eq!(
            replay.longest_road_holder,
            Some(player),
            "witnessed route must take the award natively"
        );
        assert_eq!(
            replay.longest_road_length(player),
            completion.achieved_length
        );
        assert_eq!(completion.route.len() as u8, completion.paid_roads);
    }

    fn found(state: &GameState, player: u8, config: &RaceConfig) -> PaidCompletion {
        match min_paid_completion(state, player, config, &mut || false) {
            RaceOutcome::Found(completion) => completion,
            outcome => panic!("expected a witnessed completion, got {outcome:?}"),
        }
    }

    const D235_PAIRS: [(&str, &str, &str); 3] = [
        ("engine", "e:2,-1,0", "e:3,-2,1"),
        ("manual", "e:1,0,2", "e:2,-1,0"),
        ("joining", "e:1,0,1", "e:1,0,0"),
    ];

    #[test]
    fn d235_completion_costs_are_topological_not_gap() {
        clear_cache_for_test();
        let state = sea8653_fixture::state("D235");
        assert_eq!(state.longest_road_length(0), 4);
        assert_eq!(state.longest_road_length(1), 10);
        // Static gaps (11 - post length) would claim 7/6/3 paid roads. The
        // topology-aware search witnesses 3 paid for every pair.
        for (name, a, b) in D235_PAIRS {
            let next = apply_pair(&state, a, b);
            assert_eq!(
                next.longest_road_holder,
                Some(1),
                "{name}: no immediate award, the race is multi-turn"
            );
            let completion = found(&next, 0, &RaceConfig::test());
            assert_eq!(completion.paid_roads, 3, "{name}: witnessed paid cost");
            assert_route_witnessed(&next, 0, &completion);
        }
        let joining = apply_pair(&state, "e:1,0,1", "e:1,0,0");
        let engine = apply_pair(&state, "e:2,-1,0", "e:3,-2,1");
        assert_eq!(
            found(&joining, 0, &RaceConfig::test()).achieved_length,
            12,
            "joining reaches 12 with its 3 paid roads"
        );
        assert_eq!(
            found(&engine, 0, &RaceConfig::test()).achieved_length,
            11,
            "engine pair reaches only 11 with its 3 paid roads"
        );
    }

    #[test]
    fn d235_live_budget_witnesses_all_three_pairs() {
        // Equal exploration: every candidate gets an independent budget, so a
        // late joining pair is never starved by lexical order.
        clear_cache_for_test();
        let state = sea8653_fixture::state("D235");
        for (name, a, b) in D235_PAIRS {
            let next = apply_pair(&state, a, b);
            match min_paid_completion(&next, 0, &RaceConfig::live(), &mut || false) {
                RaceOutcome::Found(completion) => {
                    assert_eq!(completion.paid_roads, 3, "{name}: live cost");
                    assert!(
                        completion.nodes_used <= LIVE_RACE_NODE_BUDGET as u64,
                        "{name}: within live budget"
                    );
                }
                outcome => panic!("{name}: live search must witness depth 3, got {outcome:?}"),
            }
        }
    }

    #[test]
    fn d235_margin_bonus_orders_equal_cost_pairs() {
        clear_cache_for_test();
        let state = sea8653_fixture::state("D235");
        let config = RaceConfig::live();
        let bonus = |a: &str, b: &str| {
            future_award_bonus(&apply_pair(&state, a, b), 0, &config, &mut || false)
        };
        let engine = bonus("e:2,-1,0", "e:3,-2,1");
        let manual = bonus("e:1,0,2", "e:2,-1,0");
        let joining = bonus("e:1,0,1", "e:1,0,0");
        // Same paid cost (3), but joining's longer witnessed trail forces a
        // rival to fund more roads to retake the trophy.
        assert!(
            joining > engine + 0.1,
            "joining margin must outrank the engine pair: {joining:.3} vs {engine:.3}"
        );
        assert!(
            (engine - manual).abs() < 1e-4,
            "equal cost and trail price equally: {engine:.3} vs {manual:.3}"
        );
        for value in [engine, manual, joining] {
            assert!(
                value > 0.0 && value <= 3.2 * 1.36 + 1e-3,
                "capped bonus: {value:.3}"
            );
        }
    }

    #[test]
    fn d235_exact_comparison_prices_the_witnessed_margin() {
        // End-to-end through the exact Road Building comparison, restricted to
        // the three named pairs so the test stays fast: each decision score
        // carries exactly its topology bonus, and joining's increment is the
        // largest without flipping the honest static order by itself.
        clear_cache_for_test();
        let particles = sea8653_fixture::particles_with_development("D235", [0, 2, 0, 0, 0]);
        let single = &particles[..1];
        let wanted: Vec<[u8; 2]> = D235_PAIRS
            .iter()
            .map(|(_, a, b)| {
                let mut pair = [sea8653_fixture::edge(a), sea8653_fixture::edge(b)];
                pair.sort_unstable();
                pair
            })
            .collect();
        let is_wanted = |action: &Action| match action {
            Action::PlayRoadBuilding {
                first,
                second: Some(second),
            } => {
                let mut pair = [*first, *second];
                pair.sort_unstable();
                wanted.contains(&pair)
            }
            _ => false,
        };
        let exclusions = single[0]
            .state
            .legal_actions()
            .into_iter()
            .filter(|action| {
                matches!(action, Action::PlayRoadBuilding { .. }) && !is_wanted(action)
            })
            .collect::<Vec<_>>();
        let result = crate::exact::solve_exact_belief_excluding(
            single,
            crate::exact::ExactActionFamily::RoadBuilding,
            &exclusions,
        );
        assert!(result.applicable);
        assert!(
            result.actions.len() >= 3,
            "the three named pairs must survive exclusion"
        );
        let mut increments = Vec::new();
        for (name, a, b) in D235_PAIRS {
            let next = apply_pair(&single[0].state, a, b);
            let endpoint = crate::eval::strategic_utility(&next, 0);
            let expected = future_award_bonus(&next, 0, &RaceConfig::live(), &mut || false);
            let scored = result
                .actions
                .iter()
                .find(|candidate| {
                    let mut pair = [sea8653_fixture::edge(a), sea8653_fixture::edge(b)];
                    pair.sort_unstable();
                    match &candidate.action {
                        Action::PlayRoadBuilding {
                            first,
                            second: Some(second),
                        } => {
                            let mut actual = [*first, *second];
                            actual.sort_unstable();
                            actual == pair
                        }
                        _ => false,
                    }
                })
                .unwrap_or_else(|| panic!("{name} pair must be scored"));
            assert!(
                (scored.decision_score - endpoint - expected).abs() < 1e-3,
                "{name}: exact score must carry exactly the race bonus"
            );
            increments.push((name, scored.decision_score - endpoint));
        }
        increments.sort_by(|left, right| right.1.total_cmp(&left.1));
        assert_eq!(
            increments[0].0, "joining",
            "joining's witnessed margin prices highest through exact"
        );
    }

    #[test]
    fn d251_five_paid_route_found_but_never_claimed_live() {
        // Static gap says 8 (13 - 5); the legal 13-road trail needs 5 paid.
        clear_cache_for_test();
        let state = sea8653_fixture::state("D251");
        assert_eq!(state.longest_road_length(0), 5);
        assert_eq!(state.longest_road_length(1), 12);
        assert_eq!(state.winner(), None);
        let completion = found(&state, 0, &RaceConfig::test());
        assert_eq!(completion.paid_roads, 5);
        assert_eq!(completion.achieved_length, 13);
        assert_route_witnessed(&state, 0, &completion);
        // A five-paid future is priced, never claimed: far below the immediate
        // +2VP scale (14.8), and honestly unknown under the live budget.
        let bonus = future_award_bonus(&state, 0, &RaceConfig::test(), &mut || false);
        assert!(
            bonus < 4.0,
            "no win claim on a contested five-paid future: {bonus:.3}"
        );
        clear_cache_for_test();
        assert!(
            matches!(
                min_paid_completion(&state, 0, &RaceConfig::live(), &mut || false),
                RaceOutcome::Unknown { .. }
            ),
            "depth 5 honestly exceeds the live budget"
        );
        assert_eq!(
            future_award_bonus(&state, 0, &RaceConfig::live(), &mut || false),
            0.0,
            "unknown races price neutrally live"
        );
    }

    #[test]
    fn d136_no_immediate_award_pair() {
        clear_cache_for_test();
        let state = sea8653_fixture::state("D136");
        let pairs = state
            .legal_actions()
            .into_iter()
            .filter(|action| matches!(action, Action::PlayRoadBuilding { .. }))
            .collect::<Vec<_>>();
        assert!(!pairs.is_empty());
        for action in &pairs {
            let mut next = state.clone();
            next.apply(action).unwrap();
            assert_ne!(
                next.longest_road_holder,
                Some(0),
                "no free pair may take the award immediately"
            );
        }
        // Spot-check pricing on the first pairs: future-only, non-negative,
        // and capped well below an immediate award.
        for action in pairs.iter().take(8) {
            let mut next = state.clone();
            next.apply(action).unwrap();
            let bonus = future_award_bonus(&next, 0, &RaceConfig::live(), &mut || false);
            assert!(
                (0.0..=3.2 * 1.72).contains(&bonus),
                "future-only capped bonus: {bonus:.3}"
            );
        }
    }

    #[test]
    fn cache_discriminates_board_graph() {
        // Identical ownership arrays on a different board graph must never
        // share a cached route. Graft the same roads/buildings/holder onto a
        // rewired graph (one own road swapped with an empty edge, adjacency
        // rebuilt) and require a separately computed, natively valid outcome.
        clear_cache_for_test();
        let base = apply_pair(&sea8653_fixture::state("D235"), "e:1,0,1", "e:1,0,0");
        let config = RaceConfig::live();
        assert_eq!(found(&base, 0, &config).paid_roads, 3);
        let owned = base
            .roads
            .iter()
            .position(|owner| *owner == Some(0))
            .expect("an own road to swap") as u8;
        let empty = base
            .roads
            .iter()
            .position(|owner| owner.is_none())
            .expect("an empty edge to swap") as u8;
        let mut board = (*base.board).clone();
        board.edges.swap(owned as usize, empty as usize);
        let incidence: Vec<[u8; 2]> = board.edges.iter().map(|edge| edge.vertices).collect();
        for vertex in &mut board.vertices {
            vertex.adjacent_edges.clear();
        }
        for (id, vertices) in incidence.iter().enumerate() {
            for vertex in vertices {
                board.vertices[*vertex as usize]
                    .adjacent_edges
                    .push(id as u8);
            }
        }
        let mut grafted = base.clone();
        grafted.board = std::sync::Arc::new(board);
        assert_eq!(grafted.roads, base.roads);
        assert_ne!(
            race_key(&grafted, 0, &config),
            race_key(&base, 0, &config),
            "graph identity belongs in the cache key"
        );
        match min_paid_completion(&grafted, 0, &config, &mut || false) {
            RaceOutcome::Found(completion) => assert_route_witnessed(&grafted, 0, &completion),
            RaceOutcome::Unknown { .. } | RaceOutcome::Exhausted { .. } => {}
        }
        assert_eq!(
            cache_len_for_test(),
            2,
            "the rewired graph must compute its own entry"
        );
        // The original entry is intact and still shared.
        assert_eq!(found(&base, 0, &config).nodes_used, 0);
    }

    #[test]
    fn cache_discriminates_award_holder() {
        clear_cache_for_test();
        let base = apply_pair(&sea8653_fixture::state("D235"), "e:1,0,1", "e:1,0,0");
        let config = RaceConfig::live();
        assert_eq!(found(&base, 0, &config).paid_roads, 3);
        // Synthetic holder flip: the search's only award input is the holder
        // (mirroring `update_longest_road`'s read set), so this isolates the
        // key field.
        let mut held = base.clone();
        held.longest_road_holder = Some(0);
        assert_ne!(
            race_key(&held, 0, &config),
            race_key(&base, 0, &config),
            "award holder belongs in the cache key"
        );
        match min_paid_completion(&held, 0, &config, &mut || false) {
            RaceOutcome::Found(completion) => assert_eq!(completion.paid_roads, 0),
            outcome => panic!("held award needs no paid roads, got {outcome:?}"),
        }
        assert_eq!(
            future_award_bonus(&held, 0, &config, &mut || false),
            0.0,
            "an already-held award prices nothing extra"
        );
        assert_eq!(cache_len_for_test(), 2);
    }

    #[test]
    fn depth_cap_reports_unknown_not_exhausted() {
        // Five pieces remain but the cap allows only two paid roads: no award
        // witness there cannot prove impossibility.
        clear_cache_for_test();
        let next = apply_pair(&sea8653_fixture::state("D235"), "e:1,0,1", "e:1,0,0");
        assert_eq!(next.players[0].roads_left, 5);
        let capped = RaceConfig {
            node_budget: 60_000,
            max_paid: 2,
        };
        assert!(
            matches!(
                min_paid_completion(&next, 0, &capped, &mut || false),
                RaceOutcome::Unknown { .. }
            ),
            "a depth cap below remaining pieces is Unknown, never Exhausted"
        );
        assert_eq!(
            future_award_bonus(&next, 0, &capped, &mut || false),
            0.0,
            "capped races price neutrally"
        );
    }

    #[test]
    fn tiny_budget_reports_unknown_never_impossible() {
        clear_cache_for_test();
        let state = sea8653_fixture::state("D235");
        let next = apply_pair(&state, "e:1,0,1", "e:1,0,0");
        let starved = RaceConfig {
            node_budget: 10,
            max_paid: 4,
        };
        let outcome = min_paid_completion(&next, 0, &starved, &mut || false);
        assert!(outcome.nodes_used() <= starved.node_budget as u64);
        assert!(
            matches!(outcome, RaceOutcome::Unknown { .. }),
            "an exhausted budget is unknown, not impossible"
        );
        assert_eq!(
            future_award_bonus(&next, 0, &starved, &mut || false),
            0.0,
            "unknown races must not become fake zeros that outrank nothing"
        );
        // The same topology is completable with a sufficient budget.
        assert_eq!(found(&next, 0, &RaceConfig::test()).paid_roads, 3);
    }

    #[test]
    fn piece_limit_makes_completion_exhausted() {
        clear_cache_for_test();
        let mut next = apply_pair(&sea8653_fixture::state("D235"), "e:1,0,1", "e:1,0,0");
        next.players[0].roads_left = 2;
        assert!(
            matches!(
                min_paid_completion(&next, 0, &RaceConfig::test(), &mut || false),
                RaceOutcome::Exhausted { .. }
            ),
            "two pieces cannot witness a three-paid route"
        );
        assert_eq!(
            future_award_bonus(&next, 0, &RaceConfig::test(), &mut || false),
            0.0
        );
    }

    #[test]
    fn stop_request_aborts_as_unknown() {
        clear_cache_for_test();
        let next = apply_pair(&sea8653_fixture::state("D235"), "e:1,0,1", "e:1,0,0");
        assert!(
            matches!(
                min_paid_completion(&next, 0, &RaceConfig::test(), &mut || true),
                RaceOutcome::Unknown { .. }
            ),
            "cooperative stop must abort the search, not fabricate a result"
        );
        assert_eq!(
            cache_len_for_test(),
            0,
            "transient stops must not poison the cache for later decisions"
        );
        // A later uncancelled call retries honestly instead of replaying the
        // aborted Unknown.
        let retry = found(&next, 0, &RaceConfig::test());
        assert_eq!(retry.paid_roads, 3);
        assert_eq!(cache_len_for_test(), 1);
    }

    #[test]
    #[ignore = "cold exact RoadBuilding timing with the live candidate set"]
    fn profile_cold_exact_roadbuilding() {
        clear_cache_for_test();
        let particles = sea8653_fixture::particles_with_development("D235", [0, 2, 0, 0, 0]);
        let started = std::time::Instant::now();
        let result = crate::exact::solve_exact_belief(
            &particles,
            crate::exact::ExactActionFamily::RoadBuilding,
        );
        eprintln!(
            "candidates={} worlds={} chosen={:?} elapsed={:?}",
            result.actions.len(),
            result.worlds,
            result.chosen,
            started.elapsed()
        );
    }

    #[test]
    #[ignore = "live-budget calibration profile for road_race"]
    fn profile_live_budgets() {
        clear_cache_for_test();
        let state = sea8653_fixture::state("D235");
        let pairs = state
            .legal_actions()
            .into_iter()
            .filter(|action| matches!(action, Action::PlayRoadBuilding { .. }))
            .collect::<Vec<_>>();
        let started = std::time::Instant::now();
        let mut found_count = 0;
        let mut nodes = 0;
        for action in &pairs {
            let mut next = state.clone();
            next.apply(action).unwrap();
            if let RaceOutcome::Found(completion) =
                min_paid_completion(&next, 0, &RaceConfig::live(), &mut || false)
            {
                found_count += 1;
                nodes += completion.nodes_used;
            }
        }
        eprintln!(
            "pairs={} found={found_count} nodes={nodes} elapsed={:?}",
            pairs.len(),
            started.elapsed()
        );
    }
}
