use colonist_catan_core::{Action, GameState, Phase};
use colonist_catan_search::{CudaSimEngine, CudaSimPackedState};

const MAIN_LANES: usize = 32;
const RICH_MAIN_LANES: usize = 16;
const INCOMPLETE_LANES: usize = 32;
const COMPLETE_LANES: usize = 16;
const DISABLED_LANES: usize = 16;
const STEPS: usize = 12;

fn finish_setup(state: &mut GameState) {
    while matches!(state.phase, Phase::SetupSettlement | Phase::SetupRoad { .. }) {
        let action = state
            .legal_actions()
            .into_iter()
            .next()
            .expect("setup state must have legal action");
        state.apply(&action).expect("setup action must apply");
    }
}

fn base_state(seed: u64, players: u8) -> GameState {
    let mut state = GameState::randomized_base_v1(seed, players);
    finish_setup(&mut state);
    state.phase = Phase::Main;
    state.player_trades_enabled = true;
    for player in &mut state.players {
        player.policy_profile = [51, 51, 51, 102, 0];
    }
    state
}

fn first_offer(state: &GameState) -> Action {
    state
        .legal_actions()
        .into_iter()
        .find(|action| matches!(action, Action::OfferTrade { .. }))
        .expect("trade-ready state must generate an offer")
}

fn make_main(seed: u64, players: u8) -> GameState {
    let mut state = base_state(seed, players);
    state.players[state.current_player as usize].resources = [1, 0, 0, 0, 0];
    for player in 0..players as usize {
        if player != state.current_player as usize {
            state.players[player].resources = [0, 3, 3, 3, 3];
        }
    }
    state
}

fn make_rich_main(seed: u64, players: u8) -> GameState {
    let mut state = base_state(seed, players);
    state.players[state.current_player as usize].resources = [0, 0, 0, 0, 4];
    for player in 0..players as usize {
        if player != state.current_player as usize {
            state.players[player].resources = [4; 5];
        }
    }
    state
}

fn make_incomplete(seed: u64, players: u8, disabled: bool) -> GameState {
    let mut state = base_state(seed, players);
    for player in &mut state.players {
        player.resources = [4; 5];
    }
    let offer = first_offer(&state);
    state.apply(&offer).expect("offer must apply");
    if disabled {
        state.player_trades_enabled = false;
    }
    state
}

fn make_complete(seed: u64, players: u8) -> GameState {
    let mut state = make_incomplete(seed, players, false);
    let accept = state
        .legal_actions()
        .into_iter()
        .find(|action| matches!(action, Action::RespondTrade { accept: true }))
        .expect("responder must be able to accept");
    state.apply(&accept).expect("accept must apply");
    while !state
        .legal_actions()
        .iter()
        .any(|action| matches!(action, Action::ConfirmTrade { .. }))
    {
        let reject = state
            .legal_actions()
            .into_iter()
            .find(|action| matches!(action, Action::RespondTrade { accept: false }))
            .expect("remaining recipient must be able to reject");
        state.apply(&reject).expect("reject must apply");
    }
    state
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut states = Vec::new();
    for lane in 0..MAIN_LANES {
        states.push(make_main(40_000 + lane as u64, if lane % 2 == 0 { 3 } else { 4 }));
    }
    for lane in 0..RICH_MAIN_LANES {
        states.push(make_rich_main(
            40_500 + lane as u64,
            if lane % 2 == 0 { 3 } else { 4 },
        ));
    }
    for lane in 0..INCOMPLETE_LANES {
        states.push(make_incomplete(
            41_000 + lane as u64,
            if lane % 2 == 0 { 3 } else { 4 },
            false,
        ));
    }
    for lane in 0..COMPLETE_LANES {
        states.push(make_complete(
            42_000 + lane as u64,
            if lane % 2 == 0 { 3 } else { 4 },
        ));
    }
    for lane in 0..DISABLED_LANES {
        states.push(make_incomplete(
            43_000 + lane as u64,
            if lane % 2 == 0 { 3 } else { 4 },
            true,
        ));
    }

    let mut engine = CudaSimEngine::new()?;
    engine.upload_states(&states)?;
    engine.seed_rollout_rng(0x77aa_1100_4455_99cc)?;

    let mut offers = 0usize;
    let mut high_hand_offers = 0usize;
    let mut bundled_offers = 0usize;
    let mut accepts = 0usize;
    let mut rejects = 0usize;
    let mut counters = 0usize;
    let mut confirms = 0usize;
    let mut cancels = 0usize;
    let mut transitions = 0usize;

    for step in 0..STEPS {
        engine.generate_rollout_actions()?;
        let actions = engine.download_generated_actions()?;
        for (lane, (state, action)) in states.iter().zip(actions.iter()).enumerate() {
            let legal = state.legal_actions();
            if !legal.contains(action) {
                let counters = legal
                    .iter()
                    .filter(|candidate| matches!(candidate, Action::CounterTrade { .. }))
                    .collect::<Vec<_>>();
                return Err(format!(
                    "GPU generated illegal trade-lane action step={step} lane={lane} phase={:?} action={action:?} trade={:?} cursor={} round={} hand={:?} legal_count={} counters={counters:?}",
                    state.phase,
                    state.trade,
                    state.trade_cursor,
                    state.trade_negotiation_round,
                    state.players[state.actor() as usize].resources,
                    legal.len(),
                )
                .into());
            }
            match action {
                Action::OfferTrade { give, receive, .. } => {
                    offers += 1;
                    if state.players[state.actor() as usize].resource_total() > 3 {
                        high_hand_offers += 1;
                    }
                    let give_total = give.iter().map(|value| *value as usize).sum::<usize>();
                    let receive_total = receive
                        .iter()
                        .map(|value| *value as usize)
                        .sum::<usize>();
                    if give_total > 1 || receive_total > 1 {
                        bundled_offers += 1;
                    }
                }
                Action::RespondTrade { accept: true } => accepts += 1,
                Action::RespondTrade { accept: false } => rejects += 1,
                Action::CounterTrade { .. } => counters += 1,
                Action::ConfirmTrade { .. } => confirms += 1,
                Action::CancelTrade => cancels += 1,
                _ => {}
            }
        }

        engine.apply_generated_actions()?;
        for (state, action) in states.iter_mut().zip(actions.iter()) {
            state.apply(action)?;
        }
        let gpu = engine.download_packed_states()?;
        for (lane, (state, actual)) in states.iter().zip(gpu.iter()).enumerate() {
            let expected = CudaSimPackedState::new(state)?;
            if &expected != actual {
                let first = expected
                    .words()
                    .iter()
                    .zip(actual.words())
                    .position(|(left, right)| left != right)
                    .unwrap_or(usize::MAX);
                return Err(format!(
                    "GPU trade-generated state mismatch step={step} lane={lane} field={first} cpu={} gpu={}",
                    expected.words().get(first).copied().unwrap_or_default(),
                    actual.words().get(first).copied().unwrap_or_default(),
                )
                .into());
            }
        }
        transitions += states.len();
    }

    if offers == 0
        || high_hand_offers == 0
        || bundled_offers == 0
        || rejects == 0
        || confirms + cancels == 0
    {
        return Err(format!(
            "trade generator did not exercise required families: offers={offers} high_hand_offers={high_hand_offers} bundled_offers={bundled_offers} accepts={accepts} rejects={rejects} counters={counters} confirms={confirms} cancels={cancels}"
        )
        .into());
    }

    println!(
        "{{\"kind\":\"cuda-resident-trade-generated-parity\",\"parity\":true,\"transitions\":{},\"offers\":{},\"highHandOffers\":{},\"bundledOffers\":{},\"accepts\":{},\"rejects\":{},\"counters\":{},\"confirms\":{},\"cancels\":{},\"gpu\":\"{}\"}}",
        transitions,
        offers,
        high_hand_offers,
        bundled_offers,
        accepts,
        rejects,
        counters,
        confirms,
        cancels,
        engine.device_identity().name,
    );
    Ok(())
}
