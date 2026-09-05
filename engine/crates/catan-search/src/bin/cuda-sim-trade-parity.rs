use colonist_catan_core::{Action, GameState, Phase};
use colonist_catan_search::{CudaSimEngine, CudaSimPackedState};

fn finish_setup(state: &mut GameState) {
    while matches!(state.phase, Phase::SetupSettlement | Phase::SetupRoad { .. }) {
        let action = state
            .legal_actions()
            .into_iter()
            .next()
            .expect("setup state must have a legal action");
        state.apply(&action).expect("setup action must apply");
    }
}

fn trade_ready_state(seed: u64, players: u8) -> GameState {
    let mut state = GameState::randomized_base_v1(seed, players);
    finish_setup(&mut state);
    state.phase = Phase::Main;
    state.player_trades_enabled = true;
    for player in &mut state.players {
        player.resources = [4; 5];
    }
    state
}

fn first_matching(
    state: &GameState,
    predicate: impl Fn(&Action) -> bool,
    label: &str,
) -> Result<Action, Box<dyn std::error::Error>> {
    state
        .legal_actions()
        .into_iter()
        .find(predicate)
        .ok_or_else(|| format!("missing legal {label} action in phase {:?}", state.phase).into())
}

fn apply_and_compare(
    engine: &mut CudaSimEngine,
    state: &mut GameState,
    action: Action,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    engine.apply_actions(std::slice::from_ref(&action))?;
    state.apply(&action)?;
    let expected = CudaSimPackedState::new(state)?;
    let actual = engine.download_packed_states()?.remove(0);
    if expected != actual {
        let first = expected
            .words()
            .iter()
            .zip(actual.words())
            .position(|(left, right)| left != right)
            .unwrap_or(usize::MAX);
        return Err(format!(
            "trade parity mismatch after {label} action {action:?}, field={first}, cpu={}, gpu={}",
            expected.words().get(first).copied().unwrap_or_default(),
            actual.words().get(first).copied().unwrap_or_default(),
        )
        .into());
    }
    Ok(())
}

fn offer(state: &GameState) -> Result<Action, Box<dyn std::error::Error>> {
    first_matching(state, |action| matches!(action, Action::OfferTrade { .. }), "offer")
}

fn complete_with_rejections(
    engine: &mut CudaSimEngine,
    state: &mut GameState,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let legal = state.legal_actions();
        if legal.iter().any(|action| matches!(action, Action::CancelTrade)) {
            return Ok(());
        }
        let reject = legal
            .into_iter()
            .find(|action| matches!(action, Action::RespondTrade { accept: false }))
            .ok_or_else(|| format!("{label}: missing rejection while trade incomplete"))?;
        apply_and_compare(engine, state, reject, label)?;
    }
}

fn rejection_and_cancel(engine: &mut CudaSimEngine) -> Result<usize, Box<dyn std::error::Error>> {
    let mut state = trade_ready_state(31_001, 4);
    engine.upload_states(std::slice::from_ref(&state))?;
    let action = offer(&state)?;
    apply_and_compare(engine, &mut state, action, "offer-reject")?;
    complete_with_rejections(engine, &mut state, "reject")?;
    let cancel = first_matching(&state, |action| matches!(action, Action::CancelTrade), "cancel")?;
    apply_and_compare(engine, &mut state, cancel, "cancel")?;
    if state.last_rejected_trade.is_none() || state.trade.is_some() || state.phase != Phase::Main {
        return Err("cancel did not preserve the CPU rejection contract".into());
    }
    apply_and_compare(engine, &mut state, Action::EndTurn, "end-turn-after-cancel")?;
    if state.last_rejected_trade.is_some() {
        return Err("end turn did not clear last rejected trade".into());
    }
    Ok(1)
}

fn accept_and_confirm(engine: &mut CudaSimEngine) -> Result<usize, Box<dyn std::error::Error>> {
    let mut state = trade_ready_state(31_101, 4);
    engine.upload_states(std::slice::from_ref(&state))?;
    let action = offer(&state)?;
    apply_and_compare(engine, &mut state, action, "offer-confirm")?;
    let accept = first_matching(
        &state,
        |action| matches!(action, Action::RespondTrade { accept: true }),
        "accept",
    )?;
    apply_and_compare(engine, &mut state, accept, "accept")?;
    while !state
        .legal_actions()
        .iter()
        .any(|action| matches!(action, Action::ConfirmTrade { .. }))
    {
        let reject = first_matching(
            &state,
            |action| matches!(action, Action::RespondTrade { accept: false }),
            "post-accept reject",
        )?;
        apply_and_compare(engine, &mut state, reject, "post-accept-reject")?;
    }
    let confirm = first_matching(
        &state,
        |action| matches!(action, Action::ConfirmTrade { .. }),
        "confirm",
    )?;
    apply_and_compare(engine, &mut state, confirm, "confirm")?;
    if state.trade.is_some() || state.last_rejected_trade.is_some() || state.phase != Phase::Main {
        return Err("confirm did not close trade cleanly".into());
    }
    Ok(1)
}

fn counter_and_confirm(engine: &mut CudaSimEngine) -> Result<usize, Box<dyn std::error::Error>> {
    let mut state = trade_ready_state(31_201, 3);
    engine.upload_states(std::slice::from_ref(&state))?;
    let action = offer(&state)?;
    apply_and_compare(engine, &mut state, action, "offer-counter")?;
    let counter = first_matching(
        &state,
        |action| matches!(action, Action::CounterTrade { .. }),
        "counter",
    )?;
    apply_and_compare(engine, &mut state, counter, "counter")?;
    if state.trade_negotiation_round != 1 {
        return Err("counter did not advance negotiation round".into());
    }
    let accept = first_matching(
        &state,
        |action| matches!(action, Action::RespondTrade { accept: true }),
        "counter accept",
    )?;
    apply_and_compare(engine, &mut state, accept, "counter-accept")?;
    let confirm = first_matching(
        &state,
        |action| matches!(action, Action::ConfirmTrade { .. }),
        "counter confirm",
    )?;
    apply_and_compare(engine, &mut state, confirm, "counter-confirm")?;
    Ok(1)
}

fn disabled_cleanup(engine: &mut CudaSimEngine) -> Result<usize, Box<dyn std::error::Error>> {
    let mut state = trade_ready_state(31_301, 4);
    engine.upload_states(std::slice::from_ref(&state))?;
    let action = offer(&state)?;
    apply_and_compare(engine, &mut state, action, "offer-before-disable")?;
    state.player_trades_enabled = false;
    engine.upload_states(std::slice::from_ref(&state))?;
    complete_with_rejections(engine, &mut state, "disabled-reject")?;
    let cancel = first_matching(&state, |action| matches!(action, Action::CancelTrade), "disabled cancel")?;
    apply_and_compare(engine, &mut state, cancel, "disabled-cancel")?;
    Ok(1)
}

fn root_actor_parity(engine: &mut CudaSimEngine) -> Result<usize, Box<dyn std::error::Error>> {
    let mut response = trade_ready_state(31_401, 3);
    response.players[0].public_victory_points = 2;
    response.players[1].public_victory_points = 4;
    response.players[2].public_victory_points = 3;
    let response_offer = offer(&response)?;
    response.apply(&response_offer)?;
    let response_action = first_matching(
        &response,
        |action| matches!(action, Action::RespondTrade { accept: false }),
        "root response",
    )?;

    let mut counter_complete = trade_ready_state(31_402, 3);
    counter_complete.players[0].public_victory_points = 2;
    counter_complete.players[1].public_victory_points = 4;
    counter_complete.players[2].public_victory_points = 3;
    let counter_offer = offer(&counter_complete)?;
    counter_complete.apply(&counter_offer)?;
    let counter = first_matching(
        &counter_complete,
        |action| matches!(action, Action::CounterTrade { .. }),
        "root counter",
    )?;
    counter_complete.apply(&counter)?;
    let accept = first_matching(
        &counter_complete,
        |action| matches!(action, Action::RespondTrade { accept: true }),
        "root counter accept",
    )?;
    counter_complete.apply(&accept)?;
    let confirm = first_matching(
        &counter_complete,
        |action| matches!(action, Action::ConfirmTrade { .. }),
        "root counter confirm",
    )?;

    let states = vec![response, counter_complete];
    let roots = vec![vec![response_action], vec![confirm]];
    engine.upload_states(&states)?;
    let result = engine.search_root_actions(&roots, 4, 0, 0x7788_99aa_bbcc_ddee)?;
    for (index, (state, row)) in states.iter().zip(result.rows.iter()).enumerate() {
        let actor = state.actor() as usize;
        let expected = state.players[actor].victory_points() as f32;
        let observed = row
            .first()
            .ok_or_else(|| format!("missing root stats for trade actor scenario {index}"))?
            .mean_victory_points;
        if observed != expected {
            return Err(format!(
                "trade root actor mismatch scenario={index} actor={actor} expected={expected} observed={observed}"
            )
            .into());
        }
    }
    Ok(states.len())
}

#[allow(clippy::type_complexity)]
fn generated_trade_policy_parity(
    engine: &mut CudaSimEngine,
) -> Result<(usize, usize, usize, usize, usize, usize), Box<dyn std::error::Error>> {
    const LANES: usize = 32;
    const STEPS: usize = 96;
    let mut states = (0..LANES)
        .map(|lane| {
            let mut state = trade_ready_state(32_000 + lane as u64 * 17, 3);
            state.players[0].resources = [1, 0, 0, 0, 0];
            state.players[1].resources = [0, 1, 0, 0, 0];
            state.players[2].resources = [0, 0, 1, 0, 0];
            state
        })
        .collect::<Vec<_>>();
    engine.upload_states(&states)?;
    engine.seed_rollout_rng(0xa55a_1234_5566_7788)?;

    let mut offers = 0usize;
    let mut responses = 0usize;
    let mut counters = 0usize;
    let mut confirmations = 0usize;
    let mut cancellations = 0usize;
    let mut transitions = 0usize;

    for step in 0..STEPS {
        if states.iter().any(GameState::is_terminal) {
            break;
        }
        engine.generate_rollout_actions()?;
        let actions = engine.download_generated_actions()?;
        for (lane, (state, action)) in states.iter_mut().zip(actions.iter()).enumerate() {
            match action {
                Action::OfferTrade { .. } => offers += 1,
                Action::RespondTrade { .. } => responses += 1,
                Action::CounterTrade { .. } => counters += 1,
                Action::ConfirmTrade { .. } => confirmations += 1,
                Action::CancelTrade => cancellations += 1,
                _ => {}
            }
            state.apply(action).map_err(|error| {
                format!(
                    "CPU rejected GPU-generated trade action step={step} lane={lane} phase={:?} action={action:?}: {error}",
                    state.phase
                )
            })?;
        }
        engine.apply_generated_actions()?;
        transitions += states.len();
        let gpu = engine.download_packed_states()?;
        for (lane, (cpu, gpu)) in states.iter().zip(gpu.iter()).enumerate() {
            let expected = CudaSimPackedState::new(cpu)?;
            if &expected != gpu {
                let first = expected
                    .words()
                    .iter()
                    .zip(gpu.words())
                    .position(|(left, right)| left != right)
                    .unwrap_or(usize::MAX);
                return Err(format!(
                    "generated trade parity mismatch step={step} lane={lane} field={first}: cpu={} gpu={}",
                    expected.words().get(first).copied().unwrap_or_default(),
                    gpu.words().get(first).copied().unwrap_or_default(),
                )
                .into());
            }
        }
    }

    if offers == 0 || responses == 0 || counters == 0 || confirmations + cancellations == 0 {
        return Err(format!(
            "generated trade policy did not exercise its lifecycle: offers={offers} responses={responses} counters={counters} confirmations={confirmations} cancellations={cancellations}"
        )
        .into());
    }
    Ok((
        transitions,
        offers,
        responses,
        counters,
        confirmations,
        cancellations,
    ))
}


fn per_seat_disabled_policy_parity(
    engine: &mut CudaSimEngine,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut state = trade_ready_state(31_501, 3);
    engine.upload_states(std::slice::from_ref(&state))?;
    let action = offer(&state)?;
    apply_and_compare(engine, &mut state, action, "per-seat-disable-offer")?;

    let responder = state.trade_cursor;
    state.domestic_trade_disabled |= 1 << responder;
    engine.upload_states(std::slice::from_ref(&state))?;
    engine.seed_rollout_rng(0x51a5_0001)?;
    engine.generate_rollout_actions()?;
    let generated = engine.download_generated_actions()?.remove(0);
    if generated != (Action::RespondTrade { accept: false }) {
        return Err(format!(
            "disabled responder generated {generated:?} instead of mandatory rejection"
        )
        .into());
    }
    state.apply(&generated)?;
    engine.apply_generated_actions()?;
    let expected = CudaSimPackedState::new(&state)?;
    let actual = engine.download_packed_states()?.remove(0);
    if expected != actual {
        return Err("per-seat disabled responder diverged after rejection".into());
    }

    while !state
        .legal_actions()
        .iter()
        .any(|candidate| matches!(candidate, Action::CancelTrade))
    {
        let reject = first_matching(
            &state,
            |candidate| matches!(candidate, Action::RespondTrade { accept: false }),
            "per-seat remaining reject",
        )?;
        engine.upload_states(std::slice::from_ref(&state))?;
        apply_and_compare(engine, &mut state, reject, "per-seat remaining reject")?;
    }
    let creator = state
        .trade
        .ok_or("completed trade disappeared before creator-disable check")?
        .creator;
    state.domestic_trade_disabled |= 1 << creator;
    engine.upload_states(std::slice::from_ref(&state))?;
    engine.seed_rollout_rng(0x51a5_0002)?;
    engine.generate_rollout_actions()?;
    let generated = engine.download_generated_actions()?.remove(0);
    if generated != Action::CancelTrade {
        return Err(format!(
            "disabled trade creator generated {generated:?} instead of mandatory cancellation"
        )
        .into());
    }
    state.apply(&generated)?;
    engine.apply_generated_actions()?;
    let expected = CudaSimPackedState::new(&state)?;
    let actual = engine.download_packed_states()?.remove(0);
    if expected != actual {
        return Err("per-seat disabled creator diverged after cancellation".into());
    }
    Ok(2)
}

fn high_hand_offer_policy_parity(
    engine: &mut CudaSimEngine,
) -> Result<usize, Box<dyn std::error::Error>> {
    const LANES: usize = 32;
    const ATTEMPTS: usize = 96;
    let states = (0..LANES)
        .map(|lane| {
            let mut state = trade_ready_state(33_000 + lane as u64 * 19, if lane % 2 == 0 { 3 } else { 4 });
            for player in &mut state.players {
                player.resources = [0; 5];
                player.policy_profile = [0, 0, 0, 102, 0];
            }
            state.bank = [19; 5];
            let actor = state.current_player as usize;
            state.players[actor].resources = [4; 5];
            state.bank = [15; 5];
            state.validate().expect("high-hand trade oracle state must validate");
            state
        })
        .collect::<Vec<_>>();
    let admitted = states
        .iter()
        .map(|state| {
            let offers = state
                .legal_actions()
                .into_iter()
                .filter(|action| matches!(action, Action::OfferTrade { .. }))
                .collect::<Vec<_>>();
            if offers.is_empty() {
                Err("high-hand oracle state has no domestic offer".to_string())
            } else {
                Ok(offers)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    engine.upload_states(&states)?;
    engine.seed_rollout_rng(0xfeed_3300_1234_5678)?;
    let mut observed = 0usize;
    let mut rich = 0usize;
    for _ in 0..ATTEMPTS {
        engine.generate_rollout_actions()?;
        let actions = engine.download_generated_actions()?;
        for (lane, action) in actions.iter().enumerate() {
            let Action::OfferTrade { give, receive, .. } = action else {
                continue;
            };
            observed += 1;
            if !admitted[lane].contains(action) {
                return Err(format!(
                    "GPU high-hand offer is outside the CPU admitted top-96 set lane={lane}: gpu={action:?} admitted_count={}",
                    admitted[lane].len()
                )
                .into());
            }
            let give_total = give.iter().copied().sum::<u8>();
            let receive_total = receive.iter().copied().sum::<u8>();
            let give_kinds = give.iter().filter(|amount| **amount > 0).count();
            let receive_kinds = receive.iter().filter(|amount| **amount > 0).count();
            if give_total > 1 || receive_total > 1 || give_kinds > 1 || receive_kinds > 1 {
                rich += 1;
            }
        }
        if observed >= LANES && rich > 0 {
            break;
        }
    }
    if observed == 0 || rich == 0 {
        return Err(format!(
            "high-hand GPU offer oracle lacked coverage: observed={observed} rich={rich}"
        )
        .into());
    }
    Ok(observed)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = CudaSimEngine::new()?;
    let scenarios = rejection_and_cancel(&mut engine)?
        + accept_and_confirm(&mut engine)?
        + counter_and_confirm(&mut engine)?
        + disabled_cleanup(&mut engine)?
        + root_actor_parity(&mut engine)?
        + per_seat_disabled_policy_parity(&mut engine)?;
    let high_hand_offers = high_hand_offer_policy_parity(&mut engine)?;
    let (generated, offers, responses, counters, confirmations, cancellations) =
        generated_trade_policy_parity(&mut engine)?;
    println!(
        "{{\"kind\":\"cuda-resident-trade-parity\",\"parity\":true,\"scenarios\":{},\"highHandOffers\":{},\"generatedTransitions\":{},\"offers\":{},\"responses\":{},\"counters\":{},\"confirmations\":{},\"cancellations\":{},\"gpu\":\"{}\"}}",
        scenarios,
        high_hand_offers,
        generated,
        offers,
        responses,
        counters,
        confirmations,
        cancellations,
        engine.device_identity().name,
    );
    Ok(())
}
