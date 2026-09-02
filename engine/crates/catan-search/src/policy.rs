use colonist_catan_core::{Action, GameState, NodeKind, Resource, SplitMix64};

use crate::eval::{
    RoadFrontierContext, RobberDenialContext, city_value, hand_transition_value,
    observed_marginal_development_value, prepare_road_frontier_context,
    prepare_robber_denial_context, production_pips, road_frontier_value,
    road_frontier_value_with_context, robber_denial, robber_denial_with_context, vertex_value,
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

fn policy_family(action: &Action) -> usize {
    match action {
        Action::Roll => 0,
        Action::ResolveRoll { .. } => 1,
        Action::Discard { .. } => 2,
        Action::MoveRobber { .. } => 3,
        Action::ResolveSteal { .. } => 4,
        Action::PlaceSettlement { .. } | Action::BuildSettlement { .. } => 5,
        Action::PlaceRoad { .. } | Action::BuildRoad { .. } => 6,
        Action::BuildCity { .. } => 7,
        Action::BuyDevelopment => 8,
        Action::PlayKnight { .. } => 9,
        Action::PlayRoadBuilding { .. } => 10,
        Action::PlayYearOfPlenty { .. } => 11,
        Action::PlayMonopoly { .. } => 12,
        Action::ResolveDevelopment { .. } => 13,
        Action::MaritimeTrade { .. } => 14,
        Action::OfferTrade { .. } => 15,
        Action::RespondTrade { .. } => 16,
        Action::CounterTrade { .. } => 17,
        Action::ConfirmTrade { .. } => 18,
        Action::CancelTrade => 19,
        Action::EndTurn => 20,
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
        before.saturating_sub(state.card_discard_limit) as f32
            - after.saturating_sub(state.card_discard_limit) as f32
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
    road_pair_coherence_from_values(state, first, second, first_value, road_frontier_value(state, second, actor))
}

fn road_pair_coherence_from_values(
    state: &GameState,
    first: u8,
    second: u8,
    first_value: f32,
    second_value: f32,
) -> f32 {
    let first_vertices = state.board.edges[first as usize].vertices;
    let second_vertices = state.board.edges[second as usize].vertices;
    let connected = first_vertices
        .iter()
        .any(|vertex| second_vertices.contains(vertex));
    if connected {
        first_value.max(second_value) + first_value.min(second_value) * 0.42 + 1.2
    } else {
        // Two disconnected free roads are only attractive if each has an
        // independently strong, immediately useful frontier.
        first_value.max(second_value) + first_value.min(second_value) * 0.12 - 2.4
    }
}

fn cached_road_frontier_value(
    state: &GameState,
    edge: u8,
    actor: u8,
    cache: &mut [Option<f32>],
    context: &mut Option<RoadFrontierContext>,
) -> f32 {
    let slot = &mut cache[edge as usize];
    if let Some(value) = *slot {
        return value;
    }
    let context = context.get_or_insert_with(|| prepare_road_frontier_context(state, actor));
    let value = road_frontier_value_with_context(state, edge, actor, context);
    *slot = Some(value);
    value
}

fn road_pair_coherence_cached(
    state: &GameState,
    first: u8,
    second: Option<u8>,
    actor: u8,
    cache: &mut [Option<f32>],
    context: &mut Option<RoadFrontierContext>,
) -> f32 {
    let first_value = cached_road_frontier_value(state, first, actor, cache, context);
    let Some(second) = second else {
        return first_value;
    };
    let second_value = cached_road_frontier_value(state, second, actor, cache, context);
    road_pair_coherence_from_values(state, first, second, first_value, second_value)
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
    action_prior_nonwinning(state, action, actor)
}

fn action_prior_nonwinning(state: &GameState, action: &Action, actor: u8) -> f32 {
    let base = match action {
        Action::PlaceSettlement { vertex } | Action::BuildSettlement { vertex } => {
            3.0 + vertex_value(state, *vertex, actor)
        }
        Action::PlaceRoad { edge } | Action::BuildRoad { edge } => {
            0.02 + road_frontier_value(state, *edge, actor)
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
        Action::BuyDevelopment => {
            let player = &state.players[actor as usize];
            let queued_actions = [0, 2, 3, 4]
                .into_iter()
                .map(|card| player.development[card])
                .sum::<u8>() as f32;
            let congestion = 1.0 / (1.0 + queued_actions * 0.28);
            0.05 + observed_marginal_development_value(state, actor) * 1.15 * congestion
        }
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
    personalize_action_prior(state, action, actor, base)
}

fn personalize_action_prior(state: &GameState, action: &Action, actor: u8, base: f32) -> f32 {
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

fn action_prior_nonwinning_cached(
    state: &GameState,
    action: &Action,
    actor: u8,
    road_cache: &mut [Option<f32>],
    road_context: &mut Option<RoadFrontierContext>,
    robber_context: &mut Option<RobberDenialContext>,
) -> f32 {
    let base = match action {
        Action::PlaceRoad { edge } | Action::BuildRoad { edge } => {
            0.02 + cached_road_frontier_value(state, *edge, actor, road_cache, road_context)
        }
        Action::PlayRoadBuilding { first, second } => {
            1.0
                + road_pair_coherence_cached(
                    state,
                    *first,
                    *second,
                    actor,
                    road_cache,
                    road_context,
                )
        }
        Action::MoveRobber { hex, victim } | Action::PlayKnight { hex, victim } => {
            let context = robber_context
                .get_or_insert_with(|| prepare_robber_denial_context(state, actor));
            let steal = victim
                .map(|player| {
                    state.players[player as usize].resource_total() as f32 * 0.12
                        + state.players[player as usize].public_victory_points as f32 * 0.22
                })
                .unwrap_or(0.0);
            0.2 + robber_denial_with_context(state, *hex, actor, context).max(-0.1) + steal
        }
        _ => return action_prior_nonwinning(state, action, actor),
    };
    personalize_action_prior(state, action, actor, base)
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
    if let Some(winning) = actions.iter().find(|action| {
        let mut next = state.clone();
        next.apply(action).is_ok() && next.winner() == Some(actor)
    }) {
        return winning.clone();
    }
    // Rollouts operate on an exact sampled world, but the acting policy may
    // condition only on that player's observation. Feeding the determinized
    // state into the learned model made simulated players quietly omniscient.
    let observed = state.observed_state(actor);
    let ranked = normalize_priors(&observed, actions, actor);
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
    let mut family_counts = [0_u16; 21];
    let mut scratch = state.clone();
    let mut road_cache = vec![None; state.board.edges.len()];
    let mut road_context = None;
    let mut robber_context = None;
    for action in actions {
        family_counts[policy_family(action)] += 1;
    }
    let mut scored = actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            let learned_multiplier = learned
                .as_ref()
                .and_then(|values| values.get(index))
                .map_or(1.0, |logit| logit.clamp(-4.0, 4.0).exp());
            // Parameter count is not strategic evidence. Without this family
            // normalization, ten equivalent roads or ninety trade bundles get
            // ten/ninety times the aggregate prior mass of EndTurn.
            let family_size = family_counts[policy_family(action)].max(1) as f32;
            let prior = if state.node_kind() == NodeKind::Chance {
                state.chance_weight(action) as f32
            } else if scratch.clone_from_and_apply(state, action).is_ok()
                && scratch.winner() == Some(actor)
            {
                10_000.0
            } else {
                action_prior_nonwinning_cached(
                    state,
                    action,
                    actor,
                    &mut road_cache,
                    &mut road_context,
                    &mut robber_context,
                )
            };
            let score = (prior * learned_multiplier / family_size).max(0.0001);
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

/// Policy distribution for an actor inside a hidden-state simulation.
///
/// Legality comes from the exact particle because a player knows their own
/// cards. Policy features come from the canonical observation so neither the
/// hand-written prior nor the learned model can inspect anybody else's hidden
/// resource/development identities.
pub(crate) fn normalize_observed_priors(
    state: &GameState,
    actions: &[Action],
    actor: u8,
) -> Vec<(Action, f32)> {
    normalize_priors(&state.observed_state(actor), actions, actor)
}

/// Caps the ordered root with relevance-conditional spatial quotas rather than
/// one unconditional slot per enum family. Spatial coverage matters more than
/// proving every development-card family is represented.
pub(crate) fn rank_with_class_quotas(
    state: &GameState,
    actions: &[Action],
    actor: u8,
    cap: usize,
) -> Vec<(Action, f32)> {
    let ranked = normalize_priors(state, actions, actor);
    let selected = order_scored_with_state_quotas(state, actor, ranked);
    truncate_root_preserving_end_turn(selected, cap)
}

fn push_unique(selected: &mut Vec<(Action, f32)>, candidate: &(Action, f32)) {
    if !selected.iter().any(|(action, _)| action == &candidate.0) {
        selected.push(candidate.clone());
    }
}

fn push_top_matching(
    selected: &mut Vec<(Action, f32)>,
    ranked: &[(Action, f32)],
    limit: usize,
    predicate: impl Fn(&Action) -> bool,
) {
    let mut kept = 0;
    for candidate in ranked {
        if kept >= limit {
            break;
        }
        if predicate(&candidate.0) {
            let before = selected.len();
            push_unique(selected, candidate);
            if selected.len() > before {
                kept += 1;
            }
        }
    }
}

fn road_endpoint(state: &GameState, action: &Action) -> Option<u8> {
    let edge = match action {
        Action::BuildRoad { edge } | Action::PlaceRoad { edge } => *edge,
        _ => return None,
    };
    let actor = state.actor();
    state.board.edges[edge as usize]
        .vertices
        .into_iter()
        .find(|vertex| {
            !state.board.vertices[*vertex as usize]
                .adjacent_edges
                .iter()
                .any(|neighbor| state.roads[*neighbor as usize] == Some(actor))
        })
        .or_else(|| state.board.edges[edge as usize].vertices.first().copied())
}

fn domestic_trade_material(state: &GameState, actor: u8, action: &Action, prior: f32) -> bool {
    let Action::OfferTrade {
        recipients,
        give,
        receive,
    } = action
    else {
        return matches!(action, Action::CounterTrade { .. }) && prior >= 0.01;
    };
    if prior < 0.008 {
        return false;
    }
    let acceptance = public_offer_acceptance_probability(state, actor, *recipients, give, receive);
    let received = receive.iter().sum::<u8>() as f32;
    let given = give.iter().sum::<u8>() as f32;
    acceptance * (received + 0.35) / (given + 0.35) >= 0.04
}

fn maritime_trade_material(state: &GameState, actor: u8, action: &Action, prior: f32) -> bool {
    let Action::MaritimeTrade {
        give,
        receive,
        ratio,
    } = action
    else {
        return false;
    };
    if prior < 0.01 {
        return false;
    }
    completes_build(state, *give, *receive, *ratio) > 0.0
        || (state.players[actor as usize].resource_total() > state.card_discard_limit
            && prior >= 0.02)
}

fn development_action_relevant(state: &GameState, actor: u8, action: &Action, prior: f32) -> bool {
    match action {
        Action::BuyDevelopment => {
            prior >= 0.02 && crate::eval::marginal_development_value(state, actor) >= 0.08
        }
        Action::PlayKnight { .. } => prior >= 0.015,
        Action::PlayYearOfPlenty { .. } | Action::PlayMonopoly { .. } => prior >= 0.02,
        Action::PlayRoadBuilding { .. } => prior >= 0.015,
        _ => false,
    }
}

/// Relevance-conditional root ordering.
///
/// Unconditional one-per-family quotas spent most of a 16-wide branch cap on
/// categorical coverage and starved alternate settlements and roads. Prefer:
/// two settlements, three route-distinct roads, two cities, material trades,
/// one relevant development line, trophy/hand-safety when active, and end turn.
pub(crate) fn order_scored_with_state_quotas(
    state: &GameState,
    actor: u8,
    ranked: Vec<(Action, f32)>,
) -> Vec<(Action, f32)> {
    let mut selected = Vec::<(Action, f32)>::new();

    push_top_matching(&mut selected, &ranked, 4, |action| {
        action_class(action) == ActionClass::Mandatory
    });

    push_top_matching(&mut selected, &ranked, 2, |action| {
        matches!(
            action,
            Action::PlaceSettlement { .. } | Action::BuildSettlement { .. }
        )
    });

    push_top_matching(&mut selected, &ranked, 2, |action| {
        matches!(action, Action::BuildCity { .. })
    });

    // Prefer spatially distinct road endpoints over duplicate steps toward the
    // same vertex. The difference between the best and second-best road is
    // often larger than the gap between an average road and EndTurn.
    let mut road_endpoints = Vec::<u8>::new();
    for candidate in &ranked {
        if road_endpoints.len() >= 3 {
            break;
        }
        let Some(endpoint) = road_endpoint(state, &candidate.0) else {
            continue;
        };
        if road_endpoints.contains(&endpoint) {
            continue;
        }
        let before = selected.len();
        push_unique(&mut selected, candidate);
        if selected.len() > before {
            road_endpoints.push(endpoint);
        }
    }

    for candidate in ranked.iter().filter(|(action, prior)| {
        matches!(
            action,
            Action::OfferTrade { .. } | Action::CounterTrade { .. }
        ) && domestic_trade_material(state, actor, action, *prior)
    }) {
        if selected
            .iter()
            .filter(|(action, _)| {
                matches!(
                    action,
                    Action::OfferTrade { .. } | Action::CounterTrade { .. }
                )
            })
            .count()
            >= 2
        {
            break;
        }
        push_unique(&mut selected, candidate);
    }

    if let Some(maritime) = ranked.iter().find(|(action, prior)| {
        matches!(action, Action::MaritimeTrade { .. })
            && maritime_trade_material(state, actor, action, *prior)
    }) {
        push_unique(&mut selected, maritime);
    }

    // Collapse low-value development families. Keep at most two representatives
    // among buy/play lines that clear a relevance floor.
    let mut development_kept = 0usize;
    for candidate in &ranked {
        if development_kept >= 2 {
            break;
        }
        if development_action_relevant(state, actor, &candidate.0, candidate.1) {
            let before = selected.len();
            push_unique(&mut selected, candidate);
            if selected.len() > before {
                development_kept += 1;
            }
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
        push_unique(&mut selected, trophy);
    }

    let held = state.players[actor as usize].resource_total();
    if held > state.card_discard_limit
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

    // Reserve EndTurn after spatial/material lines so truncation and adaptive
    // budgets cannot drop the only legal "do nothing" action. Exact tactical
    // proofs may still omit it when another action wins immediately.
    if let Some(end_turn) = ranked
        .iter()
        .find(|(action, _)| matches!(action, Action::EndTurn))
    {
        push_unique(&mut selected, end_turn);
    }

    for candidate in ranked {
        push_unique(&mut selected, &candidate);
    }
    selected
}

/// Truncate a ranked root while always retaining legal `EndTurn` when present.
pub(crate) fn truncate_root_preserving_end_turn(
    ranked: Vec<(Action, f32)>,
    branch_cap: usize,
) -> Vec<(Action, f32)> {
    if ranked.len() <= branch_cap {
        return ranked;
    }
    let end_turn = ranked
        .iter()
        .find(|(action, _)| matches!(action, Action::EndTurn))
        .cloned();
    let mut truncated = ranked.into_iter().take(branch_cap).collect::<Vec<_>>();
    if let Some(end_turn) = end_turn
        && !truncated
            .iter()
            .any(|(action, _)| matches!(action, Action::EndTurn))
    {
        if truncated.len() == branch_cap {
            truncated.pop();
        }
        truncated.push(end_turn);
    }
    truncated
}

/// Splits a node budget across root actions: ~70% on the leading group, ~20%
/// on challengers, and ~10% on the remaining uncertainty-sensitive tail.
pub fn allocate_root_node_budgets(action_count: usize, total_nodes: u32) -> Vec<u32> {
    if action_count == 0 {
        return Vec::new();
    }
    let total = total_nodes.max(action_count as u32);
    let leading = action_count.min(4);
    let challengers = action_count.saturating_sub(leading).min(3);
    let tail = action_count.saturating_sub(leading + challengers);
    let mut budgets = vec![1u32; action_count];
    let remaining = total.saturating_sub(action_count as u32);
    let leading_share = (remaining as f32 * 0.70).round() as u32;
    let challenger_share = (remaining as f32 * 0.20).round() as u32;
    let mut assigned = 0u32;
    if leading > 0 {
        let each = leading_share / leading as u32;
        let mut leftover = leading_share % leading as u32;
        for budget in budgets.iter_mut().take(leading) {
            *budget += each + u32::from(leftover > 0);
            leftover = leftover.saturating_sub(1);
        }
        assigned = leading_share;
    }
    if challengers > 0 {
        let each = challenger_share / challengers as u32;
        let mut leftover = challenger_share % challengers as u32;
        for budget in budgets.iter_mut().skip(leading).take(challengers) {
            *budget += each + u32::from(leftover > 0);
            leftover = leftover.saturating_sub(1);
        }
        assigned += challenger_share;
    }
    let tail_share = remaining.saturating_sub(assigned);
    if tail > 0 && tail_share > 0 {
        let each = tail_share / tail as u32;
        let mut leftover = tail_share % tail as u32;
        for budget in budgets.iter_mut().skip(leading + challengers).take(tail) {
            *budget += each + u32::from(leftover > 0);
            leftover = leftover.saturating_sub(1);
        }
    } else if remaining > assigned {
        // No tail: fold leftover into the leading group so the budget is spent.
        let leftover = remaining - assigned;
        let each = leftover / leading.max(1) as u32;
        let mut extra = leftover % leading.max(1) as u32;
        for budget in budgets.iter_mut().take(leading) {
            *budget += each + u32::from(extra > 0);
            extra = extra.saturating_sub(1);
        }
    }
    budgets
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{Action, GameState, Phase};

    use super::{
        action_prior, allocate_root_node_budgets, normalize_observed_priors, normalize_priors,
        order_scored_with_state_quotas, policy_family, rank_with_class_quotas,
        trade_acceptance_probability, truncate_root_preserving_end_turn,
    };

    #[test]
    fn unpromoted_policy_weights_cannot_change_production_prior_scores() {
        let mut state = GameState::standard(209, 4);
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
        let actions = state.legal_actions();

        assert!(!crate::model::learned_policy_promoted());
        assert!(crate::model::learned_action_logits(&state, &actions).is_none());

        let mut family_counts = [0_u16; 21];
        for action in &actions {
            family_counts[policy_family(action)] += 1;
        }
        let expected_total = actions
            .iter()
            .map(|action| {
                let family_size = family_counts[policy_family(action)].max(1) as f32;
                (action_prior(&state, action, 0) / family_size).max(0.0001)
            })
            .sum::<f32>();
        let ranked = normalize_priors(&state, &actions, 0);

        for (action, actual) in ranked {
            let family_size = family_counts[policy_family(&action)].max(1) as f32;
            let expected =
                (action_prior(&state, &action, 0) / family_size).max(0.0001) / expected_total;
            assert!(
                (actual - expected).abs() < 1e-6,
                "{action:?} received {actual}, expected structured-only {expected}"
            );
        }
    }

    #[test]
    fn saturated_root_preserves_end_turn_at_live_width() {
        // Construct an explicit saturated candidate list: two settlements, two
        // cities, three roads, trades, maritime, development, and EndTurn.
        // Truncation to eight must still keep EndTurn.
        let ranked = vec![
            (Action::BuildSettlement { vertex: 1 }, 0.20),
            (Action::BuildSettlement { vertex: 2 }, 0.19),
            (Action::BuildCity { vertex: 3 }, 0.18),
            (Action::BuildCity { vertex: 4 }, 0.17),
            (Action::BuildRoad { edge: 5 }, 0.16),
            (Action::BuildRoad { edge: 6 }, 0.15),
            (Action::BuildRoad { edge: 7 }, 0.14),
            (
                Action::OfferTrade {
                    recipients: 0b10,
                    give: [1, 0, 0, 0, 0],
                    receive: [0, 0, 0, 1, 0],
                },
                0.13,
            ),
            (
                Action::MaritimeTrade {
                    give: colonist_catan_core::Resource::Lumber,
                    receive: colonist_catan_core::Resource::Grain,
                    ratio: 4,
                },
                0.12,
            ),
            (Action::BuyDevelopment, 0.11),
            (Action::EndTurn, 0.05),
        ];
        let truncated = truncate_root_preserving_end_turn(ranked, 8);
        assert_eq!(truncated.len(), 8);
        assert!(
            truncated
                .iter()
                .any(|(action, _)| matches!(action, Action::EndTurn)),
            "EndTurn must survive an eight-wide saturated root"
        );
    }

    #[test]
    fn root_cap_prefers_spatial_coverage_over_every_family() {
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
        let ranked = rank_with_class_quotas(&state, &legal, 0, 8);
        let settlements = ranked
            .iter()
            .filter(|(action, _)| matches!(action, Action::BuildSettlement { .. }))
            .count();
        let roads = ranked
            .iter()
            .filter(|(action, _)| matches!(action, Action::BuildRoad { .. }))
            .count();
        assert!(
            settlements >= 1
                || !legal
                    .iter()
                    .any(|action| matches!(action, Action::BuildSettlement { .. }))
        );
        assert!(
            roads >= 1
                || !legal
                    .iter()
                    .any(|action| matches!(action, Action::BuildRoad { .. }))
        );
        assert!(
            ranked
                .iter()
                .any(|(action, _)| matches!(action, Action::EndTurn))
        );
        assert!(ranked.len() <= 8);
    }

    #[test]
    fn root_cap_keeps_relevant_playable_development_lines() {
        let mut state = GameState::standard(215, 4);
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
        state.players[0].development[0] = 1;
        state.players[0].development[3] = 1;
        state.players[0].development[4] = 1;

        let ranked = rank_with_class_quotas(&state, &state.legal_actions(), 0, 10);
        let development_lines = ranked
            .iter()
            .filter(|(action, _)| {
                matches!(
                    action,
                    Action::PlayKnight { .. }
                        | Action::PlayYearOfPlenty { .. }
                        | Action::PlayMonopoly { .. }
                        | Action::BuyDevelopment
                )
            })
            .count();
        assert!(
            development_lines >= 1,
            "at least one relevant development line must survive the relevance floor"
        );
        // Relevance quotas insert at most two development representatives
        // early; additional high-prior parameters may still fill later slots.
        assert!(ranked.len() <= 10);
    }

    #[test]
    fn adaptive_root_budgets_concentrate_on_leading_actions() {
        let budgets = allocate_root_node_budgets(8, 400);
        assert_eq!(budgets.len(), 8);
        assert_eq!(budgets.iter().sum::<u32>(), 400);
        let leading = budgets[..4].iter().sum::<u32>();
        let rest = budgets[4..].iter().sum::<u32>();
        assert!(leading > rest);
        assert!(budgets[0] >= budgets[7]);
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
    fn family_prior_mass_is_invariant_to_duplicate_parameters() {
        let mut state = GameState::standard(221, 4);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [1, 1, 0, 0, 0];
        let road = state
            .legal_actions()
            .into_iter()
            .find(|action| matches!(action, Action::BuildRoad { .. }))
            .expect("fixture exposes a road");
        let actions = vec![road.clone(), Action::EndTurn];
        let duplicated = vec![road.clone(), road.clone(), Action::EndTurn];
        let base = normalize_priors(&state, &actions, 0);
        let expanded = normalize_priors(&state, &duplicated, 0);
        let road_family = policy_family(&road);
        let base_mass = base
            .iter()
            .filter(|(action, _)| policy_family(action) == road_family)
            .map(|(_, prior)| prior)
            .sum::<f32>();
        let expanded_mass = expanded
            .iter()
            .filter(|(action, _)| policy_family(action) == road_family)
            .map(|(_, prior)| prior)
            .sum::<f32>();

        assert!((base_mass - expanded_mass).abs() < 1e-5);
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
        let first_priors = normalize_observed_priors(&first, &first_actions, 1);
        let second_priors = normalize_observed_priors(&second, &second_actions, 1);
        assert_eq!(first_priors, second_priors);
    }
}
