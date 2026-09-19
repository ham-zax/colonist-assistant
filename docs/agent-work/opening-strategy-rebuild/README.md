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
├─ Wave 3 deterministic validation / C1+E1
│  COMPLETE + INTEGRATED
│  Tooling integrated as 002b6a8 + 213ae85; final report integrated as 0e270f4.
│  User-capped terminal campaign: 15/15 complete, all pairwise comparisons inconclusive, admittedLabels=[].
│  Static opening validation is complete; terminal outcomes are explicitly pre-D2 continuation evidence.
│
├─ D31 resource-liquidity / D1+D2
│  COMPLETE + REVIEWED + INTEGRATED
│  Source commits c58fa11 + e068c5d; integrated as a4bb9e6 + 35815e3.
│
├─ Independent D1+D2 review / R3 / Agent R
│  COMPLETE — PASS
│  Full range 1b2a0c2..e068c5d approved; review blocker discharged.
│
├─ Jev calibration / B2 / Agent B
│  DEFERRED / NON-PRODUCTION RESEARCH
│  No conclusive labels are available under the user-capped campaign and TYPESAFE_API_KEY is unset.
│
├─ Road-intent candidate / F1 / Agent F
│  COMPLETE — REJECT/HOLD
│  Validation branch agent/f1-road-intent-validation @ 0b3f1ba.
│  Exact D27 gate did not reproduce; 4-stream matched continuation was mixed.
│  F2 is not authorized. Old wip/road-intent-deadline @ 2592caf remains provenance only.
│
└─ Main checkout
   COMPLETE — LOCAL MAIN INTEGRATED
   Current local main and origin/main both point to 9aee018.

Planned effort: 95% COMPLETE/INTEGRATED | 5% OPTIONAL/DEFERRED B2

## Session Ledger

| Agent | Mission | Status | Role | Workspace | Disposition | Reusable |
| --- | --- | --- | --- | --- | --- | --- |
| A | A1 | COMPLETE + INTEGRATED | implement | /home/hamza/repo/colonist-opening-architecture | integrated as 70e1461 | yes |
| A | A2 | COMPLETE + INTEGRATED | implement repair | /home/hamza/repo/colonist-opening-architecture | integrated as 8496200 | yes |
| B | B1 | COMPLETE + INTEGRATED | investigate + research tooling | /home/hamza/repo/colonist-jev-calibration | COMPLETE; reuse as B2 after final post-D2 labels + credential | yes |
| C | C1 | COMPLETE + INTEGRATED | independent validation + research tooling | /home/hamza/repo/colonist-opening-validation | tooling integrated as 002b6a8 + 213ae85 | no |
| E | E1 | COMPLETE + INTEGRATED | validation recovery + finalization | /home/hamza/repo/colonist-opening-validation | report integrated as 0e270f4 | no |
| D | D1 | COMPLETE + INTEGRATED | investigation + research tooling | /home/hamza/repo/colonist-d31-liquidity | integrated as a4bb9e6 | yes |
| D | D2 | COMPLETE + INTEGRATED | implementation repair | /home/hamza/repo/colonist-d31-liquidity | integrated as 35815e3 | yes |
| R | R1 | COMPLETE — BLOCKING FINDINGS | independent review | read-only target a169b28..15b2365 | COMPLETE | yes |
| R | R2 | COMPLETE — PASS | independent re-review | read-only repair range 15b2365..034b01f | review blocker discharged | yes |
| R | R3 | COMPLETE — PASS | independent review | read-only target 1b2a0c2..e068c5d | review blocker discharged | yes |
| F | F1 | COMPLETE — REJECT/HOLD | causal validation + research tooling | /home/hamza/repo/colonist-road-intent-validation | final report 0b3f1ba; F2 not authorized | no |
| D31 preserved branch | evidence donor | PRESERVED | historical diagnostics | research/d31-road-wip @ 300914b | do not cherry-pick wholesale | yes |

## Blocker Ledger

| Blocked item | Blocker | Owner | Discharge condition | Status/evidence |
| --- | --- | --- | --- | --- |
| A1/A2 integration | R2 re-review | R2 | R2 reports pass | DISCHARGED — R2 passed; integrated as 70e1461 + 8496200 |
| Wave 3 deterministic validation | C1 recovery/finalization | E1 | E1 verifies C1 artifacts, commits final report, and separates pre-D2 terminal evidence from static opening evidence | DISCHARGED — E1 complete at ad1eb59; integrated as 0e270f4 |
| B2 live/held-out Jev calibration | conclusive labels + local TypeSafe credential | B2 | conclusive terminal labels exist and local TYPESAFE_API_KEY is available | DEFERRED — user capped terminal evidence at 15 arms; all four comparisons inconclusive; admittedLabels=[]; credential unset |
| merge back to main | reviewed production changes + user-capped final validation | planner | production review gates pass and capped final validation finds no blocking production defect | DISCHARGED — local main at 9aee018 |
| D1 diagnosis | reviewed opening/economy base | D1 | establish/falsify general defect | DISCHARGED — c58fa11 established persistent-production invariant violation |
| D1+D2 integration | independent review | R3 | Agent R reports no blocking findings on 1b2a0c2..e068c5d | DISCHARGED — R3 passed; integrated as a4bb9e6 + 35815e3 |
| road-intent admission | exact D27 reconstruction + matched terminal evidence | F1 | exact gate reproduced, negative controls hold, and matched continuation establishes a coherent advantage | CLOSED — F1 REJECT/HOLD; gate reproduction failed and matched continuation was mixed |
| road-intent integration | F1 admission + F2 hardening + independent review | F2/Agent R | F1 admits, F2 rebuilds/synchronizes current-base package, Agent R reports no blockers | CANCELLED — F2 not authorized |

## Dependency map

```text
A1 ─> R1 BLOCK ─> A2 ─> R2 PASS ─> reviewed integration ─> C1 checkpoint ─> E1 COMPLETE ─> main merge
D1 ─> D2 ─> R3 PASS ────────────────────────────────────────────────────────────────┘

B1 calibration framework ─> B2 DEFERRED
                           └─ needs conclusive labels + local TYPESAFE_API_KEY

road-intent 2592caf provenance ─> current-main carry 2d29125 ─> F1 REJECT/HOLD
                                                         └─ stop; no F2, no R4, no main integration

Under the user's 15-arm cap, final deterministic validation is complete: recorded corpus 27/0/1, generated cohort 36 cases, terminal campaign 15/15 with zero cutoffs, all four terminal comparisons inconclusive, and zero admitted B2 labels. Static opening evidence remains valid after D2; terminal outcomes are explicitly pre-D2 continuation evidence. No production defect was found, so inconclusive terminal ranking evidence is not treated as a production blocker.
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
- Agent C: /home/hamza/repo/colonist-opening-validation
- Agent D: /home/hamza/repo/colonist-d31-liquidity
- Agent F: /home/hamza/repo/colonist-road-intent-validation

Main checkout is clean and held as the stable integration destination. Continue implementation in the assigned isolated worktrees.

## Review / integration policy

A1 is substantial strategy/search implementation and requires independent Agent R review before integration. Agent R is read-only. Blocking findings return to Agent A as A2; Agent R then re-reviews as R2.

D2 changes a shared CPU/CUDA evaluator contract. The full D1+D2 range must receive independent Agent R review before integration. If that review finds blocking defects, return repairs to the same Agent D session before re-review.

B1 may be planner-integrated if it remains research-only. If B1 changes production-facing contracts, include it in independent review before production admission.

## Testing / validation authority

The user explicitly authorized experimentation, tests, simulations, regression fixtures, iterative algorithm improvement, and both trade modes. Agents may add/modify focused tests and run relevant suites/simulations within their mission. Do not broaden into unrelated repository test campaigns.

## Execution lifetime

A1+A2 and D1+D2 are complete, reviewed, and integrated. C1+E1 are complete and integrated; E1 finalized the user-capped evidence package at ad1eb59. B1 is complete. B2 is deferred until both conclusive labels and a local Jev credential exist; it is not a production merge gate.

## Future / blocked work

- C1 — COMPLETE; tooling integrated as `002b6a8` + `213ae85`.
- E1 — COMPLETE; finalized report integrated as `0e270f4`.
- C2 — NOT REQUIRED for the current conclusion. It becomes optional only if final-semantics terminal directionality is later desired.
- B2 — DEFERRED. The capped campaign produced zero conclusive labels and `TYPESAFE_API_KEY` is unset, so the calibration contract cannot currently be satisfied. This is research-only and does not block production integration.
- Main merge — COMPLETE at `9aee018`; current `origin/main` also points to `9aee018`.
- D1+D2 — COMPLETE + REVIEWED + INTEGRATED as `a4bb9e6` + `35815e3`.
- R3 — COMPLETE — PASS; review blocker discharged.
- Road-intent F1 — COMPLETE at `0b3f1ba`; admission decision is REJECT/HOLD. Exact D27 trigger did not reproduce on current main and the four matched continuation pairs were mixed.
- Old road-intent branch — `wip/road-intent-deadline@2592caf` remains provenance only; do not integrate it directly.
- F2 — NOT AUTHORIZED. No road-intent hardening, WASM/frontend rebuild, R4 review, or main integration follows from F1.

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
- 2026-09-19: D31 may resume in parallel as D1 because reviewed A1+A2 semantics are integrated and C1 owns a separate worktree. D1 must use the corrected four-stream CRN result (BuyDevelopment actor win 3/4, EndTurn 1/4), treat `300914b` as an evidence donor rather than an integration candidate, and keep `2592caf` road-intent work separate.
- 2026-09-19: D1 completed at `c58fa11`. It established a general evaluator defect: a pure hand spend can increase the strategic-utility contribution of unchanged board production because persistent production is multiplied by hand-dependent `dynamic_resource_weights`. D1 did not justify a generic dev-card penalty. D2 is now READY in the same Agent D session; CPU/CUDA persistent-production semantics are the narrow repair owner.
- 2026-09-19: planner verification found `git diff --check 1b2a0c2..c58fa11` reported trailing whitespace on lines 5–6 of the D1 report despite the D1 finish report saying diff-check passed.
- 2026-09-19: D2 completed at `e068c5d`. It fixed persistent-production valuation in CPU and exact CUDA, kept dynamic weights for marginal uses, converted the H1 probe into a permanent invariant regression, preserved spend-now guards, and resolved the D1 report whitespace. Planner verification confirms the worktree is clean and `git diff --check 1b2a0c2..e068c5d` passes.
- 2026-09-19: R3 passed with no blocker, major, or directly related minor finding. It independently re-ran the invariant/spend-now tests and the 69-case hardware CPU/CUDA parity check, and approved `1b2a0c2..e068c5d` for integration.
- 2026-09-19: planner integrated D1+D2 onto `orchestration/opening-rebuild` as `a4bb9e6` + `35815e3`. The integrated D1+D2 code/report surfaces are content-identical to reviewed HEAD `e068c5d`; `git diff --check 241f1e4..35815e3` passes.
- 2026-09-19: the Codex C1 session ended due usage limits after the user reduced the terminal campaign to 15 valid arms. Planner inspection found C1 substantially complete at `8146fe5`: recorded corpus 27/0/1, 36 generated cases, exactly 15 admitted terminal arms, zero cutoffs, all four pairwise comparisons inconclusive, and `b2-handoff-v1.json` contains `admittedLabels: []`. Generated/matched/recorded/prior SHA manifests all verify.
- 2026-09-20: E1 completed at `ad1eb59`, verified the capped evidence package, corrected one research-only provenance boolean in ignored artifacts, and finalized the report. The report explicitly marks all 15 terminal arms as pre-D2 continuation evidence while preserving the static A1+A2 opening evidence. C2 is not required for the current conclusion.
- 2026-09-20: planner integrated C1 tooling/report onto `orchestration/opening-rebuild` as `002b6a8`, `213ae85`, and `0e270f4`. Clean cherry-picks preserved the already integrated D1 diagnostic consumer in `jev-strategy-lab`; `git diff --check 6edfde6..0e270f4` passes. Under the user's explicit 15-arm cap, B2 is deferred rather than treated as a production blocker because there are no conclusive labels to calibrate against.
- 2026-09-20: local `main` was fast-forwarded through the reviewed opening/D31/C1 integration and later reached `9aee018`; current `origin/main` also points to `9aee018`.
- 2026-09-20: planner reopened the held road-intent lane as F1. A fresh validation worktree `/home/hamza/repo/colonist-road-intent-validation` on `agent/f1-road-intent-validation` was created from current local main `9aee018`, and preserved candidate `2592caf` was cleanly cherry-picked as `2d29125`. F1 is diagnostic/admission-only: exact hill6758 D27 reconstruction, deterministic gate reproduction, 4-stream event-family CRN forced-root comparison, deeper-search diagnostic, and D17-style negative controls.
- 2026-09-20: F1 completed at `0b3f1ba` with **REJECT/HOLD**. Corrected D27 state hash `af247102fe7f32c6` uses the first 24 pre-root Balanced-Dice rolls. The 4,740-node deterministic replay chose Knight and edge 71 had a worse LCB than edge 53, so the exact gate did not reproduce. Four matched forced-root streams split one candidate-only win, one baseline-only win, one both-win, and one both-loss; deeper 160k search chose edge 69, not edge 71. Negative controls passed. F2 is not authorized.
