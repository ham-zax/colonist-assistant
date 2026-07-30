from pathlib import Path

path = Path("engine/crates/catan-search/src/shared.rs")
source = path.read_text()
source = source.replace(
    """    if normalized.len() <= limit {\n        return normalized;\n    }\n\n    let observer = normalized[0].state.actor();\n""",
    """    let observer = normalized[0].state.actor();\n""",
)

old = """    let mut selected = Vec::<usize>::new();
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
"""
new = """    #[derive(Clone, Copy)]
    struct SignatureBucket {
        signature: u64,
        leader: usize,
        mass: f32,
        priority: u32,
    }

    fn signature_priority(signature: u64, players: u8) -> u32 {
        (0..players)
            .map(|player| {
                let bits = ((signature >> (u64::from(player) * 8)) & 0xff) as u8;
                u32::from(bits & (1 << 5) != 0) * 100
                    + u32::from(bits & (1 << 2) != 0) * 40
                    + u32::from(bits & (1 << 1) != 0) * 20
                    + u32::from(bits & 1 != 0) * 12
                    + u32::from(bits & (1 << 3) != 0) * 10
                    + u32::from(bits & (1 << 4) != 0) * 4
            })
            .sum()
    }

    let mut selected = Vec::<usize>::new();
    let mut buckets = Vec::<SignatureBucket>::new();
    for (index, particle) in normalized.iter().enumerate() {
        let signature = particle_signature(&particle.state, observer);
        if let Some(bucket) = buckets
            .iter_mut()
            .find(|bucket| bucket.signature == signature)
        {
            bucket.mass += particle.weight;
            let leader = &normalized[bucket.leader];
            if particle.weight > leader.weight
                || (particle.weight == leader.weight
                    && particle.state.state_hash() < leader.state.state_hash())
            {
                bucket.leader = index;
            }
        } else {
            buckets.push(SignatureBucket {
                signature,
                leader: index,
                mass: particle.weight,
                priority: signature_priority(signature, normalized[0].state.board.num_players),
            });
        }
    }
    buckets.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| right.mass.total_cmp(&left.mass))
            .then_with(|| left.signature.cmp(&right.signature))
            .then_with(|| {
                normalized[left.leader]
                    .state
                    .state_hash()
                    .cmp(&normalized[right.leader].state.state_hash())
            })
    });
    let reserve_limit = (limit / 3).max(1).min(limit);
    selected.extend(
        buckets
            .iter()
            .take(reserve_limit)
            .map(|bucket| bucket.leader),
    );
"""
if source.count(old) != 1:
    raise SystemExit(f"expected one signature reservation block, got {source.count(old)}")
source = source.replace(old, new)

needle = """    #[test]
    fn strategic_particle_selection_is_permutation_invariant() {
"""
insert = """    #[test]
    fn strategic_particle_subset_reserves_decisive_signature_before_arbitrary_low_bits() {
        let mut base = GameState::standard(95, 3);
        base.victory_target = 6;
        let mut particles = Vec::new();

        particles.push(BeliefParticle {
            state: base.clone(),
            weight: 1.0,
        });
        let mut settlement = base.clone();
        settlement.players[1].resources = SETTLEMENT_COST;
        particles.push(BeliefParticle {
            state: settlement,
            weight: 1.0,
        });
        let mut city = base.clone();
        city.players[1].resources = CITY_COST;
        particles.push(BeliefParticle {
            state: city,
            weight: 1.0,
        });
        let mut monopoly = base.clone();
        monopoly.players[1].development[DevCard::Monopoly.index()] = 1;
        particles.push(BeliefParticle {
            state: monopoly,
            weight: 1.0,
        });
        let mut hidden_vp = base.clone();
        hidden_vp.players[1].development[DevCard::VictoryPoint.index()] = 1;
        particles.push(BeliefParticle {
            state: hidden_vp,
            weight: 1.0,
        });
        let mut ore_concentration = base.clone();
        ore_concentration.players[1].resources[Resource::Ore.index()] = 2;
        particles.push(BeliefParticle {
            state: ore_concentration,
            weight: 1.0,
        });

        let mut decisive = base.clone();
        decisive.longest_road_holder = Some(1);
        decisive.largest_army_holder = Some(1);
        decisive.players[1].development[DevCard::VictoryPoint.index()] = 1;
        let decisive_signature = particle_signature(&decisive, base.actor());
        particles.push(BeliefParticle {
            state: decisive,
            weight: 0.001,
        });

        let selected = select_strategic_particles(&particles, 6);
        assert!(
            selected.iter().any(|particle| {
                particle_signature(&particle.state, base.actor()) == decisive_signature
            }),
            "near-win hidden-VP tails must outrank arbitrary low-bit signatures"
        );
    }

    #[test]
    fn strategic_particle_subset_coalesces_duplicates_below_limit() {
        let state = GameState::standard(96, 3);
        let particles = (0..4)
            .map(|_| BeliefParticle {
                state: state.clone(),
                weight: 1.0,
            })
            .collect::<Vec<_>>();
        let selected = select_strategic_particles(&particles, 12);
        assert_eq!(selected.len(), 1);
        assert!((selected[0].weight - 1.0).abs() < 1e-6);
    }

"""
if source.count(needle) != 1:
    raise SystemExit(f"expected one test insertion point, got {source.count(needle)}")
source = source.replace(needle, insert + needle)
path.write_text(source)
