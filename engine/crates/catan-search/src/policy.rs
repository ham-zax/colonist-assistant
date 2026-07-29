use colonist_catan_core::{Action, GameState, NodeKind, Resource, SplitMix64};

use crate::eval::{
    city_value, hand_transition_value, observed_marginal_development_value, production_pips,
    road_frontier_value, robber_denial, vertex_value,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActionClass {
    Mandatory,
    Settlement,
    City,
    ExpansionRoad,
    Development,
    DomesticTrade,
    MaritimeTrade,
    Trophy,
    HandSafety,
    EndTurn,
}

pub fn action_class(action: &Action) -> ActionClass {
    match action {
        Action::Roll
        | Action::ResolveRoll { .. }
        | Action::Discard { .. }
        | Action::MoveRobber { .. }
        | Action::ResolveSteal { .. }
        | Action::ResolveDevelopment { .. }
        | Action::RespondTrade { .. }
        | Action::ConfirmTrade { .. }
        | Action::CancelTrade => ActionClass::Mandatory,
        Action::PlaceSettlement { .. } | Action::BuildSettlement { .. } => ActionClass::Settlement,
        Action::BuildCity { .. } => ActionClass::City,
        Action::PlaceRoad { .. } | Action::BuildRoad { .. } => ActionClass::ExpansionRoad,
        Action::BuyDevelopment
        | Action::PlayKnight { .. }
        | Action::PlayYearOfPlenty { .. }
        | Action::PlayMonopoly { .. } => ActionClass::Development,
        Action::PlayRoadBuilding { .. } => ActionClass::Trophy,
        Action::OfferTrade { .. } => ActionClass::DomesticTrade,
        Action::CounterTrade { .. } => ActionClass::DomesticTrade,
        Action::MaritimeTrade { .. } => ActionClass::MaritimeTrade,
        Action::EndTurn => ActionClass::EndTurn,
    }
}

/// Compact deterministic acceptance model. Its inputs are restricted to the
/// recipient's exact sampled hand and public strategic state, so an opponent
/// policy never reads third-party hidden identities.
pub fn trade_acceptance_probability(state: &GameState, recipient: u8) -> f32 {
    let Some(trade) = state.trade else {
        return 0.0;
    };
    let hand = state.players[recipient as usize].resources;
    if !hand
        .iter()
        .zip(trade.receive)
        .all(|(available, required)| *available >= required)
    {
        return 0.0;
    }
    let mut resulting_hand = hand;
    for (index, card) in resulting_hand.iter_mut().enumerate() {
        *card -= trade.receive[index];
        *card += trade.give[index];
    }
    let improvement = hand_transition_value(state, recipient, &resulting_hand);
    let received = trade.give.iter().sum::<u8>() as f32;
    let given = trade.receive.iter().sum::<u8>() as f32;
    // Hidden victory-point cards are not visible to the recipient.
    let leader = state.players[trade.creator as usize].public_victory_points as f32;
    let recipient_points = state.players[recipient as usize].victory_points() as f32;
    let creator_threat = (leader - recipient_points).max(0.0)
        + u8::from(leader >= state.victory_target.saturating_sub(2) as f32) as f32 * 1.8;
    let hand_risk_relief = {
        let before = state.players[recipient as usize].resource_total();
        let after = resulting_hand.iter().copied().sum::<u8>();
        before.saturating_sub(7) as f32 - after.saturating_sub(7) as f32
    };
    let requested_bottleneck = trade.receive[Resource::Grain.index()] as f32 * 1.25
        + trade.receive[Resource::Ore.index()] as f32 * 1.15
        + (trade.receive[Resource::Lumber.index()] + trade.receive[Resource::Brick.index()]) as f32
            * 0.72;
    let likely_decisive_help = requested_bottleneck
        * if leader >= state.victory_target.saturating_sub(2) as f32 {
            0.72
        } else if leader >= state.victory_target.saturating_sub(3) as f32 {
            0.34
        } else {
            0.08
        };
    let logit = -0.35 + improvement * 1.05 + (received - given) * 0.48 + hand_risk_relief * 0.72
        - creator_threat * 0.54
        - likely_decisive_help;
    let heuristic = (1.0 / (1.0 + (-logit).exp())).clamp(0.01, 0.99);
    let response_actions = [
        Action::RespondTrade { accept: false },
        Action::RespondTrade { accept: true },
    ];
    let generic_learned =
        crate::model::learned_action_logits(state, &response_actions).and_then(|values| {
            let maximum = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let rejected = (values[0] - maximum).exp();
            let accepted = (values[1] - maximum).exp();
            let total = rejected + accepted;
            (total > 0.0).then_some(accepted / total)
        });
    let dedicated = crate::trade_model::learned_trade_acceptance_probability(state, recipient);
    match (dedicated, generic_learned) {
        (Some(trade), Some(generic)) => heuristic * 0.25 + trade * 0.55 + generic * 0.20,
        (Some(trade), None) => heuristic * 0.35 + trade * 0.65,
        (None, Some(generic)) => heuristic * 0.45 + generic * 0.55,
        (None, None) => heuristic,
    }
    .clamp(0.01, 0.99)
}

fn public_offer_acceptance_probability(
    state: &GameState,
    actor: u8,
    recipients: u8,
    give: &[u8; 5],
    receive: &[u8; 5],
) -> f32 {
    let creator_points = state.players[actor as usize].public_victory_points as f32;
    (0..state.board.num_players)
        .filter(|recipient| recipients & (1 << recipient) != 0)
        .map(|recipient| {
            let player = &state.players[recipient as usize];
            let hand_size = player.resource_total() as f32;
            let production = production_pips(state, recipient);
            let production_total = production.iter().sum::<f32>() + 5.0;
            let possession = receive
                .iter()
                .enumerate()
                .filter(|(_, count)| **count > 0)
                .map(|(resource, count)| {
                    let expected = hand_size * (production[resource] + 1.0) / production_total;
                    (expected / *count as f32).clamp(0.04, 0.96)
                })
                .product::<f32>();
            let received_value = give
                .iter()
                .enumerate()
                .map(|(resource, count)| *count as f32 * [1.0, 1.0, 0.78, 1.25, 1.15][resource])
                .sum::<f32>();
            let given_value = receive
                .iter()
                .enumerate()
                .map(|(resource, count)| *count as f32 * [1.0, 1.0, 0.78, 1.25, 1.15][resource])
                .sum::<f32>();
            let leader_penalty = (creator_points - player.public_victory_points as f32).max(0.0)
                * 0.32
                + if creator_points >= state.victory_target.saturating_sub(2) as f32 {
                    1.15
                } else {
                    0.0
                };
            let generosity = received_value - given_value * 0.82 - leader_penalty;
            possession * (1.0 / (1.0 + (-generosity).exp()))
        })
        .fold(0.0_f32, f32::max)
        .clamp(0.01, 0.95)
}

fn completes_build(state: &GameState, give: Resource, receive: Resource, ratio: u8) -> f32 {
    let player = &state.players[state.current_player as usize];
    let mut hand = player.resources;
    if hand[give.index()] < ratio {
        return 0.0;
    }
    hand[give.index()] -= ratio;
    hand[receive.index()] += 1;
    let costs = [
        [1, 1, 0, 0, 0],
        [1, 1, 1, 1, 0],
        [0, 0, 0, 2, 3],
        [0, 0, 1, 1, 1],
    ];
    costs
        .iter()
        .enumerate()
        .filter(|(_, cost)| {
            hand.iter()
                .zip(cost.iter())
                .all(|(available, needed)| available >= needed)
        })
        .map(|(index, _)| [0.35, 1.1, 1.0, 0.65][index])
        .fold(0.0, f32::max)
}

fn road_pair_coherence(state: &GameState, first: u8, second: Option<u8>, actor: u8) -> f32 {
    let first_value = road_frontier_value(state, first, actor);
    let Some(second) = second else {
        return first_value;
    };
    let first_vertices = state.board.edges[first as usize].vertices;
    let second_vertices = state.board.edges[second as usize].vertices;
    let connected = first_vertices
        .iter()
        .any(|vertex| second_vertices.contains(vertex));
    let second_value = road_frontier_value(state, second, actor);
    if connected {
        first_value.max(second_value) + first_value.min(second_value) * 0.42 + 1.2
    } else {
        // Two disconnected free roads are only attractive if each has an
        // independently strong, immediately useful frontier.
        first_value.max(second_value) + first_value.min(second_value) * 0.12 - 2.4
    }
}

/// Strategic prior used for PUCT and the stochastic rollout policy.
///
/// Priors are deliberately shallow. Long-horizon consequences come from the
/// search tree; this layer orders expansions, preserves tactical actions, and
/// prevents obviously dead roads from consuming the early budget.
pub fn action_prior(state: &GameState, action: &Action, actor: u8) -> f32 {
    if state.node_kind() == NodeKind::Chance {
        return state.chance_weight(action) as f32;
    }
    let mut next = state.clone();
    if next.apply(action).is_ok() && next.winner() == Some(actor) {
        return 10_000.0;
    }
    let base = match action {
        Action::PlaceSettlement { vertex } | Action::BuildSettlement { vertex } => {
            3.0 + vertex_value(state, *vertex, actor)
        }
        Action::PlaceRoad { edge } | Action::BuildRoad { edge } => {
            0.15 + road_frontier_value(state, *edge, actor)
        }
        Action::Roll => 6.0,
        Action::ResolveRoll { .. } | Action::ResolveDevelopment { .. } => {
            state.chance_weight(action) as f32
        }
        Action::Discard { cards } => {
            let retained = state.players[actor as usize]
                .resources
                .iter()
                .zip(cards.iter())
                .enumerate()
                .map(|(index, (held, discarded))| {
                    (held - discarded) as f32 * [1.0, 1.0, 0.78, 1.25, 1.15][index]
                })
                .sum::<f32>();
            0.2 + retained
        }
        Action::MoveRobber { hex, victim } | Action::PlayKnight { hex, victim } => {
            let steal = victim
                .map(|player| {
                    state.players[player as usize].resource_total() as f32 * 0.12
                        + state.players[player as usize].public_victory_points as f32 * 0.22
                })
                .unwrap_or(0.0);
            0.2 + robber_denial(state, *hex, actor).max(-0.1) + steal
        }
        Action::ResolveSteal { resource, .. } => [1.0, 1.0, 0.78, 1.25, 1.15][resource.index()],
        Action::BuildCity { vertex } => 4.0 + city_value(state, *vertex, actor),
        Action::BuyDevelopment => 0.18 + observed_marginal_development_value(state, actor) * 2.2,
        Action::PlayRoadBuilding { first, second } => {
            1.0 + road_pair_coherence(state, *first, *second, actor)
        }
        Action::PlayYearOfPlenty { first, second } => {
            2.0 + [1.0, 1.0, 0.78, 1.25, 1.15][first.index()]
                + [1.0, 1.0, 0.78, 1.25, 1.15][second.index()]
        }
        Action::PlayMonopoly { resource } => {
            1.0 + state
                .players
                .iter()
                .enumerate()
                .filter(|(player, _)| *player != actor as usize)
                .map(|(player, state_player)| {
                    let production = production_pips(state, player as u8);
                    let total_weight = production.iter().sum::<f32>() + 5.0;
                    state_player.resource_total() as f32 * (production[resource.index()] + 1.0)
                        / total_weight
                })
                .sum::<f32>()
        }
        Action::MaritimeTrade {
            give,
            receive,
            ratio,
        } => 0.35 + completes_build(state, *give, *receive, *ratio),
        Action::OfferTrade {
            recipients,
            give,
            receive,
        } => {
            let mut resulting_hand = state.players[actor as usize].resources;
            for index in 0..5 {
                resulting_hand[index] = resulting_hand[index]
                    .saturating_sub(give[index])
                    .saturating_add(receive[index]);
            }
            let plan_gain = hand_transition_value(state, actor, &resulting_hand);
            let acceptance =
                public_offer_acceptance_probability(state, actor, *recipients, give, receive);
            let repeat_cost = match state.domestic_trade_count {
                0 => 1.0,
                1 => 0.42,
                _ => 0.16,
            };
            ((0.015 + plan_gain.max(0.0) * 0.62) * acceptance * repeat_cost).max(0.002)
        }
        Action::RespondTrade { accept } => {
            if !accept {
                1.0 - trade_acceptance_probability(state, actor)
            } else {
                trade_acceptance_probability(state, actor)
            }
        }
        Action::CounterTrade { give, receive } => {
            let mut resulting_hand = state.players[actor as usize].resources;
            for index in 0..5 {
                resulting_hand[index] = resulting_hand[index]
                    .saturating_sub(give[index])
                    .saturating_add(receive[index]);
            }
            let plan_gain = hand_transition_value(state, actor, &resulting_hand).max(0.0);
            let recipients = state.trade.map_or(0, |trade| 1 << trade.creator);
            let acceptance =
                public_offer_acceptance_probability(state, actor, recipients, give, receive);
            (0.006 + plan_gain * acceptance * 0.48).max(0.002)
        }
        Action::ConfirmTrade { .. } => 1.0,
        Action::CancelTrade => 0.5,
        Action::EndTurn => 0.12,
    };
    let profile = state.players[actor as usize].policy_profile;
    let normalized = |index: usize| profile[index] as f32 / 51.0;
    let personality = match action_class(action) {
        ActionClass::Settlement | ActionClass::ExpansionRoad => 0.72 + normalized(1) * 0.28,
        ActionClass::City | ActionClass::Development | ActionClass::Trophy => {
            0.72 + normalized(2) * 0.28
        }
        ActionClass::DomesticTrade => 0.65 + normalized(3) * 0.35,
        ActionClass::Mandatory if matches!(action, Action::RespondTrade { accept: false }) => {
            0.72 + normalized(4) * 0.28
        }
        _ => 0.78 + normalized(0) * 0.22,
    };
    base * personality.clamp(0.45, 1.75)
}

pub fn choose_rollout_action(
    state: &GameState,
    actions: &[Action],
    rng: &mut SplitMix64,
) -> Action {
    if state.node_kind() == NodeKind::Chance {
        return state
            .sample_chance(rng)
            .expect("chance state must expose a weighted outcome");
    }
    let actor = state.actor();
    let mut ranked = actions
        .iter()
        .cloned()
        .map(|action| {
            let prior = action_prior(state, &action, actor).max(0.001);
            (action, prior)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    if ranked[0].1 >= 10_000.0 {
        return ranked.remove(0).0;
    }
    let sample_count = ranked.len().min(5);
    let total = ranked[..sample_count]
        .iter()
        .map(|(_, prior)| prior)
        .sum::<f32>();
    let target = (rng.next_u64() as f64 / u64::MAX as f64) as f32 * total;
    let mut cursor = 0.0;
    for (action, prior) in ranked.into_iter().take(sample_count) {
        cursor += prior;
        if cursor >= target {
            return action;
        }
    }
    actions[0].clone()
}

pub(crate) fn normalize_priors(
    state: &GameState,
    actions: &[Action],
    actor: u8,
) -> Vec<(Action, f32)> {
    let learned = crate::model::learned_action_logits(state, actions);
    let mut scored = actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            let learned_multiplier = learned
                .as_ref()
                .and_then(|values| values.get(index))
                .map_or(1.0, |logit| logit.clamp(-4.0, 4.0).exp());
            let score = (action_prior(state, action, actor) * learned_multiplier).max(0.0001);
            (action.clone(), score)
        })
        .collect::<Vec<_>>();
    let total = scored.iter().map(|(_, score)| score).sum::<f32>();
    for (_, score) in &mut scored {
        *score /= total.max(0.0001);
    }
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored
}

/// Preserves at least one candidate from every relevant strategic class before
/// filling the remaining budget by prior. This prevents a global cutoff from
/// silently removing all expansion or settlement lines.
pub(crate) fn rank_with_class_quotas(
    state: &GameState,
    actions: &[Action],
    actor: u8,
    cap: usize,
) -> Vec<(Action, f32)> {
    let ranked = normalize_priors(state, actions, actor);
    let mut selected = order_scored_with_state_quotas(state, actor, ranked);
    selected.truncate(cap);
    selected
}

fn push_unique(selected: &mut Vec<(Action, f32)>, candidate: &(Action, f32)) {
    if !selected.iter().any(|(action, _)| action == &candidate.0) {
        selected.push(candidate.clone());
    }
}

/// Adds state-dependent strategic quotas that cannot be expressed by an
/// action's enum variant alone. A road can be an expansion action, a trophy
/// action, or both; likewise any conversion can be the critical hand-safety
/// line above seven cards. These candidates are inserted before the remaining
/// globally ranked actions so progressive widening cannot starve them.
pub(crate) fn order_scored_with_state_quotas(
    state: &GameState,
    actor: u8,
    ranked: Vec<(Action, f32)>,
) -> Vec<(Action, f32)> {
    let mut selected = Vec::<(Action, f32)>::new();
    for class in [
        ActionClass::Mandatory,
        ActionClass::Settlement,
        ActionClass::City,
        ActionClass::ExpansionRoad,
        ActionClass::Development,
        ActionClass::DomesticTrade,
        ActionClass::MaritimeTrade,
        ActionClass::Trophy,
        ActionClass::EndTurn,
    ] {
        if let Some(candidate) = ranked
            .iter()
            .find(|(action, _)| action_class(action) == class)
        {
            push_unique(&mut selected, candidate);
        }
    }

    if let Some(trophy) = ranked
        .iter()
        .filter(|(action, _)| {
            matches!(
                action,
                Action::BuildRoad { .. }
                    | Action::PlaceRoad { .. }
                    | Action::PlayRoadBuilding { .. }
            )
        })
        .filter_map(|candidate| {
            let before = crate::eval::longest_road_outlook(state, actor);
            let mut next = state.clone();
            next.apply(&candidate.0).ok()?;
            let after = crate::eval::longest_road_outlook(&next, actor);
            let gain = after.acquire * after.retain - before.acquire * before.retain;
            (gain > 0.015).then_some((candidate, gain))
        })
        .max_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.0.1.total_cmp(&right.0.1))
        })
        .map(|(candidate, _)| candidate)
    {
        // Insert ahead of the generic tail but after one representative from
        // every normal action family.
        push_unique(&mut selected, trophy);
    }

    let held = state.players[actor as usize].resource_total();
    if held > 7
        && let Some(safety) = ranked
            .iter()
            .filter_map(|candidate| {
                let mut next = state.clone();
                next.apply(&candidate.0).ok()?;
                let after = next.players[actor as usize].resource_total();
                (after < held).then(|| (candidate, held - after))
            })
            .max_by(|left, right| {
                left.1
                    .cmp(&right.1)
                    .then_with(|| left.0.1.total_cmp(&right.0.1))
            })
            .map(|(candidate, _)| candidate)
    {
        push_unique(&mut selected, safety);
    }

    for candidate in ranked {
        push_unique(&mut selected, &candidate);
    }
    selected
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use colonist_catan_core::{Action, GameState, Phase};

    use super::{
        action_class, normalize_priors, order_scored_with_state_quotas, rank_with_class_quotas,
        trade_acceptance_probability,
    };

    #[test]
    fn root_cap_preserves_every_relevant_action_class() {
        let mut state = GameState::standard(211, 4);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [5, 5, 4, 5, 5];
        let legal = state.legal_actions();
        let expected = legal.iter().map(action_class).collect::<BTreeSet<_>>();
        let ranked = rank_with_class_quotas(&state, &legal, 0, 16);
        let actual = ranked
            .iter()
            .map(|(action, _)| action_class(action))
            .collect::<BTreeSet<_>>();
        assert!(expected.is_subset(&actual));
    }

    #[test]
    fn state_quotas_preserve_a_real_hand_safety_conversion() {
        let mut state = GameState::standard(219, 3);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [4, 4, 2, 1, 1];
        let ranked = normalize_priors(&state, &state.legal_actions(), 0);
        let ordered = order_scored_with_state_quotas(&state, 0, ranked);
        let held = state.players[0].resource_total();
        assert!(ordered.iter().take(12).any(|(action, _)| {
            let mut next = state.clone();
            next.apply(action).is_ok() && next.players[0].resource_total() < held
        }));
    }

    #[test]
    fn recipients_are_less_likely_to_feed_a_near_winning_leader() {
        let mut state = GameState::standard(223, 4);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [0, 0, 1, 0, 0];
        state.players[1].resources = [0, 0, 0, 1, 0];
        state
            .apply(&Action::OfferTrade {
                recipients: 1 << 1,
                give: [0, 0, 1, 0, 0],
                receive: [0, 0, 0, 1, 0],
            })
            .unwrap();
        state.players[0].public_victory_points = 5;
        let ordinary = trade_acceptance_probability(&state, 1);
        state.players[0].public_victory_points = 8;
        let leader = trade_acceptance_probability(&state, 1);
        assert!(
            leader < ordinary,
            "a likely decisive leader conversion must face an explicit embargo penalty"
        );
    }

    #[test]
    fn opponent_priors_ignore_third_party_hidden_cards_and_exact_deck_identity() {
        let mut first = GameState::standard(227, 3);
        while matches!(
            first.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = first.legal_actions()[0].clone();
            first.apply(&action).unwrap();
        }
        first.phase = Phase::Main;
        first.current_player = 1;
        first.players[1].resources = [2, 2, 2, 2, 3];
        first.players[0].resources = [4, 0, 0, 0, 0];
        first.players[0].development = [0, 1, 0, 0, 0];
        first.development_deck = [10, 2, 1, 1, 1];

        let mut second = first.clone();
        second.players[0].resources = [0, 0, 0, 4, 0];
        second.players[0].development = [1, 0, 0, 0, 0];
        second.development_deck = [9, 3, 1, 1, 1];

        assert_eq!(first.observation_hash(1), second.observation_hash(1));
        let first_actions = first.legal_actions();
        let second_actions = second.legal_actions();
        assert_eq!(first_actions, second_actions);
        let first_priors = normalize_priors(&first, &first_actions, 1);
        let second_priors = normalize_priors(&second, &second_actions, 1);
        assert_eq!(first_priors, second_priors);
    }
}
