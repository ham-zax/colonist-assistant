use colonist_catan_core::{Action, GameState, Phase};
use colonist_catan_search::{plan_current_turn, TurnPlanConfig};

fn main_state(players: u8) -> GameState {
    let mut state = GameState::standard(7, players);
    while matches!(state.phase, Phase::SetupSettlement | Phase::SetupRoad { .. }) {
        let action = state.legal_actions()[0].clone();
        state.apply(&action).expect("legal setup action");
    }
    state.phase = Phase::Main;
    state.current_player = 0;
    state.player_trades_enabled = false;
    state.domestic_trade_disabled = 1;
    state
}

#[test]
fn turn_plans_exclude_immediate_inverse_maritime_cycles() {
    let mut state = main_state(2);
    let old_hand = state.players[0].resources;
    let hand = [0, 0, 0, 3, 5];
    for resource in 0..5 {
        state.bank[resource] = state.bank[resource].saturating_add(old_hand[resource]);
        state.bank[resource] = state.bank[resource].saturating_sub(hand[resource]);
    }
    state.players[0].resources = hand;

    let plans = plan_current_turn(
        &state,
        TurnPlanConfig {
            maximum_nodes: 4_000,
            root_cap: 28,
            ..TurnPlanConfig::default()
        },
    );

    for plan in plans {
        for pair in plan.actions.windows(2) {
            let [
                Action::MaritimeTrade {
                    give: first_give,
                    receive: first_receive,
                    ..
                },
                Action::MaritimeTrade {
                    give: second_give,
                    receive: second_receive,
                    ..
                },
            ] = pair
            else {
                continue;
            };
            assert!(
                first_give != second_receive || first_receive != second_give,
                "planner retained an immediately reversing maritime cycle: {:?}",
                plan.actions
            );
        }
    }
}
