# Recommendation Engine Correctness Verification Ledger

Date: 2026-09-01
Repository: `/home/hamza/repo/colonist-assistant`
Baseline evidence commit: `ffc92b8` (`Freeze v9 recommendation evidence`)

This ledger records task-level correctness evidence for the implementation plan. Historical audit reproductions remain in their original reports; this file records whether each reproduction is closed and why.

## Task 1 - Remove posterior-changing resource resampling

**Status:** PASS - F1 closed

**Production owner:** `src/core/tracker.ts`

**Change:**

- removed ESS-triggered equal-weight rejuvenation;
- when unique worlds are within `MAX_WORLDS`, retain the normalized weighted posterior exactly;
- when compaction is required, use deterministic systematic resampling across all 4,096 strata, preserve repeated selections as multiplicity, and merge repeated selected states by their `1 / 4096` sample mass;
- removed min/max tail reservation/support injection;
- `reweightTradeEvidence()` now normalizes evidence-updated weights without resampling;
- `effectiveParticleCount()` remains a diagnostic of the actual weighted posterior.

**Original F1 reproduction:** before the change, the focused regression reproduced the audit corruption directly: expected supported accepted-trade mass `0.9889001`, but tracker rejuvenation returned `0.9166666667`, promoting a roughly `0.00100908` tail to `1/12 = 0.0833333` mass. After the change the same regression retains `0.9889001 / 0.0110999` Bayesian mass.

**Decision consequence:** `engine/crates/catan-search/src/exact.rs` contains a focused exact mandatory trade fixture proving that the Bayesian weighting chooses `ConfirmTrade { partner: 1 }` while the rejuvenated `0.9166667 / 0.0833333` weighting chooses `CancelTrade`. The production adapter regression separately proves that the corrected Bayesian accepted-trade mass crosses the TypeScript -> WASM boundary unchanged. The original raw historical accepted-trade fixture that produced the audit's exact numeric scores is no longer available as a retained artifact, so it was not reconstructed or silently substituted.

**Acceptance evidence:**

- rare-tail Bayesian mass preserved: PASS;
- repeated normalization with no new evidence is probability-idempotent: PASS;
- total posterior mass after >4,096-world compaction is 1: PASS;
- dominant 0.99 mass after systematic compaction stays within one `1/4096` quantum: PASS;
- adapter -> WASM preserves accepted-trade supported mass `0.9889001`: PASS;
- exact trade solver weight-sensitivity regression: PASS.

**Focused validation:**

```text
npm test -- tests/tracker-posterior-integrity.test.ts tests/deep-search-adapter.test.ts
18 tests passed

cargo test -p colonist-catan-search accepted_trade_decision_is_sensitive_to_rejuvenated_tail_mass
1 test passed

cargo fmt --all -- --check
green after formatter application

git diff --check
green
```

**Task boundary:** closed. This ledger entry is included with the Task 1 implementation commit; Task 2 remains a separate worktree state and must not be folded into this commit.

## Task 2 - Migrate active trades to one creator-relative contract

**Status:** PASS - F2 closed

**Canonical contract:** `ActiveTradeOffer` stores only `creatorGive` and `creatorReceive`. Local-user `give`/`receive` are derived with `localTradeBundles()` at UI/evaluation boundaries; no compatibility aliases remain.

**Original F2 reproduction:** before the migration, the focused incoming-offer fixture (`Rival gives lumber / User gives brick`) conditioned the wrong hidden world: `P(Rival has lumber) = 0.0140845`. The tracker diff interpreted the user-relative stored `give` vector as creator-owned resources.

**Change:**

- `bridge.ts` now copies Colonist `offeredResources` -> `creatorGive` and `wantedResources` -> `creatorReceive` directly;
- active-trade snapshots/diff evidence remain creator-relative through offer/accept/reject/counter/expire events;
- `localTradeBundles()` derives incoming/outgoing local orientation without storing a second representation;
- the WASM adapter consumes the same creator-relative contract directly;
- runtime validation and all internal fixtures/callers migrated atomically with no `give`/`receive` aliases;
- `overlay.reconciledState()` no longer reapplies visible active-trade Bayesian evidence. The active-trade snapshot diff/session ingestion path is the single owner of panel-derived evidence.

**Acceptance evidence:**

- incoming Rival-lumber offer now drives Rival-lumber posterior above 0.98 and Rival-brick below 0.02: PASS;
- re-diffing an unchanged active-trade snapshot emits no event: PASS;
- open -> accept, reject, and counter transitions each emit exactly one corresponding tracker event: PASS;
- local view helper preserves incoming/outgoing UI bundle semantics: PASS;
- TypeScript contract migration has no obsolete `ActiveTradeOffer.give`/`receive` caller: PASS (`tsc --noEmit` clean and bounded caller sweep);
- duplicate overlay Bayesian update removed: PASS.

**Focused validation:**

```text
npm test -- tests/trade-belief-board-diff.test.ts tests/tracker-posterior-integrity.test.ts tests/trade-guard.test.ts
20 tests passed

npm run check
green

git diff --check
green
```

**Task boundary:** implementation and focused validation complete. Independent integration review reproduced the focused validation and found no blocking Task 2 defect; this ledger entry is included with the dedicated Task 2 commit.

## Task 3 - Migrate public development history to one authoritative vector

**Status:** PASS - F3 and F10 closed

**Canonical contract:** `BoardPlayerPublicState` stores public development-card history only as `playedDevelopmentCards` and exposes `hasPlayedDevelopmentThisTurn` per player. Knight counts used by strategy/evaluation and the Rust request are derived from `playedDevelopmentCards.knight`; no public `playedKnights` authority remains.

**Change:**

- `bridge.ts` maps every Colonist `developmentCardsUsed` entry through the existing development-card identity mapping and captures every player's `hasUsedDevelopmentCardThisTurn`;
- public development evidence is merged per player and per card by taking the maximum of tracker history and the public snapshot;
- `basePlayers` propagates opponent development-play turn state while preserving the exact local flag when available;
- development-world reconstruction subtracts public plays and the exact local held cards from the physical 25-card deck before assigning hidden opponent cards;
- impossible public plays, exact local holdings, or hidden-card totals now throw an explicit development-card state-integrity error instead of clamping or silently underfilling the deck;
- coach, engine, strategy, development, trade, overlay, validators, and fixtures derive Knight/history values from the authoritative public vector.

**Acceptance evidence:**

- 10 public Knights + 2 Monopoly + 2 Road Building + 2 Year of Plenty + 9 hidden held development cards produces `playedDevelopment = [10, 0, 2, 2, 2]` and an empty deck in every generated world: PASS;
- packaged WASM exposes no `buy-development` action when the real deck is empty: PASS;
- opponent `hasPlayedDevelopmentThisTurn = true` reaches Rust as `playedDevelopmentThisTurn = true`: PASS;
- generated development worlds conserve all 25 physical cards: PASS;
- an impossible extra hidden card fails with an explicit state-integrity error: PASS;
- public per-type counts above physical deck composition fail explicitly: PASS;
- stale-field sweep found no remaining `BoardPlayerPublicState.playedKnights` consumer: PASS. Remaining `playedKnights` symbols are derived metrics or the existing Rust/WASM wire field.

**Focused validation:**

```text
npm test -- tests/deep-search-adapter.test.ts
16 tests passed

npm run check
green

git diff --check
green
```

**Task boundary:** implementation and focused validation complete. Integration review found no blocking Task 3 defect and confirmed no Task 4 implementation was included; this ledger entry is included with the dedicated Task 3 commit.
