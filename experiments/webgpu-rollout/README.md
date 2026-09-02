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
