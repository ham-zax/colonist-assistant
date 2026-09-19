# Agent D — Mission D1: D31 Resource-Liquidity Causal Diagnosis

## Role

Role: investigation + research tooling  
Review independence: separate from Agent A/C work  
Production mutation authority: none in D1

D1 is diagnostic. Do not implement a production strategy repair in this mission. If a general invariant is proved, return it as the owner for a later D2 implementation mission.

## Repository

Repository: `/home/hamza/repo/colonist-assistant`

Assigned worktree:

`/home/hamza/repo/colonist-d31-liquidity`

Branch:

`agent/d1-d31-liquidity`

Base:

reviewed opening integration on `orchestration/opening-rebuild`.

## Read first

- `AGENTS.md`
- `docs/JEV_STRATEGY_RESEARCH_PLAYBOOK.md`
- `docs/agent-work/opening-strategy-rebuild/SESSION_RECOVERY.md`
- this mission file

Load `mcp-harness-router`, `systematic-debugging`, `causal-coding`, and `persistent-agent-loop`.

## Preserved evidence donors

### D31 / midgame diagnostic WIP

Reference branch:

`research/d31-road-wip`

Reference commit:

`300914b wip(strategy): preserve D31 and road diagnostics`

This commit is an evidence donor only. Do **not** cherry-pick it wholesale.

It mixes:

- Jev lab diagnostics;
- strategic-utility breakdown instrumentation;
- a candidate production semantic change to weighted production;
- save/spend tests;
- road diagnostics;
- depth/search changes;
- CUDA/lib changes.

Separate behavior-neutral instrumentation from candidate production changes before reusing anything.

### Road-intent candidate

Reference only:

`wip/road-intent-deadline @ 2592caf`

Do not merge, cherry-pick, or modify this lane in D1. It is a separate unreviewed search-authority candidate and overlaps `depth.rs`.

## Corrected D31 evidence

Do not use the early single-stream conclusion that EndTurn was proved superior.

The corrected counterfactual harness partitions common random numbers by causal event family. Across four independent matched continuation streams:

- BuyDevelopment won for the actor in 3/4;
- EndTurn won for the actor in 1/4.

Therefore:

- D31 does not justify a development-card nerf;
- D31 remains useful as a strategic-liquidity/value-decomposition probe;
- any production repair must be supported by a general invariant and counterexamples, not the old single-stream winner.

Exact D31 turn-19 context retained from the research record:

- hand approximately `[brick=1, grain=1, lumber=0, ore=1, wool=1]`;
- zero native ore production;
- zero native lumber production;
- the ore was imported at 4:1 immediately before the choice;
- engine chose BuyDevelopment;
- competing action was EndTurn.

## Objective

Re-establish the D31 question on the reviewed opening/economy base and determine whether there is a real evaluator/search semantic defect independent of the old Jev preference.

Primary question:

> Does spending a scarce imported bottleneck resource improperly increase or insufficiently decrease strategic utility because hand-dependent weights revalue persistent assets, or is the current BuyDevelopment preference defensible once matched outcomes and exact decomposition are used?

## Required hypotheses to test

### H1 — persistent production revaluation

The preserved WIP changed strategic weighted production from hand-dependent dynamic resource weights to fixed base weights, with the invariant:

> Spending cards from hand must not make unchanged board production appear more valuable merely because the hand-dependent bottleneck weights changed.

On the current reviewed base:

1. reproduce whether unchanged board production can change its weighted-production contribution after pure hand spending;
2. isolate the exact term(s) responsible;
3. determine whether this is an evaluator accounting defect or intentional state-dependent marginal valuation;
4. construct both a positive case and a counterexample before proposing any repair.

Do not import the old base-weight change just because it exists in `300914b`.

### H2 — imported bottleneck opportunity cost

Test whether a zero-native-production resource imported at 4:1 receives adequate opportunity cost when spent.

Distinguish:

- current hand liquidity;
- persistent production;
- bank conversion;
- next-build ETA;
- closed-economy access;
- expansion/city/dev transition value;
- discard risk;
- development-card expected value.

A correct diagnosis must identify the owner term, not merely observe that BuyDevelopment is legal.

### H3 — development-card value

Inspect:

- `marginal_development_value`;
- observation-safe development-draw belief;
- queued/action-card diminishing value;
- largest-army race effects;
- VP-card closeout effects;
- whether spending an irreplaceable import is being double-counted or ignored.

Do not apply a generic BuyDevelopment penalty.

### H4 — search arbitration

The earlier investigation found that same-turn actions do not consume search depth. Treat that as a previously falsified hypothesis; re-open it only if current source contradicts the prior result.

Verify that EndTurn is retained/ranked at the relevant root and that the disagreement is not simply root pruning or action omission.

## Diagnostic tooling

Reuse only the useful, behavior-neutral ideas from `300914b`, such as:

- strategic-utility component breakdown;
- resource-only action delta;
- expected development-card outcome decomposition;
- matched event-family RNG streams.

Prefer research-bin/test-local instrumentation over production evaluator API expansion when possible.

Any diagnostic code must not alter production decisions.

## Counterfactual protocol

For D31 and any generated analogs:

- reconstruct the same pre-root state;
- force BuyDevelopment vs EndTurn only at the root;
- use the same continuation policy afterward;
- partition post-root randomness by causal event family;
- use multiple independent continuation seeds;
- report per-stream terminal outcomes plus intermediate actor metrics;
- use both trade settings where meaningful;
- never promote one seed to universal truth.

Reuse the four corrected streams if artifacts are available; add only enough new streams to discriminate a concrete hypothesis.

## Counterexamples

A valid liquidity defect must preserve cases where spending now is strategically correct.

Include at least:

- immediate or near-immediate VP closeout;
- meaningful Largest Army race;
- excess/non-bottleneck resources;
- a development purchase that improves the best build transition;
- a state where saving creates no realistic conversion path.

## Success conditions

D1 is complete when it can answer one of:

### A. General defect established

Return:

- exact violated invariant;
- production owner;
- minimal reproducer;
- counterexample coverage;
- matched evidence;
- narrow D2 implementation scope.

Do not implement the repair in D1.

### B. No general defect established

Return D31 to the false-positive/seed-sensitive corpus with:

- the value decomposition;
- why BuyDevelopment can be rational under current semantics;
- which old hypothesis was falsified;
- what evidence would be required to reopen it.

## Out of scope

- opening evaluator changes;
- C1 Wave 3 opening validation;
- road-intent arbitration;
- D17/D27 road fixes;
- B1/B2 Jev calibration;
- GPU runtime routing;
- merging to main.

## Finish report

Return:

1. status: complete / blocked / needs decision;
2. Agent D, Mission D1;
3. workspace/branch and commits/artifacts;
4. exact D31 reconstruction on the reviewed base;
5. H1-H4 results;
6. matched continuation evidence actually used;
7. counterexamples used;
8. whether a general production defect is established;
9. if yes, exact D2 owner/invariant and files;
10. if no, exact reason D31 remains false-positive/seed-sensitive.
