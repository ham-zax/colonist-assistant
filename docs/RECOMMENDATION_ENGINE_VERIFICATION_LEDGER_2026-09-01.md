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
