//! Stage-1 isolated planning validation: spend-now versus save-for-build.
//!
//! All fixtures are synthetic post-roll Main-phase states with our domestic
//! trading masked (product own-player-only semantics) unless stated. The
//! production iterative wave entry (1500 nodes per wave, depth 3, baseline
//! policy) is paired against the fixed-work deep reference (40k nodes,
//! depth 5). Choices asserted here matched the deep reference at the time of
//! writing; reference values are recorded in
//! docs/ADAPTIVE_STRATEGY_LAYER_DESIGN_2026-09-06.md rather than assumed.
//!
//! Stage-1 outcome: no systematic complaint-direction failure reproduced.
//! The engine saves (EndTurn) when the reference saves and spends when
//! spending is unambiguous. EndTurn is retained on every fixture. Two
//! thin-margin flips are retained below as ignored diagnostics, not
//! regressions: they reproduce on demand but neither side is established
//! ground truth. See the design doc for the unresolved causal question and
//! the live evidence needed to discriminate it.

use colonist_catan_core::{Action, GameState, Phase, Resource};

use crate::BeliefParticle;
use crate::depth::{
    BeliefDepthResult, search_weighted_belief_maxn_bounded,
    search_weighted_belief_maxn_iterative_timed_excluding,
};

/// Play setup, then force a post-roll Main-phase decision for player 0 with
/// our domestic trading masked (product own-player-only restriction).
fn main_phase_state(seed: u64, players: u8) -> GameState {
    let mut state = GameState::standard(seed, players);
    while matches!(
        state.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    ) {
        let action = state.legal_actions()[0].clone();
        state.apply(&action).unwrap();
    }
    state.phase = Phase::Main;
    state.current_player = 0;
    state.domestic_trade_disabled = 1;
    state
}

fn set_hand(state: &mut GameState, player: usize, hand: [u8; 5]) {
    state.players[player].resources = hand;
}

fn single_particle(state: GameState) -> Vec<BeliefParticle> {
    vec![BeliefParticle { state, weight: 1.0 }]
}

/// Production wave entry: the live decision path minus wall-clock pressure.
fn production_entry(particles: &[BeliefParticle]) -> BeliefDepthResult {
    search_weighted_belief_maxn_iterative_timed_excluding(particles, 3, 12, 1_500, 0, 0, &[])
        .unwrap()
}

/// Fixed-work deep reference for the same decision.
fn deep_reference(particles: &[BeliefParticle]) -> BeliefDepthResult {
    search_weighted_belief_maxn_bounded(particles, 5, 16, 40_000).unwrap()
}

fn endturn_ranked_and_retained(report: &BeliefDepthResult) -> bool {
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

/// Seed 3 gives player 0 ore-rich production (13 pips). One ore short of a
/// city with only a dev purchase competing: both budgets save.
#[test]
fn two_player_save_for_city_matches_reference() {
    let mut state = main_phase_state(3, 2);
    set_hand(&mut state, 0, [0, 0, 1, 2, 2]);
    set_hand(&mut state, 1, [0, 0, 0, 0, 0]);
    assert!(state.legal_actions().contains(&Action::EndTurn));
    assert!(state.legal_actions().contains(&Action::BuyDevelopment));
    let particles = single_particle(state);
    let live = production_entry(&particles);
    let deep = deep_reference(&particles);
    assert_eq!(live.chosen, Some(Action::EndTurn));
    assert_eq!(deep.chosen, Some(Action::EndTurn));
    assert!(endturn_ranked_and_retained(&live));
    assert!(endturn_ranked_and_retained(&deep));
}

/// City affordable now on the same board: immediate spending dominates and
/// both budgets agree. Saving is not universally preferred.
#[test]
fn two_player_city_now_counterexample_matches_reference() {
    let mut state = main_phase_state(3, 2);
    set_hand(&mut state, 0, [0, 0, 0, 2, 3]);
    set_hand(&mut state, 1, [0, 0, 0, 0, 0]);
    assert!(state
        .legal_actions()
        .iter()
        .any(|action| matches!(action, Action::BuildCity { .. })));
    let particles = single_particle(state);
    let live = production_entry(&particles);
    let deep = deep_reference(&particles);
    assert!(matches!(live.chosen, Some(Action::BuildCity { .. })));
    assert!(matches!(deep.chosen, Some(Action::BuildCity { .. })));
    assert!(endturn_ranked_and_retained(&live));
}


/// Builds the contested-settlement pair on one board. Returns
/// (no_race, race, target). Both give us a one-bank-trade-away settlement
/// hand; race adds an opponent road plus exact settlement cost so the target
/// is immediately settleable for the opponent.
fn race_pair(seed: u64, mask_own_trades: bool) -> Option<(GameState, GameState, u8)> {
    let mut base = main_phase_state(seed, 2);
    if !mask_own_trades {
        base.domestic_trade_disabled = 0;
    }
    set_hand(&mut base, 0, [8, 8, 0, 0, 0]);
    for _ in 0..4 {
        let mut roads: Vec<u8> = base
            .legal_actions()
            .iter()
            .filter_map(|action| match action {
                Action::BuildRoad { edge } => Some(*edge),
                _ => None,
            })
            .collect();
        if roads.is_empty() {
            return None;
        }
        roads.sort_by_key(|edge| {
            let endpoints = base.board.edges[usize::from(*edge)].vertices;
            let crowded = endpoints
                .iter()
                .flat_map(|v| base.board.vertices[usize::from(*v)].adjacent_vertices.iter())
                .filter(|v| base.buildings[usize::from(**v)].is_some())
                .count();
            (crowded, *edge)
        });
        let edge = roads[0];
        base.apply(&Action::BuildRoad { edge }).unwrap();
    }
    set_hand(&mut base, 0, [1, 1, 1, 1, 0]);
    set_hand(&mut base, 1, [1, 1, 1, 1, 0]);
    let ours: Vec<u8> = base
        .legal_actions()
        .iter()
        .filter_map(|action| match action {
            Action::BuildSettlement { vertex } => Some(*vertex),
            _ => None,
        })
        .collect();
    for vertex in ours {
        let incident: Vec<u8> = base.board.vertices[usize::from(vertex)]
            .adjacent_edges
            .iter()
            .filter(|edge| base.roads[usize::from(**edge)].is_none())
            .copied()
            .collect();
        if incident.is_empty() {
            continue;
        }
        // Verify the opponent can settle the target once roaded.
        let mut trial = base.clone();
        trial.current_player = 1;
        trial.roads[usize::from(incident[0])] = Some(1);
        let takes = trial.legal_actions().iter().any(|action| {
            matches!(action, Action::BuildSettlement { vertex: v } if *v == vertex)
        });
        if !takes {
            continue;
        }
        // Our short hand: settlement cost minus brick plus 4:1 lumber.
        set_hand(&mut base, 0, [5, 0, 1, 1, 0]);
        let mut no_race = base.clone();
        set_hand(&mut no_race, 1, [0, 0, 0, 0, 0]);
        let mut race = base.clone();
        race.roads[usize::from(incident[0])] = Some(1);
        set_hand(&mut race, 1, [1, 1, 1, 1, 0]);
        return Some((no_race, race, vertex));
    }
    None
}

/// Contested settlement race (seed 15): variant B roads the opponent to our
/// target and funds their settlement cost; variant A leaves them empty.
/// The modeled opponent takes the target in B (verified directly, the
/// disproof gate). The production entry must price that loss into EndTurn:
/// waiting is near-free in A but destroys the site in B. A's exact choice is
/// deliberately unasserted (live/deep margins there are 0.002-0.011 noise).
#[test]
fn race_site_loss_priced_into_endturn() {
    let (no_race, race, target) = race_pair(15, true).expect("seed 15 race pair");
    // Independently checkable funding path: bank lumber into brick, then the
    // target is settleable this turn.
    let bank = no_race
        .legal_actions()
        .into_iter()
        .find(|action| {
            matches!(
                action,
                Action::MaritimeTrade {
                    give: Resource::Lumber,
                    receive: Resource::Brick,
                    ratio: 4
                }
            )
        })
        .expect("4:1 lumber into brick");
    let mut funded = no_race.clone();
    funded.apply(&bank).unwrap();
    assert!(
        funded.legal_actions().iter().any(
            |action| matches!(action, Action::BuildSettlement { vertex } if *vertex == target)
        ),
        "bank funds the target now"
    );
    // Disproof gate: the modeled opponent takes the target in B.
    let mut opp_turn = race.clone();
    opp_turn.current_player = 1;
    let opp = production_entry(&single_particle(opp_turn));
    assert!(
        matches!(opp.chosen, Some(Action::BuildSettlement { vertex }) if vertex == target),
        "modeled opponent takes the target: {:?}",
        opp.chosen
    );
    // Mechanism: EndTurn craters exactly where the site is contested.
    let live_a = production_entry(&single_particle(no_race));
    let live_b = production_entry(&single_particle(race));
    let end_value = |report: &BeliefDepthResult| {
        report
            .actions
            .iter()
            .find(|candidate| candidate.action == Action::EndTurn)
            .map(|candidate| candidate.value[0])
            .unwrap()
    };
    let (end_a, end_b) = (end_value(&live_a), end_value(&live_b));
    eprintln!("race EndTurn uncontested={end_a:.4} contested={end_b:.4}");
    assert!(
        end_b + 0.05 < end_a,
        "site loss priced into EndTurn: A={end_a:.4} B={end_b:.4}"
    );
    // In B the funded spend is the clear answer (live margin 0.45).
    assert!(
        matches!(live_b.chosen, Some(Action::MaritimeTrade { .. })),
        "race answered by bank-funded settle: {:?}",
        live_b.chosen
    );
    assert!(endturn_ranked_and_retained(&live_a));
    assert!(endturn_ranked_and_retained(&live_b));
}

/// Timed production entry mirror for wall-clock checks.
#[allow(dead_code)]
fn timed_entry(particles: &[BeliefParticle], budget_ms: u32) -> BeliefDepthResult {
    search_weighted_belief_maxn_iterative_timed_excluding(particles, 3, 12, 1_500, budget_ms, 0, &[])
        .unwrap()
}

/// Retained diagnostic (ignored): race pair with our domestic trades enabled
/// and through the timed entry. Observation only: offer roots flood the
/// fixed-work comparison and timed values vary with machine load, so neither
/// asserts a choice. Run on demand when trade-setting or timing behavior is
/// in question.
#[test]
#[ignore]
fn diagnostic_race_trades_enabled_and_timed() {
    for mask in [true, false] {
        let (no_race, race, target) =
            race_pair(15, mask).expect("seed 15 race pair");
        let end_value = |report: &BeliefDepthResult| {
            report
                .actions
                .iter()
                .find(|candidate| candidate.action == Action::EndTurn)
                .map(|candidate| candidate.value[0])
        };
        let live_a = production_entry(&single_particle(no_race.clone()));
        let live_b = production_entry(&single_particle(race.clone()));
        eprintln!(
            "mask={mask} fixed A: chosen={:?} end={:.4?} | B: chosen={:?} end={:.4?}",
            live_a.chosen,
            end_value(&live_a),
            live_b.chosen,
            end_value(&live_b)
        );
        let timed_a = timed_entry(&single_particle(no_race), 2_000);
        let timed_b = timed_entry(&single_particle(race), 2_000);
        eprintln!(
            "mask={mask} timed A: chosen={:?} nodes={} end={:.4?} | B: chosen={:?} nodes={} end={:.4?} target={target}",
            timed_a.chosen,
            timed_a.nodes,
            end_value(&timed_a),
            timed_b.chosen,
            timed_b.nodes,
            end_value(&timed_b)
        );
    }
}

/// M2 admission audit (ignored, exploratory): runs the adaptive policy next
/// to baseline on realistic decisions and reports, per decision, proposal
/// counts, evidence tiers, omission reasons, and whether the winner changes.
/// Answers whether zero admissions come from silent generators, already-
/// retained proposals, or protected capacity.
#[test]
#[ignore]
fn probe_m2_admission_audit() {
    use crate::StrategyPolicy;
    use crate::depth::{
        BeliefDepthConfig, search_weighted_belief_maxn_iterative_timed_excluding_with_strategy_policy,
    };
    fn m2_entry(particles: &[BeliefParticle]) -> crate::depth::BeliefDepthResult {
        search_weighted_belief_maxn_iterative_timed_excluding_with_strategy_policy(
            particles,
            BeliefDepthConfig {
                maximum_depth: 3,
                branch_cap: 12,
                maximum_nodes: 1_500,
                time_budget_ms: 0,
                strategy_policy: StrategyPolicy::AdaptiveCandidateAdmissionV1,
                strategic_particle_limit: usize::MAX,
            },
            0,
            &[],
        )
        .unwrap()
    }
    // Fixture set: stage-1 save, race pair (both variants), D14-lite (2p).
    let mut fixtures: Vec<(&str, GameState)> = Vec::new();
    let mut save = main_phase_state(3, 2);
    set_hand(&mut save, 0, [0, 0, 1, 2, 2]);
    set_hand(&mut save, 1, [0, 0, 0, 0, 0]);
    fixtures.push(("save-2p", save));
    if let Some((no_race, race, _)) = race_pair(15, true) {
        fixtures.push(("race-A", no_race));
        fixtures.push(("race-B", race));
    }
    for (tag, state) in &fixtures {
        let base = production_entry(&single_particle(state.clone()));
        let m2 = m2_entry(&single_particle(state.clone()));
        eprintln!("=== {tag} baseline={:?} m2={:?}", base.chosen, m2.chosen);
        if let Some(shadow) = m2.provenance.strategy_shadow {
            let admit = &shadow.admission;
            eprintln!(
                "    proposed={} distinct={} already_retained={} candidates={} selected={} admitted={} protected_roots={}",
                admit.proposed_count,
                admit.distinct_proposed_count,
                admit.already_baseline_retained_count,
                admit.challenger_candidates_considered,
                admit.challengers_selected,
                admit.challengers_admitted,
                admit.protected_root_count
            );
            for proposal in &shadow.proposals {
                eprintln!(
                    "    proposal {:?} action={:?} retained={} selected={} admitted={} omission={:?}",
                    proposal.strategy,
                    proposal.action,
                    proposal.retained,
                    proposal.selected_as_challenger,
                    proposal.admitted,
                    proposal.omission_reason
                );
            }
        } else {
            eprintln!("    NO strategy_shadow in provenance");
        }
    }
}

/// Finds a simple path of `count` empty edges starting from `tip`,
/// avoiding `visited` vertices so the placed chain stays connected.
fn find_road_path(
    state: &GameState,
    tip: u8,
    count: usize,
    visited: &mut Vec<u8>,
) -> Option<Vec<u8>> {
    if count == 0 {
        return Some(Vec::new());
    }
    let mut candidates: Vec<u8> = (0..state.board.edges.len() as u8)
        .filter(|edge| {
            state.roads[usize::from(*edge)].is_none()
                && state.board.edges[usize::from(*edge)].vertices.contains(&tip)
        })
        .collect();
    candidates.sort();
    for edge in candidates {
        let vertices = state.board.edges[usize::from(edge)].vertices;
        let next_tip = if vertices[0] == tip { vertices[1] } else { vertices[0] };
        if visited.contains(&next_tip) {
            continue;
        }
        visited.push(next_tip);
        if let Some(mut rest) = find_road_path(state, next_tip, count - 1, visited) {
            rest.push(edge);
            return Some(rest);
        }
        visited.pop();
    }
    None
}

/// Extends `player`'s road network tip-to-tip from `tip`, returning the new
/// tip. Returns None when no connected length-`count` path exists.
fn extend_tip(state: &mut GameState, player: u8, tip: u8, count: usize) -> Option<u8> {
    let mut visited = vec![tip];
    let mut path = find_road_path(state, tip, count, &mut visited)?;
    path.reverse();
    let mut tip = tip;
    for edge in path {
        state.roads[usize::from(edge)] = Some(player);
        let vertices = state.board.edges[usize::from(edge)].vertices;
        tip = if vertices[0] == tip { vertices[1] } else { vertices[0] };
    }
    Some(tip)
}

/// Stage-2b omission hunt (ignored): Longest-Road takeover. Both sides hold
/// connected 5-chains, the opponent holds the award; our hand funds the 6th
/// road for an immediate +2 swing with no win attached. Offer flood is on so
/// truncation can bite the low-prior award road. Reports retention,
/// M2 proposal/admission, and the deep-reference winner.
#[test]
#[ignore]
fn probe_award_takeover_omission() {
    for seed in 0u64..40 {
        let mut base = main_phase_state(seed, 2);
        base.domestic_trade_disabled = 0;
        // Our chain from our first setup road.
        let our_start = match base.roads.iter().enumerate().find_map(|(i, owner)| {
            (*owner == Some(0)).then_some(i as u8)
        }) {
            Some(edge) => edge,
            None => continue,
        };
        let our_tip = base.board.edges[usize::from(our_start)].vertices[0];
        // Opponent chain from their first setup road.
        let opp_start = match base.roads.iter().enumerate().find_map(|(i, owner)| {
            (*owner == Some(1)).then_some(i as u8)
        }) {
            Some(edge) => edge,
            None => continue,
        };
        let opp_tip = base.board.edges[usize::from(opp_start)].vertices[0];
        let (Some(_), Some(_)) = (
            extend_tip(&mut base, 0, our_tip, 4),
            extend_tip(&mut base, 1, opp_tip, 4),
        ) else {
            if seed < 3 {
                eprintln!("seed={seed} extension dead-end");
            }
            continue;
        };
        base.players[0].roads_left = 9;
        base.players[1].roads_left = 9;
        base.longest_road_holder = Some(1);
        base.players[1].has_longest_road = true;
        base.players[1].public_victory_points += 2;
        set_hand(&mut base, 0, [1, 1, 0, 0, 0]);
        set_hand(&mut base, 1, [0, 0, 0, 0, 0]);
        // Verify an immediate takeover exists: some legal road flips holder.
        let takeover = base.legal_actions().into_iter().find(|action| {
            let Action::BuildRoad { .. } = action else {
                return false;
            };
            let mut next = base.clone();
            next.apply(action).is_ok()
                && next.longest_road_holder == Some(0)
        });
        if takeover.is_none() && seed < 3 {
            let ours: Vec<u8> = base
                .legal_actions()
                .iter()
                .filter_map(|action| match action {
                    Action::BuildRoad { edge } => Some(*edge),
                    _ => None,
                })
                .collect();
            eprintln!("seed={seed} no-flip, our road options={ours:?} holder={:?}", base.longest_road_holder);
        }
        let Some(takeover) = takeover else { continue };
        eprintln!("seed={seed} takeover={takeover:?}");
        let particles = single_particle(base);
        let live = production_entry(&particles);
        let retained_takeover = live
            .provenance
            .retained_roots
            .iter()
            .any(|root| root.action == takeover);
        eprintln!(
            "  live chosen={:?} takeover_retained={retained_takeover} ranked_total={} pruned={}",
            live.chosen,
            live.provenance.ranked_root_count,
            live.provenance.pruned_root_count
        );
        let deep = search_weighted_belief_maxn_bounded(&particles, 5, 16, 40_000).unwrap();
        eprintln!("  deep chosen={:?}", deep.chosen);
        for candidate in deep.actions.iter().take(5) {
            eprintln!(
                "    deep action={:?} value={:.4}",
                candidate.action, candidate.value[0]
            );
        }
        return;
    }
    panic!("no award-takeover construction found in 40 seeds");
}

/// Coverage invariant across every save/spend shape probed: EndTurn is always
/// ranked and never pruned at live work. A future save-blindness must come
/// from valuation or horizon quality, not root admission.
#[test]
fn endturn_survives_root_admission_on_all_shapes() {
    let shapes: [(u8, u64, [u8; 5]); 7] = [
        (2, 3, [1, 1, 1, 2, 2]),
        (2, 3, [0, 0, 1, 2, 2]),
        (2, 10, [1, 1, 1, 2, 2]),
        (2, 26, [1, 1, 1, 2, 2]),
        (3, 3, [0, 0, 1, 2, 2]),
        (3, 3, [1, 1, 1, 2, 2]),
        (3, 10, [0, 0, 1, 2, 2]),
    ];
    for (players, seed, hand) in shapes {
        let mut state = main_phase_state(seed, players);
        set_hand(&mut state, 0, hand);
        for player in 1..players {
            set_hand(&mut state, usize::from(player), [0, 0, 0, 0, 0]);
        }
        let live = production_entry(&single_particle(state));
        assert!(
            endturn_ranked_and_retained(&live),
            "players={players} seed={seed} hand={hand:?}"
        );
        assert!(
            live.chosen.is_some(),
            "players={players} seed={seed} hand={hand:?}"
        );
    }
}

/// Retained diagnostic (ignored): 3p seed 3 dev-vs-save. The production entry
/// buys development (0.8020) over EndTurn (0.7838) while the deep reference
/// saves (0.8196 vs 0.7603). Both roots reach our next decision with full
/// mass through the production entry, so this is valuation divergence across
/// work budgets, not coverage or reachability. Neither side is established
/// ground truth; run on demand when investigating save-vs-spend valuation.
#[test]
#[ignore]
fn diagnostic_three_player_dev_vs_save_flip() {
    let mut state = main_phase_state(3, 3);
    set_hand(&mut state, 0, [0, 0, 1, 2, 2]);
    set_hand(&mut state, 1, [0, 0, 0, 0, 0]);
    set_hand(&mut state, 2, [0, 0, 0, 0, 0]);
    let particles = single_particle(state);
    let live = production_entry(&particles);
    let deep = deep_reference(&particles);
    eprintln!("live chosen={:?}", live.chosen);
    for candidate in live.actions.iter().take(4) {
        eprintln!(
            "  live action={:?} value={:.4}",
            candidate.action, candidate.value[0]
        );
    }
    eprintln!("deep chosen={:?}", deep.chosen);
    for candidate in deep.actions.iter().take(4) {
        eprintln!(
            "  deep action={:?} value={:.4}",
            candidate.action, candidate.value[0]
        );
    }
}

/// Retained diagnostic (ignored): 2p seed 3 with road materials. Global
/// fixed-work and the deep reference prefer a road; the production entry
/// prefers EndTurn by 0.0035. Three entries give three answers inside noise.
/// Do not promote any side without a discriminating reference.
#[test]
#[ignore]
fn diagnostic_two_player_road_vs_save_instability() {
    let mut state = main_phase_state(3, 2);
    set_hand(&mut state, 0, [1, 1, 1, 2, 2]);
    set_hand(&mut state, 1, [0, 0, 0, 0, 0]);
    let particles = single_particle(state);
    let live = production_entry(&particles);
    eprintln!("live chosen={:?}", live.chosen);
    for candidate in live.actions.iter().take(4) {
        eprintln!(
            "  live action={:?} value={:.4}",
            candidate.action, candidate.value[0]
        );
    }
}
