# GPU strategy campaign — 2026-09-02

This directory freezes the strategy experiments run against `gpu-resident-sim` after the shared opening/evaluator fixes and the paired GPU searched-agent benchmark landed.

Authoritative benchmark implementation:

- `engine/crates/catan-arena/src/bin/gpu-sim-campaign.rs` — resident weighted-policy profile sweeps.
- `engine/crates/catan-arena/src/bin/gpu-sim-agent-benchmark.rs` — paired searched candidate vs resident weighted opponents.
- Benchmark implementation commit: `763c5c8` (`Add paired GPU searched strength benchmark`).

The searched benchmark is not live MaxN. Its reported `searchSemantics` are `sampled-root-actions + fixed-step gpu-weighted continuations`. Use it to measure strategy priors and GPU search strength, not as a calibrated estimate of the browser agent's win rate.

## Profiles

Profile fields are:

`balanced, expansion, cityDevelopment, tradeFlexible, tradeResistant`

The main searched-agent comparisons used:

| Name | Profile | Intent |
| --- | --- | --- |
| Neutral | `51,51,51,51,51` | No strong strategic specialization |
| OWS / city-dev | `0,0,102,102,0` | City/development engine with domestic-trade flexibility |
| Hybrid growth | `0,102,102,102,0` | Expansion plus city/development, trade-flexible |
| Expansion | `0,102,0,102,0` | Settlement/road expansion with city/dev emphasis suppressed |

These are policy/search priors, not hard rules. Search may choose actions outside the profile's nominal archetype.

## Searched-agent screening results

All screening games enabled player trades, used paired board/chance blocks, completed without truncation, and used:

- root samples: `4`
- rollouts per root action: `16`
- rollout steps: `32`
- max turns: `600`
- max actions: `20000`

### Four players

64 games per profile, 16 games from each candidate seat.

| Profile | Wins | Win rate | 95% Wilson CI | Mean victory margin |
| --- | ---: | ---: | ---: | ---: |
| OWS / city-dev | 50/64 | **78.13%** | 66.57–86.50% | **+2.516** |
| Hybrid growth | 48/64 | **75.00%** | 63.18–83.99% | **+2.422** |
| Expansion | 46/64 | **71.88%** | 59.87–81.41% | **+2.188** |
| Neutral | 43/64 | **67.19%** | 55.00–77.43% | **+1.297** |

Theoretical fair share is 25%.

The P4 ordering supports city/development as the strongest finishing engine. Expansion remains useful, but expansion plus city/dev beats pure expansion in the screening cohort.

### Three players

63 games per profile, 21 games from each candidate seat.

| Profile | Wins | Win rate | 95% Wilson CI | Mean victory margin |
| --- | ---: | ---: | ---: | ---: |
| Neutral | 56/63 | **88.89%** | 78.80–94.51% | **+4.635** |
| Expansion | 55/63 | **87.30%** | 76.89–93.42% | **+3.619** |
| OWS / city-dev | 53/63 | **84.13%** | 73.19–91.14% | **+3.524** |
| Hybrid growth | 52/63 | **82.54%** | 71.38–89.96% | **+3.746** |

Theoretical fair share is 33.33%.

P3 does not support a global OWS bias. Neutral search performs best, with expansion a close second. The larger board-space-per-player makes a flexible prior safer than forcing the P4 specialization.

## Higher-fidelity P4 check

The higher search budget used:

- root samples: `8`
- rollouts per root action: `32`
- rollout steps: `64`
- 4 paired board/chance blocks = 16 games per profile

Results:

| Profile | Wins | Win rate | Mean victory margin |
| --- | ---: | ---: | ---: |
| Neutral | 14/16 | 87.5% | +4.125 |
| Hybrid growth | 14/16 | 87.5% | +4.125 |
| OWS / city-dev | 14/16 | 87.5% | +4.000 |

At the larger search budget, the three priors converge on the same win count. This is evidence that stronger search can override a mediocre strategic prior. Do not interpret the 16-game 87.5% estimate as a precise population win rate.

## Resident weighted-policy sweep

`gpu-grid-trades-baseline-stable.json` is the stable 32-profile extreme grid with player trades enabled and separate policy RNG from the paired board/chance RNG.

Across the full extreme grid:

- `cityDevelopment=102` averaged a 23.63% candidate win rate vs 17.97% at `0`.
- `expansion=102` averaged 21.97% vs 19.63% at `0`.
- `tradeResistant=102` averaged 19.53% vs 22.07% at `0`.
- The best extreme profile was `0,102,102,102,0` at 23/64 = 35.94% against neutral weighted opponents.
- Within `expansion=102, cityDevelopment=102`, the best trade combination was `tradeFlexible=102, tradeResistant=0`.

These weighted-policy results motivated the hybrid prior used in the searched-agent comparison. The searched-agent results supersede the raw weighted-policy ranking when judging a search-capable agent.

## Reproduce

Run from `engine/`. Ensure the directory containing `libnvrtc.so.13` is on `LD_LIBRARY_PATH`.

P4 searched screening example:

```bash
cargo run --release -p colonist-catan-arena --features cuda-sim --bin gpu-sim-agent-benchmark -- \
  --players 4 \
  --blocks 16 \
  --chunk-blocks 16 \
  --player-trades \
  --root-samples 4 \
  --rollouts-per-action 16 \
  --rollout-steps 32 \
  --candidate-profile 0,0,102,102,0
```

P3 uses the same command with `--players 3 --blocks 21 --chunk-blocks 21`.

Higher-fidelity P4 check:

```bash
cargo run --release -p colonist-catan-arena --features cuda-sim --bin gpu-sim-agent-benchmark -- \
  --players 4 \
  --blocks 4 \
  --chunk-blocks 4 \
  --player-trades \
  --root-samples 8 \
  --rollouts-per-action 32 \
  --rollout-steps 64 \
  --candidate-profile 0,0,102,102,0
```

The JSON files serialize the board/game/search seeds and matching semantics. Re-running the same command and seeds regenerates the same matched game cohort. The frozen files contain campaign summaries, not per-turn trajectories.

## Frozen files

- `gpu-grid-no-trade.json` — 27-profile P4 no-trade discovery grid.
- `gpu-confirm-no-trade.json` — larger P4 no-trade shortlist confirmation.
- `gpu-grid-trades-baseline-stable.json` — stable 32-profile P4 player-trade grid.
- `gpu-agent-benchmark-smoke.json` — high-fidelity neutral P4 searched check.
- `gpu-agent-hybrid-4blocks.json` — high-fidelity hybrid P4 searched check.
- `gpu-agent-ows-4blocks.json` — high-fidelity OWS P4 searched check.
- `gpu-agent-screen-neutral.json` — 64-game P4 neutral screening run.
- `gpu-agent-screen-hybrid.json` — 64-game P4 hybrid screening run.
- `gpu-agent-screen-ows.json` — 64-game P4 OWS screening run.
- `gpu-agent-screen-expansion.json` — 64-game P4 expansion screening run.
- `gpu-agent-screen3-neutral.json` — 63-game P3 neutral screening run.
- `gpu-agent-screen3-hybrid.json` — 63-game P3 hybrid screening run.
- `gpu-agent-screen3-ows.json` — 63-game P3 OWS screening run.
- `gpu-agent-screen3-expansion.json` — 63-game P3 expansion screening run.
