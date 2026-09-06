# Agent A1 — Milestone 2 Experimental Candidate Admission

**Mission:** A1 — implement Milestone 2 end-to-end on the current local `main` worktree.

**Goal:** Turn Milestone-1 strategy evidence into an **explicitly opt-in experimental root-admission policy** that may admit at most three evidence-backed strategy challengers into the existing root cap, then compare every admitted root with the same authoritative search/evaluator. The production/default policy must remain unchanged when the experiment is not requested.

**Architecture:** Keep strategy generation and admission evidence in `catan-search`; keep final action authority in the existing shared search. Add an explicit strategy-policy identity to the request/search configuration so candidate admission is testable and benchmarkable without changing the default engine. Reuse existing root-cap, mandatory/safety, EndTurn, spatial-promotion, complete-wave, WASM, tracing, and arena infrastructure rather than creating a parallel planner.

**Tech stack:** Rust (`catan-core`, `catan-search`, `catan-wasm`, `catan-arena`), CUDA/native routing boundaries where needed for capability safety, TypeScript worker/background contracts, generated WASM package, Vitest/Cargo tests, existing arena benchmark tooling.

## 1. Authoritative starting state

Repository:

`/home/hamza/repo/colonist-assistant`

Workspace:

- current checkout / `main` only;
- one writer: Agent A1;
- local Milestone-1 HEAD before this coordination brief: `06b14f8` (`Add Milestone 1 strategy shadow diagnostics`);
- Milestone-0 controlled-player baseline: `c21e4ae`;
- pre-M0 reference used by the earlier empirical screen: `d80e4b4`.

Important Git state before this brief was created:

`main...origin/main [ahead 2, behind 1]`

`origin/main` points to sibling checkpoint `d9f41c8`; local `main` intentionally carries `c21e4ae -> 06b14f8` instead. **Do not pull, fetch-and-merge, rebase, reset, cherry-pick, or otherwise reconcile remote history as part of A1. Do not push.** Work only from the local assigned checkout and report the final local commit(s).

Read first:

1. `/home/hamza/repo/colonist-assistant/AGENTS.md`
2. `docs/ADAPTIVE_STRATEGY_LAYER_DESIGN_2026-09-06.md`
3. `docs/MILESTONE_0_1_EMPIRICAL_SCREEN_2026-09-06.md`
4. `engine/crates/catan-search/src/strategy.rs`
5. `engine/crates/catan-search/src/reachability.rs`
6. `engine/crates/catan-search/src/depth.rs`
7. `engine/crates/catan-search/src/shared.rs`
8. `engine/crates/catan-wasm/src/lib.rs`
9. `engine/crates/catan-wasm/src/native_gpu.rs`
10. `src/worker/deep-search.ts`, `src/background/index.ts`, and `src/background/native-gpu.ts`
11. `engine/crates/catan-arena/src/main.rs`

Use `causal-coding` before implementation mutation. Use `mcp-harness-router` for authoritative repository/process work. Keep the mission bounded with `moyu`/`ponytail` principles where useful. If a long benchmark or build needs durable execution, use `persistent-agent-loop` plus the routed terminal/wait primitives.

## 2. Non-negotiable mission boundary

This is **Milestone 2 only**.

In scope:

- explicit experimental strategy-policy identity;
- evidence-ranked challenger selection from Milestone-1 strategy proposals;
- admission of at most three additional distinct strategy challengers into the existing root cap;
- fixed admitted roster for the decision;
- existing common search/evaluator deciding the winner;
- diagnostic/provenance output for what was proposed, admitted, omitted, protected, and why;
- CPU/WASM end-to-end experimental request path;
- safe native-GPU capability/routing behavior for an explicit experimental request;
- arena/offline switch sufficient to compare baseline admission vs Milestone-2 admission;
- targeted correctness tests and a bounded matched empirical screen.

Out of scope:

- Milestone 3 dynamic challenger re-entry / reconsideration;
- increasing the root cap to make the experiment look better;
- comparing a shallow challenger with a deeper incumbent as if they shared a horizon;
- strategy-specific utility bonuses or a second evaluator;
- evaluator-weight tuning to force desired decisions;
- learned value/policy promotion, retraining, or schema work;
- transition-aware Mref economic forecasting (Milestone 4);
- full belief-contingent future-self optimization (Milestone 5);
- changing dice/dev/steal chance law;
- changing CUDA rollout policy or cutoff semantics unless an unavoidable compatibility defect is proven;
- production/default promotion of the experimental strategy policy;
- remote Git reconciliation or push.

If completing Milestone 2 appears to require one of those out-of-scope changes, stop and return `needs decision` with the concrete blocker instead of silently expanding the mission.

## 3. Required policy contract

Introduce one canonical experimental strategy-policy identity:

`adaptive-candidate-admission-v1`

Use a typed/central owner in Rust rather than scattering string comparisons.

Contract:

- missing/absent strategy-policy request => current repaired baseline admission behavior;
- explicit `adaptive-candidate-admission-v1` => Milestone-2 admission enabled;
- explicit unknown strategy-policy identity => fail closed or route to a backend that explicitly supports the requested identity; never silently ignore the request;
- response echoes the strategy-policy identity actually used when an explicit experiment is active;
- cache/request identity must not conflate baseline and experimental admission;
- production routing remains baseline by default.

The exact field name may follow existing request naming conventions, but prefer `strategyPolicy` at JS/WASM boundaries and an enum such as `StrategyPolicy` in Rust.

## 4. Admission semantics

Start from the **same complete observation-safe ranked proposal domain and the same existing baseline admission machinery** used before Milestone 2. Do not invent a separate legal-action generator.

For one decision:

1. Run existing validation, exact/mandatory handling, tactical authority, root exclusions, belief construction, root ordering, spatial/closeout evidence, and repaired controlled-player continuation exactly as the baseline requires.
2. Produce Milestone-1 context/reachability/strategy proposals before the new admission policy changes the retained root roster.
3. Each catalog strategy may contribute at most **two** proposals. Deduplicate all strategy proposals by canonical first `Action`; one action may carry multiple supporting strategy tags but receives only one search slot/value.
4. Compute the baseline retained roster using the existing root-cap/admission rules first. Milestone 2 may replace only candidates that are not protected by the rules below.
5. Protect, at minimum, every action the baseline admission needs for existing correctness/safety semantics:
   - immediate/proven winning roots and mandatory forced-loss escape roots;
   - any retained `EndTurn` reservation required by the existing baseline;
   - the baseline ordering leader when it is present in the baseline retained roster;
   - existing retained spatial/closeout promotion roots used for current tactical/strategic protection.
   If these protected roots consume the cap, optional strategy challengers are omitted and the provenance says so.
6. Admit at most **three additional distinct strategy challengers** total. The total retained roots must never exceed the backend's existing configured root cap.
7. A challenger is meaningful only when it is not already retained by the baseline. Do not count a strategy tag attached to an already-retained action against the three-challenger allowance.
8. Rank optional challenger coverage by this evidence hierarchy:
   - **Tier A — sound necessary-source evidence:** e.g. `FutureDevelopmentVpRequired` backed by the reachability bound;
   - **Tier B — directly contested/scarce opportunity evidence:** spatial race, Longest Road/Largest Army competition, or an existing concrete root-impact/closeout signal;
   - **Tier C — bounded current-turn utility evidence:** production/development option proposals ordered by existing planner/root diagnostic evidence when available.
   Within a tier, use existing baseline rank/planner evidence; final ties use deterministic canonical action identity. This ranking allocates consideration only. It must not become final utility.
9. When a challenger is admitted, evict only the lowest-priority **unprotected** baseline retained candidate needed to stay within the existing cap. If no such slot exists, omit the challenger.
10. Freeze this admitted set for the decision. **No omitted candidate may re-enter later in the same decision.** Re-entry/reconsideration is Milestone 3.
11. Every admitted root is evaluated by the same existing common search/evaluator/controlled-player continuation semantics as every baseline root. No strategy-local score may directly choose the final action.
12. Existing complete-wave/fallback authority wins over partial experimental work. A timeout or incomplete comparison cannot replace a previously complete authoritative result merely because a challenger was encountered first.

## 5. Diagnostic / provenance requirements

Extend the Milestone-1 shadow evidence rather than replacing it. The trace must make the experiment auditable.

For each distinct strategy proposal, record enough information to answer:

- supporting strategy ID(s) and reason(s);
- baseline rank, if in the ranked domain;
- whether it was already baseline-retained;
- evidence tier used for admission ordering;
- whether it was selected as a challenger;
- whether it was actually admitted;
- if omitted/rejected, why (examples: already retained, duplicate action, challenger limit, protected-capacity exhaustion, lower evidence rank, not legal/common observation domain);
- which baseline candidate, if any, it displaced;
- final common-search result/rank for admitted roots where the existing provenance already exposes it.

At the decision level record:

- strategy-policy identity;
- root cap;
- baseline retained count;
- challenger candidates considered;
- challengers admitted (<=3);
- protected-root count;
- omitted challenger count and reasons;
- baseline search winner versus final authoritative action, preserving existing exact/safety replacement trace.

Do not claim a proposal was evaluated when it was omitted before common search.

## 6. Backend and routing requirements

### CPU / WASM

The explicit experiment must work end-to-end through the packaged WASM request path and produce the admission provenance above.

Prefer centralizing the policy/config in `catan-search` and passing it through the existing belief-search configuration instead of adding a second search entry point.

### Native GPU

Milestone 2 does **not** require reimplementing strategy generators in CUDA kernels. The design allows host-side admission before CUDA evaluation, but semantic capability must be explicit.

Choose the smallest correct implementation:

- either support the exact same fixed challenger-admission contract in the native Rust host before GPU root rollout; **or**
- mark the experimental policy unsupported by the current native companion and route explicit experimental requests to CPU/WASM.

Do not let an explicit `adaptive-candidate-admission-v1` request fall through to baseline native GPU behavior while pretending the request was honored. Do not change ordinary native-GPU routing when the field is absent.

If a native protocol/capability field is necessary to distinguish support safely, keep it minimal and version/capability driven. Do not perform unrelated protocol redesign.

## 7. Offline arena / empirical isolation

Add the minimum offline control needed to compare:

- repaired baseline admission; versus
- `adaptive-candidate-admission-v1`;

while holding search algorithm, node budget, controlled-player continuation, evaluator, belief policy, board/chance seeds, player count, victory target, and dice mode fixed.

Prefer an arena/config flag that maps to the same central Rust `StrategyPolicy` used by production/WASM rather than duplicating admission logic in the arena.

This is candidate-admission isolation. Do not combine Milestone-3 re-entry, evaluator tuning, learned models, or other experimental mechanisms into the same A/B.

## 8. File ownership map

Likely primary files (inspect actual ownership before editing):

- `engine/crates/catan-search/src/strategy.rs` — strategy evidence/admission ranking and diagnostics;
- `engine/crates/catan-search/src/depth.rs` — policy config plumbing and CPU/CUDA-exact root-roster integration;
- `engine/crates/catan-search/src/shared.rs` — only if a shared root-admission primitive belongs here; preserve baseline behavior for callers not requesting the experiment;
- `engine/crates/catan-search/src/lib.rs` — exports;
- `engine/crates/catan-wasm/src/lib.rs` — request/response strategy-policy contract and provenance serialization;
- `engine/crates/catan-wasm/src/native_gpu.rs` — only for exact host-side support or explicit capability-safe rejection/routing contract;
- `src/generated/wasm/colonist_search.d.ts` and packaged WASM — regenerated by the repository build workflow, not hand-invented;
- `src/core/engine.ts` — public TS diagnostic types;
- `src/worker/deep-search.ts` — request/provenance mapping;
- `src/background/index.ts`, `src/background/native-gpu.ts` — only if explicit experiment routing/capability requires it;
- `engine/crates/catan-arena/src/main.rs` — offline strategy-policy A/B switch using the central implementation;
- focused tests adjacent to existing Rust/TS admission and packaged-WASM tests;
- `docs/ADAPTIVE_STRATEGY_LAYER_DESIGN_2026-09-06.md` — update Milestone-2 status only after implementation/evidence is real.

Do not touch CUDA/PTX merely because native GPU exists. Only touch CUDA rollout/packed-state artifacts if the chosen host-side design provably requires a semantic change; otherwise keep them unchanged.

## 9. Implementation tasks

### Task 1 — Central strategy-policy and admission result

**Files:** primarily `strategy.rs`, `lib.rs`, and the smallest config owner used by belief search.

**Produces:**

- typed baseline vs `adaptive-candidate-admission-v1` policy;
- evidence-tiered, deterministic challenger ordering;
- a fixed admission result containing retained roots plus admission diagnostics;
- baseline path that preserves current behavior when the experiment is disabled.

**Acceptance:** same inputs with baseline policy produce the same retained-root behavior as before A1; experimental policy can add no more than three challengers and never increases total cap.

### Task 2 — CPU/CUDA-exact common-search integration

**Files:** `depth.rs` plus shared owner only where required.

**Produces:** experimental retained roster before root search; every retained root goes through the existing common comparator and complete-wave discipline.

**Acceptance:** no strategy-local score becomes final authority; no fixed-roster candidate re-entry; mandatory/exact/safety/EndTurn/protected roots remain correct.

### Task 3 — WASM/TypeScript request and provenance contract

**Files:** WASM lib, generated declarations/artifact through normal build, TypeScript engine/worker types and mapping.

**Produces:** optional explicit `strategyPolicy`; unknown explicit values fail closed; response/trace exposes actual policy plus admission evidence.

**Acceptance:** absent field remains backward-compatible baseline; explicit experiment survives TS -> WASM -> Rust -> response round trip.

### Task 4 — Native routing/capability safety

**Files:** native host/background owners only as required.

**Produces:** either exact native-host support for the fixed strategy admission contract or explicit CPU/WASM routing for experimental requests.

**Acceptance:** no explicit experiment can silently execute baseline native GPU admission; ordinary default native routing is unchanged.

### Task 5 — Arena control for isolated A/B

**Files:** arena config/CLI and the central search config plumbing it needs.

**Produces:** deterministic baseline vs experimental admission runs with matched seeds and identical search budgets.

**Acceptance:** arena reports the strategy-policy identity/config and does not duplicate the admission algorithm.

### Task 6 — Correctness verification and bounded empirical screen

Testing/validation is explicitly authorized for A1.

You may:

- add/modify focused Rust and TypeScript tests required to prove the Milestone-2 contract;
- run targeted Cargo tests/checks, TypeScript typecheck, packaged WASM build/boundary tests, and native-feature compile checks relevant to the touched code;
- run a bounded matched arena screen after correctness checks pass.

Required focused assertions should cover, at minimum:

1. baseline policy preserves the existing root roster;
2. at most three distinct challengers are admitted;
3. total root cap never increases;
4. mandatory/immediate-win/forced-loss protection cannot be evicted by a strategy challenger;
5. retained EndTurn protection is preserved under the experiment;
6. baseline leader protection is preserved when baseline admission retained it;
7. duplicate multi-strategy support uses one action slot;
8. necessary-source evidence outranks lower-tier optional proposals;
9. no unprotected slot => challenger omitted with an explicit reason;
10. hidden-world ordering / observation-equivalent particles do not change the admission policy for the same actor observation;
11. missing strategy policy stays baseline;
12. unknown explicit policy fails closed;
13. packaged WASM returns the experimental identity and provenance when requested;
14. native routing never silently drops the explicit experimental request.

Use the smallest relevant existing test commands first. Do not run the entire repository test suite unless an authoritative repository rule requires it or a targeted failure makes broader evidence necessary.

After correctness is green, run a **small matched pilot**, not a promotion campaign. Use fixed node budgets and time budgets disabled where practical so process contention cannot change policy. Compare baseline vs `adaptive-candidate-admission-v1` on the same seeds with seat rotation. Include 2-player and at least one multiplayer stratum; use 2/3/4 players if practical under the existing reduced-effort profile. Record wins, mean rank/VP, cutoffs, admission frequency, how often a challenger displaced a baseline root, how often an admitted challenger became the common-search winner, and any latency/search-depth change. Label the result pilot evidence only.

Do not tune the implementation against those pilot seeds and rerun until it looks good. If the first honest pilot is negative, report it.

## 10. Success conditions

A1 is complete only when all of the following are true:

- default/missing policy is behaviorally the repaired baseline admission path;
- explicit `adaptive-candidate-admission-v1` is available through the intended offline/WASM path;
- no more than three distinct strategy challengers can enter a decision;
- total root cap is unchanged;
- protected baseline roots cannot be displaced by optional strategy challengers;
- the admitted set is fixed for the decision; no Milestone-3 re-entry exists;
- common search/evaluator remains sole ordinary winner authority;
- strategy-local evidence is used only for admission ordering;
- diagnostics clearly distinguish proposed/already-retained/admitted/omitted/evaluated;
- unknown explicit policy is not silently ignored;
- native routing is semantically safe for explicit experimental requests;
- targeted correctness/build/package checks pass;
- bounded empirical A/B is reported without overstating significance;
- `docs/ADAPTIVE_STRATEGY_LAYER_DESIGN_2026-09-06.md` accurately reflects the resulting Milestone-2 status;
- repository has no unrelated working-tree changes introduced by A1.

## 11. Commit and handoff contract

When implementation and authorized verification are complete:

1. inspect the entire attributable diff;
2. do not modify/reconcile `origin/main`;
3. create a coherent local commit (or a minimal series if generated-artifact discipline genuinely requires it) on the assigned local `main`;
4. **do not push**;
5. leave the worktree clean except for any pre-existing state you found at mission start;
6. return the exact base commit, final HEAD, commit list, and `git status --short --branch`;
7. return the finish report below.

## 12. Required finish report

Return exactly enough for the planner/reviewer to continue:

1. `status: complete | blocked | needs decision`;
2. `Agent A, Mission A1`;
3. role: implementation;
4. repository/workspace/branch;
5. starting HEAD and final HEAD, plus commit(s);
6. concise behavior/interface summary;
7. exact files materially changed;
8. tests/builds/benchmarks actually run and their results;
9. empirical pilot configuration and raw summary, including negative results;
10. native routing/capability disposition;
11. confirmation that Milestone 3+ was not implemented;
12. deviations from this brief;
13. unresolved blockers/decisions;
14. any specific areas the independent reviewer should scrutinize.

After A1 returns, **do not start Milestone 3**. The next action is independent review of A1. Blocking review findings go back to this same Agent A session as A2; the independent reviewer then re-reviews via R2.
