use super::*;
use colonist_catan_core::Phase;
use colonist_catan_search::{
    belief_road_cut_continuation_assessment, compute_spatial_root_impacts,
};

const ACTOR: u8 = 0;
const ATTACKER: u8 = 1;
const D95_CUT_VERTEX: u8 = 30;
const D95_BRIDGE_EDGE: u8 = 36;
const D95_APPROACH_EDGE: u8 = 37;

fn d95_corridor(attacker_hand: [u8; 5]) -> GameState {
    let mut state = GameState::randomized_base_v1(1, 4);
    state.phase = Phase::Main;
    state.current_player = ACTOR;
    state.roads.fill(None);
    state.buildings.fill(None);

    // Two actor branches. The D95 bridge joins them into a Longest Road that
    // can then be split by a settlement at vertex 30.
    for edge in [21_u8, 38, 52, 49] {
        state.roads[edge as usize] = Some(ACTOR);
    }
    // Opponent feeder ending at vertex 25. Edge 37 is the one-road approach
    // from that feeder to the exposed D95 cut vertex.
    state.roads[35] = Some(ATTACKER);

    state.players[ACTOR as usize].resources = [1, 1, 0, 0, 0];
    state.players[ATTACKER as usize].resources = attacker_hand;
    state.bank = [19; 5];
    state.update_longest_road();
    state
}

fn ranked_root(action: Action, prior: f32) -> RankedGpuRoot {
    RankedGpuRoot {
        action,
        prior,
        legal_weight: 1.0,
    }
}

fn aggregate_root_for_ordering(
    action: Action,
    terminal_outcome: f32,
    terminal_rate: f32,
    terminal_variance: f32,
    victory_margin: f32,
    victory_margin_variance: f32,
) -> AggregatedRoot {
    AggregatedRoot {
        action,
        prior: 0.5,
        availability: 1,
        legal_weight: 1.0,
        samples: 100,
        errors: 0,
        terminal_outcome,
        terminal_rate,
        terminal_variance,
        victory_margin,
        victory_margin_variance,
        strategic_margin: victory_margin,
        strategic_margin_variance: victory_margin_variance,
        mean_turn: 50.0,
        candidate_vp: 0.0,
        candidate_vp_variance: 0.0,
        opponent_vp: 0.0,
        opponent_vp_variance: 0.0,
    }
}

#[test]
fn d95_bridge_exposes_cut_and_causal_protector_can_win_deeper_arbitration() {
    let bridge = Action::BuildRoad {
        edge: D95_BRIDGE_EDGE,
    };
    let defense = Action::BuildRoad {
        edge: D95_APPROACH_EDGE,
    };

    let direct_world = d95_corridor([2, 2, 1, 1, 0]);
    let impact_report =
        compute_spatial_root_impacts(&direct_world, ACTOR, std::slice::from_ref(&bridge));
    let impact = impact_report
        .actions
        .iter()
        .find(|impact| impact.action == bridge)
        .expect("D95 bridge must have root-impact evidence");
    let cut = impact
        .introduced_road_fragility
        .critical_vertices
        .iter()
        .find(|cut| cut.vertex == D95_CUT_VERTEX)
        .expect("D95 bridge must expose the known cut vertex");

    assert!(cut.additional_road_loss > 0);
    assert!(cut.award_loss);
    assert!(cut.award_loss_introduced);
    assert!(impact.introduced_road_fragility.award_vp_exposure > 0);

    let exposed_vertices = impact
        .introduced_road_fragility
        .critical_vertices
        .iter()
        .map(|cut| cut.vertex)
        .collect::<Vec<_>>();
    let conversion_world = d95_corridor([10, 0, 1, 1, 0]);
    let continuation = belief_road_cut_continuation_assessment(
        [(&direct_world, 0.4_f32), (&conversion_world, 0.6_f32)],
        ACTOR,
        &bridge,
        &exposed_vertices,
    );
    let d95_continuation = continuation
        .continuations
        .iter()
        .find(|evidence| evidence.vertex == D95_CUT_VERTEX && evidence.opponent == ATTACKER)
        .expect("belief search must prove the D95 road -> settlement cut");

    assert!(continuation.award_loss_posterior > 0.0);
    assert!(d95_continuation.award_loss_posterior > 0.0);
    assert!(d95_continuation.approach_edges.contains(&D95_APPROACH_EDGE));
    assert!(d95_continuation.maritime_trade_required_posterior > 0.0);

    // The brickless conversion world must still prove the same attack through
    // authoritative maritime-trade + legal-action + GameState::apply semantics.
    let conversion_only = belief_road_cut_continuation_assessment(
        [(&conversion_world, 1.0_f32)],
        ACTOR,
        &bridge,
        &exposed_vertices,
    );
    let conversion_path = conversion_only
        .continuations
        .iter()
        .find(|evidence| evidence.vertex == D95_CUT_VERTEX && evidence.opponent == ATTACKER)
        .expect("maritime conversion world must preserve the D95 continuation");
    assert!(conversion_path.maritime_trade_required_posterior > 0.99);
    assert!(conversion_path.approach_edges.contains(&D95_APPROACH_EDGE));

    let evidence = RootFragilityAssessment {
        action: bridge.clone(),
        introduced_road_fragility: impact.introduced_road_fragility.clone(),
        road_cut_continuation: continuation,
    };
    let shallow_bridge = aggregate_root_for_ordering(bridge.clone(), 0.0, 0.0, 0.0, 4.0, 0.0);
    let shallow_defense = aggregate_root_for_ordering(defense.clone(), 0.0, 0.0, 0.0, 2.0, 0.0);
    assert_eq!(
        compare_roots(&shallow_bridge, &shallow_defense),
        std::cmp::Ordering::Greater,
        "the fixture must begin with the shallow bridge preferred"
    );
    assert!(
        horizon_escalation_trigger(&shallow_bridge, &evidence).is_some(),
        "clear unresolved award-loss exposure must request deeper arbitration"
    );

    let retained = vec![
        ranked_root(bridge.clone(), 0.8),
        ranked_root(defense.clone(), 0.2),
        ranked_root(Action::EndTurn, 0.1),
    ];
    // Deliberately omit the defensive road from shallow racers. It must enter
    // solely because A1 identified it as the causal approach-edge protector.
    let escalation = horizon_escalation_contenders(0, &[0], &retained, &evidence);
    assert!(escalation.contains(&0));
    assert!(
        escalation.contains(&1),
        "D95 defensive approach edge must remain eligible for escalation"
    );

    // These are fresh common-horizon measurements, not shallow moments plus a
    // fragility penalty. The deeper evidence is therefore free to reverse the
    // provisional winner using only measured terminal/margin statistics.
    let deeper = vec![
        aggregate_root_for_ordering(bridge, 0.4, 0.5, 0.01, 5.0, 0.01),
        aggregate_root_for_ordering(defense, 0.4, 0.5, 0.01, 6.0, 0.01),
        aggregate_root_for_ordering(Action::EndTurn, -0.2, 0.5, 0.01, 0.0, 0.01),
    ];
    let final_order = escalated_root_order(&escalation, &deeper);
    assert_eq!(
        final_order.first().copied(),
        Some(1),
        "deeper common-horizon evidence must be able to replace the shallow bridge"
    );
}

#[test]
fn censored_losses_do_not_displace_settlement_conversion() {
    // D142/D147/D150 comparator reproduction, not a fabricated full-game state.
    // The report supplies completion/outcome rates; leaf values here isolate
    // the bug: observing fewer losses must not overrule stronger VP conversion.
    let settlement = aggregate_root_for_ordering(
        Action::BuildSettlement { vertex: 30 }, -0.50, 0.50, 0.25, -1.0, 0.04);
    for action in [
        Action::EndTurn,
        Action::BuildRoad { edge: 36 },
        Action::MaritimeTrade {
            give: colonist_catan_core::Resource::Lumber,
            receive: colonist_catan_core::Resource::Grain, ratio: 3,
        },
    ] {
        let delayed_loss = aggregate_root_for_ordering(action, -0.125, 0.125, 0.109375, -2.0, 0.04);
        let roots = vec![delayed_loss, settlement.clone()];
        assert_eq!(terminal_bounds(&roots[0]).0, -1.0);
        assert_eq!(terminal_bounds(&roots[1]).0, -1.0);
        assert_eq!(racing_contenders(&[0, 1], &roots), vec![1]);
        assert_eq!(escalated_root_order(&[0, 1], &roots)[0], 1);
        assert_eq!(escalated_root_order(&[1, 0], &roots)[0], 1);
    }
}

#[test]
fn strong_completed_terminal_evidence_still_beats_a_better_leaf() {
    let mut win = aggregate_root_for_ordering(Action::EndTurn, 1.0, 1.0, 0.0, -3.0, 0.0);
    let mut loss = aggregate_root_for_ordering(Action::BuildRoad { edge: 36 }, -1.0, 1.0, 0.0, 3.0, 0.0);
    win.samples = 1_000;
    loss.samples = 1_000;
    assert_eq!(escalated_root_order(&[1, 0], &[win, loss]), vec![0, 1]);
}

#[test]
fn fewer_terminal_losses_cannot_break_an_equal_leaf_tie() {
    let mut productive = aggregate_root_for_ordering(Action::BuildSettlement { vertex: 30 }, -0.5, 0.5, 0.25, -1.0, 0.04);
    productive.prior = 0.9;
    let mut delay = aggregate_root_for_ordering(Action::EndTurn, -0.125, 0.125, 0.109375, -1.0, 0.04);
    delay.prior = 0.02;
    assert_eq!(escalated_root_order(&[0, 1], &[delay, productive])[0], 1);
}

#[test]
fn escalated_three_root_order_is_input_order_independent() {
    // With complete and well-sampled outcomes, root 2 is outside the terminal
    // tier. Root 0 leads the strategic tier; root 1 remains a terminal contender.
    // This protects the global-tier ordering against the old non-transitive
    // pairwise confidence-overlap semantics.
    let mut roots = vec![
        aggregate_root_for_ordering(Action::BuildRoad { edge: 10 }, 0.80, 1.0, 0.36, 5.0, 0.04),
        aggregate_root_for_ordering(Action::BuildRoad { edge: 11 }, 0.79, 1.0, 0.3759, 4.8, 0.04),
        aggregate_root_for_ordering(Action::BuildRoad { edge: 12 }, 0.40, 1.0, 0.84, 6.0, 0.04),
    ];
    for root in &mut roots { root.samples = 10_000; }
    let expected = vec![0, 1, 2];
    for input in [
        vec![0, 1, 2],
        vec![0, 2, 1],
        vec![1, 0, 2],
        vec![1, 2, 0],
        vec![2, 0, 1],
        vec![2, 1, 0],
    ] {
        assert_eq!(escalated_root_order(&input, &roots), expected);
    }
    assert_eq!(escalated_root_order(&[0, 1], &roots), vec![0, 1]);
    assert_eq!(escalated_root_order(&[1, 2], &roots), vec![1, 2]);
    assert_eq!(escalated_root_order(&[0, 2], &roots), vec![0, 2]);
}
