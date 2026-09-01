# Recommendation Engine Before/After Evidence Plan

**Purpose:** Measure whether the corrected recommendation engine is stronger in complete games and difficult midgame positions without using win rate to excuse correctness violations.

**Relationship to the correctness plan:** This is a separate verification/evidence plan. `RECOMMENDATION_ENGINE_IMPLEMENTATION_PLAN_2026-09-01.md` decides how confirmed defects are repaired. This document freezes the `deep-maxn-v9` baseline before those repairs, then replays the same evidence against the corrected engine. A version must pass the correctness gates before strength results are used to prefer it.

## Existing machinery we should reuse

The repository already contains the core of the evaluation harness:

- `engine/crates/catan-arena/src/main.rs` runs deterministic native games with 2, 3, or 4 players.
- The arena supports `random`, `weighted`, `maxn`, `alphabeta`, `uct`, and `puct` engines plus explicit mixed `--lineup` configurations.
- Each arena block preserves the same board seed and chance seed while rotating the candidate through every seat. This is the primary seat-bias control.
- Search engines use weighted hidden-information particles by default. `--perfect-information` is an explicit diagnostic mode and must not be used for strength claims.
- `scripts/benchmark-local.mjs` runs seat-balanced native matrices and writes JSON/Markdown plus incremental checkpoints.
- `scripts/read-arena-checkpoint.mjs` reads partial/complete arena checkpoints.
- `scripts/replay-decisions.mjs` and `scripts/replay-engine.ts` rerun captured real positions through packaged WASM configurations.
- `scripts/profile-live-search.ts` measures live-budget sensitivity on captured positions.
- `scripts/benchmark-colonist.mjs` drives the built extension through real four-player Base-map games against three Colonist bots.
- `scripts/audit-colonist-benchmark.mjs` audits those browser games for recommendation/execution failures and suspicious actions.
- `scripts/run-expert-iteration.mjs` and the training scripts are useful research tooling, but learned-policy training is not part of this evidence plan until structural correctness is green.

The missing capability is a persistent counterfactual takeover corpus: save an exact nonterminal arena position plus the random-stream state, then continue that identical position with different candidate engines.

## Current baseline smoke evidence

The optimized native arena was built from:

- Git commit: `23f5eea3aaf06ce7679d0b5490168bf8a842f6ce`
- Engine revision: `deep-maxn-v9`
- Search profile used for the smoke: depth 4, branch cap 8, 4,000 nodes, 350 ms, 24 posterior particles, 12 strategic particles
- Information mode: weighted belief
- Validation: enabled

Two one-block smoke runs verified that matched seat rotation works. They are not strength estimates because the sample is deliberately tiny.

| Players | Seed | Games | MaxN wins | Weighted wins | MaxN win share | Fair share | MaxN mean rank | MaxN mean VP |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 3 | 93000001 | 3 | 1 | 2 | 33.3% | 33.3% | 2.333 | 6.667 |
| 4 | 94000001 | 4 | 0 | 4 | 0.0% | 25.0% | 3.500 | 4.750 |

The four-player smoke also reported a search-deadline share of about 31.7%. That is a reproducible challenge seed, not proof that MaxN is globally weaker than `weighted`.

## Evidence rules

- Freeze seeds, challenge-selection rules, and metrics before looking at corrected-engine outcomes.
- Never cherry-pick only positions where the new engine wins.
- Compare variants from the same state with the same future chance stream and the same opponent random-stream states.
- Rotate candidate seats in whole-game tests.
- Cluster confidence intervals by source game/block, not by treating many snapshots from one source game as independent samples.
- Keep perfect-information runs labeled as oracle diagnostics. They are not production-strength evidence.
- A cheating or rule-invalid engine is disqualified even if it wins more games.
- Report cutoffs, crashes, illegal actions, and timeouts separately. Do not silently count infrastructure failures as losses or omit them from denominators.
- Keep production search settings explicit in every report. `benchmark-local.mjs` currently does not pass the live MaxN depth/branch/node/time settings, so the evidence tooling must add that explicit profile before its results are called production-like.

# Phase E0 - Freeze the pre-fix `deep-maxn-v9` baseline

Do this before Task 1 of the correctness implementation changes recommendation semantics.

### E0.1 Record build identity and live search configuration

Capture:

- Git SHA and dirty state;
- `ENGINE_REVISION`;
- TypeScript live `maxNodes`, `timeBudgetMs`, `depth`, `branchCap`, tactical budget;
- tracker/WASM/Rust particle limits;
- player count;
- information mode;
- arena/extension build identity.

### E0.2 Make the native benchmark use an explicit production profile

Extend `scripts/benchmark-local.mjs` so a run can pass the actual MaxN parameters through to `colonist-arena`, including:

```text
maxnDepth = 4
maxnBranch = 8
maxnNodes = 4000
maxnTimeMs = 350
beliefParticles = 24
strategicParticles = 12   # v9 baseline only; v10 uses the corrected Rust contract
```

Do not rely on arena defaults when making before/after claims.

### E0.3 Freeze whole-game baseline seeds

Primary strength matrix:

- 3-player and 4-player games;
- MaxN candidate versus `weighted` and `alphabeta` baselines;
- matched block/seat rotation;
- fixed deterministic seed range shared with the post-fix run;
- enough blocks that the block-bootstrap interval is useful.

Two-player games may remain a diagnostic matrix, but the primary release evidence is 3-player and 4-player because the extension is intended for ordinary multiplayer Colonist games and the real-browser benchmark is four-player.

Persist the JSON, Markdown, and checkpoint files. Do not regenerate the v9 baseline after source changes and call it the same baseline.

# Phase E1 - Add exact counterfactual takeover snapshots

## Goal

Take a player who is behind in a real simulated game, freeze the exact game at the start of that player's turn, and ask:

> From this identical position and identical future random stream, how often can the control policy, `deep-maxn-v9`, the corrected engine, and a larger reference search recover?

This measures comeback ability directly instead of inferring it from static evaluator scores.

## Snapshot boundary

Capture only at a stable player decision boundary:

- phase `PreRoll` at the beginning of the target player's turn;
- nonterminal state;
- no pending discard, robber, development chance, or trade-response protocol.

A snapshot needs enough data to reconstruct the exact continuation:

- complete dynamic `GameState` fields;
- board seed and player count (the standard board can be reconstructed deterministically);
- current state hash;
- chance RNG internal state;
- every per-player policy RNG internal state;
- source block, seat rotation, turn, and engine lineup;
- source build SHA and `ENGINE_REVISION`.

`GameState` and `SplitMix64` are already `Clone`/`Copy`-friendly in memory. Persistent cross-build replay needs an arena-only snapshot DTO plus a stable way to restore `SplitMix64` state. Do not add a legacy v9 policy path to production code merely to compare versions.

## Challenge selection

Select challenge states by rules fixed before v10 results exist. For a candidate seat at `PreRoll`, include the state when all of the following are true:

1. the game is nonterminal;
2. the target has played enough of the game to be meaningfully behind (`turn >= players * 5`);
3. the target is either last by public victory points or at least 2 public VP behind the current leader;
4. the target's current evaluator win value is `<= 0.25`;
5. the state hash has not already been included from that source game.

Sample at most one challenge state per target seat per source game. This prevents one long game from dominating the corpus.

Freeze a balanced corpus before post-fix evaluation:

- 50 three-player challenge states;
- 100 four-player challenge states;
- balanced across candidate seats as closely as the source games permit.

If the fixed seed range does not produce enough qualifying states, extend the seed range deterministically; do not relax the criteria after inspecting variant outcomes.

## Fork arms

From every frozen challenge snapshot, run these continuations:

1. **Control:** the original seat policy continues.
2. **v9 takeover:** `deep-maxn-v9` controls only the target seat; all other policies are unchanged.
3. **Reference takeover:** corrected semantics with a larger search budget; this is a ceiling/sensitivity reference, not ground truth.
4. **v10 takeover:** after the implementation plan is complete, the corrected production engine controls the target seat.

The v9/control/reference outcomes are recorded before recommendation semantics change. The persistent snapshot is then replayed for v10 later.

## Common-random-number control

Each fork starts with clones/restorations of:

- the exact same `GameState`;
- the exact same chance RNG state;
- the exact same opponent policy RNG states;
- the same candidate-seat RNG state when the policy uses randomness.

After the policies choose different actions the states naturally diverge. The random streams still start from the same point, which removes avoidable variance without pretending the future should remain state-identical.

## Takeover metrics

Record per challenge state and per arm:

- terminal win/loss;
- final rank;
- final victory points;
- VP gained after takeover;
- turns to terminal/cutoff;
- Longest Road/Largest Army acquisition;
- roads, settlements, cities, development purchases;
- domestic offers/acceptance/counters;
- cards lost to sevens;
- decision latency and deadline share;
- illegal-action/protocol failures;
- search nodes, depth, WASM particles, Rust coalesced particles.

Primary paired metrics:

- **rescue:** v9 loses, v10 wins;
- **regression:** v9 wins, v10 loses;
- **net paired rescues:** rescues minus regressions;
- **reference recovery:** reference wins, v9 loses, v10 wins;
- change in mean final rank and mean final VP.

Bootstrap intervals by source game/block. Do not bootstrap individual snapshots as independent when they came from the same source block.

# Phase E2 - Whole-game before/after arena evidence

After the corrected engine passes finding-specific correctness tests, rerun the frozen E0 matrix with the same seeds and seat rotations.

Compare at minimum:

- win share versus fair share;
- blocked 95% interval;
- mean rank;
- mean victory points;
- terminal/cutoff counts;
- mean decision latency;
- search deadline share;
- search nodes/depth;
- posterior/WASM/Rust particle counts;
- trade, build, road, development, and discard-risk metrics already emitted by the arena.

Interpretation rules:

- A higher v10 win share supports strength, but does not close a correctness finding.
- A lower win share is a release concern even if local regressions pass; inspect takeover/replay evidence before changing evaluator/search budgets.
- If a variant only wins under perfect information, treat that as evidence of hidden-information sensitivity, not production strength.

# Phase E3 - Position-level replay evidence from real Colonist traces

Use `scripts/replay-decisions.mjs` on frozen real decision traces.

For every audit fixture and selected real-game disagreement, report:

- actual historical v9 action;
- v10 production action;
- medium and maximum corrected reference actions;
- root rank/admission;
- action-family agreement;
- root values/regret;
- nodes, latency, and particle counts;
- exact/tactical/safety authority.

The recovered turn-54 case is mandatory. The evidence should show whether the corrected engine stops preferring the information-dominated action ordering for the reason identified in F11 rather than because of a later unrelated heuristic.

# Phase E4 - Real Colonist browser games

Run `scripts/benchmark-colonist.mjs` only after the packaged extension passes Task 16.

The existing script exercises:

```text
Colonist page
-> page/state extraction
-> tracker/beliefs
-> packaged WASM
-> recommendation selection
-> UI mapping
-> autonomous execution
```

Use the normal four-player Base map and three Colonist bots. Run Easy, Medium, and Hard separately. Persist the generated traces and audit each benchmark report with `scripts/audit-colonist-benchmark.mjs`.

Report:

- completed games versus requested games;
- wins and ranks by difficulty;
- startup/timeouts/stalls separately;
- execution cancellations and protocol failures;
- repeated/impossible trades;
- risky `EndTurn` events;
- post-game actions;
- source/authority distribution;
- exact installed build identity and engine revision.

Do not compare a v9 live-browser batch and v10 live-browser batch as if they had identical random games. This layer proves end-to-end real-world behavior; the deterministic native and takeover layers provide the controlled paired comparisons.

# Phase E5 - Final evidence report

Create one final report that keeps the evidence layers separate:

1. **Correctness closure:** F1-F15 reproduction status and F9 disposition.
2. **Paired position evidence:** captured Colonist replay agreement/regret.
3. **Counterfactual comeback evidence:** control/v9/reference/v10 takeover outcomes from frozen identical states.
4. **Whole-game native evidence:** matched 3p/4p seat-rotated before/after matrix.
5. **Real browser evidence:** Colonist bot games and execution audit.

A release-strength conclusion should state all five. Do not reduce the conclusion to one win-rate number.

## Proposed verification-tooling changes

These changes are separate from recommendation semantics and should be reviewed as measurement tooling:

- `engine/crates/catan-arena/src/main.rs`
  - add persistent challenge snapshot capture/replay;
  - add fork/takeover mode;
  - emit paired outcome records;
  - preserve block/seat/source identifiers.
- `engine/crates/catan-core/src/rng.rs`
  - expose a minimal stable `SplitMix64` state snapshot/restore API if persistent replay cannot otherwise restore exact RNG state.
- `scripts/benchmark-local.mjs`
  - accept/pass explicit live MaxN depth/branch/node/time and particle settings;
  - add a named production profile rather than relying on arena defaults.
- `scripts/read-arena-checkpoint.mjs`
  - summarize takeover/fork checkpoints if they share the existing checkpoint stream.
- `scripts/replay-engine.ts`
  - share the final live/reference configuration names with the correctness implementation plan.
- `benchmark-results/`
  - store immutable baseline and post-fix reports with build SHA/engine revision in filenames or report metadata.

Do not modify the production extension merely to preserve a legacy v9 engine for A/B testing.

## Completion criteria

This evidence plan is complete when:

- the v9 whole-game baseline and takeover corpus were frozen before recommendation-semantic changes;
- the same challenge snapshots were replayed with the corrected production engine;
- 3p and 4p whole-game matrices used matched seeds and seat rotation with explicit live search settings;
- real Colonist browser games exercised the packaged corrected build;
- all reports include build SHA, engine revision, configuration, failures/cutoffs, and denominators;
- the final report distinguishes correctness evidence from strength evidence and reports both rescue and regression counts.
