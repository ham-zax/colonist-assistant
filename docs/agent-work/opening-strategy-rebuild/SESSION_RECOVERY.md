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
- Status at this checkpoint: A1 complete and committed.
- A1 commit: `15b2365` — `Rebuild opening evaluator architecture`
- Review base: `a169b28`
- Worktree is clean.
- Integration is blocked by independent R1 review of `a169b28..15b2365`.

A1 owns:

- removal/rework of generic four-player strongest-rival subtraction;
- preservation of opponent self-maximization in the snake draft;
- causal contested-position denial;
- complete-build prospective-port economics;
- self-funding-aware expansion realization;
- focused regressions and matched validation in both trade modes;
- final branch commit and completion report.

Integration remains blocked until independent Agent R review passes. If R1 finds blockers, return repairs to the same Agent A session as A2, then re-review in the same Agent R session as R2.

### Agent B — Jev calibration framework

- Worktree: `/home/hamza/repo/colonist-jev-calibration`
- Branch: `agent/b1-jev-calibration`
- B1 tooling commit: `8f25ccb` (`research(jev): add opening calibration workflow`)
- B1 methodology commit on that branch: `bbfc4fe`
- Status: B1 complete.
- Integrated into orchestration branch as `e4b25ad`.

B2 is a future continuation in the **same Agent B chat** after A1 passes review/integration and a local `TYPESAFE_API_KEY` is available. Never put the key in chat, source, docs, logs, or commits.

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

Do not mix the preserved D31/road lanes into the A1 review/repair loop.

1. A1 is complete at `15b2365`.
2. Run independent Agent R review of `a169b28..15b2365` now.
3. If R1 finds blockers, send the repair back to the same Agent A session as A2, then re-review as R2.
4. If R1 passes, planner-integrate A1 into the orchestration branch, then proceed to B2/Wave 3.
5. After the opening architecture reaches a stable reviewed integration point, resume the pre-opening strategy lane.
6. When resuming the earlier lane, inspect `research/d31-road-wip@300914b` and `wip/road-intent-deadline@2592caf` together before choosing what to carry forward. They overlap in `depth.rs`, so do **not** blindly merge or cherry-pick both.
7. Re-evaluate the D31/resource-liquidity owner against the now-correct opening/economy primitives. Keep the corrected multi-seed result: no dev-card nerf is justified by D31.
8. Independently review the road-intent candidate before any integration; its focused regression passed, but the policy override still needs broader causal validation because earlier road disagreements have produced Jev false positives.
9. Continue B2/final Jev calibration only after reviewed A1 semantics are integrated and credentials are locally available.

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

First report the verified current frontier. A1 is complete at 15b2365; move to the independent R1 review gate unless repository evidence shows a newer reviewed state. After the opening architecture is reviewed/stable, resume the earlier D31/resource-liquidity and road-intent investigation from the preserved branches.
```

## Recovery rule

If this chat is lost, the user only needs to say:

> Read `/home/hamza/repo/colonist-opening-orchestration/docs/agent-work/opening-strategy-rebuild/SESSION_RECOVERY.md` and resume from the verified current frontier.

That is sufficient to rediscover the work without relying on conversational memory.
