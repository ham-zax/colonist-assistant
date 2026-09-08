# Engine stabilization acceptance — 2026-09-07

## Disposition

**Current v14 candidate: acceptance incomplete. Recorded v13 correctness results are historical evidence. Production exact-CUDA promotion: not accepted. Strategy-strength claim: no evidence of improvement.**

The governing contract is [Engine stabilization and CPU/GPU decision contract](ENGINE_STABILIZATION_AND_CPU_GPU_PLAN_2026-09-07.md). The current source and generated WASM identify the policy as `deep-maxn-v14`; most focused correctness artifacts below record the earlier `deep-maxn-v13` candidate, while a clean revision-matched v14 exact arena smoke is retained separately. Browser `deep-search` remains CPU/WASM-authoritative. The native companion exposes `gpu-root-rollout` only as a distinct experimental algorithm and exposes `deep-maxn-cuda-exact-fixed-work-v1` as a same-policy backend candidate through protocol 7 / state schema 3.

The candidate was developed from `d77935fda984e3006b8a8ad9d65eb58a6d162eaf`. Raw artifacts produced before the final isolated commit are therefore labeled as working-tree evidence rather than pretending to carry the final commit SHA.

## Gate summary

**Loot4438 blockers (source and packaged-WASM repairs verified; installed live acceptance still pending):** the belief-search wave now uses the shared parent deadline instead of per-cell wall-clock slices, and submitted trades retain an explicit pending outcome until authoritative commit or rejection evidence arrives. Focused Rust/TypeScript regressions pass, including the executor and overlay replanning boundary; the generated WASM was rebuilt, and the packaged deep-search adapter passes 30/30 tests. The installed browser build has not yet produced a post-repair consensual live game, so these repairs do not complete release acceptance. The complete export is available locally at `/mnt/c/Users/Hamza/Downloads/colonist-evidence-loot4438-1-2026-09-08T15-16-31-747Z (3).txt`. See the [investigation ledger](OPENING_AND_LIVE_EVIDENCE_INVESTIGATION_2026-09-08.md#loot4438-review-intake--document-before-implementation) for the reproduced mechanisms, repair evidence, and remaining recorder limitation.

Current rebuilt package: `0.9.1 · main@b3b92c31e687+dirty · 2026-09-08T17:23:01.208Z`; packaged WASM SHA-256 `9e805753ebf5fad491f65a2d11e848ef7dc36715846d77db597442704e4ceea8`. The rejected-trade packaged regression now performs three sequential ordinary strategic searches in 5.73 s after the B1 repair; its former 5 s test timeout was therefore stale and was raised locally to 10 s without reducing engine effort. This is evidence that the repaired search no longer abandons the available quality allowance at the old per-cell cutoff, not evidence that every live decision should consume two seconds.

Global no-player-trades benchmark lanes do not exactly match the live policy that disables domestic trading only for our seat. They must not be relabeled as product-route or strength evidence.

The historical artifact rows below describe the recorded v13 implementation session. Focused v14 replay/native repair verification is complete, as recorded in “Focused v14 repair verification” below: exact M0+trades, Mref and M2 checks passed, including cancellation/recovery and invalid-evidence rejection. This is not full release acceptance. Current-revision matched performance is now measured and fails the 2× promotion threshold; packaged execution and strength gates remain unfulfilled. Older artifacts must not be relabeled as v14 results.

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
| Exact end-to-end 2× retention | `benchmarks/engine-stabilization-2026-09-07/exact-gpu-arena-smoke-v14-ff7b9b7.json` | FAIL on clean `ff7b9b7` v14 smoke with parity: 1.455× (3P), 1.375× (4P) |
| Packaged browser recommendation → click → confirmed transition | consensual live browser fixture | NOT RUN: no consensual live-game/browser-installed artifact fixture was available in this session |
| Product strength | held-out matched complete-game campaign for the actual production route | NOT RUN / NOT CLAIMED |

## Correctness findings that changed the implementation

### T4 integration and cooperative cutoff policy — 2026-09-08

Hamza explicitly prioritized robust, fully arbitrated results over the engine's
wall-clock search target. Shared CPU/exact-MaxN finalization now preserves a
completed report through safety/exact-family arbitration after that target
expires. It still honors explicit cancellation during and after arbitration,
and sets `deadlineReached` when the total engine target has expired. The
browser's separate 12-second safety limit and stale-result rejection remain
authoritative. Untimed fixed-work settings and strategic budgets are unchanged.

The failing 50 ms fixture spent 278 ms in root scoring, 24 ms in safety
preparation, 5 ms in the one-ply floor and no time in deeper waves in one local
diagnostic probe. Thus finalization headroom alone could not satisfy an exact
50 ms ceiling. The temporary instrumentation was removed before rebuilding.

On `401956f425b3af68e4e7e57060d6f01b6c74d2fb` plus the uncommitted T1/T3/T4
repairs, the strict compiler check including the otherwise-excluded replay
script passed. The full TypeScript suite passed 416/416 tests after repairing
the protocol assertion, reducing the untimed Mref fixture's work, and supplying
the stale-Mref fixture's known board roll count and matching reconciliation
error expectation. Its late-result rejection assertions remain intact. The
adapter file passed 30/30 tests, including the formerly failing cooperative
cutoff case (about 340 ms) and untimed Mref case (about 1.1 seconds).

`npm run build` succeeded, producing `main@401956f425b3+dirty` at
`2026-09-08T02:35:20.837Z`. The packaged WASM SHA-256 is
`f631e81d82710fa51a4d907a73e1e64d75a41cde4de1ff716d32649529e1327d`.
These results do not constitute full Rust, CUDA, browser, or strength acceptance.

The two previously throwing replay fixtures, `hidden-dev-seed-stability` and
`f14-monopoly-posterior`, completed against that WASM with the existing task14
proxy gate passing and no material task15 omission reported. This was a bounded
two-fixture replay, not a new full-corpus or canonical-request-lane result.

`npm run verify:rust` subsequently passed, including Clippy. The seed-22
fixture now verifies the observed floor/wave agreement and absence of unnecessary
escalation; seed 25 retains the positive escalation case with its disagreement
precondition asserted. Mechanical Clippy repairs reuse existing configuration
and diagnostic-input structs. Full release acceptance remains incomplete.

### road4311 user-provided live logs — 2026-09-08

Evidence: `colonist-evidence-road4311-1-2026-09-08T09-28-36-648Z.txt` and
`colonist-investigation-road4311-1-2026-09-08T09-33-51-164Z.txt`, retained locally
in the user's Downloads folder, not committed. Build: the 02:35:20 artifact above.
The game is a four-player bot game; autopilot was enabled in the evidence export.

- 27 search attempts: 23 completed, four superseded. Both settlement placements
  and both road placements report successful execution. Superseded decisions
  have no recorded execution. This is bounded evidence, not exhaustive stale-action proof.
- Trade execution failed at counteroffer/decline control lookup and at offering
  grain. D15 recorded eight completed searches for one decision. Missing-control
  callbacks reset search state without new strategic evidence.
- Local repairs accept casing variations in the existing semantic class-prefix
  selectors, preserving resource/offer matching and disabled-control checks.
  This is selector hardening, not a proven explanation of D15: its recorded
  evidence already contained a `foreground-disabled` control.
  Missing trade controls retain completed advice and pause automatic execution
  for the failed signature, with a visible manual-completion notice. They do not
  create strategic exclusions. Existing trade-memory scope changes clear the pause.
- Focused action-guide, overlay, and duplicate-history tests passed 88/88;
  type checking passed. A lowercase-class counteroffer regression and the
  missing-control search-retention regressions failed before their repairs.
- Replaying the exact exported dice history through the live constructor yields
  three canonical rolls (7, 10, 10) from six raw board/log observations. A changed
  board total is rejected. Raw-source uncertainty in the export is compatible
  with validated canonical decision evidence; no dice-engine repair was made.
- The separate pasted `tile5854` error has one observed player and a four-point
  victory target. These exports do not contain that game. Tutorial versus
  incomplete roster is unresolved; do not infer a four-player parser failure.

The repaired extension was rebuilt successfully at `2026-09-08T11:40:37.905Z`
(`main@401956f425b3+dirty`). Its packaged WASM SHA-256 is
`e2f228f706ab9779039714d75419dd727fda9520e47007573ce3a23183f90b90`.
This includes the previously verified Rust/Clippy changes; the earlier live
logs and replay timings remain tied to the 02:35:20 artifact.

The repaired trade paths still require a fresh user-run live check. Browser
control is prohibited by the user's current instruction; verification uses local
fixtures and user-provided exports. This does not close T5 or prove stronger play.

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

The historical v13 RTX 3070 Ti smoke completed one matched block per player count, with three seat rotations in 3P and four in 4P. Elapsed CPU/CUDA times were 152,873.6 / 111,593.8 ms (1.3699×) for 3P and 247,970.9 / 194,935.2 ms (1.2721×) for 4P. Both lanes reported matching game results and zero cutoffs, but neither met 2×. That artifact is retained unchanged.

A clean revision-matched v14 smoke was then run from `ff7b9b717bd8bcd07d23b08bf13428b8efac2f6e` with the same 3P/4P smoke profile, seed 9100001, four threads, all-MaxN lineup, player trades disabled, maritime trades enabled and transition validation enabled. Deterministic checkpoint parity passed for all 3P and 4P games. Harness elapsed CPU/CUDA times were 160,185.2 / 110,079.6 ms (1.4552×) for 3P and 281,303.0 / 204,593.2 ms (1.3749×) for 4P. Both sides reported `deep-maxn-v14`, the exact `ff7b9b7...` build SHA and `buildDirty=false`. The result independently fails the 2× gate on the current revision, so production exact-CUDA routing remains disabled. This smoke is a parity/performance gate only; it is not a strength campaign and provides no confidence interval for gameplay quality.

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


## Grain8695 follow-up — 2026-09-08 14:46 UTC

Three live-evidence defects are locally repaired: delayed public board rolls in human games, start-banner/first-settlement hydration ordering, and non-mutating lifecycle notices classified as missing card events. Focused TypeScript checks passed (39 tests, followed by a strengthened third-roll integration case); opening Rust tests passed 30/30; type check and extension build passed. Build: `main@401956f425b3+dirty · 2026-09-08T14:46:42.254Z`.

Opening reproduction matches the recorded choices and node counts. Independent final-placement enumeration verifies the current evaluator's argmax with player trades enabled and disabled. This establishes implementation consistency, not stronger play or optimal opening quality. No opening scoring or GPU promotion change was made. See [the investigation ledger](OPENING_AND_LIVE_EVIDENCE_INVESTIGATION_2026-09-08.md) for opponent-model sensitivity, remaining quality work, and exact verification evidence. Overall stabilization is not newly marked complete by these results.


## Direct trade-control review repair — 2026-09-08

The missing-control pause now renders immediately from the failure callback for incoming trades, builders, partner confirmation, and cancellation. It retains completed advice and disables autonomous execution for the failed signature. Real executor timer regressions cover the direct-control paths without a manually injected failure or post-failure render; action-guide plus overlay tests passed 88/88, and type checking passed. This repairs a local UI-state publication defect, not the outstanding installed-artifact browser acceptance gate.

## Current v14 exact performance smoke — completed

The clean `ff7b9b717bd8bcd07d23b08bf13428b8efac2f6e` arena was rebuilt with CUDA exact support and run through the existing matched 3P/4P smoke protocol: seed 9100001, one block per lane, four threads, all-MaxN lineup, player trades disabled, maritime trades enabled and transition validation enabled. The retained report is `benchmarks/engine-stabilization-2026-09-07/exact-gpu-arena-smoke-v14-ff7b9b7.json` with sibling checkpoint files.

Deterministic parity passed in both lanes: 3/3 3P games and 4/4 4P games matched. Final harness elapsed speedups were 1.455× for 3P and 1.375× for 4P, below the required 2× in both cases. All recorded lane builds identify `deep-maxn-v14`, the exact `ff7b9b7...` SHA and `buildDirty=false`. No full campaign or strength claim is made, and production exact-CUDA promotion remains disabled.
