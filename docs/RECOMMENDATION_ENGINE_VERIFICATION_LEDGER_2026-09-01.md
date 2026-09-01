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
