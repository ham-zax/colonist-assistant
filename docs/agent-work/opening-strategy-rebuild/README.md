# Opening Strategy Rebuild - Session Coordination

**Repository:** /home/hamza/repo/colonist-assistant  
**Integration worktree:** /home/hamza/repo/colonist-opening-orchestration  
**Source of truth:** docs/JEV_STRATEGY_RESEARCH_PLAYBOOK.md plus live hill6758/task9783 evidence  
**Current wave:** 1  
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
├─ Wave 1 / A1 / Agent A
│  CONTINUE — self-reported ~60% complete
│  Objective/denial/self-funding work is in place; complete-build port semantics and full validation remain.
│
├─ Wave 1 / B1 / Agent B
│  COMPLETE + INTEGRATED
│  Calibration framework integrated as e4b25ad; live numeric calibration waits for A1 semantics and local credentials.
│
├─ D31 / preserved WIP
│  HOLD
│  research/d31-road-wip @ 300914b preserves current midgame/road diagnostics.
│
├─ Main checkout
│  CLEAN / HOLD
│  Previous road-intent edits were preserved on wip/road-intent-deadline @ 2592caf; main now matches origin/main.
│
└─ Wave 2 / R1 independent review
   PLANNED
   Blocks A1 integration once A1 reports complete.

Planned effort: 45% A1 implementation | 20% B1 calibration research | 15% review/integration | 20% final validation

## Session Ledger

| Agent | Mission | Status | Role | Workspace | Disposition | Reusable |
| --- | --- | --- | --- | --- | --- | --- |
| A | A1 | CONTINUE (~60% self-reported) | implement | /home/hamza/repo/colonist-opening-architecture | CONTINUE | yes |
| B | B1 | COMPLETE + INTEGRATED | investigate + research tooling | /home/hamza/repo/colonist-jev-calibration | COMPLETE; reuse as B2 after A1 | yes |
| R | R1 | PLANNED | independent review | read-only target after A1 | NEW SESSION later | R2 |
| D31 lane | preserved | HOLD | diagnostics | research/d31-road-wip @ 300914b | resume after opening frontier | yes |

## Blocker Ledger

| Blocked item | Blocker | Owner | Discharge condition | Status/evidence |
| --- | --- | --- | --- | --- |
| A1 integration | independent review | R1 | R1 reports no blocking findings | PLANNED |
| Wave 3 final validation | reviewed A1 integration + final calibration inputs | A1/R1/B2 | R1 passes A1, corrected feature semantics are integrated, generated/matched labels exist, and local Jev credentials are available for live collection | BLOCKED; B1 framework integrated as e4b25ad |
| merge back to main | reviewed integration + final validation | planner | R1 passes A1 and Wave 3 passes | BLOCKED |
| D31 continuation | opening rebuild integration preferred first | planner | opening Wave 2 stable | HOLD |

## Dependency map

```text
A1 opening evaluator rebuild ─> R1 review ─> integration ─> Wave 3 validation
B1 calibration framework ────────────────────────┘          └─> B2 live/held-out calibration

D31 WIP remains separate and resumes after opening integration.
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
- Do not touch the active main checkout while its concurrent writer remains unresolved.

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

A1: `persistent-agent-loop`. B1 is complete; reuse the same Agent B session as B2 after A1 publishes stable feature semantics.

## Future / blocked work

- Wave 2 — independent review + A1 integration — blocked by A1 completion.
- B2 — live Jev collection/final calibration — blocked by stable A1 feature semantics and local `TYPESAFE_API_KEY`.
- Wave 3 — recorded corpus, generated boards, matched terminal streams, both trade modes, held-out Jev calibration — blocked by reviewed A1 integration plus B2 inputs.
- D31 — resume resource-liquidity/dev-card investigation from `research/d31-road-wip` after opening architecture is stable.

## Transition log

- 2026-09-19: user approved all eight opening-evaluator rebuild items and Agent Work Planner orchestration.
- 2026-09-19: D31/road dirty state preserved on `research/d31-road-wip` @ `300914b`.
- 2026-09-19: main checkout observed receiving concurrent road-intent edits; opening work isolated to dedicated worktrees.
- 2026-09-19: B1 completed offline calibration tooling at `8f25ccb`; planner integrated it into the orchestration branch as `e4b25ad`. Live Jev calibration remains blocked by stable A1 semantics and a locally supplied credential.
- 2026-09-19: A1 reported ~60% complete: rival subtraction removed, causal denial and exact self-funding expansion implemented, hill6758/task9783 passing both trade modes; complete-build port semantics plus full corpus/simulation/build verification remain.
- 2026-09-19: the dirty main-checkout road-intent implementation was preserved separately as `wip/road-intent-deadline` @ `2592caf`; focused Rust regression and main-worktree TypeScript check passed. It remains unreviewed and is not integrated. Main was restored clean at `15cfc5e`, matching `origin/main`.
