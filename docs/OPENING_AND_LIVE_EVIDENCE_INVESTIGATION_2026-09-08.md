# Opening quality and live evidence investigation

Status: CPU wave-budget ownership and post-submit trade uncertainty are repaired in source with focused regressions; packaged post-repair evidence remains required. Longest Road transfer wording is repaired. The concatenated transient-player producer remains unresolved. Opening opponent modeling has a research reference, not a production promotion.
Baseline: `main`, `401956f425b3af68e4e7e57060d6f01b6c74d2fb`, with existing staged work preserved.
Updated: 2026-09-08.

## Objective and boundaries

Investigate the reported weak opening and repair demonstrated defects for both player-trade settings. Favor decision quality and robustness; increasing deadlines alone does not address this report. Preserve legal-action validation, cancellation, stale-result rejection, honest hidden-card uncertainty, and public-observation-only operation.

Use local evidence and tests. The user explicitly prohibited browser control. Do not commit, push, replace staged work, introduce speculative opponent cooperation, or tune weights merely to force one preferred tile. “Best” means supported by explicit comparisons and bounded search evidence, not a proven globally optimal opening or a promised live win rate.

## Evidence inventory

- User supplied the full `catan-evidence/2` export for `/|grain8695|1` in the conversation. It contains the complete board graph, decision contexts, candidates, events, and timings. Its opening geometry, ports, and placement sequence are now reconstructed in `opening_recorded_tests.rs::grain8695_state`; the full event export remains in the conversation.
- Live artifact: `0.9.1 · main@401956f425b3+dirty · 2026-09-08T11:40:37.905Z`. Do not assume the dirty source and packaged artifact are identical without replay/build verification.
- Human 1v1: hamzax versus Zalas; Balanced Dice; friendly robber; victory target 15; discard limit 9; autopilot on; player trades disabled.
- Opponent resigned. The recorded win provides no opening-strength evidence.
- Source inspection on this date confirmed the opening economy, live setup caller, score normalization, board-dice caller restriction, and unmatched-log classification below. Codebase Memory generation `2026-09-08T12:05:31Z` reported no recorded gaps for those files; that is not a completeness guarantee.

## Findings and unresolved questions

### O1 — Reported weak opening: reproduced from logs, cause not established

| Placement | Vertex | Adjacent production | Ordinary-dice pips |
| --- | --- | --- | --- |
| Our first | `v:-1,1,1` | ore 4, brick 3, lumber 8 | 10 |
| Our second | `v:0,-1,0` | grain 11, wool 6, brick 12 | 8 |
| Opponent first | `v:0,1,1` | wool 9, brick 6, brick 3 | 11 |
| Opponent second | `v:-1,0,0` | wool 6, grain 5, lumber 9 | 13 |

Our total is 18 pips with all five resources; opponent total is 24 with no ore. Weak grain and brick production are observable. These totals alone do not prove a better opening: ports, starting cards, expansion, blocking, draft order, and build conversion also matter. Ordinary-dice pips are not the next-roll probabilities of the Balanced Dice model.

D2 chose the first settlement using 3,099 nodes, 966.5 ms search time, reported depth 4, and `deadline=false`. D4 chose the second using 154 nodes, 54.1 ms, depth 1, and `deadline=false`. A deadline explanation is unsupported.

Second-placement runner-up: `v:2,-1,1`, reported value 0.7748 versus chosen 0.850. Also inspect `v:1,0,0`, `v:2,-2,1`, and the legal grain-bearing alternatives from D4. Do not prescribe an expected winner until complete endpoint economics are compared.

Ranked hypotheses to test:

1. Endpoint economy/expansion scoring overvalues this weak-grain pair. Compare raw score components and complete-build funding against legal alternatives at the same completed setup horizon.
2. Candidate pruning or limited opponent draft search excludes better complete openings. Compare production widths against a wider deterministic search while holding evaluation fixed.
3. Request/state representation differs from the supplied board or intended settings. Verify geometry, ports, setup order, starting cards, rule settings, and trade policy before changing search weights.

Owners: `engine/crates/catan-search/src/opening.rs`, its existing `opening_recorded_tests.rs`, and the setup branch in `depth.rs`. Expand to boundary conversion only if the replay demonstrates a representation mismatch.

### O2 — Trade behavior: self-funded economics confirmed, defect not demonstrated

`opening_build_economy` uses existing production, actual starting resource cards, and `state.trade_ratios(player)` with shared `build_eta_rolls` and `build_conversion_efficiency`. It explicitly assigns no speculative domestic-trade rate. Bank and port conversion are represented.

Consequences: disabled player trades must still permit bank/port conversion. Enabled trades do not imply willing counterparties. Identical opening choices across the two settings are not automatically a bug. Trace the setting through the live request and opening path, then test both settings against the same board. Any cooperative trade benefit must have defensible evidence rather than a guaranteed exchange assumption.

### O3 — Opening score presentation is relative, not calibrated probability

The setup branch of `depth.rs` rescales authoritative opening candidate values by their minimum and maximum, then computes `0.20 + normalized * 0.65`. When candidate values differ, the maximum becomes 0.850 by construction. The repeated 0.850 in this export does not establish 85% win probability or independent confidence. This monotonic transformation alone does not explain the chosen tile.

Investigate whether the UI and exported rationale adequately identify this relative scale. Preserve raw ranking and avoid an unrelated score-schema redesign.

### O4 — Existing bounded search needs measured comparison

Live setup invokes `solve_opening` with root width 24 and opponent width 4. Default rollout count is zero. `opening.rs` contains candidate truncation and existing `OpeningEvidence`; use those diagnostics to distinguish evaluation from search coverage. More nodes or rollouts are not automatically better and should not be added without comparative evidence.

### L1 — Human-game board roll arrives before log: repaired

At D13, the board had the third gameplay roll (7), while indexed evidence contained only roll 16 (8) and roll 20 (11). D13 failed around 79,640 ms. The third roll at log index 23 appeared in the captured events around 102,913 ms: roughly 23.3 seconds later.

`src/content/index.ts` calls `observeBoardDiceSnapshot` only for a board-only root or bot-only game, at both the publication and session-attachment boundaries. This excludes a human game with an attached but delayed log. The existing session method already owns board validation and ordinal reconciliation.

Repair applied: pass eligible validated board snapshots for human games too at both ingestion boundaries; preserve ordinal deduplication, conflicts, setup identity, and genuine missing-roll rejection. Do not disable Mref validation or silently substitute fair dice.

`tests/content-board-dice.test.ts` now exercises the real content entrypoint with a human game and mounted log: indexed rolls 8 and 11 precede board roll 7; the later indexed 7 must reconcile into exactly three rolls. It passed after the repair. The original focused integration probe failed before the repair with the reported reconciliation error. Existing contradiction/publication tests remain passing.

### L2 — Lifecycle notices falsely mark card history incomplete: repaired

The export classifies `hamzax won the game!`, `Zalas has left the game`, and `Zalas Resigned` as integrity-relevant unrecognized logs. Current `classifyUnmatchedLog` does not recognize these forms. They do not themselves transfer resources.

Prior focused run: `npm test -- tests/setup-dice-authority.test.ts -t grain8695` failed with expected integrity count 0, actual 3. The regression remains an unstaged change. The production classifier now recognizes the three bounded lifecycle forms; the regression passes.

Repair scope: recognize these bounded lifecycle forms using the existing non-mutating system-message classification. Preserve rejection of unknown resource-changing forms; inspect persisted-sample reclassification.

These notices occur near game end and do not explain the initial warning. A separate reproduced ordering defect explains a path to that warning; see L3 below.

## Ordered work and acceptance

1. Preserve this evidence ledger before source changes. Record each subsequent finding as confirmed, hypothesis, repaired, or verified, with an actual check result.
2. Reconstruct grain8695 using the existing recorded-opening fixture helpers, preserving graph/ports and both decision contexts. First reproduce the historical choices or explicitly explain artifact/request differences.
3. Capture raw endpoint evidence for chosen and alternative roots, under both player-trade settings. Verify starting-card assignment and legal snake-draft continuation. Compare existing width with a wider fixed-work reference to isolate pruning from scoring.
4. Repair only the demonstrated causal owner. Add a focused regression that fails for the real defect rather than merely asserting a preferred vertex. Keep existing port, scarcity, starting-card, and draft tests relevant to the changed behavior.
5. Complete L1/L2 as separate evidence-integration repairs, with the focused tests described above. Investigate the early partial-history warning independently if not resolved by authoritative evidence ingestion.
6. Run relevant Rust opening tests and TypeScript checks/tests for changed boundaries. Rebuild versioned WASM and extension if Rust production code changes. Record artifact identity and actual verification results; do not carry earlier passing results across changed code.
7. Update this ledger and stabilization acceptance with remaining limitations. A passing unit test or one replay does not prove general playing strength; any strength claim requires matched boards/chance, rotated seats, both trade settings, and reported uncertainty.

## Related work and completion limits

Earlier road4311 work addressed lowercase DOM controls and repeated automated trade failures. It is pre-existing work, not proof that grain8695 is repaired. Earlier stabilization work addressed timed-result finalization, native model compatibility, replay context, and test contract mismatches. Preserve these changes.

The overall Engine stabilization / CPU-GPU contract is not established as complete by this investigation. Live trade acceptance, current CUDA hardware/promotion evidence, broad replay coverage, and opening-strength evidence remain separate gates. Exact CUDA production promotion remains disabled according to the existing handoff; verify current acceptance documents before changing that disposition.

## Change record

- 2026-09-08: Created documentation before grain8695 production edits. Rechecked the key source boundaries. Opening cause remains unproven; no opening weights, budgets, or trade assumptions changed.


## Repair and replay results — 2026-09-08

### L3 — Start banner processed before settlement: repaired

A real parser/session regression established this sequence: scan the index-0 start banner; later hydrate the first settlement at index 2; end setup. Before repair, `partialHistory` incorrectly became true even though the retained banner and first settlement satisfy the existing narrow complete-prefix rule.

Cause: `acceptGameStartBoundary()` was called only while processing the banner. At that point, the first settlement needed for its proof had not arrived. Later scans did not recheck the retained evidence.

Repair: after processing each event batch, recheck the retained index-0 banner using the existing prefix proof. Do not broaden what counts as a complete prefix. Existing real-missing-prefix and conflict tests still pass. This reproduces a causal path consistent with the early warning; the export does not record the exact historic scan ordering, so that final attribution remains an inference.

### Opening reproduction and falsification

The fixture reproduces D2's first settlement `v:-1,1,1` with **3,099 nodes** and D4's second settlement `v:0,-1,0` with **154 nodes**, matching the recorded choices and counts. These are deterministic native setup comparisons, not a full canonical Mref replay or live win-rate experiment.

| Comparison | Result | Interpretation |
| --- | --- | --- |
| First pick: root width 24, opponent width 4, 12,000-node allowance | `v:-1,1,1`, 3,099 actual nodes | Historical choice reproduced |
| First pick: root width 54, opponent width 4, 100,000-node allowance | same choice, 13,328 nodes | More controlled-root coverage alone does not fix the complaint |
| First pick: root width 54, opponent width 54, 100,000-node allowance | `v:0,1,1`, 13,394 nodes | Opponent policy sensitivity; not proof of improvement |
| Final pick: widths 24 and 54 | same choice, 154 nodes | Root-width pruning is not the cause here |
| Final pick: independent enumeration of every legal settlement and its legal anchored roads | Every candidate value matches the best completed endpoint | Current solver correctly maximizes its current evaluator for this context |
| Final pick: root domestic-trade mask 0 versus 1 | identical choice/value; same legal bank ratios and starting cards | No invented cooperative-trade benefit |

The wider-opponent result above changes two parameters relative to the live run; the separate wide-root/unchanged-opponent run isolates the additional opponent-width effect. Width 54 is still a greedy opponent model, not minimax over all opponent continuations.

Raw evidence for the actual second pick:

- Production `[5,3,5,2,3]` in lumber/brick/wool/grain/ore order; starting hand `[0,1,1,1,0]`; maritime ratios `[4,4,4,4,4]`.
- Whole-card deterministic funding proxies: road 7.2 rolls, settlement 7.2, city 24, development card 12. These are not stochastic first-passage expectations.
- Own endpoint score 11.432724; rival 10.968895; difference 0.46382904.
- Expansion contributes 3.7961383; build access 1.590097; production/diversity 1.9699998; conversion efficiency 0.3. The other terms are exposed in existing `OpeningEvidence`.
- Stronger-grain alternative `v:2,-2,1` yields production `[5,2,5,5,3]`, starting hand `[0,0,1,2,0]`, road proxy 14.4, settlement 18, city 24, development 12; expansion term 2.4343123. More grain does not improve all immediate complete-build economics.

Inspection of `eval.rs::expansion_option_value_with_routes_and_weights` confirms that expansion already discounts the full road-plus-settlement funding delay and race survival. The hypothesis that it simply grants a future settlement without charging acquisition cost is not supported. Its heuristic weighting may still be miscalibrated; this investigation does not establish an optimal calibration.

**No production opening weights, search widths, budgets, or opponent assumptions were changed.** A different selected vertex would not by itself establish a repair. The remaining opening-quality task is to evaluate opponent-continuation robustness and endpoint utility against independent continuation outcomes, on several recorded boards and both trade policies, before promoting a model change. Do not set an expected vertex merely because it has more pips.

### Verification and retained artifacts

- Before repair: human-board integration failed with the Balanced Dice reconciliation error; lifecycle regression failed 0 versus 3; start-banner-first regression failed with `partialHistory=true`.
- After repair: 39/39 focused TypeScript tests passed across content-board-dice, setup-dice-authority, and review-balanced-publication. The content integration test was then strengthened to the actual third-roll/delayed-DOM sequence and passed again.
- Rust: `cargo test --manifest-path engine/Cargo.toml -p colonist-catan-search --release --lib opening` passed **30/30**, including the independent grain8695 endpoint enumeration under both trade policies.
- Diagnostic prints were removed. The board fixture and endpoint regression remain in the existing recorded-test module; no test dependency was added.
- Local diagnostic logs (not for Git): `/tmp/grain8695-opening-probe.log`, `/tmp/grain8695-opening-opponents.log`, `/tmp/grain8695-opening-final.log`, `/tmp/grain8695-dice-final.log`, `/tmp/grain8695-integration-final.log`.
- Final `npm run check`, `npm run build`, and `git diff --check` passed. Packaged extension: `0.9.1 · main@401956f425b3+dirty · 2026-09-08T14:46:42.254Z`. WASM SHA-256: `e2f228f706ab9779039714d75419dd727fda9520e47007573ce3a23183f90b90` (unchanged). Reload the unpacked extension and refresh Colonist tabs to use the fixes. No browser was controlled, and no commit/staging/push was performed.


## Focused review follow-up

### Trade pause publication — repaired, focused tests passed

Review identified that exhausted direct-control lookup invokes the failure callback without a final board refresh. The overlay recorded a manual pause but returned before rendering it. Partner confirmation and cancellation also bypassed the missing-control pause branch.

The overlay now recognizes `trade`, `trade-builder`, `trade-partner`, and `trade-cancel` missing controls and immediately renders retained advice with autonomous execution disabled for that signature. It does not depend on an unrelated future board publication. The repair belongs to the overlay state transition; no extra executor refresh was added.

New tests run the actual executor and fake retry timers, simulating bridge responses only to emitted refresh requests. They do not inject a failure callback or manually force a post-failure render. The previously masked retention tests also no longer force that render. The focused pre-fix run had four failures; after repair the full action-guide and overlay files passed **88/88**. Type checking passed.

### Opponent model — narrowed causal experiment

For our recorded first settlement and road, the existing model predicts our eventual vertices `[2,18]` and roads `[21,58]` correctly. Its projected own/rival values are 11.606346 / 8.708606, versus 11.432724 / 10.968895 with actual opponent occupancy. Most of that discrepancy is therefore the opponent continuation, although endpoint values are still heuristics.

A research-only ignored test in the recorded opening module compares greedy top-four opponent actions with top-four portfolio lookahead through the opponent's two consecutive pairs. Five recorded board geometries are used as controlled 2-player, 15-point scenarios, with player trades enabled and disabled. The four formerly 4-player/other-seat contexts are deliberately normalized experiments, not claimed historical replays. Neither experiment modifies production policy.

The first version scored at handoff before our final setup response. On grain8695 it improved final rival value from 8.708606 to 10.857399, but final rival value decreased on hand2325, trade5301, and town1088 despite a higher handoff score. This falsifies promotion based solely on handoff score: partial-setup and completed-setup evaluations are materially different. The follow-up experiment evaluates each rival portfolio after the existing production solver completes our final response. Results and actual work must be recorded before making a promotion decision; equal caps do not mean equal actual work.

### Release evidence

Acceptance wording now distinguishes completed focused v14 replay/native verification from incomplete full release/performance/browser acceptance. Lowercase selector handling is documented as hardening; road4311 D15 already showed a `foreground-disabled` control, so casing is not its proven cause.

A current-build 3P/4P exact CPU/CUDA matched smoke was requested with the existing benchmark script. Its result is pending; do not treat it as a full campaign or enable production promotion from a small smoke alone. Browser control remains prohibited; packaged click/transition evidence still requires a user-run consensual check.


### Completed-setup opponent comparison — result

The follow-up finished successfully: five board geometries × two trade settings × two continuation models. Both arms used the same top-four candidate source and a 100,000-transition ceiling; actual work differed substantially. Final own replies used the unchanged production opening solver and evaluator.

| Board | Trades | Greedy final rival value | Portfolio final rival value | Greedy / portfolio work |
| --- | --- | --- | --- | --- |
| grain8695 | off/on | 8.708606 | 10.857399 | 164 / 22,204 |
| hand2325 | off/on | 7.355114 | 10.320792 | 168 / 22,240 |
| trade5301 | off | 7.827077 | 9.857949 | 164 / 22,132 |
| trade5301 | on | 7.827077 | 9.692493 | 164 / 22,132 |
| town1088 | off/on | 8.402099 | 10.062921 | 168 / 22,420 |
| task394 | off | 7.431393 | 10.232584 | 168 / 21,916 |
| task394 | on | 7.439910 | 10.243802 | 168 / 21,916 |

On grain8695 the portfolio model predicts rival value 10.857399, much closer to the actual recorded endpoint value 10.968895 than the greedy prediction 8.708606. This supports the opponent-continuation diagnosis. Improvement in the other rows is optimization of the same evaluator, not independent proof of playing strength. The experiment is not equal-actual-work evidence: roughly 130–135 times as many modeled transitions are used, including final-reply solves. It must not be promoted directly into every live root.

Disposition: retain the explicit research test as a bounded reference, not production behavior. The next implementation candidate must share a controlled live allowance across opponent portfolio alternatives and still produce completed endpoints without first-branch starvation. Compare it to this reference under equal actual node limits before considering promotion; leave evaluator weights unchanged. This is a concrete search-allocation problem, not justification for an unlimited recursive opponent search.

Reproduce with `cargo test --manifest-path engine/Cargo.toml -p colonist-catan-search --release --lib recorded_opponent_portfolio_experiment -- --ignored --nocapture`. The recorded run took 45.70 seconds; local output is `/tmp/opening-opponent-complete-portfolio.log`. The earlier partial-handoff experiment is retained only as a falsified diagnostic result in this document.


## Loot4438 review intake — document before implementation

Source: complete 4-player bot-game export at `/mnt/c/Users/Hamza/Downloads/colonist-evidence-loot4438-1-2026-09-08T15-16-31-747Z (3).txt`. The reported B1/B2 observations were independently rechecked against that export and the current source. Source repairs are now staged; installed packaged post-repair evidence is still outstanding.

### B1 — Major: a child time slice aborts the entire deeper wave

Reported live evidence: 24 distinct non-reused CPU searches with a 2,000 ms base allowance attempted depth 2, retained depth 1, and reported a deadline after an average of 338 ms (minimum 143 ms). That leaves approximately 1.66 seconds unused on average; it is not proof of a wrong individual action.

Confirmed source mechanism: `depth.rs` divides remaining parent milliseconds by remaining particle × root cells and creates a private `CooperativeDeadline` for each cell. Any child's `deadline_reached` marks the whole wave incomplete and breaks out, even if the parent deadline is still far away. The same loop also exits when remaining milliseconds are fewer than remaining cells.

Owner: `engine/crates/catan-search/src/depth.rs`, the shared belief-search wave loop. Existing per-root node budgets already bound work. Investigate correcting wall-clock ownership so the parent quality deadline governs the wave while node quotas continue controlling per-cell work. Do not increase the configured 2-second allowance, publish a partially aggregated wave, or treat one child's time slice as parent exhaustion.

Repair: the wave loop no longer manufactures private `remaining_ms / remaining_cells` deadlines. Per-cell node quotas still bound work, while every continuation cell observes the same active parent deadline. A timed child can therefore abort a wave only when the shared parent allowance actually expires. Partial waves remain unpublished.

Focused verification:

- `continuation_cells_share_the_parent_deadline_budget` passed: the deterministic test hook observes the exact 10,000 ms parent budget on every depth-2 continuation cell, so the old per-cell division would fail without relying on wall-clock sleeps.
- `timed_belief_maxn_honors_one_global_deadline_without_dropping_root_actions` separately passed and retains the bounded result when the real global deadline expires.
- `belief_root_priors_and_candidates_do_not_depend_on_particle_order` passed.
- `root_work_reports_future_self_reach_without_changing_fixed_work_choice` passed.
- `zero_time_budget_is_identical_to_the_node_bounded_api` passed.

Packaged verification: `npm run build` rebuilt the Rust/WASM path successfully as `0.9.1 · main@b3b92c31e687+dirty · 2026-09-08T17:23:01.208Z`; generated WASM SHA-256 `9e805753ebf5fad491f65a2d11e848ef7dc36715846d77db597442704e4ceea8`. `tests/deep-search-adapter.test.ts` then passed 30/30. Its rejected-trade regression contains three sequential ordinary strategic searches and now takes about 5.73 s after B1; the prior default 5 s harness timeout was raised to 10 s without reducing search effort. That is package-level evidence that the repaired engine can use quality time which the old per-cell cutoff discarded.

Remaining acceptance: collect a consensual installed-browser post-repair game showing the production route under real UI scheduling. Do not require every simple decision to consume two seconds, and do not increase the nominal budget merely to demonstrate usage.

### B2 — Major: submitted trade outcome can be unknown, not failed

Reported live evidence: D41 submitted 3 grain → 1 ore, then recorded `Trade workflow timed out: Close completed trade panel`. The bank transaction subsequently appeared exactly once. D45 selected the same trade but did not execute it; D47 later ended the turn. **No duplicate trade was observed.** The demonstrated defect is false failure plus an unsafe opportunity to replan/retry.

Confirmed source mechanism: `startWorkflow` knows a step was dispatched through `workflowHasDispatchedStep`, but its timeout/failure path calls `onExecution({succeeded:false})`, cancels workflow state, and requests refresh. The overlay treats ordinary builder failure as permission to clear completed analysis and restart search. The recently repaired missing-control pause does not cover this post-submit uncertainty.

Owners: `src/content/action-guide.ts` transaction state and `src/content/overlay.ts` execution/replanning boundary. Inspect the exact submit step: `workflowHasDispatchedStep` currently also includes preparatory clicks, so do not equate every dispatched step with an economic submit. Reuse existing transaction commit validation and game identity where possible.

Repair: trade workflows now mark the economic submit step separately from preparatory clicks. Once that submit event is actually dispatched, rerender/replanning preserves the original transaction observer and reports a pending outcome instead of ordinary failure. Authoritative hand/offer commit resolves success independently of panel closure; a newly visible attributable Colonist rejection resolves failure. Elapsed time alone no longer claims non-commit. A failed `HTMLElement.click()` is not marked submitted.

Focused verification: `tests/action-guide.test.ts` passed 62/62. The regression covers immediate commit, delayed commit beyond the former timeout without a repeated submit, explicit post-submit rejection, pre-submit missing-control failure, and submit-control dispatch failure. `tests/overlay-settings.test.ts` passed 30/30 with a boundary regression proving a pending trade preserves the completed analysis/key, sends no new decision request, and publishes the awaiting-confirmation notice.

The rebuilt package includes this path and the packaged adapter passes 30/30. Remaining acceptance is the user-run installed-extension check: confirm a consensual live bank trade no longer reproduces D41's false failure/research churn. A page reload/process loss cannot preserve in-memory pending-side-effect state; the retained guarantee is ordinary rerender/replanning and same-page game continuity, with changed game identity cancelling stale callbacks.

### E1 — Longest Road transfer wording: parser coverage repair

The review reports `Longest Road passed from hamzax to Neala (+2 VPs)` and the reverse transfer marked as integrity-breaking unknowns. `classifyUnmatchedLog` already treats received/lost award notices as redundant because the board owns award state; its existing pattern does not cover `passed from X to Y`.

Repair: `src/content/session.ts` now recognizes `Longest Road|Largest Army ... passed from X to Y` as the same redundant award-audit class as existing received/lost wording; board snapshots remain authoritative. The focused live-session suite passed 37/37, including the exact loot4438 Longest Road transfer wording. No resource or award transition is synthesized from the rendered message.

### E2 — Concatenated secondary player name: producer unresolved

Reported unresolved-player text: `and took Holden built a Road Holden gave bank ...`. Alias rejection is appropriate; do not whitelist, truncate, or guess a player name downstream.

The raw export is now available. Its retained final events do **not** contain an unresolved actor/secondary player: log indices 450–458 all resolve to `P3`/Holden where applicable, including roll, production, city, bank trades, road, and buy-dev. The malformed name survives only in sticky `meta.unresolvedPlayers`; the transient raw snapshot/field that first introduced it is not retained in the compact artifact. That rules out a safe downstream reclassification but does not identify the producer. Leave this integrity flag unresolved until a future investigation log or reproducible DOM snapshot captures the originating transient value; do not erase it merely because the final event hydrated correctly.

### O5 — Additional opening evidence from loot4438

The exact loot4438 geometry is available locally from the export and was reconstructed in the temporary recorded-opening probe. Current production reproduces our first pick `v:2,-1,0`. The original temporary width-4 contiguous-opponent-portfolio model keeps our first and second settlements `v:2,-1,0` and `v:0,1,0`, while changing the modeled second road from `e:0,1,0` to the actually observed `e:1,0,2` (first road remains `e:2,-1,0`).

That fidelity improvement is not live-budget-safe. Direct work accounting found about 19,008 additional portfolio transitions on grain8695 and 49,748 on loot4438 before memoization. State-hash memoization reduced grain8695 width-4 work to about 14,688 additional transitions, still above the entire 12,000-node live opening allowance before ordinary solver work. Width 3 still added about 11,448 transitions on grain8695 and 42,428 on loot4438. A top-2 scout restricted to the snake-apex double-pick reduced grain/loot scout work to about 4,032 transitions, but it lost the loot4438 continuation improvement and therefore does not justify production promotion. Apex-only width 4 was sufficiently expensive that the diagnostic exceeded the bounded short-command window and was abandoned rather than promoted.

Together with grain8695, this strengthens the opponent-continuation diagnosis while falsifying the obvious production substitutions. Keep the full portfolio reference research-only. No evaluator weights, production opening widths, or opening budgets are changed by this investigation. A future candidate must account its scout transitions inside the existing root allowance and demonstrate useful endpoint fidelity/quality under that same work budget; otherwise the current greedy opponent policy remains the safer production behavior.

### Product-route and artifact qualifications

`disablePlayerTrades=true` prevents our seat from making domestic trades; opponents continue normal domestic trading. The reported live game supports that intended behavior. Global `--no-player-trades` arena timings are therefore a restricted correctness/performance lane, **not exact product-route or strength evidence**. Keep that limitation beside current and historical benchmark results.

The export names `main@401956f425b3+dirty`. It identifies HEAD but does not fingerprint the dirty source or establish that later unstaged overlay repairs were installed. The 7–10 loss alone is not evidence of a particular strategic error. Complete game coverage does not override integrity flags or make the record automatically benchmark-eligible.

### Updated priority and stopping boundaries

1. Repair and verify B1 without changing nominal time budgets or complete-wave authority.
2. Repair and verify B2 with explicit post-submit uncertainty and no duplicate economic action.
3. Repair E1; investigate E2 only from the actual malformed event once its export is located.
4. Opening opponent-model work remains research-only: the measured width-4/3 candidates exceed the live work envelope and the bounded apex/top-2 candidate loses the observed loot improvement. Do not promote a model until its portfolio work is accounted inside the existing allowance and improves held-out endpoint evidence at equal work.
5. Record current benchmark results with the global-no-trades limitation. Full release, product-route strength, and installed-artifact click/confirmed-transition acceptance remain incomplete; browser checks stay user-run.

Update this ledger with the exact regression result before and after each repair, the files changed, and any remaining limitations. Preserve all pre-existing staged work. Do not claim stabilization acceptance while B1/B2 remain unresolved.
