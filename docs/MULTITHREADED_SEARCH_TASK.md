# Task: multithreaded live search (WASM threads + native rayon)

Repo: `/home/hamza/repo/colonist-assistant`, branch `main`, at or after commit `606e4b6`.
Read `AGENTS.md` and `docs/LAUNCH_REVIEW_2026-09-30.md` (sections 4 and 5) first.

## Goal

Make the live Deep MaxN search use multiple CPU cores in the browser extension and in native builds (arena and tests). The chosen action and every per-root value must stay **bit-identical** to single-threaded search on fixed work. On a 4-player live decision, the search must reach a deeper completed depth within the same time budget.

## Current state (verified facts)

- **Where the engine runs.** It is compiled to single-threaded WASM (`engine/crates/catan-wasm`, export `analyze`) and runs in the MV3 background service worker (`src/background/index.ts`). The build is `scripts/build-wasm.mjs`: `cargo build --target wasm32-unknown-unknown --release`, then `wasm-bindgen --target web`. The output goes to `src/generated/wasm/`, which is committed. The GPU companion was removed from the extension, so the runtime is WASM only.
- **Search structure** (`engine/crates/catan-search/src/depth.rs`, `belief_search_backend`):
  - Stages run in order: particle preparation, root scoring (`normalize_belief_root_priors_with_diagnostics`, per particle), exact families, threat safety (a `forced_loss_weight` loop per root × particle), the one-ply floor (particle × root), then deep waves.
  - Deep waves run one after another (`while target_depth <= maximum_depth`). Each wave uses the previous completed wave's `root_allocation_priority` and `future_self_widening_targets`, so waves cannot run in parallel.
  - **The parallel unit is the cells inside one wave**, one per (particle × root × variant). Each cell builds a fresh `Searcher` with a precomputed node allowance and calls `backend.visit`. Its result is folded in with `accumulate(...)` (weighted sums), along with per-root diagnostics: nodes, cutoff depth counts, future-self mass, terminal mass, and ambiguity evidence. A wave counts only when every cell completes; a deadline mid-wave discards it.
- **Shared mutable state inside one `analyze` call:**
  - `evaluation_cache: Rc<RefCell<HashMap<u64,[f32;4]>>>`: one per call, a pure memo keyed by state hash.
  - `wave_transposition_table: Rc<RefCell<TranspositionTable>>`: one per wave, first write wins. Its key includes every input that affects the value: state, depth, allowance, alpha/beta, and more.
  - `ETA_CACHE` in `economy.rs`: already `thread_local!`, a pure memo.
  - Node counters, deadlines (`CooperativeDeadline`, cloned into each cell), and planner and tactical memos are per cell or per stage.
  - `Rc`/`RefCell` make `Searcher` `!Send`. There is no rayon or threading anywhere in the search crates today.
- **Measured** (native, 20 live requests, fixed work: depth 2, 8,000 nodes per wave; min of 3 runs):
  - 10.0 s single-threaded after commit `e935f92`.
  - WASM is about 1.45× slower than native.
  - On live 4-player states, deep waves take 80–97% of wall time.
- **Benchmark harness (reuse it).** It lives in the scratch directory `/tmp/claude-1000/-home-hamza-repo-colonist-assistant/942ae024-c426-43a7-a328-f9b82688ce34/scratchpad/`:
  - `lever1/` is a native workspace. Its `scratch-bench` binary calls `analyze_maxn_request` through a feature-gated `scratch_analyze` in a copied `catan-wasm`. It depends on the repo's `catan-core` and `catan-search` by path.
  - Usage: `target/profiling/scratch-bench <requests.json> <depth> <reps> [id-filter]`. The requests are in `wasm-perf/requests.json`; per-request stage timings are in `searchStages`.
  - `lever1/out/before.jsonl` holds the reference signatures (chosen action, all per-root values, nodes, depth). `lever1/cmp.py <file>` diffs a run against it. `lever1/bench.sh <binary> <label>` reports the min of 3 runs and the identity check.
  - `wasm-perf/bench.mjs` runs the packaged WASM under Node on the same requests.
  - If the scratch directory is gone, rebuild an equivalent harness under `engine/` or `scripts/` and document it.

## Baseline correction from the supplied checkpoint

The original `before.jsonl` was generated from `e935f92`, before the search
correctness changes in `606e4b6`. The supplied handoff records approval to use
`606e4b6` as R0 and a sequential per-cell-cache implementation as R1. Parallel
R2 must match R1 on fixed work; R0 remains the performance and quality
comparison. Per-cell transposition tables can change bounded search results
relative to R0. The handoff measured a 6.8% sequential slowdown and one lost
completed depth; these costs must remain visible in the results.

The initial request required an uncommitted implementation for review. The
subsequent instruction authorizes committing the threading implementation after
verification, before attempting further per-node optimizations.

## Design (decided; do not change without asking)

1. **Rust, native first:**
   - Add an optional `parallel` cargo feature to `catan-search` that uses `rayon`.
   - Refactor the wave cell loop into two steps: (a) build a `Vec` of cell specs (particle index, root index, variant, allowance, target depth, override), then (b) run them with `par_iter` (sequential `iter` without the feature) into per-cell result structs, then (c) fold the results **in canonical (particle, root, variant) order** with the existing `accumulate` and diagnostic updates, so f32 sums are bit-identical.
   - Replace the `Rc<RefCell<…>>` caches with per-thread caches (`thread_local!`, or a cache owned by each rayon worker). Values must not change. Only hit rates may.
   - Deadline: each cell checks `CooperativeDeadline` as today. If any cell reports `deadline_reached`, the wave is incomplete and is discarded, exactly as now.
   - Then apply the same pattern to the per-particle root-scoring loop, the `forced_loss_weight` loop, and the one-ply floor, if the profile shows they matter.
2. **WASM threads:**
   - Use `wasm-bindgen-rayon`. It needs nightly with `-C target-feature=+atomics,+bulk-memory` and `-Z build-std=panic_abort,std`. Pin the nightly in a WASM-only toolchain file or in the build script; the normal native build stays on stable 1.90+.
   - Emit a threaded WASM next to the existing single-threaded one, and keep the single-threaded build as the fallback.
3. **Extension runtime:**
   - A service worker cannot spawn workers or use Atomics wait. Host the threaded engine in an **offscreen document**: add the `offscreen` permission, create it from the background with `chrome.offscreen.createDocument` (reason `WORKERS`), and run `initThreadPool(n)` inside a dedicated worker there, with n = `min(navigator.hardwareConcurrency - 1, 8)`.
   - Add the manifest keys `cross_origin_embedder_policy: {value: "require-corp"}` and `cross_origin_opener_policy: {value: "same-origin"}` so extension pages get `crossOriginIsolated` and `SharedArrayBuffer`.
   - The background routes decision, cancel and status messages to the offscreen engine. If `crossOriginIsolated` is false or setup fails, it falls back to the current in-worker single-threaded WASM.
   - The status response reports the runtime (for example `background-wasm` vs `offscreen-wasm-threads`) and the thread count. Surface that in the existing runtime badge.
   - **Spike this first.** Before the big refactor, prove that an offscreen document in this extension reports `crossOriginIsolated === true` and can run a trivial `wasm-bindgen-rayon` pool. If it can't, stop and report.
4. **Budget:** after threading works, measure live 4-player decisions: completed depth, nodes, and stage times at 1, 2, 4 and 8 threads. Report those numbers. Do not change the live time or node budgets (`src/worker/deep-search.ts` ~57 and ~1154) without asking; propose a change with data.

## Acceptance

- **Native identity:** with `--features parallel` at 1, 2, 4 and 8 threads, the 20 requests produce byte-identical signatures to the corrected sequential R1 reference: chosen action, per-root values, nodes, depth and authority. Add a `cargo test` that checks sequential vs parallel identity on a few fixed states.
- **WASM identity:** the threaded WASM under Node, if a pool can be started there (otherwise in a headless extension page), gives the same signatures on fixed work.
- **Speed:** report fixed-work wall time at 1, 2, 4 and 8 threads (native and WASM), plus live 4-player depth before and after.
- **Tests and build:** `bun run check`, `bun run test` and `bun run build` pass. `cargo test --workspace` shows no new failures. Three `catan-search` tests already fail on `main` and are out of scope: `road4311_d14_starved_floor_reproduces_live_values`, `two_player_save_for_city_matches_reference`, `strategic_particle_f14_full_posterior_preserves_monopoly_family`.
- **Fallback:** a test proves that the background uses single-threaded WASM when the offscreen engine is unavailable.
- **Docs:** update `README.md`, `PRIVACY.md` and `docs/CHROME_WEB_STORE.md` for the `offscreen` permission (local computation only), and add a results section to `docs/LAUNCH_REVIEW_2026-09-30.md`.

## Rules

- This machine has about 7 GB of RAM shared with other work. Run one heavy job (cargo build or test, bun build, benchmark) at a time, always in the background.
- Never use `pkill -f` with a pattern that also matches your own shell's command line.
- Commit in small conventional commits ending with the repo's co-author line. Do not push. Commit the regenerated `src/generated/wasm/` files with the change that produces them. Leave the worktree clean.
- Ask before changing:
  - the design above;
  - live budgets;
  - any data or export format;
  - the manifest beyond `offscreen` and the COOP/COEP keys;
  - anything else that makes results non-identical.
- You decide: module layout, naming, test structure, the harness location, and which nightly date to pin. List these calls in your report.

## Report

- The commits.
- The identity results (verbatim `cmp.py` output per thread count).
- A speed table (threads × native/WASM × fixed-work ms).
- Live 4-player depth and stage times.
- Spike findings (the `crossOriginIsolated` result).
- Test and build output tails.
- Your judgment calls.
