# Engine stabilization: bounded subagent tasks

**User requirement clarified — 2026-09-08:** acceptance is governed by soundness and robustness, not a minimum speedup. The former 2× threshold is superseded. Preserve same-policy parity, evidence validity, cancellation/recovery, stale-result rejection, safe trade execution, and packaged recommendation-to-confirmed-transition verification. Report performance as diagnostic evidence; sub-2× speedup is not a failure or a reason by itself to block a backend. This clarification does not itself enable production routing or establish playing strength.

## Start here

**Outcome:** close the remaining v14 correctness and release-evidence gaps, then return the changes to Hamza's reviewing agent. This is a continuation of the existing implementation, not a new engine project.

Baseline inspected on 2026-09-08: `401956f425b3af68e4e7e57060d6f01b6c74d2fb`, with a clean working tree and `main` aligned with the locally recorded `origin/main`. The previously staged implementation is committed in `7a3646a`; `401956f` reconciles the duplicate milestone checkpoint. Recheck Git before assignment because another session may advance it.

Read this document's common rules and **only your assigned task**. Consult the [governing stabilization plan](ENGINE_STABILIZATION_AND_CPU_GPU_PLAN_2026-09-07.md) for that task's contract and the [acceptance report](ENGINE_STABILIZATION_ACCEPTANCE_2026-09-07.md) for evidence. The old plan's Steps 1–7 are recorded as implemented; they are not a fresh implementation backlog. Its historical defect descriptions are not proof those defects remain.

The acceptance report distinguishes historical v13 artifacts from focused v14 repair results. It does not establish full v14 acceptance, packaged live execution, or increased playing strength. Preserve the old exact-CUDA timing evidence, but do not apply the superseded 2× gate.

This document creates assignments; it does not claim a new code review or test run. No workers were launched while writing it.

## Dispatch and ownership

| ID | Responsibility | Dependency | Initial disposition |
| --- | --- | --- | --- |
| T0 | Coordinator: freeze baseline and allocate ownership | None | Ready |
| T1 | Replay request and historical-action fidelity | T0 | Repairs reviewed; replay type check passes; canonical lane still unexercised |
| T2 | Controlled continuation and strategy evidence | T0 | Original task verified without changes; see new bounded follow-up below |
| T3 | Production routing and end-to-end deadlines | T0 | Repairs reviewed; cooperative cutoff policy implemented by T4 |
| T4 | Coordinator/reviewer: integrate, rebuild, verify | T1–T3 reviewed | TypeScript and Rust/Clippy pass; live trade repairs awaiting rebuilt-artifact check |
| T5 | CUDA worker: current-revision exact parity evidence | T4; compatible GPU | Waiting |
| T6 | Browser worker: packaged CPU recommendation/execution | T4; consensual browser fixture | User logs verify placements; trade repairs require user-run follow-up; no browser control |
| T7 | Coordinator: publish acceptance disposition | T4; T5/T6 results or explicit blockers | Waiting |

T1–T3 may run concurrently with disjoint ownership. T5 and T6 use the same frozen integrated revision; serialize latency measurements so GPU/arena work does not contend with the live worker. Assign one task per agent. A task completing does not authorize it to pick another task.

### Common worker instructions

1. Read applicable `AGENTS.md` and required implementation/review skills. Record task ID, base SHA, starting Git status, allowed files, the contract being checked, and the stopping condition. You are not alone in this codebase: preserve others' changes and adjust to accepted integration changes.
2. Check current source and existing evidence before editing. Report **VERIFIED — NO CHANGE** when the assigned contract already holds. Lack of a new diff is not failure. For a repair, state the concrete input, expected behavior, observed mismatch, causal owner, and smallest falsifiable fix before mutation.
3. Use the repository's code-intelligence routing and check freshness first. Ask before index creation/reindex/clear. If the index is unavailable or incomplete, use the permitted source fallback and report the limitation. Do not spend the assignment repairing intelligence tools.
4. Own only the listed files. Read direct producers/consumers as needed; reading a file does not grant edit ownership. Return a precise request to the coordinator if repair requires a shared file, schema, generated contract, public behavior, or another worker's file. Continue independent evidence gathering while ownership is resolved.
5. Reuse existing request types, helpers, test fixtures, and runners. Prefer a local correction. New dependencies, general-purpose frameworks, engine rewrites, new strategy controllers, broad formatting, and speculative configuration are outside these tasks.
6. Keep CPU/WASM MaxN as the ordinary production policy. Preserve honest hidden information, mandatory/legal-action authority, state signatures, trade idempotence, cancellation, and Mref semantics. Keep M2 opt-in. M3–M5, learned-model promotion, the old 10-second budget branch, and automatic exact-GPU routing are outside scope.
7. Use the assigned verification as acceptance work when the task is dispatched with authorization. Test creation/modification still follows causal-coding and repository policy: a reported live failure requires a focused regression; otherwise obtain explicit test-work authorization rather than treating this generated plan as permission. Reuse existing checks first. Run only relevant checks; T4 owns the combined suite. Preserve failing assertions unless the authoritative contract demonstrates they are stale.
8. Stop after the task's acceptance matrix is answered, its evidenced in-scope repair is verified, and the handoff is written. An unrelated finding becomes a separate note, not an extra repair. Missing fixtures/hardware are **BLOCKED**, not PASS. A failed check remains FAIL.

Workers do not stage, commit, push, merge, reset, stash, clean, or overwrite the coordinator's working tree unless separately assigned that operation. In a shared checkout, only the coordinator builds generated WASM or the companion. For isolation, the coordinator may allocate worktrees from the frozen commit; a worktree at HEAD does not include somebody else's uncommitted edits. Never copy the entire dirty checkout between worktrees.

**Shared files reserved to T4:** `engine/crates/catan-wasm/src/lib.rs`, `src/core/engine.ts`, all `src/generated/wasm/` files, manifests/package files, and shared acceptance/contract documentation. This is an ownership lock, not an instruction to change them.

## T0 — Coordinator: establish the dispatch baseline

**Owner:** Hamza's coordinating/reviewing agent. **Edits:** task status in this document only, if useful.

1. Record `git rev-parse HEAD`, `git status --short --branch`, and staged/unstaged summaries. Preserve any new work. Use the current agreed commit as the base, not the transcript's historical `aa9327e` reference.
2. Allocate T1–T3 in the same checkout with the ownership below, or in separate worktrees from that base. Pass the actual absolute checkout path to every worker.
3. Pass each worker the common instructions, its one task, and the output template. Explicitly authorize the task's focused test work if desired; this avoids leaving small models to infer authority from a generated plan.

**Done:** all workers name the same base (or an explicitly recorded descendant), each has a disjoint edit set, and the coordinator has retained the original Git status. No source mutation is needed for T0.

## T1 — Replay fidelity and historical action accounting

**Question:** does a report faithfully identify the request/artifact it reproduces, and distinguish historical execution from a fresh search and a reference search?

**Read first:** governing plan S1/S2 and Step 1; acceptance report's focused v14 repair section.

**Allowed edits:** `scripts/replay-engine.ts`, `src/core/decision-trace.ts`, `src/core/llm-record.ts`, `tests/decision-trace.test.ts`, `tests/llm-record.test.ts`. A new focused replay test requires task-specific test authorization; keep it in `tests/`. Read `src/worker/deep-search.ts` as the request producer; T3 owns its edits. Rust request parsing and shared types belong to T4.

**Ordered work:**

1. Trace canonical request capture through serialization and replay. Inspect the existing runner's invocation and output rather than inventing an exported API or importing its executable entry point blindly.
2. Check the following cases against current source and existing fixtures. Exercise the actual replay normalization/runner when runnable; an adapter-only test does not prove replay correctness.

| Case | Required result |
| --- | --- |
| Canonical request with matching WASM identity | Exact fidelity only when both request and artifact requirements hold |
| Different/missing artifact identity | Explicit limitation; cannot claim exact request-and-artifact reproduction |
| Live/medium/maximum effort profiles | Normalized authoritative nested effort matches returned `effectiveEffort` |
| Explicit zero-time fixed work | CPU/exact work remains untimed and escalation does not reintroduce a time limit |
| Complete Mref context, disabled trades, exclusions, explicit M2 | Decision-affecting inputs survive capture and replay |
| Incomplete legacy evidence | Rejected from faithful lane or explicitly approximate; no silent fair-dice substitution |
| Historical action differs from reproduced winner | Historical, reproduced, and reference actions remain separate |
| Historical action absent from reference roots | Score comparison unavailable, not fabricated zero regret |

3. Repair only a demonstrated mismatch in owned files. Preserve existing schema compatibility unless T4 accepts a necessary versioned change. Use existing fixtures or clearly identified synthetic evidence; a synthetic history is not a recovered live recording.

**Verification:** existing `tests/decision-trace.test.ts` and `tests/llm-record.test.ts`; actual replay execution on a bounded existing fixture set using the frozen WASM. Read CLI/build requirements first. Run `npm run check` once if TypeScript changed. Request a T4 rebuild if Rust changes invalidate the WASM; never quietly use an old binary.

**Done:** every matrix row has a source/check reference and disposition; report exact fixture identities and which fidelity lanes actually ran. Broader replay-corpus absence remains a named limitation. Do not change the engine to force agreement with a reference score.

## T2 — Search continuation and evidence discipline

**Question:** do the existing F1/F2 repairs and associated diagnostics preserve the declared search policy without using unsupported evidence?

**Read first:** governing plan S3/S4/S6 and Steps 2–3. The old F1/F2 line numbers are navigation history, not current locations.

**Allowed edits:** under `engine/crates/catan-search/src/`: `depth.rs`, `policy.rs`, `strategy.rs`, `reachability.rs`, `exact.rs`, including their existing inline tests. Read downstream serialized consumers; T4 owns cross-language schema changes. T5 may read `depth.rs` but must return any later search repair to this owner/coordinator.

**Ordered work:**

1. Inspect current controlled-player selection and CPU/CUDA-exact host paths; trace the direct callers and confirm coverage. Verify highest observation-safe policy score is chosen before quota truncation. Preserve deterministic ties, immediate wins, and the separate opponent mixture.
2. Check existing fixtures for pre-roll Knight, a winning action in a later quota family, observation-equivalent hidden worlds, ties, and immediate wins. A fixture must establish its expected ordering; do not encode an arbitrary preferred action.
3. Check ordinary road/Knight proposals against genuine award-transfer/race fixtures. Nonownership alone must not grant contested priority. Existing conservative omission is acceptable; this task does not broaden race detection.
4. Check optimistic sampled-world bounds and proposal-status copy. A ceiling above the target is not proof of reachability; a rejected proposal is not a demonstrated valuation failure. Preserve justified necessary-source deductions.
5. Verify positive-weight Mref chance handling in the tactical solver, including impossible outcomes and surviving mass. Inspect whether the historical missing regression is already present before adding anything.
6. Check that per-root work/cutoff diagnostics distinguish terminal branches and controlled-player reach, and do not change chosen actions under identical fixed work.

**Verification:** select the existing relevant Rust test names before invoking them. Package is `colonist-catan-search`; run focused filters from `engine/`. Report each selected test and the failure it would catch. CPU-only checks do not certify CUDA execution; T5 owns that evidence.

**Done:** the six checks have explicit dispositions and any evidenced repair passes its focused acceptance. Keep v14 identity unchanged unless the actual policy contract changes; a required identity change goes to T4 for all direct consumers. Do not tune weights, add strategy families, or chase higher win rates.

## T3 — Production routing, deadlines, and stale results

**Question:** can companion availability, failure, or delay change the production algorithm or allow a result outside the decision's remaining allowance/state?

**Read first:** governing plan Steps 4–5; [CPU/GPU contract](CPU_GPU_MREF_CONTRACT.md).

**Allowed edits:** `src/background/index.ts`, `src/background/native-gpu.ts`, `src/content/decision-worker.ts`, `src/worker/deep-search.ts`, `tests/background-mref-routing.test.ts`, `tests/native-gpu-mref.test.ts`, `tests/native-gpu-schema.test.ts`, `tests/decision-service.test.ts`, `tests/deep-search-adapter.test.ts`.

**Read-only consumers:** `src/content/overlay.ts`, `src/content/session.ts`, `src/worker/analyze.ts`, shared types, Rust effort handling. Return necessary changes there to T4 or a separately scoped execution repair; do not silently enlarge ownership.

**Acceptance matrix:**

| Condition | Required behavior |
| --- | --- |
| Companion absent, rollout-only, or exact-capable | Ordinary production request still uses CPU/WASM MaxN while promotion is disabled |
| Unsupported algorithm/protocol/semantic response | Rejected; never silently reinterpreted as same-policy output |
| Transport failure in an eligible experimental route | Any allowed fallback preserves semantics and consumes only remaining allowance |
| CPU evidence escalation / delayed response | Entire request stays inside client allowance with preparation/finalization accounted for |
| Exhausted allowance | No restart with a new full budget |
| Board changes or request is superseded | Late result cannot become actionable; existing state/signature checks remain authoritative |
| Explicit zero-time offline work | Fixed-work contract remains distinct from live timeout semantics |
| Request crosses JS/Rust boundary | Durations/local clocks used consistently; unrelated monotonic origins are not compared |

**Work:** inspect each boundary, exercise existing focused tests, then repair only reproduced contract violations in owned files. Keep current production budgets and the exact-promotion gate. Do not add a second scheduler, retry framework, or UI engine selector.

**Verification:** run only the relevant files from the allowed test list via `npm test -- <paths>`; type-check after a TypeScript repair. Unit tests establish modeled boundary behavior; T6 must still verify packaged execution.

**Done:** each condition is supported by a source/check reference; failures include a minimal input/event sequence and causal owner. Return any overlay execution gap to the coordinator as a bounded follow-up, not an overlay rewrite.

## T4 — Integrator/reviewer: close shared contracts and freeze the build

**Owner:** Hamza's reviewing agent. **Dependency:** T1–T3 handoffs available. This agent applies the code-review skill when reviewing changes.

### Integration finding — 2026-09-08

T1's replay/context repairs and T3's returned stochastic-model validation passed
source review. T2 reported 21 focused CPU Rust checks passing without edits;
that report is not CUDA execution evidence. Follow-up edits now explicitly type
the replay calibration array, require protocol 7 in the schema test, and reduce
the fixed-work Mref test's nested effort while retaining zero time and its
behavioral assertions. A strict compiler check including `scripts/replay-engine.ts`
returned zero diagnostics; the normal `npm run check` excludes scripts.

The deadline repair requires more than a larger finalization reserve. The
existing packaged 50 ms regression was reproduced, then a temporary diagnostic
reported 13 ms remaining at search entry, 278 ms in root scoring, 24 ms in
threat/safety preparation, 5 ms in the complete one-ply floor, and zero deeper
wave time. This measurement is one local probe, not a latency distribution.
`belief_search_backend` in `engine/crates/catan-search/src/depth.rs` computes
root priors/planning without a deadline parameter and deliberately completes
the posterior-wide one-ply table after expiry. Consequently, the failure is
not merely loss of an already-completed deeper wave during finalization.

Hamza then explicitly chose robustness and result quality over the engine time
target. The temporary diagnostic was removed. Shared MaxN finalization now
finishes its safety/exact-family arbitration after a cooperative cutoff rather
than discarding the completed report. Explicit cancellation is still checked
during and after arbitration; expiry is recorded in `deadlineReached`. The
separate client safety limit and stale-result gates remain intact. No search
budget was increased. WASM was rebuilt through the normal build workflow.

Current integration checks: the six focused routing/service/trace/record/schema
test files passed 51/51 tests. The rebuilt adapter file passed 30/30 tests,
including the cooperative-deadline case (about 340 ms) and the reduced untimed
Mref case (about 1.1 seconds). Full release, replay, native and packaged-browser
acceptance must still be recorded separately; these focused results do not
complete the governing plan.

The subsequent full TypeScript suite passed 416/416 after correcting the
stale-Mref test fixture's missing board roll count and corresponding error
expectation. The extension build passed. Both previously throwing replay
fixtures completed and passed their existing proxy gate. Full Rust verification
failed on `depth::tests::saturated_seed_22_escalation_resumes_iterative_depth`
(204/205 search tests passed); a focused rerun reproduced the same missing
`evidence_escalation_triggered` assertion at `depth.rs:5256`.

**Next bounded T2 follow-up:** own the seed-22 escalation fixture and its direct
escalation predicate in `engine/crates/catan-search/src/depth.rs`. Determine
whether current v14 policy/work still establishes the fixture's intended
saturation/evidence precondition. Compare ordinary and escalated stage/work
diagnostics; use seed 25 only as the existing paired control. Repair the
responsible logic only if the trigger contract is violated. If the fixture is
stale, establish a valid precondition before updating it; never remove the
escalation assertions merely to pass. Preserve fixed-work CPU/CUDA semantics.
Stop after that focused regression and direct paired control pass, then return
to T4 to resume the interrupted Rust verification. No strategy tuning, new
campaign, or GPU promotion is part of this follow-up.

1. Compare each worker's patch to its declared base and ownership. Review current surrounding source, not just summaries. Verify every changed file has a causal/contract role and every claim cites actual evidence. Reject unrelated cleanup or tests that merely restate the implementation.
2. Resolve direct shared-contract repairs serially: `engine/crates/catan-wasm/src/lib.rs`, `src/core/engine.ts`, and necessary versioned producer/consumer changes. Reassign a precise repair if a worker crossed ownership. Do not hide conflicts with broad checkout/reset commands.
3. Review the integrated behavior across request capture → effective effort → search → final authority → mapped action. Look specifically for schema/version mismatch, zero-time escalation, changed deterministic ties, false fidelity claims, hidden-truth leakage, and stale-action acceptance.
4. Run the repository's combined release workflow once on the integrated candidate: `npm run verify`. This includes type-checking, TypeScript tests, Rust formatting/tests/clippy, and packaged build. Diagnose a failure before editing; independent failures outside the assigned contracts need a separate scope decision. Do not weaken checks to complete the handoff.
5. Build the companion with `npm run build:companion` for T5. Generated WASM must come from the normal build, not hand edits. Record source SHA plus dirty state, toolchain, WASM hash, companion hash, and packaged manifest/build identity. Review generated changes before including them.
6. Freeze the resulting candidate revision/artifact pair for T5/T6. If source changes after a gate, rerun only the checks invalidated by that change. Keep build directories and raw logs unversioned.

**Done:** reviewed integrated patch, actual verification results, artifact identities, and no unexplained generated/source mismatch. A failed or unavailable release check blocks a full acceptance claim. Commit/push only when Hamza's session authorization covers it.

## T5 — CUDA worker: refresh correctness evidence, then bounded retention smoke

**Dependency:** T4's frozen source/WASM/companion, compatible CUDA hardware, and assigned execution window. **Default work:** validation and evidence, not optimization.

**Potential repair files:** `scripts/verify-mref-native.mjs`, `engine/crates/catan-arena/src/bin/exact-gpu-parity.rs`, `engine/crates/catan-arena/src/bin/exact-gpu-search-parity.rs`, `engine/crates/catan-search/src/cuda_exact.rs`, `engine/crates/catan-wasm/src/native_gpu.rs`, `engine/crates/catan-native-host/src/main.rs`. Request ownership from T4 before mutation; a repair invalidates the frozen build and returns through T4. Keep resident rollout kernels and strategy tuning out of scope.

1. Record GPU/runtime availability and artifact identities. Missing hardware ends this task as BLOCKED with a resume command; it does not justify a CPU imitation labeled CUDA.
2. Run the existing exact gates from `engine/`:

   ```bash
   cargo run --release -p colonist-catan-arena --features cuda-exact --bin exact-gpu-parity
   cargo run --release -p colonist-catan-arena --features cuda-exact --bin exact-gpu-search-parity
   ```

   From the repository root, run `node scripts/verify-mref-native.mjs` against T4's rebuilt artifacts.
3. Record actual cases/counts and compare actions, values within the existing contract tolerance, authority, effective effort, nodes/depth, retained roots, and root-work evidence where the runner exposes them. Check actual request topology, Mref/invalid evidence, explicit M2 parity, cancellation and recovery. Numerical tolerance does not authorize arbitrary action differences; investigate disagreements under the declared tie rule.
4. Stop on parity failure. Return the smallest divergent request, expected/actual results, source owner and reproduction command. Do not run performance measurements on a known divergent candidate.
5. After parity passes, run only the bounded smoke, never the default 250-block campaign:

   ```bash
   npm run benchmark:gpu:exact -- --smoke --output benchmark-results/engine-stabilization-v14/exact-gpu-arena-smoke.json
   ```

   Record matched seeds, all seat rotations, settings, cutoffs, errors, and complete request timings for both 3P and 4P. Check the runner's current help before changing options. A smoke is not a held-out strength campaign and does not by itself authorize production promotion regardless of its speedup.

**Evidence destination:** raw execution output under ignored `benchmark-results/engine-stabilization-v14/`; propose curated machine-readable results with provenance under `docs/benchmarks/engine-stabilization-2026-09-08/`. T7 curates publication. Never overwrite the historical 2026-09-07 artifacts or relabel them v14.

**Done:** current-candidate gate results and bounded timing results, or a precise blocked/failing gate. A timing ratio alone neither fails acceptance nor authorizes promotion; return the soundness/robustness disposition with measured timings. Profiling/optimization is a separate assignment.

## T6 — Browser worker: packaged recommendation-to-execution evidence

**Dependency:** T4 artifacts and an available consensual test game/browser fixture. Use the agent-browser skill and its supported browser workflow. Account setup, unknown credentials, or absent consent is a human boundary; report the exact missing input.

**Default ownership:** evidence only. Read `src/content/overlay.ts`, `src/content/session.ts`, and decision-worker boundaries to correlate events. Source repair requires a separate bounded assignment from T4, with a focused regression for a reported live failure.

1. Confirm the loaded unpacked extension matches T4's build identity. Reload the extension and refresh test tabs after rebuilding; record companion identity if present. Do not reuse a stale tab as current-build evidence.
2. Record one ordinary recommendation through search winner, final authority, mapped action, click, and confirmed board transition. Preserve honest uncertainty in the record and avoid unrelated private browser/account data.
3. Exercise a board change while work is pending and verify the stale result does not click. Exercise one existing multi-click/trade flow, verifying state signature/legal target revalidation and cancellation/replanning on mismatch. Record failure/rejected-bundle behavior without creating a repeated-offer loop.
4. Cover companion connected/disconnected behavior and a mandatory action; ordinary production must remain CPU/WASM. Record actual player count and test long-name/3–4-player layout cases where the fixture supports them; unavailable cases remain explicit gaps.

**Done:** artifact identities plus trace/screenshot/event evidence link recommendation to actual state transition, with stale-work rejection demonstrated. jsdom, native parity, and a harness launch are not substitutes for this gate. Do not enable production exact routing to obtain a GPU screenshot. A missing exact packaged-execution gate continues to block exact promotion.

## T7 — Coordinator: update acceptance and return the review

**Allowed documentation edits:** this task ledger, `ENGINE_STABILIZATION_ACCEPTANCE_2026-09-07.md`, the governing plan, relevant curated benchmark evidence. Update `README.md`, `CPU_GPU_MREF_CONTRACT.md`, or `WINDOWS_GPU_RUNTIME.md` only when a verified current statement needs correction.

1. Separate historical v13 results, focused prior v14 results, and newly reproduced candidate results. Attach revision/dirty-state and artifact provenance to every new claim. A final clean SHA cannot retroactively label an earlier dirty build.
2. Give every assigned gate PASS / FAIL / BLOCKED / NOT RUN and its evidence or resume condition. Keep CPU release readiness, exact-backend correctness, informational performance measurements, packaged execution, and playing strength distinct.
3. Apply the clarified soundness/robustness requirements; do not reinstate the superseded 2× threshold. Completing these tasks never silently enables GPU routing. Keep the strength conclusion “no evidence of improvement” unless a separately designed, held-out product-route campaign supports a different conclusion.
4. Return a reviewed diff summary, remaining merge blockers, nonblocking issues, verification commands/results, and decisions requiring Hamza. Stop here; do not launch strategy or budget experiments.

## Copyable assignment

Replace the bracketed values before sending:

> Work on **[T1/T2/T3/T5/T6]** only in **[absolute checkout path]**, based on **[full SHA]**. Read `AGENTS.md`, the common instructions and your task in `docs/ENGINE_STABILIZATION_SUBAGENT_TASKS_2026-09-08.md`. You are not alone in this codebase; preserve others' work. Your allowed edits and stopping rule are those in that task. First verify current implementation; make changes only for an evidenced assigned-contract failure. **[State whether the listed focused test execution and any necessary focused test changes are explicitly authorized.]** Keep shared files with the coordinator. Return the handoff below; do not commit, push, broaden scope, or start the next task.

## Required worker handoff

```text
Task / disposition: Tn — VERIFIED NO CHANGE | FIXED | FAIL | BLOCKED
Checkout / base SHA / final SHA or dirty state:
Files changed (or none):
Observed failure and causal owner (or verified contract):
Smallest change and why each file was necessary:
Acceptance rows: result + source/check/fixture reference for each
Verification: exact commands, exit results, counts, artifact identities
Not run / limitations / precise resume condition:
Shared-file requests or independent findings (no unsolicited fixes):
Review target: commit range or explicit changed-file diff against base
```

## Deferred work — not worker assignments

GPU optimization, a larger matched campaign, held-out strength measurement, production budget changes, M2 promotion, M3/M4/M5, and learned models need separate tasks with frozen hypotheses and budgets. If resumed later, preserve the governing experiment order: depth at fixed nodes, nodes at fixed depth, then M2 off/on, then independently justified new mechanisms.

## Planning evidence

This handoff used current Git state, the preserved readable transcript's final requests, the existing stabilization/acceptance documents, runner configuration, and targeted source/test-name checks. It did not replay all transcripts, audit all 81 implementation files, or rerun acceptance. Codebase Memory project `home-hamza-repo-colonist-assistant` reported ready; coverage metadata generation `2026-09-08T01:41:31Z` matched the inspected paths with no recorded issues. That best-effort result is not completeness proof; workers must verify their own current scope.
