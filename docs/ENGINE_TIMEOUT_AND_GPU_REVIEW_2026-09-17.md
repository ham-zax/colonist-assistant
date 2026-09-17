# Engine timeout, GPU routing, and fan-out defect triage — 2026-09-17

The initial triage is retained below as historical context. The
[follow-up review and fixes](#follow-up-review-and-fixes) supersedes its verdicts
and verification counts. Some candidates were contradicted by existing guards
and recovery tests; they should not be treated as confirmed live bugs.

Scope: current checkout at `1ad3280` (branch ahead of `origin/main` by 2).
Typecheck: `npm run check` passed. Tests: 463 passed, 2 failed in
`tests/background-mref-routing.test.ts` (stale routing/message expectations).
Live Brave and native GPU behavior were not exercised.

## Confirmed findings (direct source review)

### F1 [Major] Silent GPU handshake can wedge later decisions
`src/background/native-gpu.ts:132-143,274-284,353-377`

`status()` caches one `connectPromise`. If the companion connects but never
answers `hello`, that promise never settles and every later eligible decision
reuses it. The 12-second client timeout (`src/content/decision-worker.ts:265-271`)
only cancels an active analysis (`cancelDecision`), not initialization, so
eligible decisions keep timing out until release or service-worker restart.

Fix direction: bound the `hello` wait, tear down the pending port, and fall
through to the existing same-policy CPU/WASM path.

### F2 [Major] Concurrent tabs share one GPU client and numeric decision IDs
`src/background/index.ts:25,120-129,202`
`src/background/native-gpu.ts:229-234,265-272`

All tabs share one `NativeGpuClient`. A new `analyzeWithType` cancels the
active analyze as "superseded", even when it belongs to another tab.
Cancellation keys on numeric `message.id` only (each content client restarts
at 1) and the background listener ignores the sender, so cross-tab ID
collisions can cancel unrelated work.

Fix direction: key GPU work by tab/document plus decision ID; serialize or
isolate unrelated tabs instead of treating them as superseding decisions.

### F3 [Test] Routing tests encode retired policy
`tests/background-mref-routing.test.ts:125-127,159-185`

Two failures are stale expectations, not proof of live misrouting:

- Strategy-policy test expects `remains on CPU/WASM`; source says
  `remains on its CPU/WASM owner` (`src/background/index.ts:233`).
- Disconnected-companion test expects the old never-consult policy; production
  routing now supports capable exact-MaxN companions plus transport recovery.

The second test needs behavioral assertions updated alongside the message
regex. These failures block `npm run verify`.

## Fan-out candidates (subagent-reported, spot-checked, not yet fixed)

Two auditor subagents failed to start (free-tier provider restriction), so
decision-timeout and session-storage coverage below comes only from the two
successful explorer passes plus targeted re-reads. Treat items G1–G8 as
triage input, not confirmed verdicts.

### G1 [Major candidate] Robber-victim detection is DOM-text-only
`src/page/bridge.ts:569-599`, contrast discard fallback `603-609`

Verified: victim prompt matches `[class*='actionBoxContainer-']` plus
`/choose a player to steal/iu` and scrapes `[class*='playerName-']`.
Discard has a store fallback (`actionBoxData?.type === "pickCard"`);
victim has none. A selector rename, reword, or out-of-container render
empties `robberVictimSelection`, and `overlay.ts:3911` then never produces
the mandatory steal step. Per implementation rules, DOM selectors are
unstable — this path needs a validated bridge/store fallback.

### G2 [Major candidate] Failed transfers fabricate cards
`src/core/tracker.ts:291-296`

Verified: when no world can afford a transfer, every world credits the
receiver without debiting the sender. This violates conservation and can
poison downstream bank-equality checks. Fix at this shared function rather
than per caller.

### G3 [Major candidate] Public reconciliation can empty all worlds
`src/core/tracker.ts:1105-1113,1115-1126,184-198`

Verified: `handSizes` and `bank+supply` filters assign `worlds = matching`
unconditionally. `normalizeWorldWeights([])` preserves `[]`, so one stale
public snapshot yields zero worlds and downstream
`no resource worlds consistent` throws. The exact-hand path above it keeps
old worlds and warns; these two do not.

### G4 [Major candidate] Discard simulation zeroes pre-root seats
`src/worker/deep-search.ts:1598-1611`

Verified shape: `discardRemaining` returns 0 for `index < root` and sets
`discardCursor` to root. The cited core rule scans from seat 0
(`state.rs:909-925,1023-1025`). Needs a rules-owner check: if discards
resolve in seating order from the roller rather than from the search root,
opponent discards are under-simulated and robber value is overestimated.

### G5 [Major candidate] Balanced-dice games can search under fair-iid
`src/worker/deep-search.ts:1379,1678-1680`

Verified shape: missing `stochastic` defaults to `M0_FAIR_IID_2D6_V1` while
`diceMode: board.diceMode` is still forwarded. On a balanced board with
gapped history this splits payout rules from roll sampling silently.
Fail closed or plumb the live stochastic builder instead of defaulting.

### G6 [Medium candidate] Dev-card and victim workflows skip per-step asserts
`src/content/action-guide.ts:2041-2094,2347-2353`

Plausible: trade steps get per-step `validateState`; dev/victim steps define
only `resolve`/`settleMs`, with commit validation at the end. A mid-workflow
board advance can dispatch stale follow-up clicks. Needs a live repro before
fix.

### G7 [Medium candidate] Build advice ignores affordability
`src/content/overlay.ts:2197-2199`, `src/content/action-guide.ts:1095-1115`

Plausible: `build` NextClick checks turn/phase but not hand vs cost, so a
post-search rob/spend leaves stale advice in a disabled-control retry loop.
Cheap guard at the advice layer; verify against the cost helper first.

### G8 [Minor candidate] Trade validator is index-based, executor is index-free
`src/content/overlay.ts:2154`, `src/content/action-guide.ts:642-680`

Plausible: concurrent offers can shift `activeTrades[offerIndex]`, failing
the `trade.id` check while the DOM matcher would still succeed. Fails
closed (missed window/flicker), not a wrong click. Consider validating by
trade ID lookup instead of index.

## Recommended next actions

1. Decide on F1/F2 fix scope before touching G-items.
2. Reproduce G1/G6/G7 in a live or harness-driven session; promote only on
   red reproduction per diagnosing-bugs.
3. Update the two stale routing tests as part of any routing change.
4. Keep this file as triage log; move promoted items into focused fixes with
   regression tests at the real seam.

## Follow-up review and fixes

Reviewed against `1ad3280` plus the local fixes described here. This follow-up
was performed inline without subagents. Scope: native initialization and
decision ownership, readiness callbacks, the cited tracker/adapter paths, and
the cited bridge/action validation paths. It is not an exhaustive repository
audit. Satori's publication reported uncertain freshness, so material claims
were checked against current source. Codebase Memory reported no recorded
coverage gaps for the relied-on source/test paths; that is not a completeness
guarantee.

### Disposition of the original findings

| Item | Result | Evidence and change |
| --- | --- | --- |
| F1 | Fixed | `NativeGpuClient` bounds `hello` to 2,000 ms, clears the pending timer and closes the port on timeout. Cold unavailability permits subsequent WASM decisions; a failed reconnect is classified as a transport failure. |
| F2 | Fixed | Background request identity includes tab/document/frame/client ID; native IDs are globally distinct within the service worker. A single native owner prevents cross-tab supersession. Contending requests use the same policy and stochastic model on WASM. Weighted requests/status checks cannot release another decision's active GPU port. |
| F3 | Fixed | Both obsolete routing assertions now test current behavior, including Mref-preserving transport recovery. New concurrency and cancellation cases cover the routing boundary. |
| G1 | Fixed, with a verified store schema | The bridge recognizes the public `pickPlayer` action box using the robber title and `selectWhoToRobFrom` translation keys plus `playerValidators[].color`. It checks local identity/turn ownership and restricts candidates to roster opponents with publicly nonempty hands. Other player-selection dialogs are excluded. |
| G2 | Narrowed and fixed | The real error was retaining the sender's known cards when only part of a transfer was affordable. The recovery path now debits that known part, credits the observed receipt, and keeps its missing-history warning. An observed transfer can still imply cards absent from the recorded history; blindly conserving an already-incomplete prior would also be wrong. Valid supported transfers still conserve cards and filter incompatible worlds. |
| G3 | Refuted as a missing-recovery bug | Empty support is deliberate: `tests/belief-filter.test.ts` requires rejecting contradictory public hand totals. `AssistantOverlay.reconciledState()` already attempts a physically consistent public-board posterior; `tests/recommendation-integrity.test.ts` checks this recovery and bank conservation. Preserving contradictory worlds would weaken the evidence contract. Inconsistent public snapshots may still pause analysis intentionally. |
| G4 | Representation gap remains; see below | Pre-root obligations are omitted. The current test explicitly expects `[0, 4, 5]` for hand sizes `[8, 8, 10]` with root seat 1. Correcting this requires coordinated transition handling in both Rust and CUDA. The original claim about the magnitude/direction of recommendation bias is unproven. |
| G5 | Live-path allegation refuted | `scheduleDecisionAnalysis()` calls `buildLiveDecisionStochasticInput()` and stops on unusable Balanced Dice evidence before sending a decision. Existing live recovery/revocation tests verify this. The lower-level adapter's M0 default remains available for explicit offline/comparison callers; its existence does not establish a live silent downgrade. |
| G6 | Broad allegation refuted; narrower guard fixed | `startWorkflow()` calls `validatedClick()` with `validateContinuation` before automatic follow-up clicks, including development cards. A regression now checks loss of validity between card and confirmation. The victim continuation guard was too permissive: any normal action phase could pass after the picker closed. It now requires the active victim prompt and the selected player in its legal target set. |
| G7 | Fixed | Paid-build validation now requires the current exact hand to cover `BUILD_COSTS`, in the local normal action phase after rolling. Missing hand evidence fails closed. |
| G8 | Fixed | Initial incoming/outgoing trade validation and counteroffer continuation resolve `tradeId` in the current offer collection rather than trusting a stale array position. Removed offers still fail validation. |

The G1 schema was verified from the saved public client asset
`docs/colonist-evidence/v320/raw/ui-game.72c86c9aeaa13ccd9a14.js`, specifically
`showSelectPlayer()` and `showPickPlayerActionBox()`. The bundled bridge is
exercised through jsdom without a victim DOM prompt. This verifies behavior
against the saved schema, not the current live Colonist deployment.

### Additional defects found and fixed

- **Cancellation during initialization was lost.** A request canceled before
  `activeAnalyzeId` existed could start GPU search after `hello` completed.
  Background cancellation is now retained on the request, checked after
  initialization, and passed as an abort signal into native execution.
- **Obsolete connection completion could corrupt a replacement connection.**
  Releasing a pending handshake rejected its promise asynchronously; its later
  catch could clear the new connection's cached promise and mark it unavailable.
  Connection generations now protect replacement state. A disconnect immediately
  after a hello reply also cannot publish stale readiness.
- **Readiness could remain pending indefinitely or update a destroyed client.**
  `DecisionWorkerClient.queryStatus()` now times out after 12 seconds, clears
  its timer, and permits a fresh warm-up attempt. Destroyed clients ignore late
  readiness and do not initiate another warm-up.
- **Late readiness could erase a newer evidence failure.** The full regression
  suite exposed a completion-order race in `warmDecisionEngine()`: a successful
  status reply cleared a Balanced Dice error raised by a newer decision.
  Readiness callbacks are now tied to the decision key observed when warming
  began; stale success/failure does not replace newer decision state. Extension
  context invalidation remains authoritative.

### Remaining discard-state limitation (G4)

The live adapter starts the discard cursor at the local root so it can recommend
the mandatory local discard immediately. It currently zeroes earlier seats'
obligations even when their public hands remain over the discard limit. Merely
preserving those counts is insufficient: both `GameState::discard()` and the
CUDA `ACTION_DISCARD` transition advance only to higher seat indexes before
entering robber movement.

A complete fix must preserve all outstanding public obligations and make both
transition implementations visit outstanding earlier seats before moving the
robber, without changing the underlying roller or canonical stochastic player
mapping. It needs a root-in-the-middle fixture, CPU/CUDA transition parity,
regenerated embedded PTX, and packaged-WASM verification. Those rule/backend
changes are not included in this transport and UI patch. Mandatory discard
selection already uses the bounded exact solver rather than a full strategic
rollout, so the original report's claimed robber-value overestimate should not
be presented as a measured result.

### Verification

- `npm run check` — passed.
- `npm test` — passed: 490 tests across 41 files. The packaged-WASM cold smoke
  passed in 353 ms on this run, below its one-second test budget.
- `npm run build` — passed; `dist/` was regenerated. The generated WASM source
  inputs have no changes relative to `HEAD`.
- `git diff HEAD --check` — passed.
- `npm run verify` — stopped at `cargo fmt --all -- --check`, after TypeScript
  checks and tests passed. Formatting differences are in untouched
  `catan-search/src/depth.rs`, `lib.rs`, `midgame_save_spend_tests.rs`, and
  `road4311_d14_tests.rs`. Their worktree contents match `HEAD`.
- `cargo clippy --workspace --all-targets -- -D warnings` — failed on existing
  `collapsible_if` diagnostics in `catan-search/src/opening.rs:603–604` and
  `type_complexity` in `road4311_d14_tests.rs:272`. Both files match `HEAD`.
  These checks used Rust/Clippy 1.98.0; unrelated Rust formatting/lint changes
  are not included in this patch.
- `cargo test --workspace --quiet` — passed (exit 0). The longest search suite
  finished in 477.23 seconds with 217 passed and 15 ignored; the remaining
  workspace suites and doctests also passed.

The first standalone Rust test run lost its tool session before a final exit
status could be captured, so it was rerun. Live browser play and native GPU
hardware behavior were not exercised; mocked transport and bundled-bridge jsdom
tests do not establish live CUDA parity or compatibility with a newer Colonist
client than the saved public schema.
