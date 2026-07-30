//! Shared observation-keyed belief search scaffolding.
//!
//! Production Deep MaxN still evaluates particles after an observer-consistent
//! root. Observation-safe opponent mixtures and a shared observation-keyed tree
//! remain experimental scaffolding and are not enabled by default.
//!
//! Exact safety checks still see the full posterior. Strategic MaxN searches a
//! compact representative particle subset that preserves high-mass worlds and
//! strategically distinct signatures (affordability, hidden VP, monopoly
//! concentration). The experimental PUCT tree remains available in the arena as
//! a diagnostic path and is not the live authority.

use colonist_catan_core::{Action, CITY_COST, DevCard, GameState, Resource, SETTLEMENT_COST};

use crate::mcts::BeliefParticle;
use crate::policy::{
    normalize_observed_priors, order_scored_with_state_quotas, truncate_root_preserving_end_turn,
};

/// Representative particle count for ordinary strategic search. Exact safety
/// checks still see the full posterior; strategic MaxN should not spend its
/// entire node budget repeating the same shallow tree in 32 near-duplicate
/// worlds.
pub const STRATEGIC_PARTICLE_TARGET: usize = 12;

/// Live root width after relevance-conditional quotas. Spatial coverage of
/// eight strong candidates beats categorical coverage of sixteen families.
pub const STRATEGIC_ROOT_WIDTH: usize = 8;

/// Groups belief particles by the acting player's observation identity.
pub fn group_particles_by_observation(
    particles: &[BeliefParticle],
    actor: u8,
) -> Vec<(u64, Vec<usize>)> {
    let mut groups = Vec::<(u64, Vec<usize>)>::new();
    for (index, particle) in particles.iter().enumerate() {
        let identity = particle.state.observation_hash(actor);
        if let Some((_, members)) = groups.iter_mut().find(|(key, _)| *key == identity) {
            members.push(index);
        } else {
            groups.push((identity, vec![index]));
        }
    }
    groups
}

/// Observation-safe root candidates shared across every particle that the
/// acting player cannot distinguish.
pub fn shared_root_candidates(
    state: &GameState,
    actor: u8,
    actions: &[Action],
    cap: usize,
) -> Vec<(Action, f32)> {
    let ranked = normalize_observed_priors(state, actions, actor);
    let ordered = order_scored_with_state_quotas(&state.observed_state(actor), actor, ranked);
    truncate_root_preserving_end_turn(ordered, cap.max(1))
}

fn hand_covers(hand: &[u8; 5], cost: &[u8; 5]) -> bool {
    hand.iter()
        .zip(cost.iter())
        .all(|(have, need)| *have >= *need)
}

/// Compact strategic signature for preserving distinct belief tails.
pub(crate) fn particle_signature(state: &GameState, observer: u8) -> u64 {
    let mut signature = 0u64;
    for (index, player) in state.players.iter().enumerate() {
        if index as u8 == observer {
            continue;
        }
        let shift = (index as u64) * 8;
        let mut bits = 0u64;
        if hand_covers(&player.resources, &SETTLEMENT_COST) {
            bits |= 1;
        }
        if hand_covers(&player.resources, &CITY_COST) {
            bits |= 1 << 1;
        }
        if player.development[DevCard::VictoryPoint.index()] > 0 {
            bits |= 1 << 2;
        }
        if player.development[DevCard::Monopoly.index()] > 0 {
            bits |= 1 << 3;
        }
        let ore = player.resources[Resource::Ore.index()];
        let grain = player.resources[Resource::Grain.index()];
        if ore >= 2 || grain >= 2 {
            bits |= 1 << 4;
        }
        let hidden_vp = player.development[DevCard::VictoryPoint.index()];
        let public_vp = player.victory_points().saturating_sub(hidden_vp);
        if public_vp + 2 >= state.victory_target {
            bits |= 1 << 5;
        }
        signature |= bits << shift;
    }
    signature
}

fn particle_distance(left: &GameState, right: &GameState, observer: u8) -> u32 {
    let mut distance = 0u32;
    for player in 0..left.board.num_players as usize {
        if player as u8 == observer {
            continue;
        }
        distance += left.players[player]
            .resources
            .iter()
            .zip(right.players[player].resources.iter())
            .map(|(a, b)| u32::from(a.abs_diff(*b)))
            .sum::<u32>();
        distance += left.players[player]
            .development
            .iter()
            .zip(right.players[player].development.iter())
            .map(|(a, b)| u32::from(a.abs_diff(*b)) * 3)
            .sum::<u32>();
    }
    if particle_signature(left, observer) != particle_signature(right, observer) {
        distance += 12;
    }
    distance
}

/// Selects a compact strategic coreset without inventing posterior mass.
/// Distinct high-impact signatures receive representatives, remaining slots are
/// filled by deterministic systematic sampling, and every original world's
/// normalized mass is assigned to its nearest representative.
pub fn select_strategic_particles(
    particles: &[BeliefParticle],
    limit: usize,
) -> Vec<BeliefParticle> {
    if particles.is_empty() || limit == 0 {
        return Vec::new();
    }
    let total = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>();
    let normalized = particles
        .iter()
        .cloned()
        .map(|mut particle| {
            particle.weight = if total > f32::EPSILON {
                particle.weight.max(0.0) / total
            } else {
                1.0 / particles.len() as f32
            };
            particle
        })
        .collect::<Vec<_>>();
    if normalized.len() <= limit {
        return normalized;
    }

    let observer = normalized[0].state.actor();
    let mut selected = Vec::<usize>::new();
    let mut signatures = normalized
        .iter()
        .enumerate()
        .map(|(index, particle)| (particle_signature(&particle.state, observer), index))
        .collect::<Vec<_>>();
    signatures.sort_by(|(left_sig, left), (right_sig, right)| {
        left_sig
            .cmp(right_sig)
            .then_with(|| {
                normalized[*right]
                    .weight
                    .total_cmp(&normalized[*left].weight)
            })
            .then_with(|| {
                normalized[*left]
                    .state
                    .state_hash()
                    .cmp(&normalized[*right].state.state_hash())
            })
    });
    let reserve_limit = (limit / 3).max(1).min(limit);
    let mut last_signature = None;
    for (signature, index) in &signatures {
        if selected.len() >= reserve_limit {
            break;
        }
        if last_signature == Some(*signature) {
            continue;
        }
        selected.push(*index);
        last_signature = Some(*signature);
    }

    let mut ordered = (0..normalized.len()).collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        particle_signature(&normalized[*left].state, observer)
            .cmp(&particle_signature(&normalized[*right].state, observer))
            .then_with(|| {
                normalized[*left]
                    .state
                    .state_hash()
                    .cmp(&normalized[*right].state.state_hash())
            })
    });
    let mut cumulative = 0.0f32;
    let mut cursor = 0usize;
    for stratum in 0..limit {
        if selected.len() >= limit {
            break;
        }
        let target = (stratum as f32 + 0.5) / limit as f32;
        while cursor + 1 < ordered.len() && cumulative + normalized[ordered[cursor]].weight < target
        {
            cumulative += normalized[ordered[cursor]].weight;
            cursor += 1;
        }
        let index = ordered[cursor];
        if !selected.contains(&index) {
            selected.push(index);
        }
    }
    if selected.len() < limit {
        let mut by_weight = (0..normalized.len()).collect::<Vec<_>>();
        by_weight.sort_by(|left, right| {
            normalized[*right]
                .weight
                .total_cmp(&normalized[*left].weight)
        });
        for index in by_weight {
            if selected.len() >= limit {
                break;
            }
            if !selected.contains(&index) {
                selected.push(index);
            }
        }
    }

    let mut representatives = selected
        .into_iter()
        .take(limit)
        .map(|index| {
            let mut particle = normalized[index].clone();
            particle.weight = 0.0;
            particle
        })
        .collect::<Vec<_>>();
    for particle in &normalized {
        let nearest = representatives
            .iter()
            .enumerate()
            .min_by(|(left_index, left), (right_index, right)| {
                particle_distance(&particle.state, &left.state, observer)
                    .cmp(&particle_distance(&particle.state, &right.state, observer))
                    .then_with(|| left.state.state_hash().cmp(&right.state.state_hash()))
                    .then_with(|| left_index.cmp(right_index))
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        representatives[nearest].weight += particle.weight;
    }
    representatives.retain(|particle| particle.weight > 0.0);
    representatives.sort_by_key(|particle| particle.state.state_hash());
    representatives
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{DevCard, GameState, SETTLEMENT_COST};

    use super::{
        STRATEGIC_PARTICLE_TARGET, group_particles_by_observation, particle_signature,
        select_strategic_particles, shared_root_candidates,
    };
    use crate::mcts::BeliefParticle;

    #[test]
    fn strategic_particle_subset_preserves_total_mass_shape() {
        let state = GameState::standard(77, 3);
        let particles = (0..24)
            .map(|index| BeliefParticle {
                state: state.clone(),
                weight: 1.0 + (index % 5) as f32 * 0.1,
            })
            .collect::<Vec<_>>();
        let selected = select_strategic_particles(&particles, STRATEGIC_PARTICLE_TARGET);
        assert_eq!(selected.len(), 1, "identical worlds should coalesce");
        let selected_mass = selected.iter().map(|particle| particle.weight).sum::<f32>();
        assert!((selected_mass - 1.0).abs() < 1e-6);
        let groups = group_particles_by_observation(&selected, state.actor());
        assert_eq!(groups.len(), 1);
        let legal = state.legal_actions();
        let shared = shared_root_candidates(&state, state.actor(), &legal, 8);
        assert!(!shared.is_empty());
        assert!(shared.len() <= 8);
    }

    #[test]
    fn strategic_particle_subset_preserves_settlement_affordability_tail() {
        let base = GameState::standard(91, 3);
        let mut rich = base.clone();
        rich.players[1].resources = SETTLEMENT_COST;
        let mut poor = base.clone();
        poor.players[1].resources = [0, 0, 0, 0, 0];
        poor.players[1].development[DevCard::VictoryPoint.index()] = 1;

        let mut particles = Vec::new();
        for _ in 0..20 {
            particles.push(BeliefParticle {
                state: poor.clone(),
                weight: 1.0,
            });
        }
        particles.push(BeliefParticle {
            state: rich.clone(),
            weight: 0.05,
        });

        let selected = select_strategic_particles(&particles, 8);
        let observer = base.actor();
        let rich_signature = particle_signature(&rich, observer);
        assert!(
            selected
                .iter()
                .any(|particle| particle_signature(&particle.state, observer) == rich_signature),
            "low-weight settlement-affordable worlds must survive selection"
        );
    }

    #[test]
    fn strategic_particle_subset_preserves_mass_without_inflating_rare_tail() {
        let base = GameState::standard(93, 3);
        let mut rare = base.clone();
        rare.players[1].resources = SETTLEMENT_COST;
        let mut particles = (0..20)
            .map(|_| BeliefParticle {
                state: base.clone(),
                weight: 1.0,
            })
            .collect::<Vec<_>>();
        particles.push(BeliefParticle {
            state: rare.clone(),
            weight: 0.05,
        });
        let selected = select_strategic_particles(&particles, 8);
        assert!((selected.iter().map(|particle| particle.weight).sum::<f32>() - 1.0).abs() < 1e-6);
        let observer = base.actor();
        let rare_mass = selected
            .iter()
            .find(|particle| {
                particle_signature(&particle.state, observer) == particle_signature(&rare, observer)
            })
            .map(|particle| particle.weight)
            .unwrap_or(0.0);
        assert!(rare_mass > 0.0);
        assert!(
            rare_mass < 0.02,
            "rare tail mass must not be promoted to an equal-weight particle"
        );
    }

    #[test]
    fn strategic_particle_selection_is_permutation_invariant() {
        let base = GameState::standard(95, 3);
        let mut particles = (0..24)
            .map(|index| {
                let mut state = base.clone();
                state.players[1].resources[index % 5] = (index % 4) as u8;
                BeliefParticle {
                    state,
                    weight: (index + 1) as f32,
                }
            })
            .collect::<Vec<_>>();
        let forward = select_strategic_particles(&particles, 8);
        particles.reverse();
        let reverse = select_strategic_particles(&particles, 8);
        let summarize = |items: &[BeliefParticle]| {
            items
                .iter()
                .map(|particle| {
                    (
                        particle.state.state_hash(),
                        (particle.weight * 1_000_000.0).round() as i64,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(summarize(&forward), summarize(&reverse));
    }
}
