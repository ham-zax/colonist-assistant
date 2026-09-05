use colonist_catan_core::{Action, GameState, Phase};
use colonist_catan_search::{road_intent, shared_root_candidates};

fn main_state() -> GameState {
    let mut state = GameState::standard(31, 2);
    while matches!(state.phase, Phase::SetupSettlement | Phase::SetupRoad { .. }) {
        let action = state.legal_actions()[0].clone();
        state.apply(&action).unwrap();
    }
    state.phase = Phase::Main;
    state.current_player = 0;
    for resource in 0..5 {
        state.bank[resource] += state.players[0].resources[resource];
        state.players[0].resources[resource] = 6;
        state.bank[resource] -= 6;
    }
    state.validate().unwrap();
    state
}

#[test]
fn legal_road_intents_have_replayable_targets_and_finite_scores() {
    let state = main_state();
    let mut count = 0;
    for action in state.legal_actions() {
        let Action::BuildRoad { edge } = action else { continue; };
        let intent = road_intent(&state, edge, 0);
        assert!(intent.ordering_score().is_finite());
        if let Some(target) = intent.target_vertex {
            let mut next = state.clone();
            next.apply(&action).unwrap();
            assert!(next.buildings[usize::from(target)].is_none());
            assert!(next.board.vertices[usize::from(target)].adjacent_vertices.iter()
                .all(|vertex| next.buildings[usize::from(*vertex)].is_none()));
            assert!(intent.roads_remaining <= next.players[0].roads_left);
            count += 1;
        }
    }
    assert!(count > 0, "the fixture must expose a real expansion route");
}

#[test]
fn best_authoritative_road_intent_survives_root_admission() {
    let state = main_state();
    let legal = state.legal_actions();
    let best_score = legal.iter().filter_map(|action| match action {
        Action::BuildRoad { edge } => Some(road_intent(&state, *edge, 0).ordering_score()),
        _ => None,
    }).reduce(f32::max).unwrap();
    let roots = shared_root_candidates(&state, 0, &legal, 12);
    assert!(roots.iter().any(|(action, _)| match action {
        Action::BuildRoad { edge } => (road_intent(&state, *edge, 0).ordering_score() - best_score).abs() < 1e-6,
        _ => false,
    }), "best route was pruned: {roots:?}");
}
