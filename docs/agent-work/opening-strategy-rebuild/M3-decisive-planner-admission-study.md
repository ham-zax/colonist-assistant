# Agent M — Mission M3: Decisive Planner Override Admission Study

## Role

Role: causal diagnosis / research tooling
Session: continue in the existing Agent M Codex session
Production mutation authority: none

M3 is a new research mission after:

- M1: NO GENERAL BUILD-TRANSITION DEFECT
- M2: NO GENERAL ACTION-ORDER DEFECT

Do not reinterpret either null result as a reason to tune generic save/spend or action-order weights.

## Workspace

Continue in:

`/home/hamza/repo/colonist-midgame-transition-study`

Branch:

`agent/m1-midgame-transition-study`

Expected current HEAD:

`9214f5ee2a3bbf25b83a445613964ee527a4d2d4`

Use normal Codex/local-shell operation.

Do not work in the main checkout.

Current main has an unrelated modified generated WASM file:

`src/generated/wasm/colonist_search_bg.wasm`

Do not touch, reset, stage, regenerate, or otherwise interact with that unrelated file.

## Why M3

M1 State A exposed a much sharper production-policy question than the generic save/spend hypothesis:

- ordinary search strongly favored `BuildSettlement(22)`
  - actor value `0.001509`
  - lower-confidence `0.000172`
- the engine nevertheless executed `BuildRoad(53)`
  - actor value `0.000018`
  - lower-confidence `0.000002`

The owner was the decisive current-turn planner replacement:

- road planner value `36.221`
- decisive completion mass `1.0`
- response windows `0`

The road then delayed settlement to 8/12/16 rolls in the three M1 continuations, while immediate settlement converted at roll 0 and was the only State-A root to win a continuation arm.

That evidence is not sufficient by itself for a production defect, but it is a stronger next target than continuing to mine the M2 action-order null result.

## Relevant production logic

Read:

- `engine/crates/catan-search/src/depth.rs`
  - `decisive_current_turn_plan_replacement_index`
- `engine/crates/catan-search/src/planner.rs`
  - `materially_decisive_transition`

Current semantics include:

`materially_decisive_transition` returns true for:

- terminal actor win;
- **any BuildSettlement**;
- **any BuildCity**;
- gaining Longest Road;
- gaining Largest Army.

The replacement path then requires near-certain current-turn completion/decisive mass and zero opponent response windows before allowing the planner to replace the ordinary backed-up search winner.

This rule was originally valuable for task9783-style current-turn closeout/trophy sequences. M3 must determine whether the broad settlement/city notion of “materially decisive” causes unjustified overrides in ordinary midgame states.

## Primary question

> When `decisive_current_turn_plan_replacement` changes the ordinary search winner, does that replacement improve downstream outcomes consistently, or does the current “any settlement/city is decisive” classification overgeneralize the closeout mechanism?

## Mandatory controls

### Positive control — task9783 closeout

Include the existing task9783 current-turn case where the planner replacement was introduced to recover a multi-road sequence that completes Longest Road / closeout value.

Verify the positive-control semantics on the current base.

The experiment must not imply “disable planner replacement globally” if task9783 remains mechanically justified.

### Candidate negative/ordinary control — M1 State A

Use the exact frozen M1 State A:

- source seed `9520001`
- game 0
- decision 101
- observation hash `88b38976489011f5`
- actor 1
- 3 VP
- hand `[1,2,1,2,0]`

Compare at least:

- ordinary search winner `BuildSettlement(22)`
- planner-replaced engine root `BuildRoad(53)`

Retain EndTurn only as diagnostic context unless a third forced root is needed to understand the mechanism.

## Candidate discovery

Mine deterministic current-main trajectories for natural states where:

`provenance.decisive_plan_replacement = Some((search_winner, planner_root))`

Freeze candidates before terminal outcomes.

Target 4 primary replacement states total:

1. task9783 positive control;
2. M1 State A;
3. one unrelated replacement whose decisive event is settlement/city progress but not closeout;
4. one unrelated replacement whose decisive event is trophy/terminal or another genuinely urgent transition, if naturally available.

If four high-fidelity states are not available, use three rather than manufacturing weak examples.

Do not select states based on terminal outcomes.

## Classification audit

For every replacement state record:

- current VP and victory target;
- ordinary search winner;
- replacement root;
- actor value / LCB for each;
- planner value;
- planner completion mass;
- planner decisive completion mass;
- response windows;
- exact current-turn planned action sequence;
- which action in the sequence first triggers `materially_decisive_transition`;
- whether that trigger is:
  - terminal win;
  - settlement;
  - city;
  - Longest Road;
  - Largest Army;
- VP after the triggering action;
- whether the actor is actually within plausible closeout range;
- whether the plan completes a trophy/terminal transition or merely adds ordinary economy/VP.

This classification is essential. Do not collapse all `decisive_completion_mass=1` states into one category.

## Search protocol

Use deterministic, zero-wall-clock search.

For each frozen state:

- reconstruct the exact pre-root observation;
- run current production search and capture:
  - ordinary backed-up winner;
  - decisive-plan replacement;
  - final engine root;
- use the same fixed node/particle budgets across roots;
- verify the replacement provenance is reproducible.

Suggested root diagnostic budget:

- 24 belief particles;
- 12 strategic particles;
- branch cap 12;
- 48,000 ordinary nodes where practical;
- wall-clock cutoff zero.

If the original replacement only appears at a smaller bounded budget, freeze and report that budget separately rather than silently changing it.

## Forced-root matched continuation

For each primary replacement state compare exactly two roots:

A. ordinary search winner before planner replacement
B. planner replacement root

Use:

- same exact pre-root state;
- force only the first root;
- normal current-main continuation afterward;
- event-family common random numbers;
- separate streams for rolls, development draws, and steals;
- deterministic continuation budgets;
- zero wall-clock cutoffs;
- 3 independent continuation seeds.

Target:

4 states × 2 roots × 3 seeds = 24 arms.

Hard maximum: 24 arms unless the user explicitly authorizes more.

Do not add a third root to every state.

## Measurements

For each arm record:

- winner;
- actor win/loss;
- terminal turn;
- actor final VP;
- next settlement/city/road/dev timing;
- trophy acquisition timing;
- whether the planner’s predicted current-turn sequence actually occurs under normal continuation;
- whether the alleged decisive event survives the root;
- response-window count before the claimed transition;
- discard exposure where relevant.

For the root pair also retain:

- ordinary actor value;
- lower-confidence value;
- planner value;
- decisive completion mass;
- completion mass;
- response windows;
- immediate strategic utility delta.

## Hypotheses

### H1 — overbroad settlement/city decisiveness

Any current-turn settlement/city completion may be marked “materially decisive” even when it is ordinary midgame progress and the ordinary search winner is substantially better.

Support requires repeated evidence in at least two unrelated non-closeout states.

### H2 — planner correctly captures multi-action conversion

The ordinary root value may undervalue a current-turn sequence whose first action looks weak but deterministically converts to a stronger settlement/city/trophy before any opponent response.

This is the hard negative against simply trusting the ordinary search winner.

### H3 — closeout-specific mechanism

Planner replacement may be valid for terminal/trophy closeout but too broad for ordinary settlement/city progress.

This is the most important structural distinction to test.

### H4 — finite-budget artifact

Replacement disagreements may disappear when ordinary search receives enough deterministic nodes/depth.

If so, diagnose budget sensitivity rather than changing semantics.

## Stronger-search diagnostic

For each state, run a bounded deeper fixed-node search if practical.

Ask:

- does deeper ordinary search converge toward the planner replacement?
- remain with the original search winner?
- choose a third root?

Do not use deeper search alone as outcome authority.

## General-defect bar

Conclude:

`DECISIVE-PLANNER ADMISSION DEFECT CANDIDATE ESTABLISHED`

only if all are true:

1. at least two unrelated **non-closeout settlement/city-triggered replacements** show the same mechanism;
2. the replacement overrides a materially stronger ordinary search root;
3. matched continuations are directionally consistent against the replacement, or a mechanical invariant demonstrates the replacement’s “decisive” classification is causally inappropriate;
4. task9783 or another genuine closeout/trophy positive control remains correctly handled;
5. the common owner is the decisive planner classification/replacement rule, not unrelated road scoring, evaluator noise, or chance;
6. a narrow invariant can be stated without disabling legitimate closeout planning.

If evidence is mixed or only State A is problematic:

`NO GENERAL DECISIVE-PLANNER DEFECT ESTABLISHED`

Do not implement a repair.

## Potential invariant — hypothesis only

Do not assume this is correct before evidence:

> “Current-turn settlement/city completion should not be sufficient by itself to authorize replacement of a materially stronger backed-up search winner unless the transition is closeout/terminal/trophy-like or otherwise demonstrably decisive.”

M3 exists to test whether some form of that invariant is actually supported.

## Research tooling

Reuse the M1/M2 research harness where appropriate.

You may add:

- replacement-state discovery;
- planner sequence classification;
- forced-root paired continuation;
- stronger-search diagnostics.

Do not change production search or planner semantics.

## Artifacts

Ignored artifacts:

`benchmark-results/jev-lab/m3-decisive-planner/`

Tracked report:

`docs/benchmarks/m3-decisive-planner-admission-2026-09-20.md`

Freeze:

- candidate-state manifest;
- root-pair manifest;
- continuation seeds;
- search budgets;
- artifact SHA256 manifest if practical.

## Checkpoints

Because this is Codex:

Checkpoint 1:
  frozen replacement states + classification

Checkpoint 2:
  deterministic root/replacement reproduction

Checkpoint 3:
  24-arm-or-smaller matched continuation

Checkpoint 4:
  final interpretation

Commit coherent recovery boundaries.

## Out of scope

- generic road-intent repair;
- save/spend heuristic;
- action-order heuristic;
- evaluator weight tuning;
- Jev/API;
- opening evaluator changes;
- WASM/frontend regeneration;
- production planner mutation;
- main merge/push.

## Stop conditions

Stop early if:

- fewer than two unrelated non-closeout planner replacements can be reproduced;
- State A no longer reproduces;
- the positive closeout control fails for unrelated reasons that invalidate the experiment;
- evidence is clearly mixed and no repeated common mechanism remains.

Do not expand the corpus merely to force a finding.

## Finish report

Return:

1. status;
2. Agent M — Mission M3;
3. branch/worktree/commits;
4. frozen replacement states;
5. classification of each “decisive” trigger;
6. ordinary winner vs planner replacement diagnostics;
7. per-seed matched outcomes;
8. current-turn sequence conversion results;
9. stronger-search diagnostic;
10. task9783/closeout positive-control result;
11. hypotheses supported/falsified;
12. final decision:
   - DECISIVE-PLANNER ADMISSION DEFECT CANDIDATE ESTABLISHED
   - or NO GENERAL DECISIVE-PLANNER DEFECT ESTABLISHED
13. exact invariant + narrow M4 scope if established;
14. recommended next experiment if not;
15. tests/checks;
16. final worktree status.

Do not implement production changes in M3.
