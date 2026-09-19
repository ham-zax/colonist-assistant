# Opening Strategy Rebuild - Session Coordination

**Repository:** /home/hamza/repo/colonist-assistant  
**Integration worktree:** /home/hamza/repo/colonist-opening-orchestration  
**Source of truth:** docs/JEV_STRATEGY_RESEARCH_PLAYBOOK.md plus live hill6758/task9783 evidence  
**Current wave:** 3
**Coordination mode:** durable

Session-loss recovery map: `docs/agent-work/opening-strategy-rebuild/SESSION_RECOVERY.md`

## Objective

Rebuild the opening strategy evaluator around causally meaningful primitives instead of patching static weights. Complete all eight approved items:

1. rework/remove generic four-player `max_rival * 0.34` scoring;
2. preserve opponent self-maximization in snake-draft recursion;
3. replace blanket rival subtraction with causal contested-position denial;
4. rebuild prospective-port value around complete-build conversion economics;
5. make expansion realization explicitly self-funding-aware;
6. reevaluate opening components against matched terminal simulation;
7. fit/calibrate only after feature semantics are correct;
8. validate on recorded openings, generated boards, both trade modes, and held-out/matched streams.

## Current evidence

- hill6758 live opening: engine chose second settlement `v:2,-2,1` with production [2,5,0,8,5], accepting zero wool.
- `v:-1,3,0` is the 9-grain / 10-wool + 2:1 brick-port root reached by the setup-road corridor.
- Same matched future stream: historical zero-wool root won turn 96; all-five root turn 92; brick-port root turn 84.
- After the first self-funding repair attempt, own opening value already preferred the all-five line (~9.610 vs ~9.269), but final static value still flipped because of the four-player strongest-rival subtraction.
- Opponents already greedily maximize their own setup-aware opening value during snake-draft recursion.
- task9783 remains a regression for equal-pip portfolio completeness in both player-trade modes.
- Jev is offline research only; production must remain deterministic Rust/WASM/native companion.

## Progress Snapshot

Current frontier
├─ Opening implementation / A1+A2
│  COMPLETE + REVIEWED + INTEGRATED
│  Reviewed semantics integrated as 70e1461 + 8496200.
│
├─ Wave 3 deterministic validation / C1 / Agent C
│  READY — NEW SESSION
│  Recorded corpus, generated boards, both trade modes, matched terminal streams, hard-negative labels.
│
├─ Jev calibration / B2 / Agent B
│  BLOCKED -> local TYPESAFE_API_KEY
│  Framework is integrated; credential is currently unset in the orchestration environment.
│
├─ D31 / preserved WIP
│  HOLD
│  research/d31-road-wip @ 300914b preserves current midgame/road diagnostics.
│
└─ Main checkout
   CLEAN / HOLD
   Merge waits for Wave 3 deterministic validation plus B2 held-out calibration.

Planned effort: 80% COMPLETE | 10% READY deterministic validation | 10% BLOCKED live/held-out calibration

## Session Ledger

| Agent | Mission | Status | Role | Workspace | Disposition | Reusable |
| --- | --- | --- | --- | --- | --- | --- |
| A | A1 | COMPLETE + INTEGRATED | implement | /home/hamza/repo/colonist-opening-architecture | integrated as 70e1461 | yes |
| A | A2 | COMPLETE + INTEGRATED | implement repair | /home/hamza/repo/colonist-opening-architecture | integrated as 8496200 | yes |
| B | B1 | COMPLETE + INTEGRATED | investigate + research tooling | /home/hamza/repo/colonist-jev-calibration | COMPLETE; reuse as B2 when credential exists | yes |
| C | C1 | READY — NEW SESSION | independent validation + research tooling | /home/hamza/repo/colonist-opening-validation | NEW SESSION now | no |
| R | R1 | COMPLETE — BLOCKING FINDINGS | independent review | read-only target a169b28..15b2365 | COMPLETE | yes |
| R | R2 | COMPLETE — PASS | independent re-review | read-only repair range 15b2365..034b01f | review blocker discharged | yes |
| D31 lane | preserved | HOLD | diagnostics | research/d31-road-wip @ 300914b | resume after opening frontier | yes |

## Blocker Ledger

| Blocked item | Blocker | Owner | Discharge condition | Status/evidence |
| --- | --- | --- | --- | --- |
| A1/A2 integration | R2 re-review | R2 | R2 reports pass | DISCHARGED — R2 passed; integrated as 70e1461 + 8496200 |
| Wave 3 deterministic validation | reviewed A1/A2 integration | C1 | C1 returns recorded/generated/matched evidence with no unresolved production blocker | READY |
| B2 live/held-out Jev calibration | local TypeSafe credential + C1 label handoff | B2/C1 | local TYPESAFE_API_KEY exists and C1 produces frozen labels/splits | BLOCKED — credential currently unset; C1 pending |
| merge back to main | C1 + B2 final validation | planner | deterministic validation passes and held-out calibration is completed or explicitly waived by user | BLOCKED |
| D31 continuation | opening rebuild integration preferred first | planner | opening Wave 2 stable | HOLD |

## Dependency map

```text
A1 ─> R1 BLOCK ─> A2 ─> R2 PASS ─> reviewed integration ─> C1 deterministic validation ─┐
B1 calibration framework ────────────────────────────────────────> B2 live/held-out calibration ───────┤
                                                                                                       └─> main merge

D31 WIP remains separate and resumes after the opening validation frontier.
```

## Shared contracts

- No Jev/API/network dependency in production.
- Code owns legality, arithmetic, probabilities, topology, production, conversion, and belief facts.
- Jev is a bounded critic/hypothesis generator only.
- Four-player opponent simulation and terminal objective must not double-count generic opponent strength.
- Denial must be causal: value only an opponent opportunity actually removed/worsened by our root.
- Prospective port value must be expressed through complete-build economics, not a small standalone port bonus.
- Expansion credit must account for whether the current economy can self-fund the road-plus-settlement project.
- Do not replace one unvalidated universal score with another.
- Player trades OFF and ON are both first-class validation modes.
- Preserve the CPU/WASM-only runtime gate unless a mission explicitly owns runtime routing.
- Keep main clean as the stable integration destination; implementation remains in assigned isolated worktrees.

## Workspace policy

Concurrent writers use isolated worktrees:

- Integration: /home/hamza/repo/colonist-opening-orchestration
- Agent A: /home/hamza/repo/colonist-opening-architecture
- Agent B: /home/hamza/repo/colonist-jev-calibration

Main checkout is clean and held as the stable integration destination. Continue implementation in the assigned isolated worktrees.

## Review / integration policy

A1 is substantial strategy/search implementation and requires independent Agent R review before integration. Agent R is read-only. Blocking findings return to Agent A as A2; Agent R then re-reviews as R2.

B1 may be planner-integrated if it remains research-only. If B1 changes production-facing contracts, include it in R1.

## Testing / validation authority

The user explicitly authorized experimentation, tests, simulations, regression fixtures, iterative algorithm improvement, and both trade modes. Agents may add/modify focused tests and run relevant suites/simulations within their mission. Do not broaden into unrelated repository test campaigns.

## Execution lifetime

A1+A2 are complete, reviewed, and integrated. C1 is a fresh validation session. B1 is complete; reuse the same Agent B session as B2 after C1 provides frozen labels/splits and a local Jev credential is available.

## Future / blocked work

- C1 — deterministic Wave 3 validation is READY in a new session.
- B2 — live Jev collection/final calibration is blocked by local `TYPESAFE_API_KEY` and the C1 label handoff.
- Main merge — blocked by C1 plus B2/final calibration disposition.
- D31 — resume resource-liquidity/dev-card investigation from `research/d31-road-wip` after the opening validation frontier is stable.

## Transition log

- 2026-09-19: user approved all eight opening-evaluator rebuild items and Agent Work Planner orchestration.
- 2026-09-19: D31/road dirty state preserved on `research/d31-road-wip` @ `300914b`.
- 2026-09-19: main checkout observed receiving concurrent road-intent edits; opening work isolated to dedicated worktrees.
- 2026-09-19: B1 completed offline calibration tooling at `8f25ccb`; planner integrated it into the orchestration branch as `e4b25ad`. Live Jev calibration remains blocked by stable A1 semantics and a locally supplied credential.
- 2026-09-19: A1 reported ~60% complete: rival subtraction removed, causal denial and exact self-funding expansion implemented, hill6758/task9783 passing both trade modes; complete-build port semantics plus full corpus/simulation/build verification remain.
- 2026-09-19: the dirty main-checkout road-intent implementation was preserved separately as `wip/road-intent-deadline` @ `2592caf`; focused Rust regression and main-worktree TypeScript check passed. It remains unreviewed and is not integrated. Main was restored clean at `15cfc5e`, matching `origin/main`.
- 2026-09-19: A1 completed at `15b2365` on `agent/a1-opening-architecture`; worktree clean. Review range is `a169b28..15b2365`.
- 2026-09-19: R1 returned two major blocking findings: (F1) causal denial was computed from the completed board rather than the root placement's decision-time setup state, allowing temporally impossible denial credit; (F2) unfunded expansion applied the same road+settlement ETA discount twice.
- 2026-09-19: A2 completed at `034b01f`, repairing placement-time denial and removing the duplicate ETA denominator; focused validation passed and the worktree is clean.
- 2026-09-19: R2 passed with no blocker or major findings. The review blocker is discharged.
- 2026-09-19: planner integrated A1 and A2 onto `orchestration/opening-rebuild` as `70e1461` and `8496200`. The integrated A1/A2 code surfaces are content-identical to reviewed HEAD `034b01f`; `git diff --check 2a45254..8496200` passed.
- 2026-09-19: `TYPESAFE_API_KEY` is unset in the orchestration environment, so B2 live Jev calibration remains blocked. Credential-free Wave 3 validation moves to C1.
