# Strategy rebuild session closeout — 2026-09-20

This file is the durable handoff for the opening/midgame strategy research session completed on 2026-09-20.

## Current production state

Local `main` contains the reviewed opening/evaluator work plus the complete orchestration history through the S1 handoff.

Production changes admitted during this program:

- Opening evaluator rebuild:
  - `70e1461` — opening evaluator architecture
  - `8496200` — temporal opening-evaluation repair
- D31 resource-liquidity repair:
  - `a4bb9e6` — diagnosis/research integration
  - `35815e3` — stable persistent-production valuation
- C1/E1 deterministic opening-validation tooling/report:
  - `002b6a8`
  - `213ae85`
  - `0e270f4`
- B1 Jev calibration framework:
  - integrated research-only; B2 remains deferred

No road-intent, generic save/spend, action-order, or decisive-planner production repair was admitted.

The orchestration-only commits that had remained off main were synchronized into main at session close:

- `d96cd4b` — reopen road-intent admission
- `c591980` — close road-intent admission
- `1944ae6` — advance midgame research to planner admission
- `1e88bf4` — pivot to search bottleneck attribution

The corresponding orchestration files are now content-identical between local `main` and `orchestration/opening-rebuild`.

## Experiment ledger

### Opening rebuild / A1 + A2

Result: **complete, independently reviewed, integrated**.

Key changes:

- 4-player opening objective now values the actor's own setup plus causal denial instead of generic strongest-rival subtraction.
- 2-player relative objective remains actor-minus-rival.
- port value is represented through complete-build economics.
- expansion self-funding uses one exact road+settlement ETA instead of duplicate discounting.
- causal denial is measured at the actual later opponent setup decision, not retroactively from the completed board.
- hill6758 and task9783 opening regressions pass in both trade modes.

Independent review:

- R1 found temporal-denial and duplicate-ETA defects.
- A2 repaired them.
- R2 passed.

### D31 resource-liquidity / D1 + D2

Result: **general evaluator defect established, repaired, independently reviewed, integrated**.

Root cause:

Persistent board production was multiplied by hand-dependent dynamic resource weights. A pure hand spend could therefore increase the utility assigned to unchanged production.

Repair:

- persistent production now uses base resource weights;
- dynamic weights remain for marginal/hand-sensitive terms;
- CPU and CUDA exact evaluation were kept aligned.

Evidence:

- D31 production contribution stabilized across the dev-card spend;
- deleting grain once again reduced actor utility;
- expected immediate BuyDevelopment value remained positive;
- five spend-now guards prevented a generic saving bias;
- hardware CPU/CUDA parity was verified on 69 cases.

R3 passed the full D1+D2 range.

### C1 + E1 opening validation

Result: **complete; no universal terminal ranking established**.

Recorded corpus:

- 27 passed
- 0 failed
- 1 ignored

Generated cohort:

- 36 deterministic cases
- 2/3/4 players
- trades OFF/ON
- six seeds per cell

Terminal campaign:

- user-capped at 15 arms
- zero cutoffs
- all four planned pairwise comparisons inconclusive
- `admittedLabels: []`

Consequence:

- static opening evidence remains valid;
- the 15 terminal continuations are explicitly pre-D2 evidence;
- no B2 calibration label was admitted.

### F1 road-intent admission

Report:

`docs/benchmarks/road-intent-f1-validation-2026-09-20.md`

Result: **REJECT/HOLD**.

Critical findings:

- exact hill6758 D27 gate did not reproduce on current main;
- the 4,740-node deterministic replay chose Knight;
- edge 71 had a worse lower-confidence value than edge 53;
- four matched continuation seeds were mixed:
  - one candidate-only win
  - one baseline-only win
  - one both-win
  - one both-loss
- 160k deeper search chose edge 69 rather than edge 71;
- negative controls passed.

Consequence:

- F2 is not authorized;
- no road-intent production integration;
- `wip/road-intent-deadline@2592caf` remains provenance only.

### M1 midgame build-transition study

Report:

`docs/benchmarks/m1-midgame-build-transition-2026-09-20.md`

Research branch final M1 commit:

`ea25476`

Result: **NO GENERAL BUILD-TRANSITION DEFECT ESTABLISHED**.

Protocol:

- 4 frozen midgame states
- 3 roots/state
- 3 matched CRN seeds
- 36/36 terminal arms
- zero wall-clock cutoffs

Findings:

- no repeated save-vs-spend mispricing mechanism appeared in two unrelated states;
- nominal save-for-City paths often converted to other builds first;
- discard exposure correctly made waiting expensive in the hard-negative state;
- development-card opportunity cost was explicitly represented in the clearest test;
- State A exposed a local planner-versus-search arbitration discrepancy but not a generic saving defect.

Consequence:

- no generic save bonus;
- no dev-card penalty;
- no M2 production repair.

### M2 same-turn action-order equivalence

Report:

`docs/benchmarks/m2-same-turn-action-order-2026-09-20.md`

Research branch final M2 commit:

`9214f5e`

Result: **NO GENERAL ACTION-ORDER DEFECT ESTABLISHED**.

Protocol:

- 4 frozen commutative pairs
- City/Road Building, road/City, and two two-road controls
- fixed zero-wall-clock depth/budget diagnostics
- no terminal campaign

Mechanical result:

- every pair reached the exact same post-sequence `GameState`;
- field-by-field audits found zero differences;
- deterministic search from the common post-state was identical.

Search findings:

- State C had one-sided follow-up exposure because controlled continuation keeps only the top-ranked action;
- State E followed different normal continuations;
- State F had one root pruned at pre-truncation rank 15 under branch cap 12;
- State G showed parameter-specific ranking/allocation;
- no common action-order owner repeated across states.

Consequence:

- no action-order heuristic;
- no evaluator/planner repair.

### M3 decisive-planner admission study

Report:

`docs/benchmarks/m3-decisive-planner-admission-2026-09-20.md`

Research branch final commit:

`fa5b937`

Result: **NO GENERAL DECISIVE-PLANNER DEFECT ESTABLISHED**.

The study did confirm a broad mechanical rule:

- `planner.rs::materially_decisive_transition` treats every `BuildSettlement` and `BuildCity` as decisive;
- `depth.rs::decisive_current_turn_plan_replacement_index` can then replace the ordinary backed-up root when completion/decisive mass and response-window conditions are satisfied.

Two unrelated non-closeout settlement-triggered replacements were found:

- `ad33bf6fa633dd7a`
- `986017bab0aab69d`

But admission failed because:

- matched outcomes were not consistently adverse;
- `986...` converged under stronger 48k search;
- M1 State A was actually a Longest Road trigger, not an ordinary settlement/city trigger;
- task9783 remains valid historical closeout evidence but could not be reconstructed observation-safely enough to prove a narrowing repair preserves legitimate closeout behavior.

Terminal evidence:

- 18 files
- 3 reconstructable states × 2 roots × 3 seeds
- zero cutoffs

Consequence:

- no M4 production repair.

## What the experiments collectively say

The recent null results are useful. They rule out several tempting broad fixes:

- do not add a generic save-resources bias;
- do not penalize development cards generically;
- do not add an action-order heuristic;
- do not globally disable decisive current-turn planning;
- do not resurrect the rejected road-intent override.

The remaining evidence increasingly points to **bounded-search quality** rather than one missing evaluator weight.

Observed search symptoms include:

- legal roots pruned by branch-cap truncation;
- depth frequently ending at 1–2 under realistic budgets;
- root ordering changing with deterministic node budget;
- unequal node allocation across retained roots;
- some planner/search disagreements persisting under more work.

That is why the next experiment is S1.

## Next experiment — S1 search bottleneck attribution

Mission file:

`docs/agent-work/opening-strategy-rebuild/S1-search-bottleneck-attribution.md`

Status: **READY — NEW CODEX SESSION**

S1 is research-only. It compares four candidate bottlenecks:

1. root/candidate coverage;
2. completed depth / ordinary node budget;
3. node allocation across roots/families;
4. belief-particle coverage.

S1 should start from a fresh branch/worktree based on current production `main`, selectively porting only the research instrumentation it actually needs.

Do not start with a terminal campaign. First freeze a small exact-state cohort and run deterministic fixed-budget attribution. Terminal arms are allowed only if one repeated bottleneck changes selected roots in at least two unrelated states.

## Preserved branches and worktrees

### Production / stable integration

- repository: `/home/hamza/repo/colonist-assistant`
- branch: `main`

### Orchestration history

- worktree: `/home/hamza/repo/colonist-opening-orchestration`
- branch: `orchestration/opening-rebuild`
- final orchestration head before main sync: `eef6f8e`

All files that existed only on the orchestration branch at closeout were synchronized into main.

### Midgame research

- worktree: `/home/hamza/repo/colonist-midgame-transition-study`
- branch: `agent/m1-midgame-transition-study`
- final head: `fa5b937`

The three final M1/M2/M3 reports are preserved on main.

The large research-only `jev-strategy-lab.rs` instrumentation remains on the Agent M branch and was **not** promoted into production main. It is an evidence/tooling donor for future research, not an admitted production change.

### Road-intent provenance

- original WIP: `wip/road-intent-deadline@2592caf`
- F1 validation: `agent/f1-road-intent-validation@0b3f1ba`

Do not integrate either as production behavior.

### D31 provenance

- `research/d31-road-wip@300914b`

Evidence donor only. Do not cherry-pick wholesale.

## Repository state at closeout

At the start of closeout:

- local `main`: `9aee018`
- `origin/main`: `9aee018`
- orchestration head: `eef6f8e`
- Agent M head: `fa5b937`

The orchestration commits were then cherry-picked to local `main`.

One unrelated working-tree modification remains intentionally untouched:

`src/generated/wasm/colonist_search_bg.wasm`

Do not reset, stage, regenerate, or infer ownership of that file in a future session without separate evidence.

`origin/main` is not automatically pushed by this closeout. Verify local and remote refs before making any remote update.

## Tomorrow / new-session recovery

Start a new ChatGPT session with:

```text
Resume the Colonist strategy-strength work from the 2026-09-20 closeout.

Repository:
  /home/hamza/repo/colonist-assistant

Read first:
  /home/hamza/repo/colonist-assistant/docs/agent-work/opening-strategy-rebuild/SESSION_CLOSEOUT_2026-09-20.md
  /home/hamza/repo/colonist-assistant/docs/agent-work/opening-strategy-rebuild/SESSION_RECOVERY.md
  /home/hamza/repo/colonist-assistant/docs/agent-work/opening-strategy-rebuild/S1-search-bottleneck-attribution.md

Treat WSL Git state as authoritative.

First verify:
  git status
  local main
  origin/main
  orchestration/opening-rebuild
  agent/m1-midgame-transition-study

Do not touch the existing modified:
  src/generated/wasm/colonist_search_bg.wasm

Current research conclusions:
  F1 road-intent: REJECT/HOLD
  M1: NO GENERAL BUILD-TRANSITION DEFECT
  M2: NO GENERAL ACTION-ORDER DEFECT
  M3: NO GENERAL DECISIVE-PLANNER DEFECT

No F2 or M4 production repair is authorized.

Next frontier:
  S1 — Search Bottleneck Attribution Study

S1 should be a NEW Codex Agent S session and is research-only.
```

## Resume rule

If all conversational context is lost, the three files below are sufficient:

1. `SESSION_CLOSEOUT_2026-09-20.md`
2. `SESSION_RECOVERY.md`
3. `S1-search-bottleneck-attribution.md`

Reconcile them against actual Git state before mutating anything.
