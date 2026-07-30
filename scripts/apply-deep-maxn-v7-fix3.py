from pathlib import Path
import re

path = Path("engine/crates/catan-search/src/depth.rs")
source = path.read_text()
replacement = '''    #[test]
    fn configured_strategic_particle_limit_is_applied() {
        let mut state = GameState::standard(151, 3);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        for player in 0..3 {
            for resource in 0..5 {
                state.bank[resource] += state.players[player].resources[resource];
                state.players[player].resources[resource] = 0;
            }
        }
        state.bank[Resource::Lumber.index()] -= 4;
        state.bank[Resource::Brick.index()] -= 4;
        let particles = (0..16)
            .map(|index| {
                let mut world = state.clone();
                if index % 2 == 0 {
                    world.players[1].resources = [4, 0, 0, 0, 0];
                    world.players[2].resources = [0, 4, 0, 0, 0];
                } else {
                    world.players[1].resources = [0, 4, 0, 0, 0];
                    world.players[2].resources = [4, 0, 0, 0, 0];
                }
                BeliefParticle {
                    state: world,
                    weight: 1.0,
                }
            })
            .collect::<Vec<_>>();
        let report = super::search_weighted_belief_maxn_with_config(
            &particles,
            super::BeliefDepthConfig {
                maximum_depth: 1,
                branch_cap: 4,
                maximum_nodes: 500,
                time_budget_ms: 0,
                strategic_particle_limit: 4,
            },
        )
        .unwrap();
        assert_eq!(report.posterior_particles, 16);
        assert_eq!(report.particles, 2);
    }

'''
pattern = re.compile(
    r"    #\[test\]\n    fn configured_strategic_particle_limit_is_applied\(\) \{.*?\n    \}\n\n(?=    #\[test\]\n    fn public_opening_is_identical_across_information_modes)",
    re.S,
)
source, count = pattern.subn(replacement, source)
if count != 1:
    raise SystemExit(f"configured particle test block count: {count}")
path.write_text(source)

path = Path("engine/crates/catan-search/src/shared.rs")
source = path.read_text()
pattern = re.compile(
    r"        let selected = select_strategic_particles\(&particles, STRATEGIC_PARTICLE_TARGET\);\n"
    r"        assert_eq!\(selected\.len\(\), STRATEGIC_PARTICLE_TARGET\);\n"
    r"        let groups = group_particles_by_observation\(&selected, state\.actor\(\)\);"
)
replacement = '''        let selected = select_strategic_particles(&particles, STRATEGIC_PARTICLE_TARGET);
        assert_eq!(selected.len(), 1, "identical worlds should coalesce");
        let selected_mass = selected.iter().map(|particle| particle.weight).sum::<f32>();
        assert!((selected_mass - 1.0).abs() < 1e-6);
        let groups = group_particles_by_observation(&selected, state.actor());'''
source, count = pattern.subn(replacement, source)
if count != 1:
    raise SystemExit(f"mass-shape assertion block count: {count}")
path.write_text(source)
