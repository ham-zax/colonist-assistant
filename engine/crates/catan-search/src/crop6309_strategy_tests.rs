//! crop6309 strategy regressions on recorded positions.
//!
//! Uses the fixture lane's validated geometry, pieces, hands, and belief
//! worlds (2-player, player trades disabled, M0 dice, inferred target 15).
//! Dice and victory target are labeled approximations, not exact replay.

use colonist_catan_core::{Action, Building, GameState, Port, Resource, ResourceHand};

use crate::BeliefParticle;

fn single_particle(state: colonist_catan_core::GameState) -> Vec<BeliefParticle> {
    vec![BeliefParticle { state, weight: 1.0 }]
}

fn endturn_ranked_and_retained(report: &crate::depth::BeliefDepthResult) -> bool {
    report
        .provenance
        .ranked_roots
        .iter()
        .any(|root| root.action == Action::EndTurn)
        && !report
            .provenance
            .pruned_roots
            .iter()
            .any(|root| root.action == Action::EndTurn)
}

fn port_build_etas(state: &GameState, vertex: u8, ratios: &ResourceHand) -> [f32; 4] {
    let mut prospective = state.clone();
    assert!(prospective.buildings[vertex as usize].is_none());
    prospective.buildings[vertex as usize] = Some(Building::Settlement(0));
    let production = crate::production_pips(&prospective, 0);
    [
        colonist_catan_core::ROAD_COST,
        colonist_catan_core::SETTLEMENT_COST,
        colonist_catan_core::CITY_COST,
        colonist_catan_core::DEVELOPMENT_COST,
    ]
    .map(|cost| {
        crate::economy::build_eta_rolls(&production, &state.players[0].resources, ratios, &cost)
    })
}

#[test]
fn d34_geometry_matches_recorded_opening_and_targets() {
    let state = crate::crop6309_fixture::state("D34");
    assert_eq!(
        crate::production_pips(&state, 0),
        [3.0, 5.0, 12.0, 0.0, 0.0]
    );
    let target = crate::crop6309_fixture::vertex("v:-1,2,0");
    let legal = state.legal_actions();
    assert!(legal.contains(&Action::BuildSettlement { vertex: target }));
    assert!(legal.contains(&Action::EndTurn));
    assert_eq!(state.trade_ratios(0), [4; 5]);
}

#[test]
fn earlier_productive_expansion_beats_pass_in_static_utility() {
    // Inferred target 15 is not exported; the build stays net positive at
    // 10/14/15 because both seats remain far from closing at D34.
    for (decision, victory_target) in [
        ("D34", 10),
        ("D34", 14),
        ("D34", 15),
        ("D36", 15),
        ("D38", 15),
        ("D40", 15),
        ("D42", 15),
    ] {
        let before = crate::crop6309_fixture::state_with_target(decision, victory_target);
        let target = crate::crop6309_fixture::vertex("v:-1,2,0");
        let mut after = before.clone();
        after
            .apply(&Action::BuildSettlement { vertex: target })
            .unwrap();
        let before_breakdown = crate::strategic_utility_breakdown(&before, 0);
        let after_breakdown = crate::strategic_utility_breakdown(&after, 0);
        let before_total = before_breakdown.total;
        let after_total = after_breakdown.total;
        eprintln!(
            "{decision} target={victory_target} static before={before_total:.4} after={after_total:.4} production={:.4}->{:.4} tempo={:.4}->{:.4} expansion={:.4}->{:.4}",
            before_breakdown.weighted_production,
            after_breakdown.weighted_production,
            before_breakdown.build_tempo,
            after_breakdown.build_tempo,
            before_breakdown.expansion_best,
            after_breakdown.expansion_best,
        );
        assert!(
            after_total > before_total,
            "affordable uncontested ore expansion adding a missing resource must be net positive at target {victory_target}: before={before_total:.4} after={after_total:.4}",
        );
    }
}

#[test]
fn earlier_productive_expansion_beats_pass_at_live_and_reference_work() {
    let target = crate::crop6309_fixture::vertex("v:-1,2,0");
    // A first-world diagnostic separates evaluator changes from uncertainty
    // over private opponent cards; it is not a full-belief replay.
    for decision in ["D34", "D36", "D38", "D40", "D42"] {
        let particles = single_particle(crate::crop6309_fixture::state(decision));
        let live =
            crate::depth::search_weighted_belief_maxn_bounded(&particles, 3, 12, 1_500).unwrap();
        let reference =
            crate::depth::search_weighted_belief_maxn_bounded(&particles, 5, 16, 12_000).unwrap();
        for (label, report) in [("live", &live), ("reference", &reference)] {
            let build = report
                .actions
                .iter()
                .find(|candidate| candidate.action == Action::BuildSettlement { vertex: target })
                .map(|candidate| candidate.value[0]);
            let end = report
                .actions
                .iter()
                .find(|candidate| candidate.action == Action::EndTurn)
                .map(|candidate| candidate.value[0]);
            eprintln!(
                "{decision} {label}: chosen={:?} build={build:?} end={end:?}",
                report.chosen
            );
            assert!(
                build.unwrap() > end.unwrap(),
                "{decision} {label} must prefer productive settlement to passing"
            );
            if decision == "D34" {
                assert_eq!(
                    report.chosen,
                    Some(Action::BuildSettlement { vertex: target })
                );
            }
            assert!(endturn_ranked_and_retained(report));
        }
    }
}

#[test]
fn grain_port_with_zero_grain_production_gets_no_conversion_credit() {
    use std::sync::Arc;

    // Opening portfolio with no grain production anywhere; v:1,1,1 adds only
    // lumber, and a grain 2:1 discounts selling grain the player never makes.
    let mut state = crate::crop6309_fixture::state("D34");
    let candidate = crate::crop6309_fixture::vertex("v:1,1,1");
    assert_eq!(
        state.board.vertices[candidate as usize].port,
        Some(Port::Resource(Resource::Grain))
    );
    let with_port = crate::eval::vertex_value(&state, candidate, 0);
    let before_etas = port_build_etas(&state, candidate, &[4; 5]);
    let after_etas = port_build_etas(&state, candidate, &[4, 4, 4, 2, 4]);
    assert_eq!(before_etas, after_etas);
    Arc::make_mut(&mut state.board).vertices[candidate as usize].port = None;
    let plain = crate::eval::vertex_value(&state, candidate, 0);
    assert!(
        (with_port - plain).abs() < 1e-5,
        "useless grain port must not inflate the site: with={with_port:.4} plain={plain:.4}",
    );
}

#[test]
fn matching_ore_port_with_ore_surplus_gets_positive_but_bounded_credit() {
    use std::sync::Arc;

    // After the third settlement P0 produces ore; the ore 2:1 converts a real
    // surplus and must earn a small positive option, never a blind jackpot.
    let mut state = crate::crop6309_fixture::state("D34");
    let built = crate::crop6309_fixture::vertex("v:-1,2,0");
    state
        .apply(&Action::BuildSettlement { vertex: built })
        .unwrap();
    let candidate = crate::crop6309_fixture::vertex("v:-1,3,0");
    assert_eq!(
        state.board.vertices[candidate as usize].port,
        Some(Port::Resource(Resource::Ore))
    );
    let with_port = crate::eval::vertex_value(&state, candidate, 0);
    let before_etas = port_build_etas(&state, candidate, &[4; 5]);
    let after_etas = port_build_etas(&state, candidate, &[4, 4, 4, 4, 2]);
    assert!(after_etas[1] < before_etas[1]);
    Arc::make_mut(&mut state.board).vertices[candidate as usize].port = None;
    let plain = crate::eval::vertex_value(&state, candidate, 0);
    let gain = with_port - plain;
    eprintln!("D34 after settlement ore port ETA={before_etas:?}->{after_etas:?} credit={gain:.6}");
    assert!(
        gain > 0.0 && gain < 0.5,
        "working ore port must earn bounded credit: gain={gain:.4}",
    );
}

#[test]
fn ore_port_route_is_feasible_and_advances_whole_builds_at_d63_and_d66() {
    use std::sync::Arc;

    let candidate = crate::crop6309_fixture::vertex("v:-1,3,0");
    let approach = crate::crop6309_fixture::edge("e:0,2,1");
    for decision in ["D63", "D66"] {
        let mut state = crate::crop6309_fixture::state(decision);
        let own_distances = crate::eval::road_distances(&state, 0);
        let rival_distances = crate::eval::road_distances(&state, 1);
        assert_eq!(own_distances[candidate as usize], 1);
        assert!(
            state
                .legal_actions()
                .contains(&Action::BuildRoad { edge: approach })
        );
        let before_etas = port_build_etas(&state, candidate, &[4; 5]);
        let after_etas = port_build_etas(&state, candidate, &[4, 4, 4, 4, 2]);
        assert!(after_etas[1] < before_etas[1]);
        let with_port = crate::eval::vertex_value(&state, candidate, 0);
        Arc::make_mut(&mut state.board).vertices[candidate as usize].port = None;
        let gain = with_port - crate::eval::vertex_value(&state, candidate, 0);
        assert!(gain > 0.0 && gain < 0.5);
        let route_eta = crate::economy::build_eta_rolls(
            &crate::production_pips(&state, 0),
            &state.players[0].resources,
            &state.trade_ratios(0),
            &[2, 2, 1, 1, 0],
        );
        eprintln!(
            "{decision} ore port ETA={before_etas:?}->{after_etas:?} credit={gain:.6} paid road+settlement ETA={route_eta:.3} own/rival roads=1/{}",
            rival_distances[candidate as usize]
        );
        state.apply(&Action::BuildRoad { edge: approach }).unwrap();
        assert_eq!(
            crate::eval::road_distances(&state, 0)[candidate as usize],
            0
        );
        assert!(state.buildings[candidate as usize].is_none());
        assert!(
            state.board.vertices[candidate as usize]
                .adjacent_vertices
                .iter()
                .all(|neighbor| state.buildings[*neighbor as usize].is_none())
        );
    }
}

#[test]
fn concentrated_wool_port_engine_improves_city_access_over_bank() {
    // Wool-concentrated opening production closes a city bottleneck faster
    // with its matching 2:1 than with 4:1 bank conversion alone.
    let production = [3.0, 5.0, 12.0, 0.0, 0.0];
    let bank_eta = crate::economy::build_eta_rolls(
        &production,
        &[0; 5],
        &[4; 5],
        &colonist_catan_core::CITY_COST,
    );
    let wool_port_eta = crate::economy::build_eta_rolls(
        &production,
        &[0; 5],
        &[4, 4, 2, 4, 4],
        &colonist_catan_core::CITY_COST,
    );
    assert!(
        wool_port_eta < bank_eta,
        "matching wool 2:1 must shorten city funding: bank={bank_eta:.1} port={wool_port_eta:.1}",
    );
}

#[test]
fn sea8653_bank_cycle_legality_preserved() {
    // Existing sea8653 trade-cycle and inventory rules must survive the
    // crop6309 economy fix unchanged.
    let mut traded = crate::sea8653_fixture::state("D221");
    let mut direct = traded.clone();
    assert_eq!(traded.players[0].resources, [2, 0, 1, 3, 1]);
    assert!(direct.legal_actions().contains(&Action::BuyDevelopment));
    direct.apply(&Action::BuyDevelopment).unwrap();
    traded
        .apply(&Action::MaritimeTrade {
            give: Resource::Grain,
            receive: Resource::Lumber,
            ratio: 3,
        })
        .unwrap();
    assert!(!traded.legal_actions().contains(&Action::BuyDevelopment));
    traded
        .apply(&Action::MaritimeTrade {
            give: Resource::Lumber,
            receive: Resource::Grain,
            ratio: 3,
        })
        .unwrap();
    traded.apply(&Action::BuyDevelopment).unwrap();
    assert_eq!(direct.players[0].resources, [2, 0, 0, 2, 0]);
    assert_eq!(traded.players[0].resources, [0; 5]);
}

#[cfg(feature = "cuda-exact")]
#[test]
#[ignore = "requires a CUDA device and NVRTC"]
fn crop6309_economic_evaluator_matches_cuda() {
    use std::sync::Arc;

    let mut states = Vec::new();
    let settlement = crate::crop6309_fixture::vertex("v:-1,2,0");
    for decision in ["D34", "D36", "D38", "D40", "D42"] {
        for target in [10, 14, 15] {
            let mut state = crate::crop6309_fixture::state_with_target(decision, target);
            states.push(state.clone());
            state
                .apply(&Action::BuildSettlement { vertex: settlement })
                .unwrap();
            states.push(state);
        }
    }
    for decision in ["D63", "D66"] {
        let candidate = crate::crop6309_fixture::vertex("v:-1,3,0");
        for port in [
            None,
            Some(Port::Generic),
            Some(Port::Resource(Resource::Ore)),
            Some(Port::Resource(Resource::Grain)),
        ] {
            let mut state = crate::crop6309_fixture::state(decision);
            Arc::make_mut(&mut state.board).vertices[candidate as usize].port = port;
            states.push(state);
        }
    }
    let mut evaluator = crate::cuda_exact::CudaExactEvaluator::new_on_device(0).unwrap();
    let actual = evaluator.evaluate_batch(&states).unwrap();
    for (index, (state, actual)) in states.iter().zip(actual).enumerate() {
        let expected = crate::eval::evaluate(state);
        for player in 0..state.board.num_players as usize {
            assert!(
                (actual[player] - expected[player]).abs() < 2e-5,
                "crop6309 state {index} CPU/GPU mismatch for player {player}: {} vs {}",
                expected[player],
                actual[player]
            );
        }
    }
}
