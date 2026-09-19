# Agent D — Mission D2: Persistent Production Valuation Repair

## Role

Role: implementation repair  
Session: continue in the existing Agent D chat  
Review independence: independent Agent R review required after D2  
Production mutation authority: narrowly authorized for the D1-established persistent-production invariant only

## Repository

Repository: `/home/hamza/repo/colonist-assistant`

Workspace:

`/home/hamza/repo/colonist-d31-liquidity`

Branch:

`agent/d1-d31-liquidity`

D1 diagnostic commit:

`c58fa11d7e4f52ffb2eb88f562292d1004006592`

Reviewed integration base:

`1b2a0c2d2f643a6308f7dda2f521859df1f8944c`

D2 must remain on the same branch/worktree and build directly on D1.

## Read first

- `AGENTS.md`
- `docs/agent-work/opening-strategy-rebuild/D1-d31-resource-liquidity-diagnosis.md`
- `docs/agent-work/opening-strategy-rebuild/D1-d31-resource-liquidity-diagnosis-report.md`
- `docs/JEV_STRATEGY_RESEARCH_PLAYBOOK.md`

Load `mcp-harness-router`, `causal-coding`, `systematic-debugging`, and `persistent-agent-loop`.

## Established defect

D1 proved a general evaluator invariant violation on the reviewed A1+A2 base.

A pure hand-only transition can change the strategic-utility contribution assigned to unchanged board production because `strategic_utility` multiplies persistent `production_pips` by hand-dependent `dynamic_resource_weights`.

Exact D31 evidence:

- board production unchanged;
- spending only the dev-card resources changes weighted-production contribution from `2.601153 -> 3.602191` (`+1.001038`);
- deleting only the grain card, with no board gain and no discard exposure, increases total `strategic_utility` by `+0.104158`.

The violated invariant is:

> A pure hand transition must not make already-owned board production gain value merely because transient hand scarcity changed.

D1 also established that dynamic resource weights remain useful elsewhere. Do **not** replace them globally.

## Authoritative ownership frontier

Repair exactly these production paths:

1. CPU:
   `engine/crates/catan-search/src/eval.rs`
   `strategic_utility_with_routes_and_knowledge`

2. exact CUDA:
   `engine/crates/catan-search/src/cuda/exact_eval.cu`
   CUDA `strategic_utility`

Persistent production must use hand-independent base resource weights in both paths.

Continue using `dynamic_resource_weights` for:

- hand liquidity/value;
- prospective expansion/site valuation;
- settlement/city/resource-deficit marginal valuation;
- robber/action priors;
- other genuinely marginal forward-looking uses already owned by those weights.

Do not change those surfaces without a separately demonstrated defect.

## Operative repair hypothesis

Use the existing base resource weights for the persistent board-production term only:

`sum(production_pips[r] * BASE_RESOURCE_WEIGHTS[r])`

This is already semantically consistent with the observation-safe public strategic utility path and with D1's `hand_transition_value` cross-check.

Do not introduce a new coefficient vector or D31-specific special case.

## Required acceptance evidence

The repair must directly falsify the demonstrated bug.

### H1 invariant

Add/update focused regression evidence so that:

- unchanged board production has unchanged weighted-production contribution across a pure hand spend;
- the exact D31-like positive reproducer no longer exhibits production inflation;
- deleting the D31 grain card cannot improve total strategic utility solely through persistent-production revaluation;
- the prior contextual counterexample remains stable.

Test changes are authorized.

### D31 decomposition

Re-run the exact reviewed-base D31 decomposition after the repair and report:

- weighted-production before/after dev cost;
- resource-only utility delta;
- full expected immediate BuyDevelopment delta;
- search choice at the bounded depth-1 and depth-3 probes if practical.

Do not require EndTurn to win. D31 is not the acceptance oracle.

### Spend-now guards

Preserve all five D1 counterexamples:

- VP closeout;
- Largest Army race;
- excess non-bottleneck resources;
- Road Building transition;
- no realistic saving-conversion path.

They are guards against turning the repair into a generic save bias or dev-card penalty.

### CPU/CUDA parity

The CPU and exact-CUDA strategic utility formulas must use the same persistent-production semantics.

Run the smallest existing/focused parity evidence that materially establishes this if available. If runtime CUDA execution is unavailable or disproportionate, establish source-semantic parity explicitly and report the limitation; do not broaden into a GPU runtime campaign.

## D1 instrumentation disposition

The D1 range added behavior-neutral research diagnostics:

- `strategic_utility_breakdown`;
- Jev-lab decomposition output;
- focused D1 research tests.

Before committing D2, apply the permanent-repair gate to those additions:

- retain diagnostics that have an active research consumer and remain behavior-neutral;
- remove or narrow any public production API surface that exists only as a temporary probe and has no continuing consumer;
- do not delete useful research evidence merely to minimize line count.

The final review will inspect the entire range `1b2a0c2..D2_HEAD`, not only the last commit.

## Known check discrepancy

The planner independently ran:

`git diff --check 1b2a0c2..c58fa11`

and found trailing whitespace on lines 5–6 of:

`docs/agent-work/opening-strategy-rebuild/D1-d31-resource-liquidity-diagnosis-report.md`

D1's finish report said `git diff --check` passed, so reconcile this concrete discrepancy in D2. Remove only that report whitespace; do not broaden into repository formatting cleanup.

Repository-wide `cargo fmt --check` is known to encounter unrelated pre-existing formatting drift. Do not absorb unrelated formatting changes.

## Validation authority

The user has authorized focused tests, experiments, simulations, regression fixtures, and iterative strategy validation.

Required focused checks:

- persistent-production invariant regression;
- exact D31 decomposition;
- five spend-now guard cases;
- directly affected strategic-utility tests;
- CPU/CUDA parity evidence appropriate to the touched path;
- `git diff --check 1b2a0c2..D2_HEAD`.

Do not run unrelated repository-wide campaigns.

## Out of scope

- generic BuyDevelopment penalty;
- imported-ore special case;
- development-card probability/value retuning;
- road-intent arbitration;
- D17/D27 road work;
- opening evaluator changes;
- Agent C Wave 3 implementation;
- B1/B2 Jev calibration;
- GPU runtime routing;
- main merge.

## Interaction with C1

Agent C is validating the pre-D2 integrated opening candidate in a separate worktree.

Do not modify Agent C's worktree.

D2 may proceed in parallel. Because D2 changes shared main-game strategic utility, any C1 matched-terminal evidence that depends on continuation policy may need a bounded post-D2 refresh after D2 is reviewed and integrated. Do not rerun C1's campaign inside D2.

## Review gate

D2 changes a shared CPU/CUDA evaluator contract and therefore requires independent review before integration.

After D2 commits, Agent R must review the entire D1+D2 range:

`1b2a0c2..D2_HEAD`

with special attention to:

- the H1 invariant;
- keeping dynamic weights in genuinely marginal uses;
- CPU/CUDA parity;
- no generic save/dev bias;
- disposition of D1 diagnostics;
- focused counterexamples.

Do not integrate D1/D2 before that review passes.

## Finish report

Return:

1. status: complete / blocked / needs decision;
2. Agent D, Mission D2;
3. branch/worktree and exact D2 commit;
4. exact CPU repair;
5. exact CUDA mirror repair;
6. D1 diagnostic instrumentation retained/removed and why;
7. H1 before/after evidence;
8. D31 decomposition after repair;
9. five spend-now guard results;
10. parity evidence;
11. checks actually run, including `git diff --check 1b2a0c2..HEAD`;
12. unresolved blocker for independent review.
