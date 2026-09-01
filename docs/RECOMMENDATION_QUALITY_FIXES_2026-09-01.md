# Recommendation quality fixes — 2026-09-01

This change ports the evidence-backed recommendation fixes from the reconstructed `colonist-asistant` repository into the original `colonist-assistant` codebase.

Source investigation:

- reconstructed repo commits `ccca345` and `10b7adc`
- `docs/algorithm-tdd-red-phase-2026-09-01.md`
- `docs/recommendation-quality-findings-2026-09-01.md`
- `docs/recommendation-quality-investigation.md`

The port was done behavior by behavior instead of cherry-picking the reconstructed implementation. The original repository contains the Rust search engine, so rule variants that previously required a TypeScript fallback are now modeled in the native engine.

## Fixed behavior

### Deep Search state adapter

`src/worker/deep-search.ts` now fails closed instead of silently manufacturing a valid-looking state when public evidence is inconsistent.

It now:

- rejects an unknown root player instead of optimizing seat 0;
- rejects an unknown current player instead of advancing seat 0;
- rejects games above four players instead of truncating the player list;
- rejects unknown trade creators and response participants;
- rejects unresolved edge endpoints, adjacent vertices, and adjacent hexes;
- requires exactly one blocked robber hex instead of defaulting to hex 0;
- reconstructs `robberReturnPhase` as `pre-roll` when a Knight was played before rolling;
- encodes every public discard obligation during a discard phase, not only the root player's count;
- stops hidden-hand sampling when the resource pool is exhausted instead of inventing cards beyond the standard 19-card supply;
- merges public `playedKnights` evidence with tracker development-card evidence;
- carries the live `cardDiscardLimit` and `friendlyRobber` rules into the WASM request.

### Native rule support

The original Rust engine now owns the two live rules that the reconstructed repository could not model.

`GameState` now carries:

- `card_discard_limit`;
- `friendly_robber`.

The discard limit is used consistently by:

- seven-roll discard resolution;
- domestic-trade candidate generation;
- counter-trade hand scoring;
- hand transition evaluation;
- expected discard-loss evaluation;
- root safety logic;
- search features;
- depth-search trade friction;
- policy priors and root quotas;
- learned trade-model features.

Friendly Robber is enforced in `catan-core` legal robber-action generation. A robber destination is illegal when any building adjacent to that hex belongs to a player below three public victory points. Because the rule is enforced in `GameState::legal_actions()`, exact search, MaxN, AlphaBeta, PUCT, and rollouts all use the same legality contract.

The WASM request boundary in `engine/crates/catan-wasm/src/lib.rs` accepts both rule fields. The engine revision is now `deep-maxn-v8`.

### Recommendation and UI state correctness

The TypeScript recommendation layer now:

- uses the engine's explicit `rootIndex` for candidate values, trade fallback ranking, and persisted decision traces;
- includes `eventCount`, robber position, rule settings, opponent policy posterior, and posterior worlds in the decision cache signature so materially different search states cannot reuse stale analysis;
- resolves generic `Dice rolled` log entries by unique player color instead of creating a phantom player named `Dice`;
- uses the live discard limit in coaching, heuristic simulation, and trade hand-risk scoring;
- treats a public Longest Road value of zero as potentially stale and takes the maximum of public evidence and exact board-graph length;
- filters Friendly Robber placements when any protected player occupies the target hex;
- preserves the valid one-second autopilot delay setting.

### Strategic particle determinism

Adding the new rule fields to `GameState::state_hash()` exposed an existing ordering defect in `select_strategic_particles()`.

Particles with identical strategic signature and identical state hash but different posterior weights had no canonical final sort key. Stable sort therefore inherited input order, and reversing the posterior could change systematic-sampling strata.

The selector now uses posterior weight as the final canonical tie-break. The existing permutation-invariance regression test passes again.

## Intentionally not ported

The reconstructed repo disabled Deep Search when `cardDiscardLimit != 7` or Friendly Robber was enabled. That fallback is not ported because the original repository contains the native Rust source and now models both rules directly.

The reconstructed representative-world slot-fill fix was also not copied blindly. The equivalent regression already passed in the original repository before production changes, so no TypeScript change was required for that behavior.

## Regression coverage

`tests/recommendation-integrity.test.ts` captures the mapped recommendation failures in the original repository, including:

- fail-closed player, trade, topology, and robber reconstruction;
- pre-roll robber continuation;
- public discard obligations;
- visible resource-supply conservation;
- live rule propagation into the native request;
- cache-key invalidation for seed/event changes, robber movement, opponent-policy changes, and posterior-weight changes;
- phantom `Dice` prevention;
- discard-limit-aware coaching;
- Friendly Robber placement legality;
- stale Longest Road correction.

Native regressions cover custom discard-limit resolution, Friendly Robber legality, discard-risk evaluation under a custom limit, and strategic-particle permutation invariance.

## Verification

Verification performed after the port:

```text
cargo test -p colonist-catan-core --lib
24 passed, 0 failed

cargo test -p colonist-catan-search --lib
78 passed, 0 failed

cargo test -p colonist-catan-wasm
passed

npm test
20 files passed
172 tests passed

npm run check
passed

npm run build:wasm
passed; packaged engine rebuilt as deep-maxn-v8

npm run build
passed; extension bundle rebuilt with the new WASM engine
```

The first mapped RED run reproduced 12 of 13 selected recommendation failures in the original repository. The only non-reproducing case was representative-world slot filling, which was already correct in this codebase.
