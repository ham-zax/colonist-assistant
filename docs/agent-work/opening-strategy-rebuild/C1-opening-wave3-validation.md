# Agent C — Mission C1: Wave 3 Deterministic Opening Validation

## Role

Role: independent validation + research tooling  
Review independence: separate from A1/A2 implementation  
Mutation authority: research/test artifacts only; do not repair production behavior in this mission

## Repository

Repository: `/home/hamza/repo/colonist-assistant`

Assigned worktree:

`/home/hamza/repo/colonist-opening-validation`

Branch:

`agent/c1-opening-validation`

Can start: after this worktree is created from the reviewed integration branch.

## Read first

- `AGENTS.md`
- `docs/JEV_STRATEGY_RESEARCH_PLAYBOOK.md`
- `docs/JEV_OPENING_CALIBRATION.md`
- `docs/agent-work/opening-strategy-rebuild/README.md`
- `docs/benchmarks/opening-a1-validation-2026-09-19.md`

Load `mcp-harness-router`, `persistent-agent-loop`, and `systematic-debugging` for concrete failures.

## Objective

Own the credential-free portion of Wave 3 for the reviewed A1+A2 opening evaluator.

Produce bounded, reproducible validation evidence for:

1. recorded opening regressions;
2. generated-board behavior across 2/3/4 players;
3. both player-trade modes;
4. matched terminal continuation for the known hill6758/task9783 opening disagreements;
5. hard-negative opening cases needed by the Jev calibration handoff;
6. frozen group/split metadata so later B2 Jev collection cannot leak board/template families across calibration and holdout.

This mission does **not** calibrate Jev and does not require `TYPESAFE_API_KEY`.

## Current authoritative implementation

Reviewed A1 implementation was integrated onto the orchestration branch as:

- `70e1461 Rebuild opening evaluator architecture`
- `8496200 Repair temporal opening evaluation`

R2 passed the exact repair range and discharged the review blocker.

Do not compare against the unreviewed A1 commit alone. Use the integrated A1+A2 semantics.

## Method

### A. Recorded corpus

Re-run the recorded opening corpus from the integrated code and record:

- pass/fail/ignored counts;
- elapsed time;
- any failure attributable to the reviewed evaluator change.

Do not edit production code to make the corpus pass. If an in-scope semantic defect appears, stop and report it with a minimized reproducer.

### B. Generated boards

Freeze the generated-board seed groups **before** inspecting outcomes.

Cover at least:

- 2-player;
- 3-player;
- 4-player;
- player trades disabled;
- player trades enabled.

Use seat rotation where the existing harness supports it. Preserve each board/template/rule-mode family as one split group so variants cannot leak across later calibration/holdout.

Run a small timing pilot first, then choose a bounded cohort that completes reliably. Record the frozen seeds and cohort size before using results to judge the implementation.

For each generated opening, preserve enough deterministic evidence to audit:

- chosen action;
- candidate actions;
- endpoint-complete status;
- production pips;
- build-access/conversion terms;
- prospective-port gain;
- expansion term + realization;
- causal denial;
- own/rival/rival-weight terms;
- deadline/completion status.

Do not turn engine score into ground truth. Use generated boards to find structural regressions and candidate pairs for matched continuation.

### C. hill6758 and task9783 matched continuation

Re-run matched terminal evidence from the **reviewed final semantics**, not the pre-hidden-hand or pre-A2 development branch.

For every forced-root pair/portfolio under study:

- reconstruct the same pre-root state;
- force only the root action under comparison;
- use the same continuation policy thereafter;
- match post-root randomness by causal event family;
- use multiple independent continuation seeds;
- run both player-trade modes where the fixture supports them;
- retain terminal result plus intermediate actor VP/build/production diagnostics when available.

Do not interpret one seed as a universal ranking. Report per-stream outcomes and aggregate direction separately.

The historical hill6758 single-stream results `96/92/84` and development `84/116/116` are context only. Neither is final-candidate authority.

### D. Hard negatives for later B2

Instantiate or validate concrete states for the required hard-negative classes where feasible with current tooling:

- attractive-but-bad port;
- all-five-resource but low-throughput opening;
- unreachable/self-unfundable repair target;
- non-causal denial;
- mechanically dominated setup road.

Do not manufacture terminal labels. A case enters the later metric totals only if its matched label contract is satisfied; otherwise leave it explicitly `generation_required` or `inconclusive`.

### E. Frozen calibration handoff

Create ignored/local artifacts under `benchmark-results/jev-lab/` plus a concise tracked validation report under `docs/benchmarks/`.

The tracked report must state:

- exact integrated commit;
- frozen seed/split manifest;
- recorded-corpus result;
- generated-board coverage;
- matched continuation design and outcomes;
- hard-negative status;
- any unresolved semantic concern;
- exact files B2 should consume.

Do not include credentials or raw secret-bearing material.

## Validation authority

The user authorized experiments, tests, simulations, regression fixtures, and both trade modes.

Research/test tooling changes are allowed when required to expose the evidence above. Keep them isolated from production semantics. If a production repair becomes necessary, stop and return the blocker instead of implementing it here.

Long-running commands must use the repository's normal durable terminal workflow.

## Out of scope

- changing production evaluator/search behavior;
- D31 midgame work;
- `wip/road-intent-deadline`;
- Jev API calls or threshold fitting;
- GPU runtime routing;
- merging to main.

## Success conditions

- recorded opening corpus is rerun on integrated A1+A2;
- generated-board cohort is frozen before outcome inspection and covers 2/3/4 players plus both trade modes;
- final-semantics hill6758/task9783 matched continuation evidence is rerun with multiple independent continuation streams and causal-event-family RNG matching;
- hard-negative corpus status is explicit and not mislabeled;
- later B2 receives leakage-safe split groups and label provenance;
- no production behavior is modified;
- worktree is clean or contains only committed research/test/report changes.

## Finish report

Return:

1. status: complete / blocked / needs decision;
2. Agent C, Mission C1;
3. branch/worktree and commits;
4. exact recorded-corpus result;
5. frozen generated-board cohort and coverage;
6. matched hill6758/task9783 results by trade mode and continuation seed;
7. hard-negative label status;
8. artifacts/reports produced;
9. any production defect discovered;
10. exact B2 handoff inputs and remaining blockers.
