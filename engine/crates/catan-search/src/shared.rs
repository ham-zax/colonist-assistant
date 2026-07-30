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

use colonist_catan_core::{
    Action, CITY_COST, DevCard, GameState, Resource, SETTLEMENT_COST,
};

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

fn systematic_weight_sample(
    particles: &[BeliefParticle],
    limit: usize,
    quantum: f32,
) -> Vec<BeliefParticle> {
    let mut selected = Vec::with_capacity(limit);
    let mut cursor = quantum * 0.5;
    let mut accumulated = 0.0;
    let mut index = 0usize;
    while selected.len() < limit && index < particles.len() {
        accumulated += particles[index].weight.max(0.0);
        while selected.len() < limit && cursor <= accumulated {
            let mut particle = particles[index].clone();
            particle.weight = quantum;
            selected.push(particle);
            cursor += quantum;
        }
        index += 1;
    }
    selected
}

/// Selects a compact particle subset for strategic search: reserve distinct
/// strategic signatures, then fill remaining mass with systematic resampling.
pub fn select_strategic_particles(
    particles: &[BeliefParticle],
    limit: usize,
) -> Vec<BeliefParticle> {
    if particles.len() <= limit || limit == 0 {
        return particles.to_vec();
    }
    let observer = particles
        .first()
        .map(|particle| particle.state.actor())
        .unwrap_or(0);
    let total = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let quantum = total / limit as f32;

    let mut by_weight = particles.iter().collect::<Vec<_>>();
    by_weight.sort_by(|left, right| {
        right
            .weight
            .total_cmp(&left.weight)
            .then_with(|| {
                particle_signature(&left.state, observer)
                    .cmp(&particle_signature(&right.state, observer))
            })
    });

    let reserve_slots = (limit / 3).max(1).min(limit);
    let mut reserved = Vec::<BeliefParticle>::with_capacity(reserve_slots);
    let mut seen = Vec::<u64>::new();
    for particle in &by_weight {
        if reserved.len() >= reserve_slots {
            break;
        }
        let signature = particle_signature(&particle.state, observer);
        if seen.contains(&signature) {
            continue;
        }
        seen.push(signature);
        let mut reserved_particle = (*particle).clone();
        reserved_particle.weight = quantum;
        reserved.push(reserved_particle);
    }

    let mut selected = systematic_weight_sample(particles, limit, quantum);
    if selected.is_empty() {
        return particles.iter().take(limit).cloned().collect();
    }

    // Overlay reserved signatures onto the lowest-weight sampled slots so rare
    // but decisive tails survive even when systematic sampling misses them.
    for reserved_particle in reserved.into_iter().rev() {
        let signature = particle_signature(&reserved_particle.state, observer);
        if selected
            .iter()
            .any(|particle| particle_signature(&particle.state, observer) == signature)
        {
            continue;
        }
        if let Some(slot) = selected
            .iter_mut()
            .min_by(|left, right| left.weight.total_cmp(&right.weight))
        {
            *slot = reserved_particle;
        }
    }

    selected.truncate(limit);
    selected
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
        assert_eq!(selected.len(), STRATEGIC_PARTICLE_TARGET);
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
}
