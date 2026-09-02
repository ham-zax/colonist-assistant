use colonist_catan_core::{
    Action, Building, DevCard, GameState, NodeKind, Phase, ResourceHand, SplitMix64,
};
use colonist_catan_search::BeliefParticle;

pub mod tactical_corpus;

const PARTICLE_COUNT: usize = 4;
const PARTICLE_MIX: u64 = 0x9e37_79b9_7f4a_7c15;

pub const EXACT_PARITY_CORPUS_REVISION: &str = "phase-1-handcrafted-v1";

/// One deterministic input to the exact CPU/GPU evaluator comparison.
#[derive(Clone, Debug)]
pub struct ExactParityCase {
    pub name: String,
    pub state: GameState,
}

/// The observer-consistent determinization used by the arena's belief search.
///
/// The observer's private cards stay fixed. Opponent resource identities and
/// development-card identities are shuffled independently while preserving
/// public hand sizes, the visible bank, and exact card conservation.
pub fn belief_particles(
    state: &GameState,
    observer: u8,
    count: usize,
    seed: u64,
) -> Vec<BeliefParticle> {
    assert!(count > 0, "belief particle count must be positive");
    assert!(
        observer < state.board.num_players,
        "belief observer must be a player in the state"
    );

    let opponents = (0..state.board.num_players)
        .filter(|player| *player != observer)
        .collect::<Vec<_>>();
    let resource_totals = opponents
        .iter()
        .map(|player| state.players[*player as usize].resource_total())
        .collect::<Vec<_>>();
    let development_totals = opponents
        .iter()
        .map(|player| {
            state.players[*player as usize]
                .development
                .iter()
                .copied()
                .sum::<u8>()
        })
        .collect::<Vec<_>>();
    let bought_totals = opponents
        .iter()
        .map(|player| {
            state.players[*player as usize]
                .bought_development
                .iter()
                .copied()
                .sum::<u8>()
        })
        .collect::<Vec<_>>();
    let resource_pool = opponents
        .iter()
        .flat_map(|player| {
            state.players[*player as usize]
                .resources
                .iter()
                .enumerate()
                .flat_map(|(resource, count)| std::iter::repeat_n(resource as u8, *count as usize))
        })
        .collect::<Vec<_>>();
    let mut development_pool = opponents
        .iter()
        .flat_map(|player| {
            state.players[*player as usize]
                .development
                .iter()
                .enumerate()
                .flat_map(|(card, count)| std::iter::repeat_n(card as u8, *count as usize))
        })
        .collect::<Vec<_>>();
    development_pool.extend(
        state
            .development_deck
            .iter()
            .enumerate()
            .flat_map(|(card, count)| std::iter::repeat_n(card as u8, *count as usize)),
    );

    (0..count)
        .map(|sample| {
            let mut rng = SplitMix64::new(seed ^ (sample as u64 + 1).wrapping_mul(PARTICLE_MIX));
            let mut particle = state.clone();

            let mut resources = resource_pool.clone();
            rng.shuffle(&mut resources);
            let mut resource_cursor = 0usize;
            for (opponent_index, player) in opponents.iter().enumerate() {
                particle.players[*player as usize].resources = [0; 5];
                for _ in 0..resource_totals[opponent_index] {
                    let resource = resources[resource_cursor] as usize;
                    particle.players[*player as usize].resources[resource] += 1;
                    resource_cursor += 1;
                }
            }

            let mut development = development_pool.clone();
            rng.shuffle(&mut development);
            let mut development_cursor = 0usize;
            for (opponent_index, player) in opponents.iter().enumerate() {
                let player_state = &mut particle.players[*player as usize];
                player_state.development = [0; 5];
                player_state.bought_development = [0; 5];
                for _ in 0..development_totals[opponent_index] {
                    let card = development[development_cursor] as usize;
                    player_state.development[card] += 1;
                    development_cursor += 1;
                }
                let mut held_cards = player_state
                    .development
                    .iter()
                    .enumerate()
                    .flat_map(|(card, count)| std::iter::repeat_n(card as u8, *count as usize))
                    .collect::<Vec<_>>();
                rng.shuffle(&mut held_cards);
                for card in held_cards
                    .into_iter()
                    .take(bought_totals[opponent_index] as usize)
                {
                    player_state.bought_development[card as usize] += 1;
                }
            }
            particle.development_deck = [0; 5];
            for card in development.into_iter().skip(development_cursor) {
                particle.development_deck[card as usize] += 1;
            }
            assert_valid(&particle, "belief particle");
            BeliefParticle {
                state: particle,
                weight: 1.0 / count as f32,
            }
        })
        .collect()
}

/// Build the fixed Phase-1 evaluator parity corpus.
///
/// This is intentionally a named benchmark corpus rather than a configurable
/// fixture generator. Every state is deterministic, validated, and represented
/// by a stable case name for machine-readable failure reports.
pub fn exact_parity_corpus() -> Vec<ExactParityCase> {
    let mut cases = Vec::new();
    for (players, board_seed) in [(3u8, 910_300_003u64), (4, 910_400_004)] {
        let early = setup_state(board_seed, players);
        push_case(&mut cases, format!("{players}p-early-setup"), early.clone());

        let mid = progressed_state(board_seed, players, 24, 0x1357_9bdf);
        push_case(&mut cases, format!("{players}p-mid-replay"), mid.clone());

        let late = progressed_state(board_seed, players, 72, 0x2468_ace0);
        push_case(&mut cases, format!("{players}p-late-replay"), late.clone());
        append_belief_cases(&mut cases, &late, board_seed ^ 0x10_0000, "late-belief");

        append_robber_cases(&mut cases, board_seed, players);
        append_port_and_maritime_cases(&mut cases, board_seed ^ 0x100, players);
        append_road_race_case(&mut cases, board_seed ^ 0x200, players);
        append_trophy_case(&mut cases, board_seed ^ 0x300, players);
        append_development_case(&mut cases, board_seed ^ 0x400, players);
        append_large_hand_case(&mut cases, board_seed ^ 0x500, players);
        append_no_player_trades_case(&mut cases, board_seed ^ 0x600, players);
        append_terminal_cases(&mut cases, board_seed ^ 0x700, players);
    }
    cases
}

fn push_case(cases: &mut Vec<ExactParityCase>, name: String, state: GameState) {
    assert_valid(&state, &name);
    cases.push(ExactParityCase { name, state });
}

fn append_belief_cases(
    cases: &mut Vec<ExactParityCase>,
    state: &GameState,
    seed: u64,
    label: &str,
) {
    let observer = state.actor();
    for (index, particle) in belief_particles(state, observer, PARTICLE_COUNT, seed)
        .into_iter()
        .enumerate()
    {
        push_case(
            cases,
            format!("{}-{}p-particle-{}", label, state.board.num_players, index),
            particle.state,
        );
    }
}

fn assert_valid(state: &GameState, label: &str) {
    state
        .validate()
        .unwrap_or_else(|error| panic!("{label}: invalid generated state: {error}"));
}

fn setup_state(board_seed: u64, players: u8) -> GameState {
    let mut state = GameState::standard(board_seed, players);
    while matches!(
        state.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    ) {
        let action = state
            .legal_actions()
            .into_iter()
            .next()
            .expect("setup must always expose a legal placement");
        state
            .apply(&action)
            .expect("generated setup action must be legal");
    }
    assert_valid(&state, "completed setup");
    state
}

fn progressed_state(board_seed: u64, players: u8, steps: usize, action_seed: u64) -> GameState {
    let mut state = setup_state(board_seed, players);
    let mut rng = SplitMix64::new(action_seed ^ board_seed);
    for _ in 0..steps {
        if state.is_terminal() {
            break;
        }
        let actions = state.legal_actions();
        if actions.is_empty() {
            break;
        }
        let action = if matches!(state.node_kind(), NodeKind::Chance) {
            weighted_action(&state, &actions, &mut rng)
        } else {
            actions[rng.range(actions.len())].clone()
        };
        state
            .apply(&action)
            .expect("generated replay action must be legal");
        assert_valid(&state, "replayed state");
    }
    state
}

fn weighted_action(state: &GameState, actions: &[Action], rng: &mut SplitMix64) -> Action {
    let total = actions
        .iter()
        .map(|action| u64::from(state.chance_weight(action)))
        .sum::<u64>();
    if total == 0 {
        return actions[rng.range(actions.len())].clone();
    }
    let mut target = rng.next_u64() % total;
    for action in actions {
        let weight = u64::from(state.chance_weight(action));
        if target < weight {
            return action.clone();
        }
        target -= weight;
    }
    actions.last().expect("nonempty chance action list").clone()
}

fn main_state(board_seed: u64, players: u8) -> GameState {
    let mut state = setup_state(board_seed, players);
    state.current_player = 0;
    state.phase = Phase::Main;
    state.domestic_trade_used = false;
    state.domestic_trade_count = 0;
    state.trade = None;
    state.last_rejected_trade = None;
    state.trade_cursor = 0;
    state.trade_negotiation_round = 0;
    assert_valid(&state, "main fixture");
    state
}

fn set_resources(state: &mut GameState, player: u8, desired: ResourceHand) {
    for (resource, target) in desired.into_iter().enumerate() {
        let current = state.players[player as usize].resources[resource];
        if target > current {
            let delta = target - current;
            assert!(
                state.bank[resource] >= delta,
                "fixture needs more bank resources than remain"
            );
            state.bank[resource] -= delta;
        } else {
            state.bank[resource] += current - target;
        }
        state.players[player as usize].resources[resource] = target;
    }
}

fn add_port_settlement(state: &mut GameState, player: u8) -> usize {
    let vertex = state
        .board
        .vertices
        .iter()
        .enumerate()
        .find(|(index, vertex)| {
            vertex.port.is_some()
                && state.buildings[*index].is_none()
                && vertex
                    .adjacent_vertices
                    .iter()
                    .all(|neighbor| state.buildings[*neighbor as usize].is_none())
        })
        .map(|(index, _)| index)
        .expect("standard board must have an unblocked port vertex");
    assert!(state.players[player as usize].settlements_left > 0);
    state.players[player as usize].settlements_left -= 1;
    state.players[player as usize].public_victory_points += 1;
    state.buildings[vertex] = Some(Building::Settlement(player));
    vertex
}

fn give_development(state: &mut GameState, player: u8, cards: [u8; 5]) {
    for card in DevCard::ALL {
        let index = card.index();
        assert!(state.development_deck[index] >= cards[index]);
        state.development_deck[index] -= cards[index];
        state.players[player as usize].development[index] += cards[index];
    }
}

fn append_robber_cases(cases: &mut Vec<ExactParityCase>, board_seed: u64, players: u8) {
    let mut state = setup_state(board_seed, players);
    state.current_player = 0;
    state.phase = Phase::PreRoll;
    set_resources(&mut state, 0, [8, 0, 0, 0, 0]);
    set_resources(&mut state, 1, [0, 3, 5, 0, 0]);
    state
        .apply(&Action::Roll)
        .expect("pre-roll must accept Roll");
    state
        .apply(&Action::ResolveRoll { value: 7 })
        .expect("roll chance must accept deterministic seven");
    push_case(cases, format!("{players}p-robber-discard"), state.clone());

    while state.phase == Phase::Discard {
        let action = state
            .legal_actions()
            .into_iter()
            .next()
            .expect("discard phase must expose a discard");
        state
            .apply(&action)
            .expect("generated discard must be legal");
    }
    push_case(cases, format!("{players}p-robber-move"), state.clone());

    let move_action = state
        .legal_actions()
        .into_iter()
        .find(|action| {
            matches!(
                action,
                Action::MoveRobber {
                    victim: Some(_),
                    ..
                }
            )
        })
        .or_else(|| state.legal_actions().into_iter().next())
        .expect("robber phase must expose a move");
    state
        .apply(&move_action)
        .expect("generated robber move must be legal");
    if matches!(state.phase, Phase::ResolveSteal { .. }) {
        push_case(cases, format!("{players}p-robber-steal"), state.clone());
        let steal = state
            .legal_actions()
            .into_iter()
            .next()
            .expect("steal phase must expose a resource");
        state.apply(&steal).expect("generated steal must be legal");
    }
    push_case(cases, format!("{players}p-robber-return"), state);
}

fn append_port_and_maritime_cases(cases: &mut Vec<ExactParityCase>, board_seed: u64, players: u8) {
    let mut state = main_state(board_seed, players);
    let port_vertex = add_port_settlement(&mut state, 0);
    set_resources(&mut state, 0, [4, 4, 0, 0, 0]);
    push_case(
        cases,
        format!("{players}p-port-ratios-v{}", port_vertex),
        state.clone(),
    );
    let trade = state
        .legal_actions()
        .into_iter()
        .find(|action| matches!(action, Action::MaritimeTrade { .. }))
        .expect("port fixture must expose a maritime trade");
    state
        .apply(&trade)
        .expect("generated maritime trade must be legal");
    push_case(cases, format!("{players}p-maritime-trade"), state);
}

fn append_road_race_case(cases: &mut Vec<ExactParityCase>, board_seed: u64, players: u8) {
    let mut state = main_state(board_seed, players);
    set_resources(&mut state, 0, [10, 10, 0, 0, 0]);
    for _ in 0..8 {
        let action = state
            .legal_actions()
            .into_iter()
            .filter(|action| matches!(action, Action::BuildRoad { .. }))
            .filter_map(|action| {
                let mut next = state.clone();
                next.apply(&action).ok()?;
                Some((next.longest_road_length(0), action))
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, action)| action)
            .expect("road race fixture must expose another legal road");
        state
            .apply(&action)
            .expect("generated road race action must be legal");
    }
    push_case(cases, format!("{players}p-longest-road-race"), state);
}

fn append_trophy_case(cases: &mut Vec<ExactParityCase>, board_seed: u64, players: u8) {
    let mut state = main_state(board_seed, players);
    give_played_knights(&mut state, 0, 3);
    push_case(cases, format!("{players}p-trophy-race"), state);
}

fn give_played_knights(state: &mut GameState, player: u8, count: u8) {
    let index = DevCard::Knight.index();
    assert!(state.development_deck[index] >= count);
    state.development_deck[index] -= count;
    state.played_development[index] += count;
    state.players[player as usize].played_knights = count;
    state.players[player as usize].has_largest_army = true;
    state.players[player as usize].public_victory_points += 2;
    state.largest_army_holder = Some(player);
}

fn append_development_case(cases: &mut Vec<ExactParityCase>, board_seed: u64, players: u8) {
    let mut state = main_state(board_seed, players);
    give_development(&mut state, 0, [2, 1, 1, 1, 1]);
    state.players[0].bought_development[DevCard::Knight.index()] = 1;
    push_case(cases, format!("{players}p-development-cards"), state);
}

fn append_large_hand_case(cases: &mut Vec<ExactParityCase>, board_seed: u64, players: u8) {
    let mut state = main_state(board_seed, players);
    state.card_discard_limit = 7;
    set_resources(&mut state, 0, [4, 3, 2, 2, 1]);
    set_resources(&mut state, 1, [3, 3, 2, 2, 2]);
    push_case(
        cases,
        format!("{players}p-large-hands-discard-pressure"),
        state,
    );
}

fn append_no_player_trades_case(cases: &mut Vec<ExactParityCase>, board_seed: u64, players: u8) {
    let mut state = main_state(board_seed, players);
    state.player_trades_enabled = false;
    add_port_settlement(&mut state, 0);
    set_resources(&mut state, 0, [4, 4, 0, 0, 0]);
    assert!(
        state
            .legal_actions()
            .iter()
            .all(|action| !matches!(action, Action::OfferTrade { .. }))
    );
    assert!(
        state
            .legal_actions()
            .iter()
            .any(|action| matches!(action, Action::MaritimeTrade { .. }))
    );
    push_case(cases, format!("{players}p-no-player-trades"), state);
}

fn append_terminal_cases(cases: &mut Vec<ExactParityCase>, board_seed: u64, players: u8) {
    for winner in 0..players {
        let mut state = setup_state(board_seed, players);
        give_development(&mut state, winner, [0, 1, 0, 0, 0]);
        state.current_player = winner;
        state.victory_target = 3;
        state.phase = Phase::Finished;
        push_case(cases, format!("{players}p-terminal-winner-{winner}"), state);
    }
}

#[cfg(test)]
mod tests {
    use super::exact_parity_corpus;

    #[test]
    fn exact_parity_corpus_is_deterministic_and_valid() {
        let first = exact_parity_corpus();
        let second = exact_parity_corpus();
        assert!(!first.is_empty());
        assert_eq!(first.len(), second.len());
        assert!(first.iter().any(|case| case.state.board.num_players == 3));
        assert!(first.iter().any(|case| case.state.board.num_players == 4));
        assert!(first.iter().any(|case| case.state.is_terminal()));
        assert!(first.iter().any(|case| !case.state.is_terminal()));
        assert!(
            first
                .iter()
                .any(|case| { case.name.ends_with("-late-replay") && !case.state.is_terminal() })
        );
        for marker in [
            "robber-discard",
            "port-ratios",
            "maritime-trade",
            "longest-road-race",
            "trophy-race",
            "development-cards",
            "large-hands-discard-pressure",
            "no-player-trades",
        ] {
            assert!(
                first.iter().any(|case| case.name.contains(marker)),
                "missing corpus marker {marker}"
            );
        }
        assert!(first.iter().any(|case| {
            case.name.ends_with("-longest-road-race") && case.state.longest_road_holder.is_some()
        }));
        assert!(first.iter().any(|case| {
            case.name.ends_with("-trophy-race") && case.state.largest_army_holder.is_some()
        }));
        for (left, right) in first.iter().zip(second) {
            assert_eq!(left.name, right.name);
            assert_eq!(left.state.state_hash(), right.state.state_hash());
            left.state.validate().unwrap();
        }
    }
}
