# Recommendation Engine v10 Fix Verification

Date: 2026-09-01
Branch: `recommendation-v10-agent-b`
Integration base: `7c91f69` (`Integrate recommendation v10 Tasks 4-12`)

This ledger records only evidence produced against the integrated v10 repair path. Historical reproduction evidence remains in the audit reports and is not rewritten here.

## Task 13 - Authority and search provenance

Status: **Passed**
Checkpoint: `9e8b48d` (`Record decision authority and search provenance`)

The decision trace now separates:

- tracker/source resource-world count;
- final TypeScript-to-WASM particle count;
- Rust pre-coalescing posterior count;
- Rust exact-distinct searched count;
- pre-truncation root rank and prior;
- retained roots and per-particle/total node allocation;
- whole-turn planner value and `completion_mass` where a plan exists;
- root exclusion, branch truncation, trade-safety, and exact-family-collapse reasons;
- initial Rust authority, exact-family arbitration, and safety replacement;
- TypeScript mapping failure and fresh-state execution failure from Rust authority.

`DecisionActionSource` records the explicit Rust authority values: `exact-mandatory`, `tactical-proven`, `deep-maxn`, `exact-family`, or `safety-override`. TypeScript does not substitute a different strategic candidate.

Validation:

| Command | Result |
| --- | --- |
| `npm test -- tests/deep-search-adapter.test.ts tests/trade-guard.test.ts` | **22/22 passed** |
| `npm test -- tests/decision-trace.test.ts` | **4/4 passed** |
| `npm run check` | **Passed** |
| `npm run build:wasm` | **Passed** |

## Task 14 - Joint tracker-to-WASM posterior

Status: **Gate passed**

### Sampling contract

The live posterior has one lossy boundary: the final joint sample, with a default limit of 24 particles. The same constructor exposes an offline-only `particleLimit` seam for 24/48/96 comparisons.

- Exact duplicate tracker resource worlds are merged by exact hand identity and their weights are summed.
- If resource worlds are already complete, no hidden development identity exists, and the posterior fits the requested limit, exact resource worlds and weights are preserved.
- Otherwise each final stratum carries `1/N` mass before exact duplicate final particles are merged.
- Resource-world selection, resource-card completion slots, and development-card slots use deterministic independent stratified dimensions derived from the semantic request seed.
- Omitted mass is never assigned to a nearest different resource state.
- All known opponent resource cards are reserved before missing slots are sampled, then missing cards are drawn uniformly without replacement from the residual physical pool.
- Hidden development slots are canonicalized as ready versus bought-this-turn slots. Card identity is assigned directly to each slot uniformly without replacement; an identity assigned to a bought slot is unplayable in that particle.
- There is no hidden-development `MAX_PARTICLES / 2` resource pre-cap and no second downstream live resampling step.

### Fixed audit corpus

`tests/fixtures/recommendation-audit-corpus.json` contains:

- recovered turn-54 control;
- reconstructed hidden-development seed-stability control;
- F8 forced-blocker positive control;
- frozen F14 24-world Monopoly reconstruction;
- accepted-trade exact control;
- large-bundle domestic-trade control;
- port/maritime control;
- road control;
- robber mandatory control.

Historical limitation: the repository never retained the original R2 hidden-development fixture that produced the old 5/8 `BuyDevelopment` versus 3/8 domestic-trade split. The replacement seed-stability control is explicitly marked `reconstructed-control`: it uses the recovered turn-54 race with nine hidden opponent development slots and does not claim to reconstruct the missing identities of that historical R2 state.

The recovered turn-54 repository evidence also did not retain a complete historical piece layout. The corpus uses the documented resources/bank/visible scores and a deterministic city reconstruction so the strict packaged WASM public-score validator accepts the state. F14 uses the frozen deterministic 24-world reconstruction established by Task 12.

### 24/48/96 bounded-regret gate

Search settings for all three particle limits are identical: depth 4, branch cap 8, 4,000 nodes, 350 ms.

The live 24-particle boundary is unsafe when either:

1. the 24-particle action family differs from both the 48- and 96-particle results; or
2. the live chosen root is more than `0.02` below the chosen root in both larger references under the larger reference's own backed-up values.

Observed result:

| Fixture | 24 | 48 | 96 | Regret vs 48 | Regret vs 96 | Gate |
| --- | --- | --- | --- | ---: | ---: | --- |
| recovered-turn-54 | `buy-development` | `buy-development` | `buy-development` | 0 | 0 | Pass |
| hidden-dev-seed-stability | `buy-development` | `buy-development` | `buy-development` | 0 | 0 | Pass |
| f8-forced-blocker | `build-settlement` | `build-settlement` | `build-settlement` | 0 | 0 | Pass |
| f14-monopoly-posterior | `play-monopoly` | `play-monopoly` | `play-monopoly` | 0 | 0 | Pass |
| accepted-trade-exact | `confirm-trade` | `confirm-trade` | `confirm-trade` | 0 | 0 | Pass |
| large-bundle-domestic-trade | `build-road` | `build-road` | `build-road` | 0 | 0 | Pass |
| port-maritime-control | `maritime-trade` | `maritime-trade` | `maritime-trade` | 0 | 0 | Pass |
| road-build-control | `offer-trade` | `offer-trade` | `offer-trade` | 0 | 0 | Pass |
| robber-move-control | `move-robber` | `move-robber` | `move-robber` | 0 | 0 | Pass |

The reconstructed hidden-development control was additionally run across eight deterministic sampling seeds at the live 24-particle limit. Result: **8/8 `buy-development`**, one distinct chosen action.

Particle-layer checks from the replay:

- exact one-world controls remain one particle at 24/48/96;
- F14 remains the exact 24-world posterior at 24/48/96;
- the hidden-development control constructs 24, 48, and 96 particles respectively and Rust searches the corresponding exact-distinct posterior (24 at the live boundary).

Gate result: **PASS**. Task 15 is permitted to begin.

Validation:

| Command | Result |
| --- | --- |
| `npm test -- tests/deep-search-adapter.test.ts` | **21/21 passed** |
| `npm run build:wasm` | **Passed** |
| `npm run replay:decisions -- tests/fixtures/recommendation-audit-corpus.json --rerun` | **Passed; Task 14 gate passed, 0 failed fixtures** |
| `npm run check` | **Passed** |

## Task 15 - Root-width calibration

Status: **Closed with root width 8 retained.**

The frozen Task 14 corpus was re-run with the exact final 24-particle posterior semantics under the three prescribed search configurations:

| Configuration | Depth | Root width | Node cap | Time cap |
| --- | ---: | ---: | ---: | ---: |
| live | 4 | 8 | 4,000 | 350 ms |
| reference-medium | 6 | 32 | 64,000 | 10,000 ms |
| reference-max | 6 | 32 | 250,000 | 10,000 ms |

The replay records, per fixture, the pre-truncation rank/prior of the selected root, whether it was admitted, its backed-up value, action family, node count, deadline flag, latency, and the live-root regret measured in each reference search.

Disposition:

- **No material F9 omission was found.** No concrete action outside the live top eight became the selected action in both reference-medium and reference-max on the same fixture.
- Recovered turn 54 remained `BuyDevelopment`; the stable reference action ranked 4th live with prior `0.417274...` and was admitted.
- F8 remained `BuildSettlement`; the stable reference action ranked 1st live with prior `0.593735...` and was admitted.
- F14 remained `PlayMonopoly`; the stable reference action ranked 7th live with prior `0.085404...` and was admitted.
- The large-bundle control's stable `BuildRoad` reference ranked 1st live and was admitted.
- The accepted-trade and robber controls are exact/mandatory paths, so root-width admission is not applicable.
- The port references selected different concrete maritime parameterizations, and the road control's medium/max references disagreed (`OfferTrade` versus `BuildRoad`). Neither satisfies the stable-reference omission criterion.
- Where both reference searches backed up the live-selected root, measured live-root regret was zero on the recovered turn-54, hidden-development, F8, F14, accepted-trade, large-bundle, and robber controls. The port max reference measured `0.01043`, below the Task 14/15 material threshold. The road control showed reference regret (`0.08190` medium / `0.06210` max), but the two references did not select the same action, so it is not a stable F9 admission failure.
- Several depth-6 references reached the prescribed 10 s cap before exhausting their node ceilings; this is retained as search-cost evidence rather than hidden by increasing the benchmark budget.

Therefore F9 is closed without a production root-rescue rule, progressive widening change, or root-width increase.

Validation:

| Command | Result |
| --- | --- |
| `npm run build:wasm` | **Passed** |
| `npm run replay:decisions -- tests/fixtures/recommendation-audit-corpus.json --rerun` | **Passed; Task 14 gate passed and Task 15 reported 0 material F9 fixtures** |
| `npm run check` | **Passed** |

## Task 16 - Packaged integration

Status: Pending Task 15 checkpoint.
