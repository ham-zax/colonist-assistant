# Agent 2 Mission — Task 3 Read-Only Reconnaissance

Repository: `/home/hamza/repo/colonist-assistant`
Working arrangement: main checkout only; **read-only for this mission**
Coordination base: `28272df`
Authoritative source plan: `docs/RECOMMENDATION_ENGINE_IMPLEMENTATION_PLAN_2026-09-01.md`, Task 3
Coordination map: `docs/superpowers/agent-plans/2026-09-01-recommendation-engine-v10/README.md`

## Mission

Prepare the next Task 3 implementation session by tracing the current public development-card state contract end to end, identifying the exact owners and migration risks, and reporting what will need to change after Task 2 is committed.

This is intentionally a read-only mission so it can run in parallel with Agent 1 without contaminating the shared main checkout.

## Artifact type

Read-only engineering reconnaissance.

## Scope to inspect

Trace Task 3 through the current code, especially:

- `src/core/placement.ts`
- `src/page/bridge.ts`
- `src/content/board.ts`
- `src/worker/deep-search.ts`
- `src/core/coach.ts`
- `src/core/engine.ts`
- `src/core/strategy.ts`
- `src/core/development.ts`
- `src/core/trades.ts`
- `src/content/overlay.ts`
- `tests/deep-search-adapter.test.ts`
- `tests/helpers/deep-search-fixtures.ts`
- existing audit/verification docs for F3 and F10

Determine:

- every current producer/consumer of `playedKnights` or equivalent public development history;
- how Colonist exposes `developmentCardsUsed` and `hasUsedDevelopmentCardThisTurn` in the bridge;
- where public development counts are merged with tracker history;
- where hidden development-card deck composition is sampled and where negative/impossible remainder can currently be clamped or fabricated;
- which Task 2 creator-relative trade changes alter shared files Task 3 will later edit;
- the smallest coherent Task 3 implementation boundary once Task 2 lands.

## Acceptance

Return a concise implementation brief that gives the next writer enough evidence to start Task 3 without repeating repository-wide exploration. Include:

- actual current data flow from Colonist mechanic state -> `BoardSnapshot` -> adapter -> Rust request;
- exact symbols/files that own the public played-development vector and per-player played-this-turn flag;
- all scalar `playedKnights` authorities that must be deleted or converted to derived reads;
- exact location of development deck conservation/sampling logic;
- concrete migration hazards created by current Task 2 unstaged changes;
- any factual gap that must be resolved after Task 2 commit before mutation.

## Validation

None. This mission is read-only; do not create, modify, stage, or run tests.

## Coordination boundaries

- Do not modify any repository file.
- Do not stage or commit anything.
- Do not reset, checkout, stash, clean, or otherwise alter Agent 1's uncommitted Task 2 work.
- Do not touch the untracked `.superpowers/` directory.
- Task 3 implementation begins only after the planner reviews and commits Task 2 and issues a new writer brief.

## Finish report

Return:

- status: complete / blocked / needs decision;
- confirmation that repository mutation was zero;
- current Task 3 ownership/data-flow map;
- shared-file collision notes with Task 2;
- recommended smallest Task 3 writer mission after Task 2 commit;
- unresolved risks or questions.
