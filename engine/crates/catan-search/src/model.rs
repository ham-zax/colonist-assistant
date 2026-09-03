use colonist_catan_core::{Action, GameState};

use crate::features::{
    ACTION_FEATURES, STATE_FEATURES, STRATEGIC_FEATURE_SCHEMA_VERSION, encode_actions,
    encode_heterogeneous_graph, pool_heterogeneous_graph,
};

#[allow(clippy::excessive_precision)]
mod weights {
    include!("model_weights.rs");
}

pub fn learned_model_version() -> &'static str {
    weights::MODEL_VERSION
}

pub fn learned_model_ready() -> bool {
    weights::STRATEGIC_FEATURE_SCHEMA_VERSION == STRATEGIC_FEATURE_SCHEMA_VERSION
        && weights::VALUE_HIDDEN > 0
        && weights::POLICY_HIDDEN > 0
        && weights::VALUE_W1.len() == weights::VALUE_HIDDEN * STATE_FEATURES
        && weights::VALUE_W2.len() == weights::VALUE_HIDDEN * 4
        && weights::POLICY_W1.len() == weights::POLICY_HIDDEN * (STATE_FEATURES + ACTION_FEATURES)
        && weights::POLICY_W2.len() == weights::POLICY_HIDDEN
}

pub fn learned_value_promoted() -> bool {
    learned_model_ready() && weights::VALUE_MODEL_PROMOTED
}

pub fn learned_policy_promoted() -> bool {
    learned_model_ready() && weights::POLICY_MODEL_PROMOTED
}

fn dense_relu(input: &[f32], weights: &[f32], bias: &[f32], width: usize) -> Vec<f32> {
    (0..width)
        .map(|unit| {
            let offset = unit * input.len();
            let sum = input
                .iter()
                .enumerate()
                .map(|(index, value)| weights[offset + index] * value)
                .sum::<f32>();
            (sum + bias[unit]).max(0.0)
        })
        .collect()
}

pub fn learned_value(state: &GameState) -> Option<[f32; 4]> {
    if !learned_value_promoted() {
        return None;
    }
    let observer = state.actor();
    let graph = encode_heterogeneous_graph(state, observer, false);
    let input = pool_heterogeneous_graph(&graph, observer);
    let hidden = dense_relu(
        &input,
        weights::VALUE_W1,
        weights::VALUE_B1,
        weights::VALUE_HIDDEN,
    );
    let mut logits = [f32::NEG_INFINITY; 4];
    for (output, logit) in logits
        .iter_mut()
        .enumerate()
        .take(state.board.num_players as usize)
    {
        *logit = weights::VALUE_B2[output]
            + hidden
                .iter()
                .enumerate()
                .map(|(unit, value)| {
                    weights::VALUE_W2[output * weights::VALUE_HIDDEN + unit] * value
                })
                .sum::<f32>();
    }
    let maximum = logits
        .iter()
        .take(state.board.num_players as usize)
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    let mut relative = [0.0; 4];
    let mut total = 0.0;
    for (index, value) in relative
        .iter_mut()
        .enumerate()
        .take(state.board.num_players as usize)
    {
        *value = (logits[index] - maximum).exp();
        total += *value;
    }
    for value in relative.iter_mut().take(state.board.num_players as usize) {
        *value /= total.max(f32::EPSILON);
    }
    // Network outputs are actor-relative; rotate back to canonical player ids.
    let mut canonical = [0.0; 4];
    for offset in 0..state.board.num_players {
        canonical[(observer + offset) as usize % state.board.num_players as usize] =
            relative[offset as usize];
    }
    Some(canonical)
}

pub fn learned_action_logit(state: &GameState, action: &Action) -> Option<f32> {
    learned_action_logits(state, std::slice::from_ref(action)).map(|values| values[0])
}

pub fn learned_action_logits(state: &GameState, actions: &[Action]) -> Option<Vec<f32>> {
    if !learned_policy_promoted() {
        return None;
    }
    let observer = state.actor();
    let graph = encode_heterogeneous_graph(state, observer, false);
    let state_features = pool_heterogeneous_graph(&graph, observer);
    let input_width = STATE_FEATURES + ACTION_FEATURES;
    let base_hidden = (0..weights::POLICY_HIDDEN)
        .map(|unit| {
            let offset = unit * input_width;
            weights::POLICY_B1[unit]
                + state_features
                    .iter()
                    .enumerate()
                    .map(|(index, value)| weights::POLICY_W1[offset + index] * value)
                    .sum::<f32>()
        })
        .collect::<Vec<_>>();
    let action_features = encode_actions(state, actions);
    Some(
        action_features
            .iter()
            .map(|action_features| {
                weights::POLICY_B2
                    + (0..weights::POLICY_HIDDEN)
                        .map(|unit| {
                            let offset = unit * input_width + STATE_FEATURES;
                            let activation = base_hidden[unit]
                                + action_features
                                    .iter()
                                    .enumerate()
                                    .map(|(index, value)| {
                                        weights::POLICY_W1[offset + index] * value
                                    })
                                    .sum::<f32>();
                            activation.max(0.0) * weights::POLICY_W2[unit]
                        })
                        .sum::<f32>()
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{Action, GameState};

    use super::{
        learned_action_logits, learned_model_ready, learned_policy_promoted, learned_value,
        learned_value_promoted,
    };

    #[test]
    fn under_supported_checkpoint_cannot_supply_production_heads() {
        let state = GameState::standard(229, 4);

        assert!(!learned_model_ready());
        assert!(!learned_value_promoted());
        assert!(!learned_policy_promoted());
        assert!(learned_value(&state).is_none());
        assert!(learned_action_logits(&state, &[Action::EndTurn]).is_none());
    }
}
