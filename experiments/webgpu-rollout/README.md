# H2 — Browser-native WebGPU rollout feasibility

Status: **complete**

Decision: **CONDITIONAL**

The browser-native architecture is technically credible on the RTX 3070 Ti: a Chrome ServiceWorker can run a real packed Catan root-rollout workload within default WebGPU limits, paired-`u32` SplitMix64 arithmetic can preserve the tested CUDA RNG stream exactly, and the reduced-policy WebGPU slice has ample steady-state throughput. Two demonstrated issues prevent treating this gate as approval for an immediate production migration: the literal production CUDA dependency closure is much larger than the three kernels suggest, and first-time pipeline creation for even the reduced shader was about 3.37 seconds.

Infra I1 remains the production/development path. H2 does not change production routing, CUDA, PTX, strategy logic, or the installed Windows Runtime.

## Execution surface

The WebGPU compute ran inside an actual **Chrome 152 ServiceWorker** served from `http://localhost`, which is a secure-context exception and exercises the same ServiceWorker WebGPU API/lifecycle class needed by an MV3 extension. Native Messaging was not involved in WebGPU compute.

The high-performance adapter reported:

- vendor: `nvidia`
- architecture: `ampere`
- adapter `maxStorageBuffersPerShaderStage`: 16
- adapter `maxStorageBufferBindingSize`: 2,147,483,644 bytes
- adapter `maxBufferSize`: 2,147,483,648 bytes
- adapter `maxComputeInvocationsPerWorkgroup`: 1024

The experiment deliberately requested a **default device**, not NVIDIA-specific raised limits. The resulting device exposed:

- `maxStorageBuffersPerShaderStage`: **8**
- `maxStorageBufferBindingSize`: **134,217,728 bytes (128 MiB)**
- `maxBufferSize`: **268,435,456 bytes (256 MiB)**
- `maxComputeWorkgroupSizeX`: **256**
- `maxComputeInvocationsPerWorkgroup`: **256**
- `maxComputeWorkgroupsPerDimension`: **65,535**
- `maxComputeWorkgroupStorageSize`: **16,384 bytes**

The shader uses workgroups of 128 threads.

Edge was not directly benchmarked in H2. A production migration must rerun the same browser-owned harness in Edge rather than assuming Chrome timings are identical.

## Production CUDA boundary

The production host path is still `CudaSimEngine::search_root_actions_controlled`:

1. `expand_root_rollouts_kernel`
2. repeated `run_rollout_steps_kernel` dispatches in 16-step chunks
3. `reduce_root_rollouts_kernel`
4. compact per-root stats readback

A static call-graph closure over `sim.cu` found that those three kernels transitively depend on **110 CUDA helper functions covering about 4,306 of 4,885 lines (88.1%)** if ported literally. The coupling is mostly action generation, transition legality/application, policy scoring, road/robber logic, and RNG—not CUDA warp/shared-memory machinery.

H2 therefore did not silently turn into an 88%-of-`sim.cu` rewrite. The feasibility shader is 1,369 WGSL lines and keeps the minimum representative midgame slice needed to answer the architecture question.

## Representative workload

The Rust feasibility binary creates a deterministic four-player standard Catan game using the real core rules, disables player-to-player trading, advances 66 legal actions, and stops at a real `Main` phase with multiple distinct root controls. It then uses the production `CudaSimPackedState` packer.

Representative roots:

1. `EndTurn`
2. `BuildRoad { edge: 1 }`
3. `BuildRoad { edge: 3 }`
4. `BuildRoad { edge: 17 }`
5. `BuildRoad { edge: 18 }`
6. `BuildRoad { edge: 19 }`
7. `BuildRoad { edge: 33 }`
8. `MaritimeTrade { give: Brick, receive: Lumber, ratio: 3 }`

The deterministic standard-board workload was used instead of adapting the D68 extension fixture because H2 only needs a production-valid packed/search workload; converting the D68 belief/arbitration fixture into this low-level harness would add unrelated native-GPU orchestration. No toy board/state representation is used.

## Logic ported

The WebGPU slice uses the production packed field offsets and topology layout and implements:

- resident 404-word packed state handling;
- field-major root action handling;
- root expansion and root transition application;
- roll / roll chance and production;
- discard;
- robber selection, move, and steal;
- road, settlement, and city construction;
- longest-road recomputation using three `u32` masks instead of a CUDA `u64` bitset;
- development-card purchase and draw;
- maritime trade;
- end-turn progression;
- victory/terminal state handling;
- repeated rollout stepping;
- per-root reduction and readback.

The representative workload has player trades disabled, so domestic-trade phases are intentionally outside the gate. Setup phases are also unreachable from the selected midgame state.

The **material production-policy omission is development-card play selection** (Knight, Road Building, Year of Plenty, Monopoly). The feasibility state can buy/draw development cards, but the reduced rollout policy does not later choose those play actions. Consequently H2 is a browser/GPU architecture and strategic-plausibility gate, **not full search-policy parity**.

Arena/benchmark CUDA kernels, exact-evaluator kernels, native arbitration, D68 logic, Wave 3, and Wave 4 were not ported.

## Buffer layout

The CUDA root path has more independently bound buffers than a default WebGPU device guarantees. H2 repacks the path into one uniform buffer plus **five storage buffers**:

1. packed base states — read-only;
2. topology — read-only;
3. root data — packed root actions plus root-to-base indices, read-only;
4. lane data — packed state + action + status + RNG/chance RNG, read/write;
5. root stats — atomic `u32`, read/write.

Per lane:

- state: 404 `u32` = 1,616 bytes;
- action: 12 `u32`;
- status: 1 `u32`;
- normal RNG: two `u32` words;
- chance RNG: two `u32` words;
- total: **421 `u32` = 1,684 bytes/lane**.

At 32,768 lanes the lane buffer is **55,181,312 bytes**. At the existing CUDA chunk ceiling of 65,536 lanes it is **110,362,624 bytes**, still below the default 128 MiB storage-binding limit. The current packing would reach the default binding limit at approximately **79,701 lanes**.

The 65,536-lane / 110.36 MB run completed with zero transition errors.

Reduction counters use `u32`. This is safe for the bounded production workload: sample counts are tens of thousands, horizons are at most hundreds of transitions, and turn/VP squared aggregates remain orders of magnitude below `2^32`. Emulating `u64` atomics for these counters would add complexity without serving the actual live budget.

## 64-bit RNG

WGSL has no native concrete `u64`, so H2 implements SplitMix64 with paired `u32` words:

- 64-bit add with carry;
- logical right shift;
- 32×32→64 limb multiplication;
- low-64-bit multiply;
- modulo-`u32` range selection;
- the production stream-domain constants and seed mixing.

A GPU probe compared the mixed stream seed, first SplitMix64 output, and `% 6` result against JavaScript `BigInt` reference arithmetic:

- expected: `[808633232, 4253080808, 838270476, 2111939625, 0]`
- WebGPU: `[808633232, 4253080808, 838270476, 2111939625, 0]`
- result: **exact probe parity**.

This removes native 64-bit RNG as an architectural blocker for a migration. Full CUDA/WebGPU trajectory parity is still not claimed because the feasibility rollout policy deliberately omits development-card play selection.

## Compilation and lifecycle evidence

The WGSL source is 52,556 bytes.

First successful browser run after the shader changed:

- WGSL compile reporting: **35.9 ms**
- compute pipeline creation: **3,371.9 ms**

Immediate later ServiceWorker reinstalls/reruns with the same shader:

- WGSL compile reporting: about **1.1 ms**
- pipeline creation: about **6.6–6.7 ms**

This is a major cold/warm delta. H2 did not restart the shared browser process merely to destroy driver/browser caches. A production decision therefore still needs evidence about cold pipeline creation across a real browser restart and whether Chrome/Edge persist an effective shader/pipeline cache. With the current ~4 s native decision floor, a multi-second cold compile is not ignorable, and a fuller production shader could compile more slowly than this reduced one.

## CUDA comparison

Same RTX 3070 Ti, same packed state, same eight roots, same seed, 4,096 rollouts/root = **32,768 lanes**.

CUDA uses the unmodified production `CudaSimEngine` path. Its measured medians over three repetitions were:

| Horizon | CUDA total | CUDA rollout-steps/s | Complete lane rollouts/s |
| --- | ---: | ---: | ---: |
| 48 | 166.45 ms | 9.45 M | 196,868/s |
| 96 | 428.13 ms | 7.35 M | 76,537/s |

A zero-step CUDA search at the same lane count measured **0.446 ms median** for root expansion + root transition + reduction + readback together. The public production API does not expose a standalone reduce timing; H2 reports this as a conservative upper bound rather than mislabeling it as pure reduction cost.

## WebGPU timing

The primary production-relevant WebGPU number encodes expansion, all 16-step rollout dispatches, reduction, and compact stats copy in **one command submission**, then maps the readback buffer:

| Horizon | WebGPU combined | WebGPU rollout-steps/s | Complete lane rollouts/s | CUDA / WebGPU measured ratio |
| --- | ---: | ---: | ---: | ---: |
| 48 | **63.4 ms** | **24.81 M** | **516,845/s** | **2.63×** |
| 96 | **128.1 ms** | **24.56 M** | **255,800/s** | **3.34×** |

These ratios are **not a claim that a full WebGPU production port will be 2.6–3.3× faster than CUDA**. The WebGPU policy slice omits development-card play selection, while CUDA executes the complete production rollout policy. The comparison establishes that browser/WebGPU dispatch, storage-buffer access, paired-`u32` RNG, real rule transitions, and reductions have substantial performance headroom rather than an obvious browser overhead wall.

For component visibility, H2 also intentionally inserts queue fences between rollout compute, reduction, and readback. Median host-visible timings were:

| Horizon | Expansion + rollout | Reduction submission/fence | Readback/map | Instrumented total |
| --- | ---: | ---: | ---: | ---: |
| 48 | 60.2 ms | 15.6 ms | 14.1 ms | 91.4 ms |
| 96 | 127.8 ms | 15.2 ms | 10.2 ms | 150.1 ms |

The isolated reduction number includes browser queue-submission/fence overhead and is not a pure GPU kernel duration. The single-submit combined measurement is the better estimate of a production execution shape.

## Lane scaling

48-step steady run after a warmup at each size:

| Lanes | Lane buffer | Total resident footprint | Time | Rollout-steps/s | Errors |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 8,192 | 13.80 MB | 13.80 MB | 70.9 ms | 5.55 M | 0 |
| 16,384 | 27.59 MB | 27.60 MB | 68.2 ms | 11.53 M | 0 |
| 32,768 | 55.18 MB | 55.19 MB | 91.4 ms | 17.21 M | 0 |
| 65,536 | 110.36 MB | 110.37 MB | 121.9 ms | **25.81 M** | 0 |

Throughput was still increasing at 65,536 lanes. The default 128 MiB binding limit, not workgroup or dispatch count, is the first obvious portable ceiling for the current packing.

## Strategic plausibility

All 32,768 lanes completed without rollout errors. The chosen midgame workload produced no terminal lanes by 48 or 96 steps in either backend, so H2 does not claim terminal-frequency parity from this fixture.

Root ordering by mean victory margin remains coherent despite the intentionally reduced WebGPU policy:

- 48 steps: Spearman rank correlation **0.857**;
- 96 steps: Spearman rank correlation **0.881**;
- both horizons: WebGPU and CUDA have the **same top-two root set** (`Road 33`, `Road 3`);
- both horizons: they have the **same bottom-two root set** (`Road 17`, `Maritime Trade`), with order varying at 96 steps.

At 48 steps both select Road 33 as the strongest margin and Road 3 second. At 96 steps CUDA puts Road 3 narrowly first and Road 33 second, while WebGPU reverses those two. Road 19 is third in both at 96 steps.

The result is strategically plausible enough to rule out a gross state/layout/RNG/transition failure, but it is not strong enough to certify production search quality while the development-card play policy remains absent.

## Decision

**CONDITIONAL**.

The browser-native backend is technically promising, and no GPU-memory, baseline-binding, dispatch-limit, integer-width, or steady-state throughput blocker was found. However, H2 demonstrates two conditions that should be resolved before authorizing a full production migration:

1. **Policy/port closure:** the literal three-kernel dependency closure is ~88% of `sim.cu`; the feasibility shader omits the one materially reachable family, development-card play selection. A focused follow-up should port/measure that remaining no-player-trade production policy closure without changing strategy weights.
2. **Cold initialization:** first pipeline creation was ~3.37 s for a reduced 52.6 KB shader. Measure cold compile/pipeline creation after real Chrome and Edge restarts with the fuller shader, and establish the cache/lazy-initialization behavior expected for an MV3 service worker.

Until those two conditions are resolved, **retain Infra I1 / CUDA as production**. H2 supports further focused WebGPU work; it does not yet justify deleting or bypassing I1.

A real production WebGPU backend would still need:

- full production rollout-policy closure for the live no-player-trade and required trade/dev-card states;
- shared/target-neutral packing and root-contract plumbing instead of the feasibility JSON handoff;
- browser-side integration at the existing async deep-search executor seam;
- migration/refactor of the native-only `NativeGpuSearchEngine` arbitration needed in browser WASM/TypeScript;
- device-loss and service-worker lazy reinitialization handling;
- Chrome and Edge cold/warm validation;
- production search-quality acceptance against the CUDA backend before any backend switch.

That is still a **large migration**, not a shader swap. The H2 evidence says the GPU/browser substrate is worth pursuing only after the two focused conditions above are closed.

## Reproduction artifacts

- `engine/crates/catan-search/src/bin/webgpu-rollout-feasibility.rs` — creates the real packed workload and records the matched CUDA reference.
- `case.json` — generated packed state/topology/root input plus CUDA measurements.
- `rollout.wgsl` — feasibility-only WebGPU kernel slice.
- `worker.js` — actual ServiceWorker WebGPU runner.
- `index.html` — minimal browser harness.
- `result-chrome-service-worker-cold.json` — first successful cold-pipeline evidence.
- `result-chrome-service-worker.json` — final warm/steady measurements.

Focused Rust validation:

```text
cargo check --manifest-path engine/Cargo.toml \
  -p colonist-catan-search --features cuda-sim \
  --bin webgpu-rollout-feasibility
```

CUDA case generation:

```text
cargo run --release --manifest-path engine/Cargo.toml \
  -p colonist-catan-search --features cuda-sim \
  --bin webgpu-rollout-feasibility -- \
  --output=experiments/webgpu-rollout/case.json \
  --rollouts=4096 --repetitions=3
```

# H3 — Production-policy closure

Status: **complete**

Decision: **PROMISING**

H3 closes the development-card policy gap left by H2 for the no-player-trade production rollout path. The WebGPU shader now uses the CUDA production selection semantics and weights for Knight, Road Building, Year of Plenty, and Monopoly, plus the corresponding transition application. No production CUDA/search file changed.

The remaining intentionally unported rollout families are domestic player trades and setup. Domestic trades are disabled in the H3 workloads. Setup is unreachable from the selected midgame states. There is no remaining known reachable policy omission for the no-player-trade rollout path exercised by this gate.

## Development-card policy surface

H3 ports the CUDA helpers and selection behavior needed by `generate_rollout_action_lane()`:

- Knight: `development_playable`, `knight_policy_base`, `robber_blocks_actor_production`, and the production weighted robber selection;
- Road Building: paired legal-road enumeration, `road_building_pair_policy_score`, settlement-access detection, longest-road takeover scoring, and cycle-closure scoring;
- Year of Plenty: production resource weights plus `immediate_build_completion_score` for resource pairs;
- Monopoly: `observed_monopoly_resource_weight`, production resource weights, and immediate-build conversion scoring;
- transitions for `ACTION_PLAY_KNIGHT`, `ACTION_PLAY_ROAD_BUILDING`, `ACTION_PLAY_YEAR_OF_PLENTY`, and `ACTION_PLAY_MONOPOLY`.

The H3 port also aligns the feasibility shader's friendly-robber filter with the existing CUDA rule because Knight depends directly on that selector. No strategic weights were changed.

## Development-active workload

H3 derives a development-active state from the H2 deterministic real midgame state. It transfers one Knight, Road Building, Year of Plenty, and Monopoly from the real development deck into the current player's playable development hand, decrements the deck, and calls `GameState::validate()` before packing the state with `CudaSimPackedState`.

The eight roots are:

1. `EndTurn`
2. `BuildRoad { edge: 1 }`
3. `BuildRoad { edge: 3 }`
4. `PlayKnight { hex: 0, victim: None }`
5. `PlayRoadBuilding { first: 1, second: Some(0) }`
6. `PlayYearOfPlenty { first: Lumber, second: Lumber }`
7. `PlayMonopoly { resource: Lumber }`
8. `MaritimeTrade { give: Brick, receive: Lumber, ratio: 3 }`

The workload uses 4,096 rollouts/root, or 32,768 lanes.

## Development-active CUDA versus WebGPU

Production-shaped WebGPU timings submit expansion, rollout stepping, reduction, and stats copy together before mapping the compact result.

| Horizon | CUDA | WebGPU | WebGPU / CUDA latency | WebGPU throughput vs CUDA |
| --- | ---: | ---: | ---: | ---: |
| 48 | 832.79 ms | **1,092.2 ms** | 1.312× | 76.2% |
| 96 | 1,321.15 ms | **1,686.1 ms** | 1.276× | 78.4% |

CUDA rollout throughput was 1.889 M steps/s at 48 and 2.381 M steps/s at 96. WebGPU delivered 1.440 M and 1.866 M steps/s respectively.

The full policy is substantially more expensive than the reduced H2 shader on a development-heavy state, but the measured WebGPU latency remains below two seconds at both required horizons and completes without transition errors.

## Strategic agreement

The development-active result is close to CUDA despite not requiring bit-exact trajectory parity.

At 48 steps:

- Spearman root-order correlation: **1.000**;
- maximum absolute per-root victory-margin delta: **0.00513 VP**;
- CUDA and WebGPU root ordering are identical.

At 96 steps:

- Spearman root-order correlation: **0.976**;
- maximum absolute per-root victory-margin delta: **0.00757 VP**;
- the only ordering difference is a near-tie between Knight and Maritime Trade; all other positions match.

Both backends rank `BuildRoad { edge: 3 }` first and `PlayMonopoly { resource: Lumber }` second at both horizons. Both rank the chosen Road Building root last at both horizons.

All 32,768 development-active lanes completed with zero transition errors at both horizons.

## Development-card exercise

The new policy is materially exercised rather than merely compiled.

| Card | 48-step opportunities | 48-step selections | 96-step opportunities | 96-step selections |
| --- | ---: | ---: | ---: | ---: |
| Knight | 105,008 | 14,835 | 177,448 | 48,817 |
| Road Building | 78,174 | 22,362 | 93,002 | 30,226 |
| Year of Plenty | 64,877 | 25,976 | 71,858 | 30,283 |
| Monopoly | 67,706 | 25,406 | 75,638 | 30,306 |

This closes the H2 policy gap with tens of thousands of actual selections for every required card family.

## Original H2 state with the full H3 policy

The original H2 state was rerun with the same full H3 shader. Development cards are not initially in hand, but buying/drawing during rollouts makes all four play families reachable.

| Horizon | CUDA | H3 WebGPU | WebGPU / CUDA latency | WebGPU throughput vs CUDA |
| --- | ---: | ---: | ---: | ---: |
| 48 | 166.45 ms | **180.6 ms** | 1.085× | 92.2% |
| 96 | 428.13 ms | **488.4 ms** | 1.141× | 87.7% |

At 48 steps the rollout selected 4,969 Knights, 680 Road Buildings, 725 Years of Plenty, and 753 Monopolies. At 96 steps it selected 15,350 Knights, 2,173 Road Buildings, 2,280 Years of Plenty, and 2,323 Monopolies. All lanes completed with zero transition errors.

Compared with the reduced H2 WebGPU numbers on this same state (63.4 ms / 128.1 ms), the full policy costs about 2.85× / 3.81× more. That increase is real policy work, not browser transfer or dispatch overhead. The more relevant production comparison is CUDA: the full WebGPU path remains within about 8.5–14.1% latency on this ordinary midgame workload.

## Memory and lane scaling

The H3 policy closure does not change the resident lane layout or binding count:

- five storage buffers;
- 421 `u32` / 1,684 bytes per lane;
- 55.19 MB resident at 32,768 lanes;
- 110.37 MB resident at 65,536 lanes;
- approximately 79,701 lanes remain the current portable 128 MiB binding ceiling.

The development-heavy 65,536-lane / 48-step run completed with zero errors. Throughput was 1.459 M rollout-steps/s; the policy-heavy shader is close to saturation by this size, unlike the lighter H2 shader.

## Full-shader cold and warm initialization

The H3 shader grew from 52,556 to **76,438 bytes**.

The first successful run after the H3 shader change reported:

- WGSL compile reporting: **19.4 ms**;
- four compute pipelines: **14,216.7 ms**.

The immediately following run with the same shader reported:

- WGSL compile reporting: **1.9 ms**;
- four compute pipelines: **25.1 ms**.

The larger first-use pipeline cost is therefore a cache-sensitive one-time event in this measurement, not repeated per-search latency. H3 records it but does not optimize it; the gate explicitly allows cold initialization latency unless it causes repeated operational failure. No such failure occurred.

## H3 decision

**PROMISING**.

The materially missing H2 policy is now present and heavily exercised. WebGPU keeps the production packed-state layout, exact tested SplitMix64 arithmetic, five-storage-buffer layout, 65,536-lane capacity, zero transition errors, sub-two-second 48/96 development-heavy searches, and near-CUDA strategic aggregates. On the original H2 state, the full policy is within about 8.5–14.1% of CUDA latency.

This is enough evidence to justify planning a browser-native production backend migration. It is **not** authorization to begin that migration in H3.

A production migration still needs engineering work rather than more policy-feasibility proof:

- replace the feasibility JSON handoff with a shared/target-neutral packed-state and root contract;
- move the required native-only root racing, belief handling, and arbitration into a browser-capable boundary;
- integrate WebGPU behind the existing asynchronous deep-search executor without changing user-visible strategy semantics;
- handle WebGPU device loss and MV3 ServiceWorker recreation/lazy pipeline initialization;
- run Chrome and Edge production acceptance and strategic regression before switching the backend;
- retain I1/CUDA as fallback until that migration has passed its own acceptance gate.

H3 does not modify `sim.cu`, `sim.ptx`, native production search, D68, Wave 3, Wave 4, or the installed Windows GPU Runtime.

## H3 evidence

- `case-development.json` — validated real development-active state and matched production CUDA reference.
- `result-h3-policy-closure.json` — compact H3 browser/CUDA timing, policy-exercise, strategic-order, memory, and cold/warm evidence.

Generate the development-active CUDA case with:

```text
cargo run --release --manifest-path engine/Cargo.toml \
  -p colonist-catan-search --features cuda-sim \
  --bin webgpu-rollout-feasibility -- \
  --profile=development \
  --output=experiments/webgpu-rollout/case-development.json \
  --rollouts=4096 --repetitions=3
```

Serve the repository over a localhost HTTP origin and open:

```text
/experiments/webgpu-rollout/index.html?case=case-development.json
```
