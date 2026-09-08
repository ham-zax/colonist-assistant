# Engine stabilization acceptance — 2026-09-07

## Disposition

**Current v14 candidate: acceptance incomplete. Recorded v13 correctness results are historical evidence. Production exact-CUDA promotion: not accepted. Strategy-strength claim: no evidence of improvement.**

The governing contract is [Engine stabilization and CPU/GPU decision contract](ENGINE_STABILIZATION_AND_CPU_GPU_PLAN_2026-09-07.md). The current source and generated WASM identify the policy as `deep-maxn-v14`; the artifacts below record the earlier `deep-maxn-v13` candidate. Browser `deep-search` remains CPU/WASM-authoritative. The native companion exposes `gpu-root-rollout` only as a distinct experimental algorithm and exposes `deep-maxn-cuda-exact-fixed-work-v1` as a same-policy backend candidate through protocol 7 / state schema 3.

The candidate was developed from `d77935fda984e3006b8a8ad9d65eb58a6d162eaf`. Raw artifacts produced before the final isolated commit are therefore labeled as working-tree evidence rather than pretending to carry the final commit SHA.

## Gate summary

The PASS entries below describe the recorded v13 implementation session, not acceptance of the current v14 request/search orchestration. The v14 review found replay effort normalization and untimed native parity request defects. Their repairs require focused verification; older parity artifacts must not be relabeled as v14 results. Packaged execution and strength gates remain unfulfilled.

| Gate | Evidence | Result |
| --- | --- | --- |
| One production algorithm | Background/native routing and protocol-7 capability contract | PASS: companion availability cannot replace MaxN with rollout |
| Replay/effective-request fidelity | `scripts/replay-engine.ts`, recommendation audit fixtures, adapter checks | PASS for the supported reconstructed/canonical lanes; incomplete legacy Mref evidence stays explicitly limited |
| Controlled future-self semantics | focused search regressions plus exact-backend parity | PASS: controlled player uses pre-quota observation-safe argmax; opponents retain modeled mixture |
| M2 evidence discipline | strategy/reachability regressions and diagnostics | PASS as an opt-in experiment; no production promotion |
| End-to-end deadline ownership | worker/background/engine deadline checks | PASS for the implemented duration/cancellation contract; timed child cutoffs propagate `deadlineReached` |
| Exact evaluator parity | `benchmarks/engine-stabilization-2026-09-07/exact-gpu-evaluator-parity.json` | PASS: 69 states, 2P/3P/4P, max abs error `2.9802322e-7`, zero failures |
| Exact fixed-work search parity | `benchmarks/engine-stabilization-2026-09-07/exact-gpu-search-parity.json` | PASS: 15/15 exact action matches, identical node/depth work, max abs error `5.9604645e-7` |
| Production-shaped native exact parity | `benchmarks/engine-stabilization-2026-09-07/native-host-exact-parity.jsonl` | PASS: M0+trades, Mref, explicit M2; max abs error `3.8743019e-7`; invalid evidence rejected; cancellation recovered |
| Live/recorded board topology | same native exact parity lane | PASS after repair: exact evaluator prepares the actual request adjacency topology instead of assuming canonical generated indices |
| Frozen arena product route | `benchmarks/engine-stabilization-2026-09-07/native-exact-takeover-smoke.jsonl` | PASS: protocol 7 exact capability required, exact algorithm recorded, 7,987 nodes, depth 2, zero protocol/illegal-action failures, terminal completion |
| Exact end-to-end 2× retention | `benchmarks/engine-stabilization-2026-09-07/exact-gpu-arena-smoke.json` | FAIL for recorded v13 smoke: 1.370× (3P), 1.272× (4P) |
| Packaged browser recommendation → click → confirmed transition | consensual live browser fixture | NOT RUN: no consensual live-game/browser-installed artifact fixture was available in this session |
| Product strength | held-out matched complete-game campaign for the actual production route | NOT RUN / NOT CLAIMED |

## Correctness findings that changed the implementation

### Focused v14 repair verification — 2026-09-08

The replay builder now normalizes GPU root capacity, rollout count and horizon to the Rust contract before checking effective effort. The shared CPU/exact-MaxN request owner preserves explicit zero-time fixed work and disables evidence escalation for that mode; defaults, positive time limits and experimental rollout time floors remain unchanged. Native parity checks assert returned zero time and matching effective effort.

Observed on the current v14 working tree:

- `npm run check`, Rust formatting and diff whitespace checks passed.
- The focused Rust test `fixed_work_effort_is_explicit_and_maxn_only` passed, covering legacy/nested zero-time requests, defaults, positive limits and disabled escalation.
- The actual replay configuration and effort-check functions passed against rebuilt WASM for live, medium and maximum profiles on the existing robber-move control fixture. This verifies effort fidelity, not the full replay corpus or historical action reconstruction.
- `npm run build:wasm` and `npm run build:companion` succeeded.
- `node scripts/verify-mref-native.mjs` passed on the RTX 3070 Ti: nine exact comparisons across M0+trades, Mref and explicit M2 at requested depths 1/3/5 matched chosen actions, authority, nodes, completed depth, retained roots, root-work diagnostics and effective effort. Maximum absolute value error was `3.8743019104003906e-7`. Timed cutoff retained a chosen action, invalid evidence was rejected, and cancellation plus host recovery completed in approximately 1,502 ms.

These are focused repair results, not a new full acceptance or performance campaign. The v13 raw artifacts below remain unchanged; no new matched arena speedup, packaged live-click, held-out strength or full-suite result is claimed.

### Exact search effort schedule

The older exact search used one direct maximum-depth traversal while CPU fixed-work MaxN retained the last complete iterative depth wave under one global node budget. A depth-1 falsification probe matched exactly; depth-4 diverged. Exact CUDA now uses the same iterative fixed-work schedule and preserves the original decision budget for root-planning provenance. The current 15-case search gate matches actions, values, nodes and completed depths.

### Recorded/live board topology

The exact evaluator originally uploaded topology from `Board::randomized_base_v1(0, 4)` and only validated live board counts (`19/54/72`). A production-shaped D68-derived request had different index adjacency: all 54 vertex entries and 66 of 72 edge entries differed from that canonical ordering. CPU and CUDA evaluator values therefore diverged before search despite synthetic parity passing. `CudaExactEvaluator` now caches the currently prepared topology, uploads the actual request board topology when it changes, and rejects mixed-topology belief batches. Production-shaped CPU/native exact values then agreed within the frozen tolerance.

### Arena release lane

The existing native takeover lane still used protocol 6 / state schema 2 and sent the ambiguous rollout `analyze` request. It now requires protocol 7, verifies `exactMaxn.algorithm == deep-maxn-cuda-exact-fixed-work-v1`, sends `analyze-exact`, verifies the response algorithm, records exact MaxN nodes/depth/search-particle counts, and labels the outcome `native-gpu-exact-maxn`. Historical schema-1 takeover snapshots default `playerTradesEnabled=true` only because their embedded source revision predates the no-player-trades option; this is a version-backed migration, not a general missing-evidence fallback.

## Strategy roadmap relationship

The stabilization contract takes priority over the adaptive-strategy milestone roadmap. M2 (`adaptive-candidate-admission-v1`) remains explicit and opt-in. The bounded M2 pilot is neutral/non-positive: no admitted challenger became the common-search winner. M3 candidate reconsideration, M4 transition-aware economics and M5 full contingent continuation were intentionally not started. Learned value/policy promotion remains disabled.

Exact CUDA supporting explicit M2 in parity tooling does not promote either M2 or exact CUDA. `gpu-root-rollout` still rejects the M2 strategy identity because it is a different algorithm.

## Performance and promotion rule

The exact-backend correctness gates are necessary but not sufficient. The existing acceleration contract requires at least **2× end-to-end elapsed speedup in both the 3P and 4P matched arena lanes**. The candidate must also retain cancellation/deadline semantics and obtain packaged browser execution evidence before any production exact routing is enabled.

The recorded RTX 3070 Ti smoke completed one matched block per player count, with three seat rotations in 3P and four in 4P. Elapsed CPU/CUDA times were 152,873.6 / 111,593.8 ms (1.3699×) for 3P and 247,970.9 / 194,935.2 ms (1.2721×) for 4P. Both lanes reported matching game results and zero cutoffs, but neither met 2×. This small smoke provides no confidence interval or strength claim and does not measure the later v14 candidate. The historical artifact is retained unchanged.

If the 2× gate is not met, exact CUDA remains experimental/fixed-work parity tooling and production stays CPU/WASM. The gate is not weakened after observing the result.

## Frozen future experiment order

No 10-second production budget is promoted from the old GPU win-rate screen. If later strategy/budget work resumes, freeze and run the experiments in this order:

1. depth-cap change with node budget fixed;
2. node-budget change at fixed depth;
3. M2 off/on at otherwise identical work;
4. only then consider M3/M4/M5 as independent mechanisms.

Record per-root completed work, future-self reach mass, cutoffs, latency and matched-block gameplay outcomes. Do not use the resident `gpu-weighted` screening win rate as a browser-strength estimate.

## Remaining limitations

- Exact CUDA opening placement is unsupported and must use the same-policy CPU opening owner.
- Production exact routing remains disabled unless every promotion gate is satisfied.
- No browser-installed extension/companion artifact pair was exercised in a consensual live game in this session.
- No held-out product-strength campaign was run; backend parity does not establish stronger play.
- The named Mref law remains a public/reference hypothesis, not proof of Colonist's private server implementation.
- The reported bad live game itself was not available as a complete installed-artifact/browser execution fixture.

## Strength conclusion

**no evidence of improvement**

This candidate improves reproducibility, backend correctness, routing discipline and evidence quality. It does not establish a playing-strength gain, a live win-rate increase, or a reason to promote M2, learned models, rollout GPU, or a larger production time budget.
