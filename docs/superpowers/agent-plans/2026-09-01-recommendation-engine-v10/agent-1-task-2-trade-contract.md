# Agent 1 Mission — Task 2 Creator-Relative Active Trades

Repository: `/home/hamza/repo/colonist-assistant`
Working arrangement: main checkout only; **you are the sole writer while this mission is active**
Coordination base: `28272df`
Authoritative source plan: `docs/RECOMMENDATION_ENGINE_IMPLEMENTATION_PLAN_2026-09-01.md`, Task 2
Coordination map: `docs/superpowers/agent-plans/2026-09-01-recommendation-engine-v10/README.md`

## Mission

Complete Task 2: migrate every active-trade representation and consumer to one creator-relative contract, close F2, and leave the main working tree ready for planner review.

A substantial Task 2 draft already exists unstaged. Treat that draft as input. Inspect it first, preserve correct work, and complete or repair it rather than resetting the checkout or starting the migration over.

## Artifact type

Executable behavior plus focused tests required by the authoritative implementation plan.

## Ownership

You own Task 2 behavior and the files explicitly listed by Task 2, including current or necessary edits in:

- `src/core/placement.ts`
- `src/page/bridge.ts`
- `src/core/trade-beliefs.ts`
- `src/core/trades.ts`
- `src/core/trade-guard.ts`
- `src/content/trade-verdicts.ts`
- `src/content/overlay.ts`
- `src/worker/deep-search.ts`
- `src/content/board.ts`
- `tests/trade-belief-board-diff.test.ts`
- `tests/tracker-posterior-integrity.test.ts`
- `tests/trade-guard.test.ts`
- `tests/helpers/deep-search-fixtures.ts`
- directly affected Task 2 fixtures/consumers only when the contract migration requires them

Do not modify unrelated tasks or begin Task 3.

## Required contract

`ActiveTradeOffer` stores only creator-relative vectors:

- `creatorGive`
- `creatorReceive`

The bridge maps Colonist `offeredResources` directly to `creatorGive` and `wantedResources` directly to `creatorReceive`.

When a caller needs the local user's perspective, derive `{ give, receive }` through one pure helper at the UI/value boundary. Do not keep compatibility aliases or a second stored orientation.

Active-trade snapshot diffs are the sole owner of panel-derived Bayesian trade evidence. Re-rendering or reconciling an unchanged trade panel must not reweight the posterior again. Completed real trade events remain resource transfers, not another soft likelihood update.

## Acceptance

The mission is complete when all Task 2 acceptance criteria in the source plan are satisfied, including:

- incoming `Rival gives lumber / User gives brick` evidence raises the probability that Rival has lumber, not brick;
- unchanged active-trade snapshots are probability-idempotent;
- open -> accepted/rejected/countered transitions emit exactly one corresponding tracker event;
- no `ActiveTradeOffer` instance stores both creator-relative and local-relative resource vectors;
- incoming, outgoing, and counteroffer UI/evaluation behavior remains local-user-relative through the derived helper;
- the unconditional active-panel `reweightTradeEvidence()` call is absent from overlay reconciliation;
- all internal consumers compile against the creator-relative contract without compatibility aliases.

## Required validation

The source plan explicitly requires:

```bash
npm test -- tests/trade-belief-board-diff.test.ts tests/tracker-posterior-integrity.test.ts tests/trade-guard.test.ts
```

Run that focused validation when the implementation is candidate-complete. Do not expand to unrelated suites unless a direct Task 2 failure requires it.

## Coordination boundaries

- Task 1 is committed at `28272df`; do not undo it.
- Task 3 is blocked and must not be implemented in this mission.
- Another agent may be doing read-only Task 3 reconnaissance. It must not write; you remain the only repository writer.
- The untracked `.superpowers/` directory is external to this mission. Do not touch, stage, delete, or commit it.
- Do **not** stage or commit your Task 2 changes. The planner will review the diff, stage the task boundary, and commit it.

## Finish report

Return:

- status: complete / blocked / needs decision;
- concise explanation of the final creator-relative trade contract;
- exact files changed;
- focused validation result;
- any pre-existing draft defects you corrected;
- any Task 3-relevant interface notes;
- unresolved risks or blockers;
- explicit confirmation that you left changes unstaged and uncommitted.
