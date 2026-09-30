# Launch review — 2026-09-30

Pre-launch review of the overlay UX, recorders, launch documents, and the live
search algorithm and performance. Status is as of this date. "Confirmed" means
the finding was checked against the code; "unverified" means it was reported
by an independent review and not yet reproduced.

## 1. Overlay UX (done)

- The overlay font never loaded: Chrome ignores `@font-face` inside a shadow
  root. It is now registered with the FontFace API on `document.fonts`
  (`src/content/fonts.ts`).
- Decisions carry a plain-language reason (`DecisionRationale.plain`,
  `plainDeepSearchReason` in `src/core/engine.ts`) without raw engine values.
  Technical detail stays in the "Why" disclosure and in records.
- The top-moves panel uses "Engine pick / Near tie / Slightly worse /
  Clearly worse / Ranked lower by final checks". Raw values moved to tooltips.
- The opponent-turn view was rebuilt into Threats, Your income (dice coverage
  per resource, including blocked tokens) and Seven risk.
- Settings: diagnostics now live under "Advanced & diagnostics".
- Regression tests: `tests/overlay-ux.test.ts`,
  `tests/decision-rationale-plain.test.ts`, `tests/ui-preview-smoke.test.ts`.
- Preview harness: `bun run ui:preview` renders fixture states to
  `scripts/ui-preview/out/`.

## 2. Recorders (done)

- **Investigation log noise.** `stochastic-input` rows were logged on every
  render: 168 identical rows in one turn, 84% of the log, which evicted real
  evidence from the 1,200-entry ring. Four channels now fold identical
  repeats into the original row with a `repeat` count
  (`FOLDED_CHANNELS` in `src/content/investigation-recorder.ts`).
- Storage-quota failures are caught and reported in the export header
  (`@saveError`). The export header also carries `@build` and `@exported`.
- Reset now clears the investigation log.
- **Game record.** The persist throttle went from 220 ms to 1,500 ms. A
  generation guard drops captures started before a reset. Export reads the
  in-memory record first.
- The compact evidence export is about 194 KB per 5 minutes of play; 58% of
  that is `roots`. It is left as is: each bucket is capped at 12 rows, and the
  pruned-root bucket is the evidence needed for finding A10 below.

## 3. Launch documents (done except where noted)

- `PRIVACY.md` and `docs/CHROME_WEB_STORE.md` now disclose `nativeMessaging`
  (the optional local GPU companion) and the user-initiated game-record and
  investigation-log exports.
- Autopilot copy now says "friendly games where all players agree" (the
  popup, the overlay, and a test).
- Links point to `ham-zax/colonist-assistant`; `package.json` declares MIT.
- **Open:** the store screenshots in `assets/screenshots` are stale (old name
  and UI).
- **Open:** whether to drop `nativeMessaging` and the companion from the store
  build (recommended once multithreaded WASM lands, see §5).

## 4. Search algorithm review (live Deep MaxN path)

Independent review (Codex, gpt-6.1-sol), plus our own verification.

| # | Status | Location | Finding | Smallest fix |
|---|---|---|---|---|
| A1 | confirmed | `catan-search/src/depth.rs:1399` | The observation-safe opponent mixture truncates `ranked` to `remaining` without renormalizing, which biases opponent-node values toward 0. | Do not truncate the opponent branch (unfunded children are already `evaluate_cached`), or renormalize the kept weights. |
| A2 | confirmed | `catan-search/src/threats.rs:473` | Settlement and city affordability are checked independently, so a hand that affords only one counts as both. `largest_army_outlook ≥ 0.95` is used as an immediate-award check. | Validate the combined line on a scratch state. |
| A3 | unverified | `catan-search/src/threats.rs:681` | The blocker check returns false when `current_player == protected`, which admits irrelevant moves as blockers. | Verify, then gate on the threatened player. |
| A4 | confirmed | `catan-wasm/src/lib.rs` | The deadline does not bound planning, preparation or finalization, and the WASM cancel callback is always `false`. Measured: about 6.8 s of root scoring and threat safety on a 24-particle 4p state, which leaves the search at depth 0. | Parallelize those per-particle loops and check the deadline inside them. |
| A5 | intentional | `src/worker/deep-search.ts:57` | 4-player live budget is 10 s / 48k nodes per wave (commit 1e6b12a), against the 12 s hard cutoff. | Revisit after the speed work. |
| A6 | perf | depth search edges | Two `state.clone()` + `apply` per edge. | Use `clone_from_and_apply`. |
| A7 | perf | depth search nodes | Legal moves are generated repeatedly, and before the transposition lookup. | Look up first; generate once. |
| A8 | quality | depth search | Greedy self-continuation. | Needs arena evidence. |
| A9 | quality | opponent model | The 3-action opponent support can drop EndTurn, trades and development plays. | Needs arena evidence. |
| A10 | quality | root admission | The root cap of 10 can drop every development line. | Needs arena evidence. |
| A11 | perf | `eval.rs` | Longest road is computed 16× per evaluation. | Compute once per evaluation. |

No wrong-player backup, chance-weighting error or hidden-card leak was found.
Per AGENTS.md, A1, A2 and A8–A10 need matched arena runs before and after.

## 5. Performance

### Measurements (WASM under Node 24; 10 real decision states × 2 seeds)

Fixed work is depth 2, branch cap 10, 8,000 nodes per wave and no deadline,
117,820 nodes per pass. Each figure is the median of 3 runs.

| Variant | wasm bytes | fixed-work ms | nodes/s | action mismatches |
|---|---|---|---|---|
| A: current release build | 1,257,589 | 48,837 | 2,413 | 0 |
| B: `+simd128` | 1,246,864 | 48,481 | 2,430 | 0 |
| C: A + `wasm-opt -O3` | 1,190,234 | 48,303 | 2,439 | 0 |
| D: B + `wasm-opt -O3` | 1,181,226 | 46,617 | 2,527 | 0 |

- `wasm-opt` needs explicit feature flags; `--all-features` output fails to
  instantiate with `unknown import kind 0x7f`.
- SIMD and wasm-opt are within run-to-run noise (about 5% at best).
- A native build of the same search is 1.43–1.55× faster than WASM, with
  identical results.
- About 2,400 nodes/s is a very high per-node cost; each node is ~0.4 ms.

Live-setting stage timings for variant A (4p: 10 s and 48k nodes per wave;
3p: 2 s and 8k):

- On most states the deep waves take 80–97% of wall time, so they are the
  parallel target.
- The 4-player states reach a deepest completed depth of only 1 in 10 s.
- `hidden-dev-seed-stability` (4p, 24 particles) spends about 3.4 s in root
  scoring and about 3.4 s in threat safety before the waves begin. It ends at
  depth 0. This confirms A4: preparation is serial and is not bounded by the
  deadline. Threading therefore also has to cover the per-particle
  root-scoring and threat-safety loops.

At the baseline revision there was no search-internal threading; the arena
parallelized only whole games.

### Where the time can come from

1. **Behavior-preserving engine speedups (A6, A7, A11):** expected 1.5–2×.
   Gate: identical chosen actions on fixed-work runs.
2. **Multithreaded WASM:** expected 3–6× with 4–8 threads. Design below.
3. **Cross-decision reuse** (keep the transposition table and use ponder
   results): later.

Rejected:
- **WebGPU port.** The CUDA companion is only 1.27–1.37× faster than CPU, WGSL
  has no u64 (state hashes are 64-bit), and the port is about 7k lines of
  branchy CUDA.
- **SIMD:** measured no gain.

### Multithreading design

Structure of the search (`belief_search_backend`, `depth.rs:1952+`): one tree
over the weighted posterior, run as iterative depth waves. Within a wave, each
(particle × root × variant) cell gets a fresh `Searcher` with a precomputed
node allowance. Results are accumulated with particle weights, and a wave is
committed only when complete. Waves are sequential: root budget priority and
widening targets come from the previous completed wave.

Therefore:

- **Parallel unit:** the cells of one wave. Collect the cell specs, run them
  in parallel, then merge with `accumulate` in the canonical
  (particle, root, variant) order, so the f32 sums are bit-identical to the
  sequential run.
- **Shared state:** `evaluation_cache` (whole call) and the per-wave
  `TranspositionTable` are `Rc<RefCell<…>>` memos. The implementation makes transposition tables cell-owned; a shared table
  can change budgeted subtree values and node counts through cache hits. Pure
  evaluation memos may change hit rates without changing their values.
- **Runtime:** separate WASM instances without shared memory cannot sync per
  wave inside one `analyze` call. The plan is real WASM threads
  (`wasm-bindgen-rayon`, which needs nightly `build-std` with atomics) running
  in an offscreen document (the `offscreen` permission). The manifest sets
  `cross_origin_embedder_policy` / `cross_origin_opener_policy` so extension
  pages are cross-origin isolated and get `SharedArrayBuffer`. The native
  build uses rayon too, so the arena and companion benefit.
- **Acceptance:**
  - Native test: parallel and sequential results are identical on fixed work.
  - A spike confirms `crossOriginIsolated` in the offscreen document.
  - Nodes per second are measured at 1/2/4/8 threads.
  - Sequential fallback when isolation is unavailable.


## 6. Multithreaded search implementation and verification

The extension packages portable WASM and a threaded WASM build. An isolated
MV3 offscreen document hosts a dedicated engine worker and a Rayon pool of
`min(max(hardwareConcurrency - 1, 1), 8)` threads. Setup failure selects the
portable background engine; later execution errors do not repeat a dispatched
search. New requests check offscreen readiness before choosing their backend.
Queued cancellation removes work; active cancellation suppresses the stale
result and waits for synchronous WASM to finish before the next search.

Native Rayon covers root scoring, blocker verification, the posterior-wide
one-ply floor, and cells within each deep wave. Cell results fold in canonical
order. Deadline-expired queued cells do not start, and node diagnostics count
all joined work even when a wave is discarded. Only completed waves supply root
evidence. No live time or node budgets changed.

### Baseline and identity

The supplied checkpoint establishes that the old `before.jsonl` came from
`e935f92`, not `606e4b6`. It records approval to use `606e4b6` as R0 and the
sequential per-cell-cache implementation as R1. The per-cell transposition
trade-off can change bounded results relative to R0; the supplied checkpoint
reported a 6.8% sequential regression and one lost completed depth. Those
historical measurements were not rerun here. R2 must match R1 on fixed work.

Fresh native replay compared all 20 requests (depth 2, 8,000-node wave
allowance, no clock or escalation): chosen action, all per-root statistics,
nodes, completed depth, authority, and deadline flag. All 1/2/4/8-thread
comparisons printed `compared:20`, `mismatch:[]`, `extra:[]`,
`duplicateIds:false`. The in-tree gate additionally compares the actual
sequential iterator path with every thread count on 2-, 3-, and 4-player states;
float values are compared by their bits and provenance is checked.

Browser portable versus threaded replay also matched all 20 fixed-work
signatures at 1/2/4/8 threads. Native and browser comparisons are separate; JSON
formatting across runtimes is not an identity gate.

### Fixed-work speed

All configurations processed the same 127,274 reported search nodes. Native
numbers are the minimum of three runs; browser numbers are one run. This is a
shared, noisy machine, not a clean-machine microbenchmark.

| Configuration | Native ms | Native nodes/s | Browser WASM ms | WASM nodes/s |
| --- | ---: | ---: | ---: | ---: |
| Sequential portable | 10,303 | 12,353 | 17,218 | 7,392 |
| Rayon 1 | 10,883 | 11,695 | 17,535 | 7,258 |
| Rayon 2 | 6,051 | 21,035 | 10,479 | 12,146 |
| Rayon 4 | 3,711 | 34,294 | 11,985 | 10,620 |
| Rayon 8 | 3,030 | 42,008 | 18,660 | 6,821 |

Native 8-thread throughput is 3.40× the sequential implementation. Browser
2-thread throughput is 1.64× portable; 8 threads regress. The browser reported
six hardware threads, and host load averaged approximately four. More browser
threads are not an automatic win. The earlier approximately 2,400 nodes/s was
an observed rate before the earlier engine optimizations, not a configured node
limit. These runs include the prior performance and correctness commits and
process 127,274 rather than 117,820 nodes. Only the portable-versus-threaded
comparison in this table measures the additional gain from threading.

### Four-player completed depth

Two representative fixture replays used the current live allowance:
10,000 ms, 48,000 nodes per wave, depth cap 5, unchanged escalation settings.
These are captured-state replays, not fresh live games or strength estimates.

| Threads | Recovered turn 54 | Hidden development posterior |
| --- | ---: | ---: |
| Portable | 2 | 1 |
| 1 | 2 | 1 |
| 2 | 2 | 2 |
| 4 | 3 | 2 |
| 8 | 2 | 1 |

At four threads both fixtures reached a deeper completed decision depth within
the same time and node budgets. Those initial timed measurements preceded the
fix to report nodes spent in discarded waves, so their node counts are not used
for throughput claims. Final packaged measurements are recorded below.

### Real offscreen spike and packaging

Chrome reported an actual `OFFSCREEN_DOCUMENT` with `tabId: -1`; both that
document and its engine worker were cross-origin isolated. WASM memory used a
`SharedArrayBuffer`, `initThreadPool(4)` completed, and a Worker-constructor probe
counted four actual Rayon workers. An actual request returned depth 1 and 1,441
nodes. This supersedes the earlier Atomics-only probe.

The WASM-only toolchain is `nightly-2025-11-15` with `rust-src`; native and
portable builds stay on stable. The build adapts wasm-bindgen-rayon's
bundlerless helper to a packaged worker URL under the existing MV3 CSP and
preserves its Apache license. Manifest additions are limited to `offscreen`
and COOP/COEP. README, privacy policy, and store permission copy explain that
all computation remains local.

### Final packaged smoke and checks

The final packaged extension reported `offscreen-wasm-threads`, five threads
on the six-thread browser, and `deep-maxn-v14`. Cold pool initialization took
190.6 ms. A temporary relay appended only to the disposable extension copy
sent raw fixture requests from its service worker through the production
offscreen listener; no relay or private fixture is included in the product.

| Four-player fixture | Elapsed ms | Reported nodes | Completed decision depth | Retained roots with completed work |
| --- | ---: | ---: | ---: | ---: |
| Recovered turn 54 | 10,061 | 97,725 | 2 | 10/10 |
| Hidden development posterior | 10,086 | 66,149 | 2 | 10/10 |

Both returned Deep MaxN authority, a complete one-ply floor, and a cooperative
deadline. Counts include joined work in unfinished waves; root evidence still
comes only from completed work. These final five-thread runs improve the hidden
posterior fixture's depth over the portable replay, while the recovered fixture
stays at depth 2. After the offscreen document was closed, the next ordinary
status request selected `background-wasm` with one thread, confirming fallback
for later requests without redispatching an existing analysis.

Verification:

- `bun run check`: passed.
- `bun run test`: 47 files, 525 tests passed; focused runtime tests also passed.
- `bun run build`: passed, including both packaged WASM variants.
- Sequential versus Rayon 1/2/4/8 fixed-work gate: passed on 2/3/4-player states.
- Release Rust workspace search suite: 231 passed, 16 ignored, and exactly the
  three previously known failures: `strategic_particle_f14_full_posterior_preserves_monopoly_family`,
  `two_player_save_for_city_matches_reference`, and
  `road4311_d14_starved_floor_reproduces_live_values`. Earlier workspace crates
  passed; Cargo stops at the failed search crate. Separate WASM tests: 3 passed.
- Debug `verify:rust`: the identity gate passed; a pre-existing debug opening
  test exceeded ten minutes and was interrupted. The release run above completed.
- Strict Clippy remains failing on pre-existing warnings (argument count,
  collapsible conditions, and fixture type complexity); new warnings in the
  changed code were corrected. The combined `bun run verify` gate is not green.
- `git diff --check`: passed. Private fixtures and benchmark output stay outside
  the repository. A fresh live-game strength evaluation and the reference-machine
  cold search smoke were not run; pool initialization is not the full cold search.

The separate port-economy memo experiment remains an unvalidated patch outside
the working tree. It has no measured speed claim and is not part of this
threading implementation.

### Reproduction

Use the externally supplied request corpus; do not commit private fixtures or
benchmark output. Native harness:

```bash
cd engine
cargo build --release -p colonist-catan-wasm --example threading_benchmark --features native-benchmark
./target/release/examples/threading_benchmark /absolute/path/requests.json fixed 3 > /tmp/threading-sequential.jsonl
cargo build --release -p colonist-catan-wasm --example threading_benchmark --features native-benchmark,parallel
RAYON_NUM_THREADS=4 ./target/release/examples/threading_benchmark /absolute/path/requests.json fixed 3 > /tmp/threading-four.jsonl
cd ..
node scripts/compare-threading.mjs /tmp/threading-sequential.jsonl /tmp/threading-four.jsonl
```

Repeat with 1/2/4/8 threads. The browser/offscreen harness is documented in
[scripts/threading-browser/README.md](../scripts/threading-browser/README.md).
Both harnesses leave search budgets unchanged in fixed work except disabling
the clock and selecting depth 2. Timed `live-current` replay uses the existing
four-player configuration.
