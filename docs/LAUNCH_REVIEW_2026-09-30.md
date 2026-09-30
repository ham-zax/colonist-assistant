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
| A4 | unverified | `catan-wasm/src/lib.rs` | The deadline does not bound planning, preparation or finalization, and the WASM cancel callback is always `false`. | Stage timings (in progress) will show whether this matters. |
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

### Measurements (WASM, node 24, 10 real decision states × 2 seeds)

| Variant | Result |
|---|---|
| A: current release build | deadline runs: 34.7 s total, 141,525 nodes |
| B: `+simd128` | deadline runs: 35.0 s total, 138,123 nodes (no gain) |
| C/D: `wasm-opt -O3` | output failed to instantiate (`unknown import kind 0x7f`) |

On 4-player states the live search completes only depth 1 (about 2k nodes)
inside a 2 s budget. There is no search-internal threading; the arena
parallelizes only whole games.

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
  `TranspositionTable` are `Rc<RefCell<…>>` memos. Both become per-thread;
  keys include every value-affecting input, so this changes node efficiency,
  not values.
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
