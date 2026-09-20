# Colonist Strategy Work — Session Recovery Map

Use this file when a ChatGPT session is lost or a new session needs to resume the strategy work without relying on chat history.

Repository: `/home/hamza/repo/colonist-assistant`

## Stable base

- Main checkout: `/home/hamza/repo/colonist-assistant`
- Branch: `main`
- Stable commit at this checkpoint: `15cfc5e` (`chore(jev): strengthen critic evidence controls`)
- At the checkpoint recorded here, `main` matched `origin/main` and was clean.

Do not reconstruct missing work from main. The unfinished lanes were preserved on dedicated branches before main was cleaned.

## Current opening-rebuild effort

### Agent A — opening evaluator architecture

- Worktree: `/home/hamza/repo/colonist-opening-architecture`
- Branch: `agent/a1-opening-architecture`
- Mission: `A1`
- Mission file:
  `/home/hamza/repo/colonist-opening-architecture/docs/agent-work/opening-strategy-rebuild/A1-opening-evaluator-architecture.md`
- Status at this checkpoint: A1+A2 complete, independently reviewed, and integrated.
- A1 source commit: `15b2365` — `Rebuild opening evaluator architecture`
- A2 source repair commit: `034b01f` — `Repair temporal opening evaluation`
- R2 status: pass; no blocker or major findings.
- Integrated orchestration commits: `70e1461` + `8496200`.
- Agent A worktree is clean.
- Next opening-rebuild action is credential-free Wave 3 validation in C1.

A1 owns:

- removal/rework of generic four-player strongest-rival subtraction;
- preservation of opponent self-maximization in the snake draft;
- causal contested-position denial;
- complete-build prospective-port economics;
- self-funding-aware expansion realization;
- focused regressions and matched validation in both trade modes;
- final branch commit and completion report.

R1 found two major blockers:
1. causal denial used the completed board rather than the placement-time setup state, allowing temporally impossible denial credit;
2. unfunded expansion effectively applied the road+settlement ETA discount twice.

A2 completed in the same Agent A session at `034b01f`. Its durable mission file is:
`/home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/A2-opening-evaluator-review-repairs.md`

R2 passed in the same Agent R session. Its durable mission file is:
`/home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/R2-opening-evaluator-repair-review.md`

Wave 3 deterministic validation was assigned to C1. Mission file:
`/home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/C1-opening-wave3-validation.md`

The original Codex Agent C session ended due usage limits after the user capped the terminal campaign at 15 valid arms.

Fresh web Agent E / Mission E1 recovered and finalized that work. Mission file:
`/home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/E1-c1-recovery-finalization.md`

E1 completed at `ad1eb59`. The C1 tooling/report are integrated on the orchestration branch as:

- `002b6a8 Add deterministic opening validation tooling`
- `213ae85 Make strategy lab search budgets deterministic`
- `0e270f4 docs(strategy): finalize C1 wave3 validation`

Final validation under the user cap: recorded corpus 27/0/1; generated cohort 36 cases; terminal campaign 15/15 with zero cutoffs; all four pairwise terminal comparisons inconclusive; `b2-handoff-v1.json` contains `admittedLabels: []`.

The report explicitly distinguishes static opening evidence from the 15 pre-D2 continuation outcomes. A post-D2 terminal rerun is not required for the current conclusion.

D31 resource-liquidity diagnosis completed as D1 at `c58fa11`. Mission file:
`/home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/D1-d31-resource-liquidity-diagnosis.md`

Durable D1 report:
`/home/hamza/repo/colonist-d31-liquidity/docs/agent-work/opening-strategy-rebuild/D1-d31-resource-liquidity-diagnosis-report.md`

D1 established a general evaluator defect: pure hand spending can increase the utility contribution of unchanged board production because persistent production is multiplied by hand-dependent `dynamic_resource_weights`. D31 still does not justify a generic development-card nerf.

D2 completed in the same Agent D session at `e068c5d`. Mission file:
`/home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/D2-persistent-production-repair.md`

R3 passed the exact range `1b2a0c2..e068c5d` with no blocker, major, or directly related minor finding. Mission file:
`/home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/R3-d31-persistent-production-review.md`

D1+D2 are integrated on the orchestration branch as:

- `a4bb9e6 research(strategy): diagnose D31 resource liquidity`
- `35815e3 fix(strategy): stabilize persistent production valuation`

The integrated code/report surfaces are content-identical to reviewed HEAD `e068c5d`. Treat `research/d31-road-wip@300914b` as an evidence donor only, and keep `wip/road-intent-deadline@2592caf` separate.

B2 live/held-out Jev calibration is deferred. The user-capped campaign produced zero conclusive labels, so the calibration contract cannot currently be satisfied even before considering credentials; `TYPESAFE_API_KEY` is also unset. B2 is research-only and is not a production merge blocker. Do not paste the key into chat; supply it only in the local environment if B2 is resumed later.

### Agent B — Jev calibration framework

- Worktree: `/home/hamza/repo/colonist-jev-calibration`
- Branch: `agent/b1-jev-calibration`
- B1 tooling commit: `8f25ccb` (`research(jev): add opening calibration workflow`)
- B1 methodology commit on that branch: `bbfc4fe`
- Status: B1 complete.
- Integrated into orchestration branch as `e4b25ad`.

B2 is a future continuation in the **same Agent B chat** only if conclusive terminal labels are later created and a local `TYPESAFE_API_KEY` is available. Under the current 15-arm cap it is deferred, not a production gate. Never put the key in chat, source, docs, logs, or commits.

### Orchestration / source of truth

- Worktree: `/home/hamza/repo/colonist-opening-orchestration`
- Branch: `orchestration/opening-rebuild`
- Coordination README:
  `/home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/README.md`
- Jev playbook:
  `/home/hamza/repo/colonist-opening-orchestration/docs/JEV_STRATEGY_RESEARCH_PLAYBOOK.md`

Read the README first for the current frontier. This recovery file preserves the branch map and the pre-opening work that must not be forgotten.

## Pre-opening work preserved for later resumption

Two separate branches preserve the work that was underway before the opening-evaluator issue became the priority.

### 1. D31 / strategic diagnostics lane

Branch:

`research/d31-road-wip`

Commit:

`300914b` — `wip(strategy): preserve D31 and road diagnostics`

This commit preserves changes in:

- `engine/crates/catan-arena/src/bin/jev-strategy-lab.rs`
- `engine/crates/catan-search/src/cuda/exact_eval.cu`
- `engine/crates/catan-search/src/depth.rs`
- `engine/crates/catan-search/src/eval.rs`
- `engine/crates/catan-search/src/lib.rs`
- `engine/crates/catan-search/src/midgame_save_spend_tests.rs`
- `engine/crates/catan-search/src/road4311_d14_tests.rs`

Purpose of this lane:

- D31 save-versus-spend / development-card evaluation diagnostics;
- resource-only strategic-utility decomposition;
- the zero-native-ore / zero-native-lumber liquidity question;
- road-intent diagnostics that were being investigated alongside D31;
- research harness instrumentation needed to separate immediate evaluator effects from search effects.

Important D31 conclusion now recorded in the Jev playbook:

- the early replay that favored EndTurn was invalid because one shared RNG stream was shifted by the development-card draw;
- with event-family common random numbers, four matched continuation streams produced BuyDevelopment actor wins in three and EndTurn actor wins in one;
- D31 therefore remains a Jev false-positive / seed-sensitive research case, not evidence for a production dev-card nerf.

This branch is a preserved WIP, not reviewed production code.

### 2. Deadline-limited road-intent arbitration lane

Worktree:

`/home/hamza/repo/colonist-road-intent`

Branch:

`wip/road-intent-deadline`

Commit:

`2592caf` — `wip(search): preserve deadline-limited road arbitration`

This is the exact coherent seven-file implementation that had been left unstaged in main before cleanup. It contains:

- `engine/crates/catan-search/src/depth.rs`
  - deadline-limited same-target road-intent replacement;
  - hill6758 D27 focused regression;
  - provenance for the replacement.
- `engine/crates/catan-wasm/src/lib.rs`
  - WASM provenance output.
- `src/core/engine.ts`
  - TypeScript provenance type and explanation text.
- `src/core/llm-record.ts`
  - evidence-record replacement entry.
- `src/worker/deep-search.ts`
  - WASM-to-frontend provenance mapping.
- `src/generated/wasm/colonist_search.d.ts`
  - generated type synchronization.
- `src/generated/wasm/colonist_search_bg.wasm`
  - generated WASM corresponding to that patch.

Validation already performed before preservation:

- focused Rust regression
  `hill6758_d27_deadline_limited_search_uses_same_target_road_dominance` passed;
- `npm run check` passed in the dependency-equipped main checkout;
- `git diff --check` passed before the preservation commit.

This branch is intentionally **not integrated** and **not independently reviewed**. Treat it as a reviewable candidate, not accepted production behavior.

## Recommended resume order

Do not mix the preserved D31/road lanes into the opening Wave 3 validation/calibration frontier.

1. A1 is complete at `15b2365`.
2. R1 found two major blocking findings.
3. A2 is complete at `034b01f`, repairing temporal denial causality plus duplicated expansion ETA.
4. R2 passed and discharged the review blocker.
5. A1+A2 are integrated onto `orchestration/opening-rebuild` as `70e1461` + `8496200`.
6. Run C1 deterministic Wave 3 validation from the integrated branch.
7. D1 completed at `c58fa11` and proved a general persistent-production revaluation defect while preserving the corrected D31 result: BuyDevelopment wins the actor branch in 3/4 properly matched continuation streams and EndTurn in 1/4; no generic dev-card nerf is justified.
8. D2 completed at `e068c5d`, repairing persistent production weighting in CPU `eval.rs` and mirrored CUDA `exact_eval.cu` while preserving dynamic weights for marginal uses.
9. R3 passed `1b2a0c2..e068c5d` and discharged the D1+D2 review gate.
10. D1+D2 are integrated onto `orchestration/opening-rebuild` as `a4bb9e6` + `35815e3`.
11. C1+E1 are complete and integrated as `002b6a8`, `213ae85`, and `0e270f4`.
12. The final C1 report marks all 15 terminal outcomes as pre-D2 continuation evidence. All four pairwise comparisons are inconclusive and `admittedLabels` is empty.
13. C2 is not required for the current conclusion. Create one only if post-D2 terminal directionality becomes a new research objective.
14. B2 is deferred until conclusive labels plus a local `TYPESAFE_API_KEY` exist. It does not block production integration under the current user-directed cap.
15. Local main and `origin/main` both point to `9aee018`.
16. Treat `research/d31-road-wip@300914b` as a diagnostic/evidence donor, not an integration candidate. Do not cherry-pick it wholesale.
17. The preserved road-intent candidate `2592caf` was cleanly carried onto current local main in `/home/hamza/repo/colonist-road-intent-validation`, branch `agent/f1-road-intent-validation`, commit `2d29125`.
18. F1 completed at `0b3f1ba` with **REJECT/HOLD**. The corrected D27 reconstruction uses the first 24 pre-root gameplay rolls and state hash `af247102fe7f32c6`.
19. Exact gate reproduction failed: the deterministic 4,740-node replay chose Knight, and edge 71's current-main LCB is below edge 53's. Four forced-root CRN continuation pairs were mixed rather than causally dominant; the 160k deeper search chose edge 69, not edge 71. Negative controls passed.
20. F2 is not authorized. Do not tune, harden, rebuild WASM/frontend packaging for, review, or integrate the rejected road-intent candidate. Reopening this question requires a new bounded validation mission with new evidence.
21. Agent M / M1 completed at `ea25476` with **NO GENERAL DEFECT ESTABLISHED** for generic save/spend build-transition behavior. Preserve M1 State A as a local planner-versus-search arbitration reproducer.
22. Agent M / M2 completed at `9214f5e` with **NO GENERAL ACTION-ORDER DEFECT ESTABLISHED**. Four exact commutative pairs reached identical post-sequence GameStates and identical deterministic continuations; no common representation owner repeated.
23. M3 is READY in the same Agent M Codex session. Mission file: `docs/agent-work/opening-strategy-rebuild/M3-decisive-planner-admission-study.md`.
24. M3 tests whether `materially_decisive_transition` is too broad because any settlement/city can authorize `decisive_current_turn_plan_replacement`. Mandatory controls are M1 State A and the task9783 closeout case. M3 is research-only; no production planner mutation is authorized.

## Copy-paste recovery prompt for a future ChatGPT session

```text
Resume the Colonist strategy/orchestration work from the durable recovery state.

Repository:
  /home/hamza/repo/colonist-assistant

Read first:
  /home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/SESSION_RECOVERY.md
  /home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/README.md
  /home/hamza/repo/colonist-opening-orchestration/docs/JEV_STRATEGY_RESEARCH_PLAYBOOK.md

Treat WSL Git state as authoritative. Reconcile the actual branches/worktrees against the recovery file before mutating anything.

Important preserved lanes:
  opening A1:
    /home/hamza/repo/colonist-opening-architecture
    branch agent/a1-opening-architecture

  calibration B1:
    /home/hamza/repo/colonist-jev-calibration
    branch agent/b1-jev-calibration
    commit 8f25ccb

  earlier D31/resource diagnostics:
    branch research/d31-road-wip
    commit 300914b

  preserved deadline-limited road candidate:
    /home/hamza/repo/colonist-road-intent
    branch wip/road-intent-deadline
    commit 2592caf

  orchestration source of truth:
    /home/hamza/repo/colonist-opening-orchestration
    branch orchestration/opening-rebuild

Do not assume preserved WIP branches are reviewed or integration-ready.
Do not blindly merge research/d31-road-wip and wip/road-intent-deadline because they overlap in depth.rs.

First report the verified current frontier. A1+A2 and D1+D2 are reviewed and integrated. C1+E1 are complete and integrated; under the 15-arm user cap all terminal comparisons are inconclusive and admittedLabels is empty, so no post-D2 C2 rerun is required for the current conclusion. B2 is deferred research until conclusive labels and a local TYPESAFE_API_KEY exist. Local main and origin/main both point to 9aee018. F1 completed at 0b3f1ba with REJECT/HOLD, so F2 is not authorized. Agent M completed M1 at ea25476 with no general save/spend defect and M2 at 9214f5e with no general action-order defect. M3 is READY in the same Agent M Codex session to test decisive planner replacement admission, using M1 State A plus task9783 as mandatory controls.
```

## Recovery rule

If this chat is lost, the user only needs to say:

> Read `/home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/SESSION_RECOVERY.md` and resume from the verified current frontier.

That is sufficient to rediscover the work without relying on conversational memory.
