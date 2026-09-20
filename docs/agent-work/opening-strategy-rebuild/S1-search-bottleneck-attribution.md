# Agent S — Mission S1: Search Bottleneck Attribution Study

## Session

Open a NEW Codex session for Agent S.

Use normal Codex/local-shell operation. Do not use ChatGPT Web Harness instructions.

## Role

Role: independent search-diagnostics / research tooling  
Production mutation authority: none  
Review independence: not a production review

This mission starts after Agent M completed:

- M1: no general build-transition defect;
- M2: no general same-turn action-order defect;
- M3: no general decisive-planner defect.

M3 did establish that ordinary settlement completion can mechanically satisfy the current decisive-planner admission rule in two unrelated states, but matched outcome evidence was mixed and the closeout control was not reconstructable enough to authorize a repair. Do not implement or tune that rule in S1.

## Repository

Repository:

`/home/hamza/repo/colonist-assistant`

Verified production base:

`main == origin/main == 9aee018fe1c8522870ed9961740946a943f75109`

Preferred isolated worktree:

`/home/hamza/repo/colonist-search-bottleneck`

Preferred branch:

`agent/s1-search-bottleneck`

Create/reuse the worktree from the verified main ref, not from the dirty main checkout contents.

Important unrelated main-worktree state:

`src/generated/wasm/colonist_search_bg.wasm` is modified in the main checkout.

Do not touch, reset, stage, regenerate, or infer ownership of that file.

## Read first

1. `AGENTS.md`
2. `docs/JEV_STRATEGY_RESEARCH_PLAYBOOK.md`
3. M1 report:
   `/home/hamza/repo/colonist-midgame-transition-study/docs/benchmarks/m1-midgame-build-transition-2026-09-20.md`
4. M2 report:
   `/home/hamza/repo/colonist-midgame-transition-study/docs/benchmarks/m2-same-turn-action-order-2026-09-20.md`
5. M3 report:
   `/home/hamza/repo/colonist-midgame-transition-study/docs/benchmarks/m3-decisive-planner-admission-2026-09-20.md`

The Agent M branch contains useful research-only `jev-strategy-lab` instrumentation. You may inspect and selectively port only the research tooling needed for S1. Do not cherry-pick the whole M1-M3 branch.

## Why S1

The recent studies increasingly point to finite-search behavior rather than a single evaluator weight:

- M2 State F had a legal paired road pruned as `BranchTruncated` at pre-truncation rank 15 under branch cap 12.
- M2 State C/E/G values and orderings changed non-monotonically across depth/budget.
- M3 state `986017bab0aab69d` changed ordinary winner at 48k and closed the backed-up value gap.
- M3 State A retained a large planner/search disagreement even at 48k.
- Several current-main decisions complete only depth 1 or 2 under realistic node budgets.

The next high-value question is therefore not “which heuristic should be added?” but:

> Which bounded-search resource is actually limiting decision quality most often: root candidate coverage, within-root node allocation, completed depth, or belief-particle allocation?

S1 is an attribution benchmark. Its purpose is to identify the highest-leverage search bottleneck before authorizing any architecture change.

## Primary hypotheses

### H1 — branch/root coverage bottleneck

The baseline branch cap or family admission policy omits strategically competitive roots often enough to alter the selected action.

### H2 — depth/node bottleneck

The correct root is admitted, but bounded search does not reach enough completed depth for stable ordering.

### H3 — node-allocation bottleneck

The root set is adequate, but node allocation across roots/families is too skewed, leaving competitive roots under-evidenced.

### H4 — belief-particle bottleneck

Decision instability is primarily caused by insufficient or poorly allocated belief-world coverage rather than ordinary tree search.

### H5 — no dominant bottleneck

Observed instability is state-specific; no one search resource explains enough cases to justify a generic production change.

Do not decide the answer before the experiment.

## Frozen state cohort

Use a small, pre-registered cohort of 6–8 states.

Mandatory evidence donors, if reconstruction fidelity is sufficient:

1. M2 State F — branch-truncated road control.
2. M2 State G — parameter-specific road allocation control.
3. M2 State C — same-turn City/Road-Building state.
4. M3 M1-State-A reproducer `88b38976489011f5`.
5. M3 ordinary case `ad33bf6fa633dd7a`.
6. M3 ordinary case `986017bab0aab69d`.

Optionally add up to two unrelated current-main states where:

- baseline completed depth is only 1–2;
- the top root margin is small or root admission is visibly constrained;
- the state can be reconstructed exactly.

Freeze the cohort before inspecting the comparative configuration outcomes.

Do not select states based on which configuration “wins.”

## Stage A — mechanical candidate-coverage audit

For each state, enumerate the complete legal root set and record:

- legal root count;
- pre-truncation policy rank;
- action family;
- baseline retained/pruned status;
- prune reason;
- baseline prior;
- baseline allocated nodes;
- whether an exact-family/planner/safety path exists.

Then run deterministic root-admission diagnostics with the SAME total ordinary node budget under:

- baseline branch cap 12;
- expanded branch cap 20;
- expanded branch cap 32, if legal-root count makes this meaningful.

Do not change evaluator weights or action semantics.

Primary coverage metric:

- does a baseline-pruned root become competitive/top-ranked when admitted?
- does expanding coverage merely dilute nodes and reduce completed depth?
- which action families are disproportionately pruned?

## Stage B — fixed-root depth/budget audit

Using the full baseline retained root set, compare fixed ordinary node budgets:

- 12k;
- 48k;
- 96k;
- optionally 192k only for states still unresolved at 96k.

Zero wall-clock cutoffs.

Record:

- completed depth;
- chosen root;
- root value / LCB;
- root ordering;
- node allocation per root;
- value-margin stability;
- whether the recommendation changes;
- whether a previously large disagreement collapses.

Do not treat the largest-budget root as terminal truth. It is a diagnostic reference only.

## Stage C — particle sensitivity audit

Only for states whose root ordering remains unstable after Stage B.

Compare a bounded particle matrix while holding ordinary node budget fixed:

- 12 belief particles;
- 24 belief particles;
- 48 belief particles if practical.

Keep strategic-particle semantics explicit.

Record recommendation stability, LCB width, and node dilution.

Do not run particle sweeps on every state automatically.

## Stage D — attribution

For every state classify the dominant observed limiter:

- ROOT_COVERAGE
- DEPTH
- NODE_ALLOCATION
- PARTICLE_UNCERTAINTY
- PLANNER/ARBITRATION
- STABLE / NO LIMITER
- MIXED

A classification must be supported by concrete counterfactual evidence, not by intuition.

## Terminal evidence

Do not begin with a terminal campaign.

Only if one bottleneck class changes the selected root in at least TWO unrelated states under a mechanically coherent mechanism, run a small forced-root matched continuation check:

- baseline production root;
- bottleneck-relieved root;
- 2 independent CRN seeds per qualifying state initially.

Maximum terminal arms without user approval: 16.

Use event-family CRN and zero wall-clock continuation search.

If the two-seed screen is mixed, report mixed. Do not automatically add seeds.

## Admission bars

### ROOT-COVERAGE DEFECT CANDIDATE

Require at least two unrelated states where:

1. baseline prunes a legal root;
2. admitting that root under the same total node budget makes it a stable top competitor;
3. the effect is not merely node dilution/noise;
4. a control state shows expanded coverage does not always improve/alter the recommendation.

### DEPTH/BUDGET DEFECT CANDIDATE

Require at least two unrelated states where:

1. the same candidate set is available;
2. shallow bounded search selects a root that is unstable;
3. greater deterministic work converges in a consistent direction;
4. the mechanism is not explained by planner/exact-family replacement.

### NODE-ALLOCATION DEFECT CANDIDATE

Require repeated evidence that competitive roots are admitted but systematically under-allocated relative to weaker roots/families, and that a research-only reallocation diagnostic stabilizes the decision.

Do not implement reallocation in S1.

### PARTICLE DEFECT CANDIDATE

Require repeated evidence that recommendation instability tracks belief-particle coverage rather than ordinary node/depth limits.

## Hard negatives

Include controls where:

- baseline cap 12 is sufficient;
- larger branch cap harms depth without changing the correct/stable root;
- more nodes do not change ordering;
- more particles widen cost without useful stability.

The study must be able to conclude that “more search” is not universally better.

## Research-only tooling

You may add narrow harness support for:

- full legal-root dumps;
- retained/pruned provenance;
- pre-truncation ranks;
- per-root node allocation;
- fixed branch-cap overrides;
- fixed node-budget overrides;
- particle-count overrides;
- stable state replay;
- comparison summaries.

No production strategy/search semantics may change.

## Artifacts

Ignored:

`benchmark-results/jev-lab/s1-search-bottleneck/`

Tracked report:

`docs/benchmarks/s1-search-bottleneck-attribution-2026-09-20.md`

Freeze:

- state manifest;
- configuration manifest;
- seed list if terminal screening becomes admissible;
- SHA256 manifest where practical.

## Checkpoints

Checkpoint 1:
  frozen state cohort + baseline mechanical audit

Checkpoint 2:
  branch-cap and depth/budget results

Checkpoint 3:
  particle sensitivity only where justified

Checkpoint 4:
  bounded terminal screen only if admission bar is reached

Checkpoint 5:
  final attribution

Commit coherent recovery boundaries.

## Final decision

Return exactly one top-level conclusion:

- ROOT-COVERAGE DEFECT CANDIDATE ESTABLISHED
- DEPTH/BUDGET DEFECT CANDIDATE ESTABLISHED
- NODE-ALLOCATION DEFECT CANDIDATE ESTABLISHED
- PARTICLE DEFECT CANDIDATE ESTABLISHED
- MULTIPLE SEARCH BOTTLENECKS ESTABLISHED
- NO DOMINANT SEARCH BOTTLENECK ESTABLISHED

If a defect candidate is established, define a narrow S2 production-repair experiment scope but DO NOT implement it.

## Verification

Before finish:

- git diff --check;
- relevant arena/lab tests;
- exact frozen hashes for all admitted states;
- no production strategy/search files changed;
- clean worktree;
- terminal-arm count within cap if any.

## Finish report

Return:

1. status;
2. Agent S — Mission S1;
3. branch/worktree/commits;
4. frozen state cohort;
5. baseline root-admission audit;
6. branch-cap results;
7. node/depth results;
8. particle results where run;
9. per-state bottleneck attribution;
10. any bounded terminal screen;
11. hard-negative behavior;
12. top-level final decision;
13. narrow S2 scope if established;
14. recommended next experiment if not;
15. tests/checks;
16. final worktree status.

Do not implement production changes in S1.
