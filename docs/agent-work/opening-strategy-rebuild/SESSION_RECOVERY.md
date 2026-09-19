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

Wave 3 deterministic validation is assigned to C1. Mission file:
`/home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/C1-opening-wave3-validation.md`

The user reports Agent C is working.

D31 resource-liquidity diagnosis may now run in parallel as D1. Mission file:
`/home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/D1-d31-resource-liquidity-diagnosis.md`

D1 must start from reviewed A1+A2 semantics. Treat `research/d31-road-wip@300914b` as an evidence donor only, and keep `wip/road-intent-deadline@2592caf` separate.

B2 live/held-out Jev calibration remains blocked because `TYPESAFE_API_KEY` is currently unset in the orchestration environment. Do not paste the key into chat; supply it only in the local environment when ready.

### Agent B — Jev calibration framework

- Worktree: `/home/hamza/repo/colonist-jev-calibration`
- Branch: `agent/b1-jev-calibration`
- B1 tooling commit: `8f25ccb` (`research(jev): add opening calibration workflow`)
- B1 methodology commit on that branch: `bbfc4fe`
- Status: B1 complete.
- Integrated into orchestration branch as `e4b25ad`.

B2 is a future continuation in the **same Agent B chat** after C1 provides frozen labels/splits and a local `TYPESAFE_API_KEY` is available. Never put the key in chat, source, docs, logs, or commits.

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
7. After C1 produces frozen labels/splits and a local `TYPESAFE_API_KEY` exists, continue B2 in the same Agent B session.
8. In parallel with C1, run D1 from the reviewed opening base. Preserve the corrected D31 result: BuyDevelopment wins the actor branch in 3/4 properly matched continuation streams and EndTurn in 1/4; no dev-card nerf is justified.
9. Treat `research/d31-road-wip@300914b` as a diagnostic/evidence donor, not an integration candidate. Do not cherry-pick it wholesale.
10. Keep `wip/road-intent-deadline@2592caf` separate from D1; it overlaps `depth.rs` and remains unreviewed.
11. Merge back to main only after the opening final-validation disposition.
12. If D1 proves a general production invariant, create a bounded D2 implementation mission and require independent review before integrating it.
13. Independently review the road-intent candidate before any eventual integration; its focused regression passed, but the policy override still needs broader causal validation because earlier road disagreements produced Jev false positives.

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

First report the verified current frontier. A1+A2 are reviewed and integrated as 70e1461 + 8496200. C1 may be active on opening Wave 3 validation. D1 resource-liquidity diagnosis may run concurrently from a separate worktree using research/d31-road-wip@300914b only as an evidence donor. Keep wip/road-intent-deadline@2592caf separate. B2 remains blocked until C1 labels/splits exist and a local TYPESAFE_API_KEY is available.
```

## Recovery rule

If this chat is lost, the user only needs to say:

> Read `/home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/SESSION_RECOVERY.md` and resume from the verified current frontier.

That is sufficient to rediscover the work without relying on conversational memory.
