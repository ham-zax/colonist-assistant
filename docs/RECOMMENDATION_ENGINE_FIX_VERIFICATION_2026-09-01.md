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

Status: **Passed.**

Task 16 integrated the v10 correctness waves into the packaged extension and closed the browser/runtime authority path.

### Packaged/runtime identity

- Search engine revision: `deep-maxn-v10`.
- Windows Edge loaded the unpacked extension from `/home/hamza/repo/colonist-assistant-v10-b/dist` and displayed the Agent B build identity.
- Live Edge evidence showed the in-game Colonist Assistant attached to a real game and executing the packaged background WASM path.
- The midgame attach regression discovered during release proof (`worlds: []` after public reconciliation) was repaired at the public-board recovery boundary. A live reload subsequently reached WASM search instead of either `Deep Search has no resource worlds consistent with public evidence` or `Deep Search could not construct a resource world consistent with public evidence`.
- Accepted outgoing-trade execution was revalidated after repairing the content control resolver that could otherwise target a rejected player's inert X instead of the active cancel control. The user confirmed the rebuilt Edge package completed that workflow correctly.
- The release build also exposes `Disable player trades` (default off). When enabled it maps to `playerTradesEnabled = false`, invalidates stale decision identity, preserves decline/cancel cleanup, blocks player offer/accept/counter/confirm execution, and leaves bank/port maritime trades legal.

### Targeted packaged turn-54 replay

The replay script has no fixture filter, so Task 16 extracted only `recovered-turn-54` into a temporary one-fixture corpus and ran the packaged rerun against that file. This is not the prohibited whole-corpus medium/max CPU replay.

Result:

- chosen action: `buy-development`;
- authority: `deep-maxn`;
- source worlds: `1`;
- constructed/WASM/Rust-posterior/Rust-search particles: `1 / 1 / 1 / 1`;
- pre-truncation roots: `102`;
- retained roots: `8`;
- selected root rank/prior: rank `4`, prior `0.41727426648139954`;
- selected root node allocation: `699` nodes in the live run;
- selected root planner completion mass: `0.034596070647239685`;
- Task 14 bounded-regret gate: passed at 24/48/96;
- Task 15 disposition: no material F9 omission, root width 8 retained.

The single-fixture reference-medium and reference-max controls also chose `buy-development`; they were bounded to this one captured state and did not restart the expensive corpus sweep.

### Final verification

| Command / evidence | Result |
| --- | --- |
| Focused Task 16 TypeScript/WASM regressions, including midgame recovery, accepted-trade control resolution, and no-player-trades legality | **Passed** |
| `npm run verify` on the final implementation tree | **Passed (exit 0)** |
| Targeted packaged `recovered-turn-54` rerun | **Passed; `buy-development`, `deep-maxn`** |
| Live Edge midgame attach/reload | **Passed** |
| Live Edge accepted outgoing-trade workflow after resolver repair | **Passed by user confirmation** |
| Fresh-state action validation | **Retained; player-trade toggle is also part of decision identity and execution legality** |

### CPU/GPU replay note

The previously attempted expensive final whole-corpus CPU replay was intentionally interrupted before producing a report. It was **not** restarted for Task 16. Task 15's committed calibration evidence remains authoritative. Future high-throughput reference sweeps should use the GPU path after parity is established; Task 16 used only the bounded one-fixture turn-54 replay described above.
