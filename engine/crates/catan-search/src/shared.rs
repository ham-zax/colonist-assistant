//! Shared observation-keyed belief search scaffolding.
//!
//! Production Deep MaxN (`deep-maxn-v5`) still evaluates particles after an
//! observer-consistent root, but deeper simulated opponents now select with an
//! observation-safe public utility. That removes strategy fusion from opponent
//! action choice: worlds that look identical to an opponent produce the same
//! continuation policy.
//!
//! Exact safety checks still see the full posterior. Strategic MaxN searches a
//! compact representative particle subset. The experimental PUCT tree remains
//! available in the arena as a diagnostic path and is not the live authority.

use colonist_catan_core::{Action, GameState};

use crate::mcts::BeliefParticle;
use crate::policy::{normalize_observed_priors, order_scored_with_state_quotas};

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
    let mut ordered = order_scored_with_state_quotas(&state.observed_state(actor), actor, ranked);
    ordered.truncate(cap.max(1));
    ordered
}

/// Selects a compact, weight-stratified particle subset for strategic search
/// while leaving the caller free to keep the full posterior for exact solvers.
pub fn select_strategic_particles(
    particles: &[BeliefParticle],
    limit: usize,
) -> Vec<BeliefParticle> {
    if particles.len() <= limit || limit == 0 {
        return particles.to_vec();
    }
    let total = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let quantum = total / limit as f32;
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
    if selected.is_empty() {
        particles.iter().take(limit).cloned().collect()
    } else {
        selected
    }
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::GameState;

    use super::{
        STRATEGIC_PARTICLE_TARGET, group_particles_by_observation, select_strategic_particles,
        shared_root_candidates,
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
}
