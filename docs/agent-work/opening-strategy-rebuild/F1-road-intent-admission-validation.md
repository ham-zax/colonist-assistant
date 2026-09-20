# Agent F — Mission F1: Road-Intent Admission Validation

## Role

Role: independent causal validation / research tooling
Session: NEW web session
Production mutation authority: none in F1

F1 decides whether the preserved deadline-limited road-intent override has enough causal evidence to deserve production hardening and review. It does **not** integrate or tune the override.

## Repository / workspace

Repository:

`/home/hamza/repo/colonist-assistant`

Validation worktree:

`/home/hamza/repo/colonist-road-intent-validation`

Branch:

`agent/f1-road-intent-validation`

Current validation HEAD:

`2d29125` — the preserved road-intent candidate `2592caf` cleanly cherry-picked onto current local main `9aee018`.

Current local main already contains the reviewed opening rebuild, reviewed D31 persistent-production repair, and capped Wave 3 validation.

The old candidate branch remains:

`wip/road-intent-deadline @ 2592caf`

Treat that branch as provenance only.

## Important packaging caveat

The preserved candidate commit includes a generated WASM binary produced on the old `15cfc5e` base.

F1 is a Rust/search-policy admission study. Do not treat the carried generated WASM binary as a current-base final artifact.

If F1 establishes an admission case, a later F2 implementation-hardening mission must rebuild/synchronize the generated WASM/frontend artifacts from the current base before independent review.

## Read first

- `AGENTS.md`
- `docs/JEV_STRATEGY_RESEARCH_PLAYBOOK.md`
- `docs/agent-work/opening-strategy-rebuild/SESSION_RECOVERY.md`
- this mission file
- the D17 false-positive context in the C1 validation report

Load:

- `mcp-harness-router`
- `systematic-debugging`
- `causal-coding`
- `persistent-agent-loop`

## Preserved candidate

The production candidate adds `deadline_limited_road_intent_replacement_index()` in `engine/crates/catan-search/src/depth.rs`.

It activates only when:

- the search deadline was reached;
- completed depth is at most 1;
- the shallow winner is `BuildRoad`;
- another road has the same target vertex;
- candidate legal weight is no worse;
- candidate lower-confidence value is no worse;
- candidate roads remaining is no worse;
- candidate ETA is no worse;
- candidate survival is no worse;
- candidate target value is no worse;
- candidate portfolio value is strictly better;
- candidate frontier gain is strictly better.

It is intentionally a deadline/depth-1 fallback, not a general road ranker.

The preserved focused unit regression proves only the gate logic. It does **not** prove terminal strategic superiority.

## Primary live case: hill6758 D27

Source evidence:

`/mnt/c/Users/Hamza/Downloads/colonist-evidence-hill6758-2-2026-09-19T07-49-29-720Z (2).txt`

The live D27 decision is turn 31 for P3.

Observed shallow search:

- engine road: `e:2,-2,2`;
- candidate alternative: `e:1,-1,2`;
- completed depth: 1;
- deadline reached while attempting depth 2.

For the two roads, the preserved evidence reports the same intended settlement target `v:1,-1,1`, same roads remaining `0`, same ETA `14.4`, same survival `0.995`, and same target value `8.7885`.

Reported road-intent differences:

### `e:2,-2,2`

- shallow value about `0.0256` in the preserved unit scenario;
- portfolio `3.7085`;
- frontier gain `-0.5078`;
- ordering score `1.5809`.

### `e:1,-1,2`

- shallow value about `0.0129`;
- portfolio `5.7278`;
- frontier gain `0.6028`;
- ordering score `3.2822`;
- evidence also records critical-expansion protection.

The candidate override prefers `e:1,-1,2` only under the deadline-limited gate.

## Known caution: D17

D17 is an existing road disagreement where the structurally attractive alternative was a Jev false positive / mechanically weaker road.

Therefore F1 must not infer:

`better road-intent heuristic -> better terminal outcome`.

Use D17 as a negative-control mentality: admission requires matched or stronger causal evidence, not just the road-intent score.

## Objective

Answer:

> On the current reviewed strategy base, does replacing the deadline-limited D27 road `e:2,-2,2` with `e:1,-1,2` improve the actor's downstream outcome strongly enough to justify retaining the override as a production candidate?

## Required work

### A. Exact D27 reconstruction

Reconstruct the pre-root D27 state from the live evidence.

Verify as much exact identity as available:

- turn/actor/phase;
- roads/buildings;
- hand/public knowledge;
- dice model/history;
- player-trade setting;
- root legal actions;
- chosen/search-ranked roads;
- the two road-intent feature vectors.

If some live history is irrecoverably partial, record that limitation. Do not silently invent hidden state.

Prefer an observation-safe state spec or replay prefix already supported by the lab.

### B. Reproduce the gate on current main + candidate

Using deterministic search budgets:

1. reproduce a deadline/depth-1 condition where the baseline shallow winner is the recorded D27 road;
2. confirm the candidate override selects `e:1,-1,2`;
3. confirm the override does not fire when:
   - deadline is not reached, or
   - completed depth is greater than 1.

Do not weaken the gate to make the case reproduce.

### C. Forced-root matched continuation

Compare exactly two root actions from the same D27 pre-root state:

- `e:2,-2,2`;
- `e:1,-1,2`.

Protocol:

- force only the first root action;
- use the same continuation policy afterward;
- use event-family common random numbers;
- use deterministic node budgets and zero wall-clock cutoffs;
- use at least **4 independent continuation seeds**;
- preserve per-stream actor outcome, winner, terminal turn, final VP, and useful expansion/route metrics;
- do not let one stream become authority.

Four streams are sufficient for F1. Add at most two more only if a concrete ambiguity can be resolved by them. Do not start a large benchmark campaign.

### D. Stronger-search check

Where practical, compare with a bounded deeper / non-deadline search on the exact D27 root.

This is diagnostic evidence only.

Questions:

- Does deeper search converge toward the override road?
- Does it prefer the original road?
- Does it prefer a third action such as Knight or EndTurn?

Do not change the candidate based solely on this check.

### E. Negative controls

At minimum prove:

- the gate remains inactive on the preserved D17-style mechanically weaker road comparison, or explain exactly why that case is not reconstructable;
- a candidate with worse survival / ETA / lower-confidence evidence cannot pass the gate;
- deeper completed search remains authoritative.

Existing focused tests may be reused where they directly establish these controls.

### F. Admission decision

Use the following standard.

#### Admit for F2 hardening

Only if the evidence shows:

- exact D27 gate behavior is reproduced;
- matched continuation gives a coherent advantage to `e:1,-1,2` or otherwise demonstrates the baseline deadline choice is causally dominated;
- negative controls hold;
- no broader generic road-ranker behavior is required.

A 4-stream split such as 2–2 or mostly ties is **not** enough to admit.

#### Reject / keep HOLD

If:

- matched outcomes are mixed/inconclusive;
- the original road wins materially;
- the exact live state cannot be reconstructed well enough to support the claim;
- the gate needs broader tuning or exceptions;
- the evidence only restates the structural heuristic.

Do not repair or retune the candidate in F1.

## Research tooling

You may add narrowly scoped research-only state specs, scripts, or ignored benchmark artifacts if current harness support is insufficient.

Do not alter production strategy/search behavior in F1.

Do not modify the existing `wip/road-intent-deadline` worktree.

## Suggested artifact location

Use ignored artifacts under:

`benchmark-results/jev-lab/road-intent-f1/`

Track only a concise report if needed:

`docs/benchmarks/road-intent-f1-validation-2026-09-20.md`

## Out of scope

- opening evaluator changes;
- D31/dev-card work;
- B2 Jev calibration;
- generic road scoring retuning;
- production repair of the candidate;
- frontend/WASM regeneration;
- merging to main;
- pushing origin/main.

## Finish report

Return:

1. status: complete / blocked / needs decision;
2. Agent F, Mission F1;
3. branch/worktree and commits/artifacts;
4. exact D27 reconstruction fidelity;
5. exact gate reproduction;
6. per-seed forced-root terminal outcomes;
7. stronger-search diagnostic;
8. negative-control results;
9. admission decision: **ADMIT TO F2** or **REJECT/HOLD**;
10. if admitted, the exact invariant and F2 hardening scope;
11. any unresolved evidence limitation;
12. final worktree status.
