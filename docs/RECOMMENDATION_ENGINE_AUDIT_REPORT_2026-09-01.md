# Colonist Assistant Recommendation Engine Audit Report

Date: 2026-09-01
Repository: `/home/hamza/repo/colonist-assistant`
Branch: `main`
Reviewed HEAD: `aebe9ede07ae5f80cc683a6252efef7a7e209f6f` (`Fix belief search integrity`)
Live engine: `deep-maxn-v9`

This is the concise entry point for the recommendation-engine audit. The complete evidence, reproductions, source paths, refutation attempts, strategic controls, and benchmark notes are in [`END_TO_END_RECOMMENDATION_REVIEW_2026-09-01.md`](./END_TO_END_RECOMMENDATION_REVIEW_2026-09-01.md). Runtime/build observations are in [`RUNTIME_RECOMMENDATION_FINDINGS_2026-09-01.md`](./RUNTIME_RECOMMENDATION_FINDINGS_2026-09-01.md). The execution-ready repair sequence is in [`RECOMMENDATION_ENGINE_IMPLEMENTATION_PLAN_2026-09-01.md`](./RECOMMENDATION_ENGINE_IMPLEMENTATION_PLAN_2026-09-01.md).

This report remains the evidence ledger. The implementation plan is separate so repair decisions do not rewrite the original reproduction record.

## Verdict

`deep-maxn-v9` is not yet reliable enough to claim that its displayed recommendation is the strongest defensible action given the information available to the player.

The main problem is not missing generic Catan strategy. The engine already models topology, production, ports, resource scarcity, development-card value, Longest Road, Largest Army, robber pressure, discard risk, expansion paths, and whole-turn continuations. The failures reproduced in this audit are earlier or more structural: belief probabilities can be wrong, public state can be reconstructed incorrectly, legal Colonist actions can be absent, simulated opponents can use information they do not know, search traversal can starve strategically equivalent continuations, belief compression can change the action family, and the UI can bypass Rust authority in an accepted trade.

## Confirmed findings

| ID | Severity | Status | Failure |
| --- | --- | --- | --- |
| F1 | Major | Reproduced | `resampleDegenerateWorlds()` changes posterior probability mass and can flip an exact mandatory trade decision. |
| F2 | Major | Reproduced | Incoming active-trade `give/receive` orientation is reversed at tracker reconciliation; the same visible offer can also be applied repeatedly. |
| F3 | Major | Reproduced | Missing public development-play history can manufacture a phantom development deck and make impossible `BuyDevelopment` appear legal. |
| F4 | Major | Reproduced | Rust exact authority can choose `CancelTrade` while the overlay fast-confirm path executes `ConfirmTrade`, including a trade that gives an opponent an immediate win. |
| F5 | Major | Reproduced | Native domestic-trade generation rejects Colonist-legal bundles above two cards, so legal `3 -> 1` / `4 -> 1` player offers can never be recommended. |
| F6 | Significant | Reproduced | Domestic-trade roots do not propagate planner completion, so the intended whole-turn planner adjustment never protects those roots. |
| F7 | Major | Reproduced | Deep MaxN opponent policy is not information-set-consistent; an opponent can choose differently based only on hidden third-party cards it cannot observe. |
| F8 | Major | Reproduced | The engine can choose `EndTurn` despite a verified legal settlement that prevents an otherwise immediate opponent win. |
| F9 | Significant | Measured | Root width 8 can prune an action that becomes preferred at width 12; adding nodes alone cannot recover a root that never entered the tree. |
| F10 | Minor | Source-proven | Opponent `playedDevelopmentThisTurn` is reconstructed as false, weakening the one-development-card-per-turn state contract for simulated opponents. |
| F11 | Blocker | Reproduced on captured turn 54 | Same-turn recursive search shares one node prefix across ranked siblings. Earlier domestic-trade branches can starve a later commutative continuation and make a dominated order look better. |
| F12 | Major | Reproduced | Midgame public-board fallback seeds opponents with empty hands; reconciling real public hand sizes can collapse all resource worlds to zero and abort Deep Search. |
| F13 | Minor | Reproduced | `finalActionSource` can label a deep-selected action as `placement-heuristic` or `coach-goal`, making traces unreliable for authority diagnosis. |
| F14 | Major | Reproduced | The live 12-particle strategic coreset can change the final action family from `PlayMonopoly(Grain)` on the full 24-world posterior to `EndTurn`; the split persists through 128k nodes. |
| F15 | Minor | Reproduced | Monopoly family replacement can duplicate an identical root action, producing impossible `legal_weight = 2.0` on a one-particle posterior and wasting root budget. |

## Highest-confidence causal failures

### Belief state can be wrong before search

F1, F2, F3, F10, and F12 are state/input failures. A stronger evaluator or a deeper search cannot repair them because the search is being asked to solve the wrong posterior or wrong public state.

The most direct examples are:

- F1: a roughly `0.10%` tail was inflated to `8.33%` by posterior rejuvenation, and the exact accepted-trade decision changed;
- F2: a 50/50 rival hand prior became roughly `98.59%` probability on the wrong resource, then `99.98%` after the unchanged offer was conditioned again;
- F3: the adapter reconstructed six drawable development cards when the public deck was actually empty;
- F12: a recovered midgame snapshot produced `24 -> 0` resource worlds during public reconciliation.

### Search does not preserve the real decision problem

F7, F8, F11, F14, and F15 are search-contract failures.

F11 is the clearest real recommendation reproduction. On recovered turn 54, the engine recommends spending four brick for one lumber before buying a development card. For every possible development-card draw, the sequences

```text
MaritimeTrade -> BuyDevelopment -> Resolve(card)
BuyDevelopment -> Resolve(card) -> MaritimeTrade
```

reach the same state and evaluation. Buying first additionally reveals the card before deciding whether the maritime trade is still useful. The engine nevertheless prefers the less-informed order because an earlier domestic-trade subtree consumes shared same-turn search budget and starves the later maritime sibling after the development draw.

F14 establishes a separate approximation failure. On a valid 24-world posterior where every world has the same root observation, full-posterior search chooses `PlayMonopoly(Grain)` while the production 12-world coreset chooses `EndTurn`. At 128k nodes the full-posterior value gap between Monopoly and EndTurn is about `0.027` normalized value, and increasing nodes does not remove the split.

### Final authority is not single-source

F4 proves that a correct Rust exact decision is not sufficient. The overlay can fast-confirm an accepted trade before normal deep action mapping. A reproduced valid position had:

```text
Rust exact: CancelTrade
ConfirmTrade threat: ImmediateWin
overlay fast-confirm path: can execute ConfirmTrade
```

The final recommendation/execution architecture therefore needs an explicit authority contract; it cannot rely on "Rust searched it correctly" as proof that the user sees or executes that action.

## Colonist-specific rule conclusions

Colonist, not generic tabletop CATAN, is the product authority for this audit.

Confirmed:

- Colonist allows arbitrary-size player trade bundles as long as the same resource type is not traded both ways.
- Colonist's own trade-system documentation explicitly gives offering four sheep to opponents as a supported example.
- F5 therefore represents a real Colonist action-space omission.

Not yet classified as defects:

- whether Colonist permits another completed domestic player trade after one has already completed in the same turn;
- whether an offer can be restricted to a selected recipient rather than broadcast to all eligible opponents.

Those two earlier hypotheses were removed from the confirmed defect set because product-specific behavior has not yet been proven.

## What passed adversarial controls

The audit did not support the claim that these mechanisms are absent:

- board topology round-trips through the adapter without observed vertex/edge/building ownership loss;
- ports materially affect policy/evaluation and can outrank a higher-pip non-port placement;
- road priors distinguish productive continuation edges from dead roads;
- robber logic can target a 9-VP city-ready leader over a stronger raw-production victim;
- development-card marginal value responds to remaining deck composition and Largest Army context;
- resource production, scarcity, seven/discard exposure, Longest Road, Largest Army, expansion paths, and bank shortage are represented in the evaluator;
- every reconstructed WASM particle is validated by `GameState::validate()` before strategic search.

These positive controls are why broad evaluator retuning is not the first diagnosis.

## Measured sensitivities that should remain separate from confirmed root causes

- Root width 8 versus 12 changes at least one recommendation even when additional nodes at width 8 do not.
- Hidden-development sampling changed the selected root across deterministic seeds (`BuyDevelopment` in 5/8 runs, domestic trade in 3/8).
- The strategic coreset drops additional low-probability road/Knight distinctions beyond the direct F14 reproduction.
- A 350 ms depth deadline can still occur, but the recovered turn-54 failure persists without deadline exhaustion, so timeout is not its root cause.

These measurements matter for later calibration, but they should be re-benchmarked after the correctness defects are repaired.

## Dependency groups carried into the implementation plan

These are the causal groups used by the separate implementation plan:

1. **Information state:** F1, F2, F3, F10, F12.
2. **Colonist action and authority:** F4, F5, F6.
3. **Strategic search:** F7, F8, F11, F14, F15; then re-evaluate F9 and hidden-development sampling.
4. **Auditability:** F13 and the missing decision-trace provenance described in the detailed report.

The implementation plan turns these groups into six ordered waves, with state correctness first and search calibration deferred until the confirmed correctness defects are closed.

## Verified v10 closure status

This section is an additive closure record; the original audit evidence above is intentionally preserved.

Task 16 packaged/release verification completed against the repaired `deep-maxn-v10` implementation. The focused reproductions for the confirmed defects are green at their relevant TypeScript/Rust/WASM boundaries, F9 has the Task 15 evidence-backed disposition of no material live-width omission with root width 8 retained, and the repository-wide `npm run verify` completed with exit 0.

The release proof also exercised the real extension in Windows Edge. Midgame attachment initially exposed an additional F12 release-path manifestation where a partially reconstructed tracker could reconcile to zero worlds despite a physically consistent public board; the recovery path now reseeds from the authoritative public snapshot and the live game proceeds into WASM search. Accepted outgoing-trade proof then exposed a content-side executor defect where `CancelTrade` could resolve to a rejected player's inert X; that resolver now selects the active cancel control, while `ConfirmTrade` continues to select the enabled accepted-player check. The user confirmed the rebuilt live workflow works.

A targeted packaged replay of only the recovered turn-54 fixture selected `BuyDevelopment` under `deep-maxn`. The live run reported 102 ranked roots, 8 retained roots, the selected root at rank 4 with prior `0.41727426648139954`, and consistent source/WASM/Rust particle counts of `1/1/1/1`. The targeted Task 14 gate passed and Task 15 still reported no material F9 omission.

The intentionally interrupted expensive whole-corpus CPU medium/max replay was not restarted and produced no result to reinterpret. The committed Task 15 calibration remains authoritative; future high-throughput reference sweeps belong on the GPU path after parity.

As a release-adjacent user control requested during Task 16, the packaged extension also includes `Disable player trades` with a default of off. It maps to the existing engine `player_trades_enabled` rule seam, invalidates stale decisions on toggle, prohibits assistant offer/accept/counter/confirm behavior while allowing rejection/cancellation cleanup, and preserves bank/port maritime trades. This control does not change the audit's search weights or strategy semantics.
