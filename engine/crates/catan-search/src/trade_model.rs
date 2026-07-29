use colonist_catan_core::{
    CITY_COST, DEVELOPMENT_COST, GameState, ROAD_COST, ResourceHand, SETTLEMENT_COST,
};

use crate::eval::{hand_transition_value, production_pips};

pub const TRADE_ACCEPTANCE_FEATURES: usize = 20;

#[allow(clippy::excessive_precision)]
mod weights {
    include!("trade_model_weights.rs");
}

fn contains(hand: &ResourceHand, cost: &ResourceHand) -> bool {
    hand.iter()
        .zip(cost)
        .all(|(available, required)| available >= required)
}

fn ready_builds(hand: &ResourceHand) -> f32 {
    [ROAD_COST, SETTLEMENT_COST, CITY_COST, DEVELOPMENT_COST]
        .iter()
        .enumerate()
        .map(|(kind, cost)| {
            if contains(hand, cost) {
                [0.25, 1.0, 0.9, 0.55][kind]
            } else {
                0.0
            }
        })
        .sum()
}

/// Features visible to the prospective recipient of the current offer.
///
/// The recipient's exact hand is private but known to itself. Creator and
/// third-party card identities are never encoded; only public totals, board
/// production, ports, scores, and the structured offer are used.
pub fn trade_acceptance_features(
    state: &GameState,
    recipient: u8,
) -> Option<[f32; TRADE_ACCEPTANCE_FEATURES]> {
    let trade = state.trade?;
    let recipient_state = &state.players[recipient as usize];
    if !contains(&recipient_state.resources, &trade.receive) {
        return None;
    }
    let mut after = state.clone();
    for resource in 0..5 {
        after.players[recipient as usize].resources[resource] = after.players[recipient as usize]
            .resources[resource]
            .saturating_sub(trade.receive[resource])
            .saturating_add(trade.give[resource]);
    }
    let before_hand = recipient_state.resources;
    let after_hand = after.players[recipient as usize].resources;
    let creator = &state.players[trade.creator as usize];
    let production = production_pips(state, recipient);
    let ratios = state.trade_ratios(recipient);
    let profile = recipient_state.policy_profile;
    let given = trade.receive.iter().copied().sum::<u8>() as f32;
    let received = trade.give.iter().copied().sum::<u8>() as f32;
    let requested_bottleneck = trade.receive[3] as f32 * 1.25
        + trade.receive[4] as f32 * 1.15
        + (trade.receive[0] + trade.receive[1]) as f32 * 0.72;
    let received_scarcity = (0..5)
        .map(|resource| {
            trade.give[resource] as f32 * (6.0 / (production[resource] + 1.5)).clamp(0.5, 2.2)
        })
        .sum::<f32>();
    let port_liquidity = (0..5)
        .map(|resource| trade.give[resource] as f32 * (4 - ratios[resource]) as f32 / 2.0)
        .sum::<f32>();
    Some([
        1.0,
        hand_transition_value(state, recipient, &after_hand) / 4.0,
        (received - given) / 2.0,
        (before_hand.iter().copied().sum::<u8>().saturating_sub(7) as f32
            - after_hand.iter().copied().sum::<u8>().saturating_sub(7) as f32)
            / 4.0,
        (ready_builds(&after_hand) - ready_builds(&before_hand)) / 2.0,
        received_scarcity / 4.0,
        port_liquidity / 4.0,
        requested_bottleneck / 4.0,
        creator.public_victory_points as f32 / state.victory_target as f32,
        recipient_state.public_victory_points as f32 / state.victory_target as f32,
        f32::from(creator.public_victory_points >= state.victory_target.saturating_sub(2)),
        creator.resource_total() as f32 / 20.0,
        recipient_state.resource_total() as f32 / 20.0,
        received / 4.0,
        given / 4.0,
        profile[0] as f32 / 255.0,
        profile[1] as f32 / 255.0,
        profile[2] as f32 / 255.0,
        profile[3] as f32 / 255.0,
        profile[4] as f32 / 255.0,
    ])
}

pub fn learned_trade_model_version() -> &'static str {
    weights::MODEL_VERSION
}

pub fn learned_trade_acceptance_probability(state: &GameState, recipient: u8) -> Option<f32> {
    let features = trade_acceptance_features(state, recipient)?;
    if weights::WEIGHTS.len() != TRADE_ACCEPTANCE_FEATURES {
        return None;
    }
    let logit = weights::BIAS
        + features
            .iter()
            .zip(weights::WEIGHTS)
            .map(|(feature, weight)| feature * weight)
            .sum::<f32>();
    Some((1.0 / (1.0 + (-logit).exp())).clamp(0.01, 0.99))
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{Action, GameState, Phase};

    use super::trade_acceptance_features;

    #[test]
    fn recipient_features_ignore_creator_hidden_card_identity() {
        let mut first = GameState::standard(311, 3);
        while matches!(
            first.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = first.legal_actions()[0].clone();
            first.apply(&action).unwrap();
        }
        first.phase = Phase::Main;
        first.current_player = 0;
        first.players[0].resources = [0, 1, 0, 0, 0];
        first.players[0].development = [0, 1, 0, 0, 0];
        first.players[1].resources = [1, 0, 0, 0, 0];
        first.development_deck = [10, 5, 2, 2, 2];
        first
            .apply(&Action::OfferTrade {
                recipients: 1 << 1,
                give: [0, 1, 0, 0, 0],
                receive: [1, 0, 0, 0, 0],
            })
            .unwrap();
        let mut second = first.clone();
        second.players[0].development = [1, 0, 0, 0, 0];
        second.development_deck = [14, 1, 2, 2, 2];
        assert_eq!(first.observation_hash(1), second.observation_hash(1));
        assert_eq!(
            trade_acceptance_features(&first, 1),
            trade_acceptance_features(&second, 1),
        );
    }
}
