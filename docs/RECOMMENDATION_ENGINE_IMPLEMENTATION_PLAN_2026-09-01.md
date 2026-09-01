# Recommendation Engine Correctness Implementation Plan

**Goal:** Make the displayed Colonist Assistant recommendation defensible against the complete public information state by repairing the confirmed state, legality, authority, and search-contract defects before tuning strategic weights or performance.

**Architecture:** Preserve one canonical Colonist public-state contract from the page bridge through the tracker and WASM boundary. Keep weighted hidden-state beliefs mathematically faithful, make exact/proven safety decisions authoritative through execution, and make every recursive simulated decision information-set-consistent. Rust must search every distinct particle supplied in a WASM request without further lossy compression. The TypeScript tracker-to-WASM particle boundary remains an explicit approximation and must use a mathematically defined sampler plus a bounded-regret gate rather than nearest-representative mass reassignment.

**Tech Stack:** TypeScript Chrome extension, Vitest, Rust (`colonist-catan-core`, `colonist-catan-search`, `colonist-catan-wasm`), `wasm-bindgen`, packaged WASM, existing replay/benchmark scripts.

**Strength/evidence plan:** Before changing recommendation semantics, freeze the `deep-maxn-v9` baseline described in [`RECOMMENDATION_ENGINE_BEFORE_AFTER_EVIDENCE_PLAN_2026-09-01.md`](./RECOMMENDATION_ENGINE_BEFORE_AFTER_EVIDENCE_PLAN_2026-09-01.md). That plan measures whole-game strength and comeback ability before/after this correctness work; it does not replace the finding-specific correctness gates below.

## Global Constraints

- Colonist behavior is the rules authority. Do not implement generic tabletop assumptions that have not been verified against Colonist.
- F5 covers the confirmed Colonist mismatch: legal large domestic-trade bundles. Do **not** change same-turn multiple-completed-trade behavior or recipient targeting until Colonist-specific evidence establishes those contracts.
- Do not tune evaluator constants, promote learned strategic models, or add Reddit-derived heuristics while correctness findings F1-F15 are open. The audit already showed that ports, topology, production, roads, robber pressure, trophy races, development value, and discard risk are materially modeled.
- `GameState::validate()` remains mandatory for every WASM particle. No repair may bypass resource/development conservation to keep a search alive.
- Weighted beliefs must preserve posterior probability mass. Low effective sample size is allowed to remain low when that is what the evidence implies. Any finite fallback/sampling approximation must name the conditional distribution it samples and retain explicit sample weights.
- A simulated player may condition its policy only on that player's observation. Hidden third-party cards may affect outcome values, but not the action policy chosen for an observation-identical information set. This applies to every recursive decision, including a later same-turn decision by the original root player; only the externally aggregated initial root action is exempt because it is selected before recursive search begins.
- Exact mandatory decisions and proven safety/tactical decisions are final authorities. TypeScript may fail closed or request a fresh search when an action cannot be executed, but it may not silently replace an authoritative Rust action with a conflicting action.
- Root action lists must contain unique actions. `legal_weight` and `availability_weight` must stay in `[0, 1]`.
- When the posterior makes an immediate opponent win certain and a legal root action provably removes that certain loss, the losing action cannot outrank the verified blocker.
- Same-turn search value must not depend materially on sibling traversal order or allow an early subtree to consume the entire budget needed by a later equivalent continuation.
- Preserve the existing build-identity changes and unrelated dirty source work. Do not rewrite `scripts/build.mjs`, `src/popup/index.ts`, or unrelated overlay settings code as part of these fixes.
- The earlier request for TDD-style algorithm regressions remains in force. Each reproduced defect gets a focused regression before its implementation is considered complete; broad test creation unrelated to a finding is out of scope.
- Search regressions that close live findings must exercise production-style constraints (`depth = 4`, `branchCap = 8`, `maxNodes = 4,000`) unless the finding concerns a different configured path. Generous-budget tests may supplement but cannot substitute for the live-budget closure case.
- Keep implementation waves independently reviewable. Do not mix strategic tuning into state-contract patches.

## Finding-to-Task Map

| Finding | Implementation task |
| --- | --- |
| F1 posterior mass corruption | Task 1 |
| F2 trade orientation + duplicate evidence | Task 2 |
| F3 phantom development deck | Task 3 |
| F4 Rust cancel / UI confirm authority violation | Task 5 |
| F5 large Colonist domestic bundles omitted | Task 6 |
| F6 domestic-trade planner completion | Task 7 |
| F7 information-set strategy fusion | Task 10 |
| F8 forced-loss blocker loses to `EndTurn` | Task 11 |
| F9 root-width sensitivity | Task 15, benchmark-gated |
| F10 opponent dev-play flag false | Task 3 |
| F11 same-turn sibling budget starvation | Task 9 |
| F12 public-board fallback `24 -> 0` worlds | Task 4 |
| F13 wrong final-action source telemetry | Tasks 5 and 13 |
| F14 lossy 12-particle coreset changes action family | Task 12 |
| F15 duplicate Monopoly root / `legal_weight = 2` | Task 8 |

---

# Wave 1 - Make the information state trustworthy

This wave must land before strategic-search tuning. Search cannot compensate for a false posterior, false development deck, or zero-world fallback.

### Task 1: Remove posterior-changing resource resampling

**Files:**
- Modify: `src/core/tracker.ts` (`normalizeWorldWeights`, `resampleDegenerateWorlds`, `compactWorlds`, `reweightTradeEvidence`)
- Create: `tests/tracker-posterior-integrity.test.ts`
- Extend: `tests/deep-search-adapter.test.ts` (end-to-end exact-decision consequence of the corrected weights)

**Interfaces:**
- Consumes: `HandWorld[]`, posterior likelihood updates, `MAX_WORLDS`
- Produces: normalized weighted worlds whose total mass is one and whose relative mass changes only because of evidence or explicit many-to-one compaction

**Steps:**
- [ ] Add the F1 regression first: construct the reproduced dominant/tail posterior and assert that the approximately `0.001009` tail is not promoted to an equal-weight approximately `0.08333` particle.
- [ ] Add an exact-decision regression around the same belief fixture so the corrected posterior keeps the corrected `ConfirmTrade`/`CancelTrade` result from the audit reproduction.
- [ ] Delete the support-replacement/equal-weight "rejuvenation" behavior from `resampleDegenerateWorlds()`.
- [ ] When the unique world count is `<= MAX_WORLDS`, retain the normalized weighted worlds exactly; do not resample merely because ESS is low.
- [ ] When `compactWorlds()` must reduce more than `MAX_WORLDS`, use deterministic systematic resampling without support injection: preserve multiplicity of selected high-mass worlds, then merge duplicate selected states by summing their `1 / MAX_WORLDS` sample mass. Do not reserve omitted min/max tails by overwriting samples or report the resampled ESS as if it were the pre-resampling posterior ESS.
- [ ] Keep `effectiveParticleCount()` as a diagnostic of the actual weighted posterior.

**Acceptance criteria:**
- The reproduced rare tail remains at its Bayesian mass instead of increasing by roughly 82x.
- Repeated normalization without new evidence is probability-idempotent.
- Posterior mass sums to one after tracker compaction.
- The F1 exact-decision fixture no longer flips because of resampling.

**Focused validation:**

```bash
npm test -- tests/tracker-posterior-integrity.test.ts tests/deep-search-adapter.test.ts
```

### Task 2: Migrate active trades to one creator-relative contract

**Files:**
- Modify: `src/core/placement.ts` (`ActiveTradeOffer` orientation contract)
- Modify: `src/page/bridge.ts` (active-trade extraction)
- Modify: `src/core/trade-beliefs.ts` (`snapshotActiveTrades`, `tradeBeliefEventsFromDiff`)
- Modify: `src/core/trades.ts` (derive local-user bundles for UI/evaluation callers)
- Modify: `src/core/trade-guard.ts`, `src/content/trade-verdicts.ts`, `src/content/overlay.ts`, `src/worker/deep-search.ts` (migrate `ActiveTradeOffer` consumers)
- Modify: `src/content/board.ts` (runtime shape validation)
- Extend: `tests/trade-belief-board-diff.test.ts`, `tests/tracker-posterior-integrity.test.ts`, `tests/trade-guard.test.ts`
- Modify: `tests/helpers/deep-search-fixtures.ts` (fixture migration)

**Interfaces:**
- Consumes: Colonist creator-relative `offeredResources`/`wantedResources` and consecutive active-trade snapshots
- Produces: one `ActiveTradeOffer` representation with explicitly named `creatorGive`/`creatorReceive`; UI-local give/receive orientation is derived at the caller boundary and is not stored as a second source of truth

**Steps:**
- [ ] Add the F2 incoming-offer regression before changing the contract.
- [ ] Replace ambiguous `ActiveTradeOffer.give`/`receive` with `creatorGive`/`creatorReceive`; do not keep compatibility aliases because all consumers are internal and can migrate in one wave.
- [ ] In `bridge.ts`, populate those fields directly from Colonist's creator-relative `offeredResources`/`wantedResources`; never infer creator orientation later from `incoming`.
- [ ] Add one pure local-view helper in `src/core/trades.ts` that returns the local user's `give`/`receive` bundles from `creatorGive`/`creatorReceive` plus `incoming`. Migrate overlay, trade evaluator, verdict, and guard callers to that helper where they need local orientation.
- [ ] Make `snapshotActiveTrades()` store creator-relative bundles directly. `tradeBeliefEventsFromDiff()` must emit `trade-offered`, `trade-accepted`, `trade-rejected`, and `trade-countered` events in the creator-relative orientation expected by `reweightTradeEvidence()`.
- [ ] Keep the existing active-trade snapshot diff as the single owner of panel-derived Bayesian evidence. An unchanged snapshot emits no event and cannot condition the posterior twice.
- [ ] Remove the unconditional `reweightTradeEvidence(board.activeTrades)` call from `overlay.reconciledState()`. Reconciliation applies public hand/bank evidence only; trade-panel evidence has already entered durable tracker state through `GameSession.ingestEvents()`.
- [ ] Keep the final completed `trade` event as the real resource transfer, not another soft offer/response likelihood update.
- [ ] Remove the obsolete local-orientation fields from validators, fixtures, and all `ActiveTradeOffer` callers in the same migration.

**Acceptance criteria:**
- The reproduced incoming offer "Rival gives lumber / User gives brick" raises `P(Rival has lumber)` instead of `P(Rival has brick)`.
- Re-rendering/reconciling the same unchanged active offer does not change posterior weights again.
- A transition from open -> accepted/rejected/countered emits exactly one corresponding tracker event.
- No `ActiveTradeOffer` instance stores both creator-relative and local-relative resource vectors.
- UI behavior for incoming, outgoing, and counteroffers remains local-user-relative through the derived helper.

**Focused validation:**

```bash
npm test -- tests/trade-belief-board-diff.test.ts tests/tracker-posterior-integrity.test.ts tests/trade-guard.test.ts
```

### Task 3: Migrate public development history to one authoritative vector

**Files:**
- Modify: `src/core/placement.ts` (`BoardPlayerPublicState`)
- Modify: `src/page/bridge.ts` (`developmentCardsUsed`, per-player development-turn flag)
- Modify: `src/content/board.ts` (runtime validation)
- Modify: `src/worker/deep-search.ts` (`publicDevelopmentEvidence`, `sampledDevelopmentWorld`, `basePlayers`)
- Modify: `src/core/coach.ts`, `src/core/engine.ts`, `src/core/strategy.ts`, `src/core/development.ts`, `src/core/trades.ts`, `src/content/overlay.ts` (derive Knight counts from the authoritative vector)
- Extend: `tests/deep-search-adapter.test.ts`
- Modify: `tests/helpers/deep-search-fixtures.ts` and affected public-board fixtures

**Interfaces:**
- Consumes: Colonist `mechanicDevelopmentCardsState.players[color].developmentCardsUsed`, `hasUsedDevelopmentCardThisTurn`, visible held-card counts, exact own development cards
- Produces: one public `playedDevelopmentCards: DevelopmentCardVector` source of truth, per-player current-turn play state, and a development deck that conserves the 25-card base deck

**Steps:**
- [ ] Replace `BoardPlayerPublicState.playedKnights` with `playedDevelopmentCards`; do not retain the scalar as a parallel public-state field.
- [ ] Add `hasPlayedDevelopmentThisTurn` and populate it from every player's Colonist development mechanic state, not only the local player.
- [ ] In `bridge.ts`, convert every entry of `developmentCardsUsed` through the existing development-card identity mapping. Publicly used Victory Point cards remain zero unless Colonist explicitly exposes them as used/revealed.
- [ ] Migrate all public-state consumers to derive `playedDevelopmentCards.knight` when a Knight scalar is needed. Strategy/profile objects may still contain a derived `playedKnights` metric, but they may not become an independent state authority.
- [ ] Change `publicDevelopmentEvidence()` to take the maximum consistent public count between tracker history and the Colonist snapshot for every public development-card type.
- [ ] Change `basePlayers.playedDevelopmentThisTurn` to use each player's public flag; the local exact flag remains authoritative for the user.
- [ ] Enforce the exact deck equation before sampling hidden identities: base composition minus public played cards minus exact own held cards minus required hidden opponent-card count must never become negative.
- [ ] If public counts are internally impossible, fail the request with an explicit state-integrity error. Do not clamp a negative remainder to zero and continue with a fabricated deck.
- [ ] Delete the obsolete `playedKnights` public-board field, validators, and fixtures in the same coordinated migration.

**Acceptance criteria:**
- The F3 fixture with ten public Knights, nine hidden held development cards, and six earlier public progress plays produces an empty deck and cannot recommend `BuyDevelopment`.
- Public Road Building, Year of Plenty, and Monopoly plays reduce their real deck counts even when the tracker did not witness their log entries.
- An opponent who has already used a development card this turn enters Rust with `playedDevelopmentThisTurn = true`.
- Development-card conservation holds for every generated world.
- `BoardPlayerPublicState` has one public played-development representation; Knight counts used elsewhere are derived from it.

**Focused validation:**

```bash
npm test -- tests/deep-search-adapter.test.ts
```

### Task 4: Seed a mathematically defined hidden-resource posterior on midgame attach

**Files:**
- Modify: `src/core/tracker.ts` (export deterministic `seedPublicResourceWorlds` alongside existing resource-belief operations)
- Modify: `src/content/overlay.ts` (`stateFromPublicBoard`)
- Extend: `tests/tracker-posterior-integrity.test.ts`
- Extend: `tests/deep-search-adapter.test.ts`

**Interfaces:**
- Consumes: player order, exact own hand, public opponent hand sizes, visible bank when available, player-count resource supply, deterministic seed
- Produces: weighted fallback `HandWorld` samples from an explicit conditional without-replacement resource distribution

**Steps:**
- [ ] Define the fallback prior before implementing the sampler. After conditioning on the exact own hand, treat the remaining physical resource cards as a multiset. If the bank is public, subtract its exact composition first; if the bank is hidden, include the bank as the residual unobserved holder.
- [ ] Sample opponent hands by assigning cards uniformly without replacement into canonical opponent hand slots until each public `handSize` is satisfied. With a public bank this is the multivariate-hypergeometric conditional over opponent allocations; with a hidden bank the unassigned residual cards define the sampled bank.
- [ ] Drive that joint sampler with deterministic stratified/systematic uniforms derived from the request seed. Give each draw explicit `1 / N` sample mass, then merge identical worlds by summing their sample weights. Do not assign omitted-world mass to a nearest different hand composition.
- [ ] Preserve the exact own hand and all public hand sizes in every world. Reject a public snapshot whose requested slots exceed physical supply.
- [ ] Make `stateFromPublicBoard()` call this seeder before `reconcilePublicResourceEvidence()` instead of initializing opponents to empty hands.
- [ ] Keep the same conservation checks used by the normal tracked posterior; a fallback world is not allowed to bypass them.

**Acceptance criteria:**
- The recovered turn-54 hand sizes (`9, 8, 8, 7`) no longer produce `24 -> 0` worlds on a session without prior log history.
- Every fallback world has exact public hand totals and passes resource conservation.
- Repeated construction from the same semantic snapshot and seed produces identical weighted samples.
- Empirical sample frequencies on a small enumerable fixture match the exact conditional allocation probabilities within the deterministic sample budget.
- Packaged WASM receives at least one valid world during a normal midgame attach/reload.

**Focused validation:**

```bash
npm test -- tests/tracker-posterior-integrity.test.ts tests/deep-search-adapter.test.ts
```

**Wave 1 exit gate:** F1, F2, F3, F10, and F12 reproductions pass on the corrected state pipeline before any search-strength changes begin.

---

# Wave 2 - Make Colonist legality and final authority coherent

### Task 5: Make Rust the single final action authority without duplicating rejection state

**Files:**
- Modify: `engine/crates/catan-search/src/depth.rs` and `engine/crates/catan-search/src/exact.rs` only for root exclusions that cannot be represented by core state
- Modify: `engine/crates/catan-search/src/lib.rs` if exclusion-aware wrappers remain necessary after core-state reuse
- Modify: `engine/crates/catan-wasm/src/lib.rs` (`Request` rejected-trade state/root exclusions, `Response` authority, arbitration in `analyze`)
- Modify: `src/core/engine.ts` (`DeepSearchResult` authority type)
- Modify: `src/worker/analyze.ts`, `src/worker/deep-search.ts`, `src/content/decision-worker.ts` (carry authoritative retry state through the state-locked request)
- Modify: `src/core/trade-guard.ts` (remove post-search alternate-action selection)
- Modify: `src/content/overlay.ts` (accepted outgoing trade flow, failed-action retry state, final action mapping)
- Modify: `tests/trade-guard.test.ts`
- Extend: `tests/deep-search-adapter.test.ts`

**Interfaces:**
- Consumes: Rust `chosen`, exact/tactical/family/safety arbitration, core `GameState.last_rejected_trade`, and UI execution failures not representable by core game state
- Produces: one `DecisionAuthority` attached to the final Rust action; TypeScript may execute it or fail closed, but may not choose another candidate after search

**Steps:**
- [ ] Add an explicit WASM authority enum/string to the response: `exact-mandatory`, `tactical-proven`, `deep-maxn`, `exact-family`, or `safety-override`.
- [ ] Set the authority at the exact point where `analyze()` chooses or replaces `chosen`; do not infer it later from the action kind. Parse it into `DeepSearchResult`.
- [ ] Reuse the existing core rejection owner first. The WASM adapter currently initializes `GameState.last_rejected_trade = None`; extend the request so a completed rejected outgoing offer can populate that field and let `generated_domestic_trade_offers()` suppress the repeated bundle through the existing rule-state mechanism.
- [ ] Do not encode the same rejected offer in both `last_rejected_trade` and a generic exclusion list. Core rejection state owns rule/simulation-level repeated-offer suppression.
- [ ] Retain a small root-only exclusion mechanism only for retry conditions that core state cannot represent, such as a fresh-state UI mapping failure or a failed counteroffer interaction. Exclusions contain action kind plus exact give/receive payload, are fingerprinted into the decision key, and apply only at the current WASM root.
- [ ] If root exclusions remain necessary, apply them before both exact Mandatory and Deep MaxN root arbitration. Deeper simulated legality remains untouched.
- [ ] Remove `selectUsableDeepAction()` as a mechanism for choosing another `search.actions` candidate. `preferredDeepAction()` uses Rust `chosen` unchanged.
- [ ] Remove `canAnyOpponentFulfillTrade()` from final-action arbitration. Hidden-card feasibility belongs in Rust belief/search valuation, not TypeScript post-search substitution.
- [ ] Remove `shouldConfirmAcceptedTradeImmediately()` as action authority. An accepted outgoing trade waits for the state-locked exact `ConfirmTrade`/`CancelTrade` result.
- [ ] If fresh UI validation cannot execute Rust `chosen`, record only the minimal retry state needed for a new root search, invalidate the decision key, and expose no conflicting local substitute.
- [ ] Derive `finalActionSource` directly from `DecisionAuthority`; Task 13 expands the provenance record.

**Acceptance criteria:**
- In the F4 fixture, Rust `CancelTrade` is the only recommendation/executable action exposed by the overlay; `ConfirmTrade` cannot be reached by a fast-confirm branch.
- A rejected outgoing offer is suppressed through `GameState.last_rejected_trade`, not a duplicate TypeScript/Rust rejection system.
- A UI-only failed action can trigger a fresh constrained root search without changing deeper simulated game legality.
- No TypeScript helper selects a different strategic candidate from `search.actions` after Rust returns.
- An authoritative action that cannot map against fresh UI state results in no click and a fresh search.
- Deep-selected actions that map unchanged are labeled with their real Rust authority, not `placement-heuristic` or `coach-goal`.

**Focused validation:**

```bash
npm run build:wasm
npm test -- tests/trade-guard.test.ts tests/deep-search-adapter.test.ts
```

### Task 6: Generate Colonist-legal large domestic-trade candidates without exploding the tree

**Files:**
- Modify: `engine/crates/catan-core/src/state.rs` (`generated_domestic_trade_offers`, trade validity/proposal helpers)

**Interfaces:**
- Consumes: current hand, trade ratios, bank availability, target build deficits, discard limit
- Produces: bounded but size-unrestricted strategically relevant `Action::OfferTrade` candidates

**Steps:**
- [ ] Add a core regression showing `OfferTrade { give: 3 lumber, receive: 1 brick }` is both applicable and generated when strategically relevant.
- [ ] Remove the `give_total > 2 || receive_total > 2` legality/proposal cutoff.
- [ ] Separate trade **validity** from trade **candidate generation**: the rule layer must accept any non-empty affordable bundle that does not trade the same resource in both directions; the search candidate generator remains bounded.
- [ ] Generate all single-resource give bundles for amounts `1..=held` so `3 -> 1`, `4 -> 1`, and larger endgame/safety offers are representable.
- [ ] Generate mixed bundles with a bounded best-first expansion ordered by resource surplus/opportunity cost rather than enumerating the full Cartesian product.
- [ ] Generate receive bundles from actionable build/plan deficits and hand-safety conversions, including multi-card receives when they unlock a coherent plan.
- [ ] Keep maritime-dominance pruning only when the same requested cards are actually obtainable from the bank at the current ratio; a bank shortage must not suppress a player offer.
- [ ] Retain the existing final candidate cap (`96`) after strategic ranking so Colonist legality does not become unbounded search branching.

**Acceptance criteria:**
- Legal `3 -> 1` and `4 -> 1` offers can appear in `legal_actions()`/search candidates.
- Candidate count remains bounded.
- Same-resource-on-both-sides and unaffordable bundles remain absent.
- No change is made to the unresolved Colonist contracts for multiple completed player trades in one turn or recipient targeting.

**Focused validation:**

```bash
cd engine && cargo test -p colonist-catan-core domestic_trade
```

### Task 7: Give domestic-trade plans probabilistic completion credit under the live planner budget

**Files:**
- Modify: `engine/crates/catan-search/src/planner.rs`
- Modify: `engine/crates/catan-search/src/depth.rs` (`normalize_belief_root_priors`, live planner allocation)

**Interfaces:**
- Consumes: chance nodes, modeled trade-response probabilities, root-player continuations, the live 4,000-node search budget
- Produces: `TurnPlan` value plus `completion_mass` in `[0, 1]`, so partially explored stochastic plans can contribute in proportion to the probability mass that actually reached a coherent current-turn endpoint

**Steps:**
- [ ] Add the F6 regression at the real production allocation: `maxNodes = 4,000`, 12-24 belief particles, and the same planner budget formula used by live MaxN. A trade that unlocks a settlement must receive nonzero planner credit there; a generous 100k-node-only test is not closure.
- [ ] Replace `TurnPlan.completed: bool`/`PlanValue.completed: bool` with `completion_mass: f32`. Deterministic completed endpoints return `1.0`; budget-truncated branches return `0.0` unless a descendant has actually completed positive probability mass.
- [ ] At chance and opponent trade-response nodes, aggregate `completion_mass` using the same normalized branch probabilities used for value. Preserve a representative action line only for diagnostics.
- [ ] When budget prevents recursive evaluation of a positive-probability stochastic branch, include a bounded endpoint estimate for its value but add zero completion mass for that branch. This keeps the expected value defined without pretending the continuation was searched.
- [ ] At root-player decision nodes, propagate the completion mass of the selected continuation rather than converting any partial result to a global false/true flag.
- [ ] Change `plan_adjusted_priors()` to use plans with `completion_mass > 0` and scale the planner blend by that mass. A plan with 25% completed stochastic mass can have at most 25% of the configured planner influence; a zero-mass plan gets no planner adjustment.
- [ ] Stop splitting the live planner budget so thinly that every retained root receives only one or two recursive nodes. Use a per-root floor of `state.board.num_players as u32 + 1` planner nodes: for a 4-player offer this covers the three recipient response decisions, the creator confirm/cancel decision, and one returned Main-phase decision before a completed `EndTurn` child is recognized. Set `planner_root_cap = min(quota_ranked_len, max(1, maximum_nodes / per_root_floor))`, then allocate any remainder by ranked prior. If a domestic-trade root exists in the pre-cap quota-ranked set, reserve one retained slot for the highest-ranked domestic-trade representative before filling the remaining slots.
- [ ] Preserve the global planner-node ceiling. This task reallocates the existing planner budget; it does not raise the 4,000-node strategic search budget as a substitute for correctness.

**Acceptance criteria:**
- The F6 trade -> settlement fixture receives nonzero planner prior credit under the actual live planner allocation.
- Increasing planner budget raises or preserves `completion_mass`; it never turns an actually completed branch back into zero mass.
- Unexplored response mass is not labeled complete and cannot receive full planner influence.
- Existing road -> settlement planner behavior remains unchanged.
- Planner work remains bounded by its configured node ceiling.

**Focused validation:**

```bash
cd engine && cargo test -p colonist-catan-search planner
```

**Wave 2 exit gate:** the authoritative Rust action cannot be contradicted by the overlay, and Colonist-legal large-bundle trade roots can enter and complete whole-turn planning.

---

# Wave 3 - Repair strategic-search correctness

### Task 8: Collapse Monopoly to exactly one root representative

**Files:**
- Modify: `engine/crates/catan-search/src/depth.rs`

**Interfaces:**
- Consumes: parameterized root `PlayMonopoly(resource)` actions and exact Monopoly family result
- Produces: one unique exact Monopoly root representative

**Steps:**
- [ ] Add a one-particle regression asserting every root action is unique and every `legal_weight <= 1.0`.
- [ ] Before root budgeting, locate the first Monopoly family position, remove all existing Monopoly parameterizations, and insert the exact best Monopoly action once at that position when applicable.
- [ ] Preserve the existing root-cap/order semantics for all non-Monopoly actions.
- [ ] Add a general debug/test assertion that root actions are unique before per-particle aggregation.

**Acceptance criteria:**
- The reproduced one-particle state reports `PlayMonopoly(Grain) legal_weight = 1.0`, never `2.0`.
- The selected Monopoly family remains unchanged in the simple control fixture.

**Focused validation:**

```bash
cd engine && cargo test -p colonist-catan-search monopoly
```

### Task 9: Give every same-turn sibling a local search budget

**Files:**
- Modify: `engine/crates/catan-search/src/depth.rs` (`Searcher::visit`, decision/chance child allocation; reuse `allocate_root_node_budgets()` for retained sibling lists)

**Interfaces:**
- Consumes: global node budget, ranked sibling list, chance probabilities
- Produces: recursive subtree ceilings that guarantee later siblings a bounded share without exceeding the global budget

**Steps:**
- [ ] Add the recovered turn-54 regression before changing allocation. Preserve the real hand/public state/belief fixture used in the audit reproduction and run the closure case at `depth = 4`, `branchCap = 8`, `maxNodes = 4,000`.
- [ ] Add a commutativity regression: `BuyDevelopment -> Resolve(card) -> MaritimeTrade` must not be valued below the state-identical `MaritimeTrade -> BuyDevelopment -> Resolve(card)` solely because of traversal order.
- [ ] Add a sibling-permutation regression: reordering equally ranked/root-equivalent siblings must not change the chosen action under the same total node budget.
- [ ] Refactor `Searcher::visit()` to accept an explicit subtree node ceiling in addition to the global counter.
- [ ] At each decision node, allocate the remaining subtree budget across all retained siblings before visiting the first child. Use the existing root allocation policy as the starting point so every sibling gets a nonzero floor.
- [ ] At chance nodes, reserve budget across positive-probability outcomes rather than allowing the first outcome to consume the entire local allowance.
- [ ] Allow unused child budget to remain available to later siblings, but never let an earlier child spend a later sibling's guaranteed floor.
- [ ] Keep the existing global node total as a hard ceiling.

**Acceptance criteria:**
- Recovered turn 54 no longer recommends the information-dominated action order because a prior domestic-trade subtree starved the post-draw maritime continuation.
- The maritime continuation after each possible development draw receives nonzero search work when it is retained as a legal sibling.
- Sibling permutation preserves the chosen action and keeps corresponding backed-up root values equal within `1e-6` on the deterministic regression fixture.
- Reported total nodes never exceed the configured global maximum.

**Focused validation:**

```bash
cd engine && cargo test -p colonist-catan-search turn_54
cd engine && cargo test -p colonist-catan-search node_budget
```

### Task 10: Make every recursive belief-search decision information-set-consistent

**Files:**
- Modify: `engine/crates/catan-search/src/depth.rs` (replace the root-exemption switch with a recursive information-set-safe mode)

**Interfaces:**
- Consumes: `GameState::observed_state(actor)`, `observation_hash(actor)`, observation-ranked action priors
- Produces: one deterministic policy for every recursive actor observation, including later same-turn decisions by the original root player

**Steps:**
- [ ] Add the original F7 regression with two states that have identical `observation_hash(actor)` but differ only in a third party's hidden resources; assert the modeled actor policy is identical under `depth = 4`, `branchCap = 8`, `maxNodes = 4,000`.
- [ ] Add a second regression for the root actor after a same-turn transition such as `BuyDevelopment -> ResolveDevelopment` or a trade continuation: keep the root actor's own newly revealed information the same, vary only another player's hidden resources, and assert the root actor chooses the same recursive continuation.
- [ ] Remove the `root != actor` exemption from recursive belief search. The initial root action remains externally aggregated across the posterior before `Searcher::visit()` is entered; every decision reached inside `visit()` uses the acting player's observation-safe policy, even when `actor == original_root`.
- [ ] Replace `observation_safe_root: Option<u8>` with a mode/boolean that means recursive decision policies are observation-safe. Enable it for production weighted-belief MaxN and keep it disabled only for explicitly perfect-information diagnostic search.
- [ ] Base recursive policy ranking/mixtures only on `observed_state(actor)` and the actor's observation-visible policy profile. If a development draw reveals a card to the root actor, that legitimately changes that actor's observation and may change the continuation.
- [ ] Derive the recursive mixture deterministically from the observation-ranked list so observation-identical worlds cannot diverge because of incidental action ordering. Add no cache unless profiling later proves repeated ranking is material.
- [ ] Do not expose third-party hidden development/resource identities through tie-breakers, rollout ordering, or same-turn root continuations.

**Acceptance criteria:**
- Observation-identical worlds produce the same recursive policy for every actor.
- Worlds where the acting player's **own** newly revealed information differs may produce different policies.
- The F7 third-party hidden-card swap cannot flip either an opponent decision or a later same-turn root-player continuation.
- The regression passes under production-style depth/branch/node limits, not only a generous search.

**Focused validation:**

```bash
cd engine && cargo test -p colonist-catan-search observation
```

### Task 11: Make forced-loss avoidance a posterior-level root invariant

**Files:**
- Modify: `engine/crates/catan-search/src/threats.rs` (posterior verified-immediate-threat helpers)
- Modify: `engine/crates/catan-search/src/depth.rs` (root preservation/gating)
- Modify: `engine/crates/catan-search/src/mcts.rs` (`safer_end_turn_alternative` final safety contract)
- Modify: `engine/crates/catan-wasm/src/lib.rs` (carry `safety-override` authority when the final gate changes `chosen`)

**Interfaces:**
- Consumes: weighted belief particles, candidate root actions, existing opponent immediate-win detection
- Produces: posterior threat mass per root action, with a hard survival gate only for a publicly/posterior-certain immediate loss

**Steps:**
- [ ] Add the F8 fixture: a legal settlement blocks the opponent's verified immediate main-phase win while `EndTurn` does not.
- [ ] Replace the disabled single-state coarse threat forcing with a belief-level threat check across weighted particles.
- [ ] For each candidate root action, apply the action in every world where legal and verify the threat **after transition**; do not trust action-type heuristics alone.
- [ ] Compute `forced_loss_weight`: posterior mass in which an opponent has a verified immediate win before the root player can meaningfully respond.
- [ ] Preserve verified blocker candidates ahead of ordinary root truncation whenever an immediate threat exists in any positive-weight particle (`weight > f32::EPSILON`), so search can compare them instead of pruning them away.
- [ ] Apply a lexicographic hard gate only when `forced_loss_weight >= 1.0 - 1e-6` and at least one legal candidate reduces it to `<= 1e-6`. In that case, an action that still permits the certain loss cannot beat a verified blocker because of static evaluator score.
- [ ] For genuinely uncertain hidden-world threats, keep `forced_loss_weight` as a risk signal/root-preservation input and let weighted search compare the surviving actions; do not convert every small risk difference into maximin play.
- [ ] Apply the same certain-loss gate to final `EndTurn` arbitration so an end-turn fallback cannot reintroduce the losing action after depth search.

**Acceptance criteria:**
- The F8 blocker beats `EndTurn` at production and wider/deeper settings.
- Threat blocking is based on post-apply verification across the posterior, not merely a matching vertex/edge label.
- A posterior-certain immediate loss is never accepted when a verified legal action removes it.
- Uncertain threats do not automatically force the minimum-risk action regardless of expected strategic value.

**Focused validation:**

```bash
cd engine && cargo test -p colonist-catan-search threat
```

### Task 12: Remove Rust-side lossy compression from the full WASM posterior

**Files:**
- Modify: `engine/crates/catan-search/src/shared.rs` (`coalesce_identical_particles`; retain lossy coreset only as experimental code)
- Modify: `engine/crates/catan-search/src/depth.rs` (production particle selection)

**Interfaces:**
- Consumes: the complete weighted particle set present in one WASM request
- Produces: lossless coalescing of exact identical states for strategic search; no additional 24 -> 12 or nearest-representative approximation inside Rust

**Steps:**
- [ ] Add the exact F14 24-world same-observation fixture as a regression at `depth = 4`, `branchCap = 8`, `maxNodes = 4,000`: the full 24-world request chooses `PlayMonopoly(Grain)` while current 12-world compression chooses `EndTurn`.
- [ ] Replace `select_strategic_particles()` in the production MaxN path with lossless coalescing only. Exact identical `state_hash` states may merge by summing weights; distinct states remain distinct.
- [ ] Remove the production assumption that `STRATEGIC_PARTICLE_TARGET = 12` is correctness-safe. Keep lossy coreset code only behind an explicitly experimental/benchmark-only path if it remains useful.
- [ ] Ensure exact family selection, root posterior statistics, and strategic MaxN all consume the same full distinct WASM particle set.
- [ ] Retain the global node budget; Task 9 owns fair allocation across the larger set.
- [ ] Keep the architectural claim narrow and accurate: this task does **not** prove that every tracker world reaches Rust. Task 14 owns the TypeScript tracker-to-WASM approximation and its bounded-regret evidence.

**Acceptance criteria:**
- The F14 fixture no longer changes action family because of Rust-side 24 -> 12 compression under production-style limits.
- Full 24-particle WASM input and the production Rust path agree on the Monopoly family/target in the reproduced fixture.
- Rust does not reassign a distinct particle's mass to a different state.
- Duplicate identical states may still be coalesced exactly by summing weights.
- Diagnostics distinguish tracker/source particle count, WASM particle count, and losslessly coalesced Rust search count.

**Focused validation:**

```bash
cd engine && cargo test -p colonist-catan-search strategic_particle
cd engine && cargo test -p colonist-catan-search monopoly
```

**Wave 3 exit gate:** F7, F8, F11, F14, and F15 all pass their adversarial regressions with production-style budgets. Only then evaluate branch width, hidden-dev sampling variance, or evaluator strength.

---

# Wave 4 - Make the engine explainable enough to diagnose the next bad move

### Task 13: Record the real decision authority and bounded search provenance

**Files:**
- Modify: `engine/crates/catan-search/src/depth.rs` (`BeliefDepthResult` diagnostics)
- Modify: `engine/crates/catan-wasm/src/lib.rs` (`Response` diagnostics)
- Modify: `src/core/engine.ts` (`DeepSearchResult` diagnostics)
- Modify: `src/worker/deep-search.ts` (mapping)
- Modify: `src/core/decision-trace.ts` (persisted trace schema and recorder)
- Modify: `src/content/overlay.ts` (final authority/execution mapping)

**Interfaces:**
- Consumes: tracker/WASM/Rust particle counts, root prior/truncation, planner `completion_mass`, per-root node allocation, exact/tactical/safety arbitration
- Produces: compact trace fields sufficient to explain why an action entered/exited the candidate set and which authority selected the displayed action

**Steps:**
- [ ] Record tracker/source-world count, final WASM particle count, and losslessly coalesced Rust search count separately so the TypeScript approximation boundary cannot be mistaken for the full tracker posterior.
- [ ] Record the pre-truncation ranked root actions and priors, the retained root actions, and per-root node allocations.
- [ ] Record whole-turn planner value and `completion_mass` for roots when available.
- [ ] Record exact/tactical/safety authority and any exact-family replacement.
- [ ] Record fresh-state mapping/execution failure separately from Rust authority. After Task 5 there is no ordinary TypeScript strategic-candidate substitution to record.
- [ ] Extend `DecisionActionSource` with the explicit Rust authority values instead of inferring `deep`/`mandatory` from rendering paths.
- [ ] Keep trace payloads bounded: action identifiers, numeric values, hashes/weights, and reasons; do not persist entire `GameState` blobs per node.

**Acceptance criteria:**
- A captured decision can answer: why candidate X was absent, whether it was pruned, how many nodes it received, whether planner completion affected its prior, and which layer selected/replaced the final action.
- F13 source labels match the actual deep/tactical/exact/safety path.

**Focused validation:**

```bash
npm test -- tests/deep-search-adapter.test.ts tests/trade-guard.test.ts
```

---

# Wave 5 - Correct and calibrate the remaining bounded approximations

### Task 14: Define the joint tracker-to-WASM posterior approximation and validate the 24-particle boundary

**Files:**
- Modify: `src/worker/deep-search.ts` (`selectRepresentativeWorlds`, `sampledDevelopmentWorld`, world construction, configurable offline particle limit)
- Extend: `tests/deep-search-adapter.test.ts`
- Create: `tests/fixtures/recommendation-audit-corpus.json` (fixed replayable audit states and positive controls used by Tasks 14-15)
- Modify: `scripts/replay-engine.ts` (24/48/96-particle posterior comparison fields)
- Create: `docs/RECOMMENDATION_ENGINE_FIX_VERIFICATION_2026-09-01.md`

**Interfaces:**
- Consumes: corrected weighted tracker resource worlds, the corrected remaining development deck, public opponent development-card counts, purchase-age counts, deterministic request seed
- Produces: a deterministic weighted joint resource/development sample with a live cap of 24 particles plus explicit bounded-regret evidence against larger offline particle sets

**Steps:**
- [ ] Remove nearest-representative mass reassignment from `selectRepresentativeWorlds()`. If the normalized tracker world count is within the requested particle limit and there is no hidden development identity to sample, preserve the exact worlds and weights. Otherwise sample the tracker posterior with deterministic systematic/stratified draws; each selected draw carries its sample mass and duplicate states merge by summing that mass.
- [ ] Define the hidden-development conditional distribution explicitly. Start from the base deck minus Task 3 public played counts and exact own held cards. Create canonical hidden slots for each opponent, distinguishing ready versus bought-this-turn slots from tracker purchase-age counts. Assign card identities uniformly without replacement across those slots; the remaining multiset is the development deck.
- [ ] Replace the current resource-world cross-product plus independent PRNG development draws with one deterministic joint sampler. For each of the `N` final strata, select a resource world according to tracker posterior mass and a development allocation according to the joint without-replacement distribution. Use deterministic independent dimensions derived from the semantic request seed, assign each stratum `1 / N` mass, and merge exact duplicate final particles.
- [ ] Remove the special pre-cap that reduces resource worlds to `MAX_PARTICLES / 2` whenever hidden development cards exist. The only live lossy boundary after this task is the final joint `MAX_INTERACTIVE_PARTICLES = 24` sample.
- [ ] Preserve purchase-age/identity correlation: an identity assigned to a bought-this-turn slot is unplayable in that particle; do not sample identities first and then attach age independently.
- [ ] Add small enumerable resource and development fixtures where the exact joint probabilities are known and compare the deterministic sample frequencies/weights to those probabilities.
- [ ] Add an offline `particleLimit` seam used only by tests/replay. For the fixed audit corpus, construct otherwise identical 24-, 48-, and 96-particle WASM requests and run the same production search semantics on each.
- [ ] Treat the live 24-particle boundary as materially unsafe if either (a) the 24-particle chosen action family differs from both the 48- and 96-particle results on the same fixture, or (b) its chosen root is more than `0.02` below the chosen root in both larger-particle references under the same evaluator/search settings. If that gate fails, keep Task 14 open and amend the plan before Task 15 to choose a higher/adaptive live particle budget; do not silently declare the 24-particle boundary correctness-safe.
- [ ] Compare the former seed-sensitive 5/8 `BuyDevelopment` versus 3/8 trade fixture under the joint sampler and record stability plus 24/48/96 agreement in the verification ledger.

**Acceptance criteria:**
- Repeated construction of the same semantic request and particle limit is deterministic.
- Resource and development samples come from defined joint without-replacement distributions; omitted probability mass is never assigned to a nearest different state.
- Hidden development identity counts never exceed the corrected deck and bought-this-turn identities remain unplayable.
- The live 24-particle approximation has an explicit pass/fail bounded-regret result against both 48 and 96 particles on the fixed corpus.
- Task 15 does not begin until this sampling contract is final, so root-width calibration cannot be invalidated by a later posterior change.

**Focused validation:**

```bash
npm test -- tests/deep-search-adapter.test.ts
npm run build:wasm
npm run replay:decisions -- tests/fixtures/recommendation-audit-corpus.json --rerun
```

### Task 15: Re-benchmark root width after the final posterior pipeline

**Files:**
- Modify: `tests/fixtures/recommendation-audit-corpus.json` only to add a newly confirmed focused fixture; do not replace or retune the frozen Task 14 corpus to favor an F9 outcome
- Modify: `scripts/replay-engine.ts` (corrected live/reference search configurations, root-rank/action-family/regret fields)
- Modify: `scripts/replay-decisions.mjs` (summarize F9 evidence)
- Modify: `docs/RECOMMENDATION_ENGINE_FIX_VERIFICATION_2026-09-01.md`

**Interfaces:**
- Consumes: the final corrected state/sampling pipeline from Tasks 1-14, production search, wider/deeper reference search, fixed audit corpus
- Produces: an evidence-backed disposition for F9; this task does not pre-authorize a production root-rescue algorithm

**Steps:**
- [ ] Reuse the existing replay infrastructure. The fixed corpus contains at minimum recovered turn 54, the F8 forced blocker, the F14 Monopoly posterior, an accepted-trade exact case, a large-bundle domestic-trade case, and passing port/road/robber controls.
- [ ] Define three search configurations using the same final posterior semantics: `live` = depth 4 / branch 8 / 4,000 nodes / 350 ms; `reference-medium` = depth 6 / branch 32 / 64,000 nodes / 10,000 ms; `reference-max` = depth 6 / branch 32 / 250,000 nodes / 10,000 ms.
- [ ] Measure pre-truncation root rank, action-family agreement, and root regret. A wider search cannot repair an action that was never admitted to the root set, so report admission separately from backed-up value.
- [ ] Treat an F9-style omission as material only when an action outside the live top eight becomes the chosen action in both `reference-medium` and `reference-max` on the same fixture.
- [ ] If no omitted action satisfies that stable-reference criterion across the fixed corpus, close F9 without a production root-width change.
- [ ] If the criterion is met, do **not** encode the regression winner as a runtime rule and do not implement a rescue mechanism in this task. Record the offending root ranks, priors, reference values, and latency budget, then amend the implementation plan with a generic root-admission design such as progressive widening, uncertainty-aware admission, or a bounded secondary scoring pass. That follow-up rule must be evaluated across the complete corpus before production adoption.
- [ ] Record the F9 disposition and search cost in the verification ledger.

**Acceptance criteria:**
- Root width 8 is either retained with stable-reference evidence or F9 remains open with a concrete corpus-backed admission failure.
- No production rule refers to a fixture identity, expected regression winner, seed, or benchmark label.
- More nodes are never presented as a remedy for an action pruned before search.
- F9 calibration uses the final Task 14 posterior pipeline and is not rerun against stale sampling semantics.

**Focused validation:**

```bash
npm run build:wasm
npm run replay:decisions -- tests/fixtures/recommendation-audit-corpus.json --rerun
```

---

# Wave 6 - Integration, engine revision, and real-browser proof

### Task 16: Integrate the repaired engine and prove the end-to-end authority path

**Files:**
- Modify: `engine/crates/catan-search/src/lib.rs` (`ENGINE_REVISION`)
- Modify: `docs/RECOMMENDATION_ENGINE_FIX_VERIFICATION_2026-09-01.md`
- Modify: `docs/RECOMMENDATION_ENGINE_AUDIT_REPORT_2026-09-01.md` (verified closure/status only)
- Modify: `docs/END_TO_END_RECOMMENDATION_REVIEW_2026-09-01.md` (verified closure/status only; preserve original reproduction evidence)

**Interfaces:**
- Consumes: all prior wave outputs
- Produces: packaged WASM/extension build whose displayed build identity and engine revision identify the corrected implementation

**Steps:**
- [ ] Bump `ENGINE_REVISION` from `deep-maxn-v9` to `deep-maxn-v10` only after the correctness waves are integrated so runtime traces cannot confuse old and repaired search semantics.
- [ ] Run the focused TypeScript and Rust regressions from Tasks 1-15.
- [ ] Run repository-wide type, test, Rust, WASM, and extension build verification.
- [ ] Replay the captured turn-54 state through the packaged WASM, not only a native helper, and record the final chosen action plus root diagnostics.
- [ ] Reload the built extension in the persistent browser profile and verify the popup/settings display the new source build identity and `deep-maxn-v10` runtime engine.
- [ ] Exercise a midgame attach/reload and confirm fallback beliefs remain nonzero and state-valid.
- [ ] Exercise incoming/outgoing trade states and confirm the displayed/executed action matches Rust authority, especially `CancelTrade` versus `ConfirmTrade`.
- [ ] Capture one live decision trace and verify that its final source, root candidates, node allocation, particle counts, and authority reason are internally consistent.
- [ ] Record pass/fail evidence in the verification ledger. Mark an audit finding closed only when its original reproduction is no longer reproducible for the intended reason.
- [ ] After the packaged correctness gates pass, execute the separate before/after evidence plan. Whole-game wins and comeback rescues are strength evidence, not substitutes for closing a correctness finding.

**Acceptance criteria:**
- All F1-F8 and F10-F15 confirmed defects have a passing focused reproduction at the packaged build boundary where applicable.
- F9 has an evidence-backed disposition from Task 15 rather than an assumed fix.
- `npm run verify` succeeds on the integrated tree.
- The loaded browser extension identifies the exact corrected build and reports `deep-maxn-v10`.
- A fresh-state execution check still guards every click/action.
- The final recommendation path is:

```text
Colonist public state
-> canonical public snapshot
-> mass-preserving weighted tracker beliefs
-> complete public development state
-> mathematically defined joint tracker-to-WASM sample (exact when within the live limit)
-> WASM state validation
-> Colonist-valid candidate generation
-> exact/proven safety authority
-> recursively information-set-safe whole-turn Deep MaxN
-> fair same-turn node allocation
-> lossless full distinct WASM-posterior strategic search
-> explicit authority result
-> TypeScript mapping without authority reversal
-> displayed recommendation
-> fresh-state validated execution
```

**Final validation:**

```bash
npm run verify
npm run replay:decisions -- tests/fixtures/recommendation-audit-corpus.json --rerun
```

Whole-game native, takeover, and live Colonist strength evidence then follows `docs/RECOMMENDATION_ENGINE_BEFORE_AFTER_EVIDENCE_PLAN_2026-09-01.md`.

---

## Implementation Order and Review Boundaries

Use these review boundaries during execution:

1. **State correctness:** Tasks 1-4. Do not start search tuning until this wave passes.
2. **Authority and Colonist action contract:** Tasks 5-7.
3. **Search correctness:** Tasks 8-12. Task 9 precedes full-WASM-posterior Task 12 so the larger Rust search set cannot amplify the known same-turn starvation defect.
4. **Auditability:** Task 13.
5. **Approximation contract, then calibration:** Task 14 finalizes the tracker-to-WASM posterior sampler and 24-particle regret gate; Task 15 evaluates root width only after that posterior is fixed.
6. **Packaged/browser release proof:** Task 16.

Do not combine these into one large patch. Each boundary should leave the engine in a coherent state that can be inspected independently.

## Explicitly Out of Scope Until the Above Is Green

- Broad evaluator weight retuning.
- Promoting the learned strategic value or policy model.
- Adding strategy rules because they appear in Reddit/community advice without a reproduced engine gap.
- Increasing node/time budgets as a substitute for F11 or F14.
- Implementing recipient-specific offers without Colonist-specific proof.
- Changing whether multiple completed player trades are allowed in one turn without Colonist-specific proof.
- Replacing MaxN with a different algorithm solely because current MaxN has correctness bugs that are locally repairable.

## Completion Definition

The implementation is complete when the original audit reproductions either pass under the corrected intended behavior or are explicitly retained as measured non-defects, the packaged extension preserves Rust authority through execution, recursive belief search is information-set-consistent, Rust performs no lossy compression beyond the explicit TypeScript tracker-to-WASM boundary, and that remaining live particle boundary has passed its defined 24-versus-48/96 bounded-regret gate. Whole-game and comeback strength claims are reported separately by the before/after evidence plan.
