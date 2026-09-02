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

## Fresh-seed P3 strength and speed tradeoff

After the pre-roll development-card, trade-response, and CUDA policy repairs at `aa60de6` (`Repair pre-roll and trade response correctness`), a new P3 cohort was generated instead of reusing the earlier campaign seeds.

Common contract:

- 34 matched board/chance blocks = 102 games, with the candidate rotated through all three seats.
- board seed: `535598822`
- game/chance seed: `848249963`
- search seed: `1500486845`
- neutral candidate and neutral GPU-weighted opponents: `51,51,51,51,51`
- player trades enabled
- max turns: `600`
- max actions: `20000`
- zero truncations in every reported run

### Maximum-strength canonical run

`gpu-agent-p3-canonical-102-12x32x96.json` uses 12 sampled roots, 32 rollouts per root, and 96 continuation steps.

| Metric | Result |
| --- | ---: |
| Wins | **101/102** |
| Win rate | **99.02%** |
| 95% Wilson CI | 94.65–99.83% |
| Wins by seat | 34 / 33 / 34 |
| Mean candidate VP | 10.049 |
| Mean best-opponent VP | 3.882 |
| Mean victory margin | **+6.167** |
| Mean turns | 83.87 |
| Complete games/s | **0.256** |

This is the maximum-strength reference for this fresh cohort. It is deliberately compute-heavy and should not be treated as the default live latency budget.

### Budget sweep

The same 102 matched games were used to locate the practical speed/strength knee. Wall-clock speed is shown only where the run was isolated on the RTX 3070 Ti. Two intermediate runs overlapped another GPU benchmark, so their game outcomes remain useful but their timing is intentionally omitted.

| Search budget | Wins | Win rate | Mean VP margin | Complete games/s |
| --- | ---: | ---: | ---: | ---: |
| `4×16×32` | 91/102 | 89.22% | +4.471 | — |
| `8×16×48` | 97/102 | **95.10%** | +5.265 | **0.563** |
| `10×16×48` | 96/102 | 94.12% | +5.686 | **0.589** |
| `8×16×64` | 93/102 | 91.18% | +5.196 | **0.501** |
| `8×20×48` | 97/102 | **95.10%** | **+5.676** | **0.562** |
| `8×24×48` | 98/102 | 96.08% | +5.843 | — |
| `8×24×64` | 100/102 | 98.04% | +5.676 | — |
| `12×20×48` | 96/102 | 94.12% | +5.343 | **0.583** |
| `12×32×96` | **101/102** | **99.02%** | **+6.167** | 0.256 |

The useful horizon on this cohort is around 48 rollout steps. Extending `8×16` from 48 to 64 steps reduced win rate while also slowing the run. Increasing root width without enough rollout confidence also failed to help: `10×16×48` and `12×20×48` were fast, but both won fewer games than the 8-root balanced region.

`8×20×48` is the current balanced preset: it matched `8×16×48` at 97/102 wins, improved mean victory margin from +5.265 to +5.676, and ran at essentially the same measured throughput (0.562 vs 0.563 complete games/s). `12×20×48` was only about 3.7% faster at 0.583 games/s but dropped to 96/102 wins and a +5.343 VP margin, so that extra root width is not a better overall trade. The much heavier `12×32×96` configuration bought four additional wins on this 102-game cohort, but at less than half the throughput.

Treat differences of one or two games in this 102-game sweep as screening evidence, not a precise population ordering. Use the balanced preset for practical live-search tuning and the maximum-strength preset for quality/reference campaigns until a larger multi-seed sweep supersedes this screen.

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
- `gpu-agent-p3-canonical-102-12x32x96.json` — fresh-seed 102-game P3 maximum-strength reference, 101/102 wins.
- `gpu-agent-p3-balanced-102-8x20x48.json` — fresh-seed 102-game P3 balanced speed/strength preset, 97/102 wins.
