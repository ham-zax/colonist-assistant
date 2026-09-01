# Recommendation Engine v10 Agent Coordination

Date: 2026-09-01
Repository: `/home/hamza/repo/colonist-assistant`
Source plan: `docs/RECOMMENDATION_ENGINE_IMPLEMENTATION_PLAN_2026-09-01.md`
Evidence freeze: `docs/RECOMMENDATION_ENGINE_V9_EVIDENCE_FREEZE_2026-09-01.md`
Verification ledger: `docs/RECOMMENDATION_ENGINE_VERIFICATION_LEDGER_2026-09-01.md`
Coordination base: `28272df` (`Fix tracker posterior mass preservation`)

## Execution shape

Use two human-launched AI sessions, but only one writable session at a time on the main checkout.

The user explicitly requires the main checkout and no worktrees. Tasks 2 and 3 also overlap heavily in `placement.ts`, `bridge.ts`, `board.ts`, `deep-search.ts`, `trades.ts`, `overlay.ts`, and shared fixtures. Concurrent writers in the same checkout would make ownership and staging ambiguous, so writable work is serialized.

Current allocation:

- **Agent 1 — Task 2 writer:** may modify the main checkout and complete the existing uncommitted Task 2 draft. It must leave its work unstaged and uncommitted for planner review.
- **Agent 2 — Task 3 reconnaissance:** may inspect the repository and source plan in parallel, but must not mutate, stage, commit, or run implementation changes until Task 2 is reviewed and committed. It returns a concise implementation-risk/ownership report that will seed the next Task 3 writer brief.

No branches or worktrees are created.

## Current repository state

Task 1 is complete at `28272df`.

A Task 2-shaped draft already exists unstaged in the main checkout, including creator-relative trade changes across the bridge, board shape, trade beliefs, trade evaluation, deep-search adapter, overlay, and fixtures. Agent 1 must inspect and continue that draft rather than reset or recreate it.

The pre-existing untracked `.superpowers/` directory is outside this coordination package and must not be modified, staged, deleted, or committed.

## Dependency map

| Work | Status | Writer readiness | Dependency |
| --- | --- | --- | --- |
| Task 1 — posterior mass | Complete | none | commit `28272df` |
| Task 2 — creator-relative active trades | Current writer mission | ready now | Task 1 complete |
| Task 3 — public development history | Read-only reconnaissance now; implementation blocked | after Task 2 commit | Task 2 contract must land first because of shared public-board interfaces |
| Task 4 — midgame fallback posterior | Blocked | after Task 3 commit | public-board state contract must be stable |
| Tasks 5–16 | Future waves | blocked by prior plan gates | follow source-plan order and evidence gates |

Wave 1 remains serial on the main checkout because Tasks 2–4 share production interfaces and tests. Later waves may have conceptual subproblems that can be investigated in parallel, but no two agents may concurrently write the same main checkout.

## Integration policy

1. The active writer owns one plan task through its stated acceptance criteria.
2. The writer leaves changes **unstaged and uncommitted** and returns a completion report.
3. The planner reviews the actual diff, resolves any task-boundary issue, and runs only validation explicitly required by the source plan or needed to verify the reported result.
4. The planner stages only files belonging to that task and commits them on `main`.
5. Only after that commit is the next writer mission unlocked.
6. Read-only reconnaissance may run concurrently because it cannot contaminate the working tree.

Do not fold changes from adjacent tasks into the same commit merely because they are present in the checkout.

## Shared contracts

- `ActiveTradeOffer` becomes creator-relative in Task 2: `creatorGive` and `creatorReceive` are the only stored trade resource vectors.
- Local-user trade orientation is derived at the UI/value boundary, not stored as another authority.
- Active-trade snapshot diffs are the sole owner of panel-derived trade Bayesian evidence; unchanged snapshots are idempotent.
- Task 3 must build on the Task 2 public-board shape after Task 2 is committed; it must not reintroduce ambiguous trade fields while migrating development history.
- The verification ledger is append-oriented and records task closure evidence and limitations.

## Current mission files

- `agent-1-task-2-trade-contract.md`
- `agent-2-task-3-recon.md`

## Finish-report contract

Every agent returns:

- status: `complete`, `blocked`, or `needs decision`;
- whether it mutated the repository;
- concise behavior/interface summary;
- files changed or inspected;
- explicitly required validation run, if any;
- deviations from the mission;
- coordination notes for the next task;
- unresolved risks or blockers.

The planner/user performs staging and commits after reviewing writer output.
