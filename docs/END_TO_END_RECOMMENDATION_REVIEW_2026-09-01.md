# End-to-End Colonist Assistant Recommendation Review

Date: 2026-09-01
Repository: `/home/hamza/repo/colonist-assistant`
Branch: `main`
Reviewed HEAD: `aebe9ede07ae5f80cc683a6252efef7a7e209f6f` - `Fix belief search integrity`
Live engine: `deep-maxn-v9`
Status: canonical consolidated audit report; implementation plan in `docs/RECOMMENDATION_ENGINE_IMPLEMENTATION_PLAN_2026-09-01.md`

## Executive verdict

The current engine is not yet consistently defensible as "the strongest practical action given everything the player is allowed to know."

The architecture is substantially stronger than a heuristic bot. Board topology, production, ports, development-deck valuation, trophy races, robber logic, discard risk, whole-turn search infrastructure, weighted beliefs, exact solvers, and state-validated execution all exist, and several of them passed adversarial strategic probes.

The confirmed failures now cluster in eight causal layers:

1. **Belief integrity:** resource-posterior resampling changes probability mass, and incoming trade evidence is reversed and can be applied repeatedly (F1/F2).
2. **Public-state reconstruction:** development-card history and midgame fallback state can disagree with public reality (F3/F10/F12).
3. **Colonist action completeness:** native domestic-trade generation omits Colonist-legal large bundles (F5).
4. **Whole-turn trade planning:** domestic-trade continuations do not propagate planner completion as intended (F6).
5. **Information-set safety:** simulated opponents can condition decisions on hidden facts they cannot observe (F7).
6. **Search control and survival:** a verified forced-loss blocker can lose to `EndTurn`, and same-turn sibling budget starvation can reverse a dominance/commutativity invariant on a real captured turn (F8/F11).
7. **Strategic approximation and root bookkeeping:** root width is sensitive, the 12-particle coreset can change the final action family relative to the full posterior, and Monopoly root replacement can duplicate an identical action (F9/F14/F15).
8. **Final authority and auditability:** the overlay can fast-confirm a trade that Rust exact authority vetoed; source telemetry can also mislabel actions that actually came from deep search (F4/F13).

These are recommendation-correctness defects or directly adjacent state/search defects. They are not merely complaints that the evaluator needs different Catan strategy weights. Broad evaluator tuning before these contracts are repaired would risk hiding causal failures.

This report separates confirmed defects from measured sensitivities and from Colonist product-contract questions that remain unproven.

---

## 1. Exact live production algorithm

At the reviewed revision, the live production path is:

```text
Colonist public state
-> src/page/bridge.ts
-> BoardSnapshot / tracker
-> overlay.reconciledState()
-> DecisionWorkerClient
-> src/background/index.ts
-> analyzeDecisionRequest()
-> analyzeDeepSearch()
-> buildDeepSearchRequest()
-> packaged colonist_search_bg.wasm
-> catan-wasm::analyze()
-> GameState::validate() per particle
-> exact mandatory solver where applicable
-> weighted-belief Deep MaxN
-> tactical current-turn solver
-> exact family refinement
-> EndTurn safety alternative
-> WASM chosen
-> TypeScript prompt filtering/mapping
-> overlay nextClick
-> action-guide
-> fresh-state legality validation
-> click / board bridge execution
```

### Production configuration

Ordinary live decisions in `src/worker/deep-search.ts:1088-1114` use:

| Parameter | Live value |
| --- | ---: |
| Search | weighted-belief Deep MaxN |
| Depth | 4 |
| Root/decision branch cap | 8 |
| Node budget | 4,000 |
| Depth-search deadline | 350 ms |
| Strategic particle limit | 12 |
| Interactive particles sent | <= 24 |
| Tactical depth | 14 |
| Tactical nodes | 900 |

Opening search receives a larger budget: about 12,000 nodes, 1.2 seconds, and branch cap 12. Background opening pondering can reach 18,000 nodes and 2.5 seconds.

### Algorithm authority

- **MaxN:** production.
- **PUCT / UCT:** diagnostic/arena only.
- **AlphaBeta/paranoid:** diagnostic comparison, not live authority.
- **Learned strategic value/policy model:** bundled but not promoted. The handcrafted evaluator and policy remain authoritative at this revision.
- **Learned trade model:** active.
- Exact mandatory decisions supersede normal MaxN where applicable.
- Proven tactical results supersede ordinary depth results.
- A selected exact action family can be solved after the main search.
- `safer_end_turn_alternative()` can replace an `EndTurn`.

This means evaluator-level diagnoses at this revision are meaningful; there is no hidden neural value head dominating the handcrafted utility.

---

## 2. Findings

### F1 - Posterior rejuvenation changes probabilities and can flip an exact mandatory decision

**Severity:** Major
**Classification:** correctness defect - belief probabilities
**Owner:** `src/core/tracker.ts:202`, `resampleDegenerateWorlds()`; `reweightTradeEvidence()` around `src/core/tracker.ts:833`

#### Observed behavior

When effective sample size collapses, the tracker systematically resamples the posterior. It then deliberately replaces duplicate high-probability samples with previously omitted low-probability worlds and finally gives every selected sample equal weight.

That is not posterior-preserving rejuvenation.

#### Concrete reproduction

A 12-world accepted-trade posterior had:

```text
main supported world        0.9889001
each low-probability tail   0.00100908
```

After rejuvenation:

```text
11 main copies x 1/12 = 0.916667
1 rare tail    x 1/12 = 0.083333
```

A roughly 0.10% tail therefore became 8.33%, an increase of roughly 82x.

The reported effective particle count simultaneously rose to approximately 12, making the posterior appear healthier rather than less faithful.

#### Recommendation consequence

An exact mandatory accepted-trade decision flipped.

With the corrupted belief:

```text
CancelTrade
  decision score ~= 20.734156
  root value     ~= 0.923085

ConfirmTrade
  legal support  = 0.916667
  decision score ~= 19.714148
```

With acceptance treated as hard evidence and impossible support removed:

```text
ConfirmTrade
  legal support  = 1.0
  decision score ~= 21.422661
  root value     ~= 0.948284

CancelTrade
  decision score ~= 20.734156
```

#### Expected behavior

Resampling may reduce particle count or duplicate high-mass worlds, but it must not change the represented posterior merely to increase particle diversity.

#### Root cause

The rejuvenation step treats distinct state diversity as more important than probability mass.

#### Alternative explanations ruled out

- No MaxN depth issue: this is an exact mandatory solver case.
- No evaluator tuning issue: the decision changes solely by correcting posterior support.
- No UI override involved in this reproduction.
- All reconstructed Rust worlds validate.

#### Minimal fix direction

Keep multiplicity during systematic resampling or perform a mathematically valid rejuvenation kernel that preserves the target posterior. Never replace a duplicate sample with an arbitrary omitted tail and then reset weights uniformly.

#### Repair proof

A regression should assert both:

```text
posterior event probability before resampling
~= posterior event probability after resampling
```

and that the accepted-trade case remains `ConfirmTrade` with legal support 1.0.

---

### F2 - Incoming trade evidence is reversed, and active trade evidence can be counted twice

**Severity:** Major
**Classification:** correctness defect - observation interpretation / Bayesian update
**Owner:** `src/core/trade-beliefs.ts:22`, `src/content/overlay.ts:2039-2084`, `src/core/tracker.ts:833+`

#### Observed behavior

The bridge normalizes active trades to the user's perspective:

```text
give    = cards the user gives
receive = cards the user receives
```

For an incoming trade, the WASM adapter correctly reverses that orientation back to the creator's perspective.

The tracker reconciliation path does not. It feeds the user-oriented vectors into a function that interprets `give` as cards owned by the trade creator.

#### Concrete reproduction

Two hidden worlds:

```text
A: Rival has lumber
B: Rival has brick
```

Incoming public offer:

```text
Rival gives lumber
User gives brick
```

Correct creator-oriented conditioning through the current TypeScript tracker module:

```text
P(Rival has lumber) = 0.985915
P(Rival has brick)  = 0.014085
```

Current bridge-local orientation passed unchanged into the tracker:

```text
P(Rival has lumber) = 0.014085
P(Rival has brick)  = 0.985915
```

The inference reverses.

There is a second issue: after durable trade-diff events already update the tracker, `reconciledState()` conditions on the same still-visible active offer again.

Example:

```text
prior                    50 / 50
after first observation  98.59 / 1.41
same evidence reapplied  99.98 / 0.02
```

There was no second observation.

#### Why it matters

This contaminates:

```text
P(opponent has resource)
P(opponent can build road)
P(opponent can build settlement)
P(opponent can build city)
P(opponent can buy development)
trade acceptance estimates
Monopoly value
robber steal value
opponent race threat
```

F1 proves that posterior distortions of this class are large enough to change an exact root recommendation.

#### Root cause

Two contracts disagree over the orientation and lifetime of active trade evidence.

#### Alternative explanations ruled out

A bundled executable probe imported the current `src/core/tracker.ts` directly. The same two-world prior was evaluated once with the bridge-local vectors and once with creator-relative vectors. The posteriors mirrored each other, so the error is the orientation contract at the bridge/reconciliation seam rather than the Bayesian likelihood implementation itself.

Applying the unchanged bridge-local offer a second time changed the wrong-side posterior from `0.985915` to `0.999796`, confirming that repeated reconciliation compounds one visible observation.

#### Minimal fix direction

Define one canonical trade orientation for tracker evidence and convert exactly once at the bridge boundary. Attach observation/event identity so panel-state reconciliation cannot reapply evidence already represented by a durable tracker event.

#### Repair proof

Replay an incoming trade through:

```text
bridge -> trade diff -> tracker -> reconciledState
```

and require:

- the correct creator resource gains probability;
- applying the same board snapshot again is idempotent.

---

### F3 - Publicly played progress cards can disappear from state reconstruction, creating a phantom development deck

**Severity:** Major
**Classification:** correctness defect - public state / legality
**Owner:** `src/page/bridge.ts:792-809`; `src/worker/deep-search.ts:259-365`

#### Observed behavior

The bridge can see Colonist's full public `developmentCardsUsed` array and already knows mappings for Knight, Monopoly, Road Building, Year of Plenty, and Victory Point.

But it exports only `playedKnights`.

For non-Knight cards, `publicDevelopmentEvidence()` relies on tracker history. If the extension starts or reloads after public Monopoly, Year of Plenty, or Road Building plays occurred, that history may be absent even though the current Colonist state contains the evidence.

#### Concrete reproduction

Publicly known state:

```text
10 Knights played
2 Monopoly played
2 Road Building played
2 Year of Plenty played
9 hidden dev cards still held
```

That consumes all 25 development cards:

```text
real deck = 0
```

With missing earlier log history, the current adapter reconstructs:

```text
playedDevelopment = [10,0,0,0,0]
remaining deck    = 6
```

The actual packaged `colonist_search_bg.wasm` returned:

```text
chosen = BuyDevelopment
```

Candidate values:

```text
BuyDevelopment
  mean  ~= 0.04407218
  lower ~= 0.00164477
  legal support = 1.0

best trade roots
  mean ~= 0.03861708

EndTurn
  mean ~= 0.00590658
```

The real game has no development card to buy.

#### Expected behavior

Every publicly played development card should be subtracted whether or not the extension observed its historical log line.

#### Why it matters

This is stronger than poor dev valuation. It makes an actually unavailable action the strategic winner.

#### Root cause

The public snapshot schema carries only Knight history.

#### Alternative explanations ruled out

- Rust's 25-card deck conservation works correctly when given correct counts.
- `marginal_development_value()` reacts correctly to different remaining deck compositions.
- The failure reproduces through the packaged WASM, not just TypeScript.

#### Minimal fix direction

Expose all public played development-card counts in `BoardSnapshot`, merge them with tracker evidence using a monotone maximum, and reconstruct remaining deck from that complete public evidence.

#### Repair proof

The same fixture must produce:

```text
developmentDeck total = 0
BuyDevelopment not in legal actions
```

---

### F4 - Rust can choose `CancelTrade`, while the overlay executes `ConfirmTrade`

**Severity:** Major
**Classification:** correctness defect - final recommendation authority
**Owner:** `src/core/trade-guard.ts:183-197`; `src/content/overlay.ts:2590-2636`

#### Observed behavior

For an outgoing offer with exactly one accepted player, the overlay invokes `shouldConfirmAcceptedTradeImmediately()`.

If the trade is a counteroffer, responses are complete, or no responses remain pending, `confirmImmediately` bypasses the requirement that the current deep action be `confirm-trade`.

The overlay then creates a `trade-partner` click for the accepted player.

#### Standard 10-VP reproduction

Valid state:

- opponent at 9 public VP;
- the proposed exchange gives them the missing ore for a city;
- offer is already accepted;
- both Cancel and Confirm remain protocol-legal.

Rust:

```text
ConfirmTrade threat = ImmediateWin
chosen = CancelTrade

CancelTrade:
  decision score ~= -19.208380

ConfirmTrade:
  decision score = -inf
```

The current overlay, once `responsesComplete=true`, takes the immediate-confirm branch and selects the accepted partner anyway.

#### Expected behavior

For an active decision state:

```text
Rust chosen A
-> displayed A
-> executed A
```

unless a mandatory legality transition invalidates A and forces a replan.

#### Root cause

A latency optimization became a second strategic authority.

#### Alternative explanations ruled out

- Confirm is technically legal, so ordinary legality validation does not protect against the wrong strategic action.
- Rust's trade safety correctly detects `ImmediateWin`.
- The exact mandatory solver correctly returns Cancel.

#### Minimal fix direction

Make `confirmImmediately` protocol-only after Rust has selected `ConfirmTrade`, or encode an exact logically forced confirmation condition where cancellation is no longer a legal action.

#### Repair proof

An accepted trade that newly enables an opponent win must display and execute Cancel when Rust chooses Cancel.

---

### F5 - Domestic-trade generation omits legal Colonist large-bundle offers

**Severity:** Major
**Classification:** correctness defect - missing legal Colonist candidates / search-space truncation
**Owner:** `engine/crates/catan-core/src/state.rs:984-1093`

Colonist is the product authority for this review. Its current base-game rules state that players may trade any number or combination of resources with other players for any combination or number in return, except trading the same resource type. Colonist's own trade-system article also gives `4 sheep` offered to opponents as an explicit supported example.

References:

- <https://colonist.io/catan-rules>
- <https://blog.colonist.io/improving-the-colonist-trade-system/>

The native generator instead rejects any domestic offer where:

```rust
give_total > 2 || receive_total > 2
```

#### Concrete reproduction

The lower-level state transition accepts a larger offer while the generated legal/search action set omits it:

```text
Offer 3 lumber for 1 brick

GameState::apply() -> Ok
legal_actions()    -> offer absent
```

Colonist explicitly supports even larger offers such as four cards to another player, so this is not a tabletop-rule disagreement. The production search cannot evaluate a legal Colonist action because that root never enters candidate ranking.

#### Why it matters

Large bundles can be strategically material when a player has severe resource surplus, seven-risk, a port-driven economy, or needs one scarce card immediately. Missing the action entirely is different from searching it and deciding it is poor.

#### Root cause

A search-space bounding rule is embedded in domestic-trade action generation. It changes the modeled game contract instead of merely prioritizing a complete legal action set.

#### Alternative explanations ruled out

- The lower-level trade transition accepts the action.
- Colonist's current public rules explicitly allow arbitrary bundle sizes.
- Colonist's own product article explicitly describes offering four sheep to opponents.

Two previously considered hypotheses are **not classified as findings here**: whether Colonist permits another player trade after one completed domestic trade in the same turn, and whether a Colonist offer can be recipient-targeted rather than broadcast. Those require product-specific behavioral proof before they are treated as engine defects.

#### Minimal fix direction

Keep Colonist legality complete, then bound search with value-guided proposal generation, progressive widening, or another ranking mechanism that does not redefine legal bundle size.

#### Repair proof

At minimum, a legal `3 -> 1` and `4 -> 1` player offer must be representable by the native action generator and survive through the TypeScript/WASM boundary when public state permits it.

---

### F6 - Whole-turn planning does not actually protect domestic-trade continuations

**Severity:** Significant
**Classification:** strategic-model defect / planner integration defect
**Owner:** `engine/crates/catan-search/src/planner.rs:155-202, 277-307`

#### Observed behavior

The planner explicitly models opponent responses probabilistically.

However, every non-root trade-response decision returns:

```rust
completed: false
```

unconditionally.

Then `plan_adjusted_priors()` discards every plan where `plan.completed == false`.

Domestic-trade roots therefore never receive the intended 60% whole-turn planner adjustment.

#### Concrete reproduction

A valid two-player position one grain short of settlement was run with:

```text
planner nodes = 100,000
branch cap    = 24
root cap      = 48
```

Ten representative trade roots each consumed roughly:

```text
2,083 nodes
value ~= 28.9209
completed = false
sequence = [OfferTrade]
```

The result does not improve with extra planner budget because the completion state is structurally false.

#### Candidate-value consequence

The trade roots retain their shallow policy prior rather than:

```text
40% policy + 60% normalized whole-turn endpoint
```

This is especially relevant because live root width is only eight.

#### Expected behavior

A weighted trade-response branch whose descendants reach a valid turn endpoint should propagate endpoint completeness sufficiently for its root to participate in planner prior adjustment.

#### Root cause

Completion metadata is dropped at stochastic opponent trade nodes.

#### Alternative explanations ruled out

100,000 nodes rules out ordinary planner starvation.

#### Minimal fix direction

Propagate completion probability/status through stochastic trade-response branches and allow fully evaluated expected trade continuations to influence root priors.

#### Repair proof

A `trade -> settlement` or `trade -> road -> settlement` fixture must return a completed planner root and receive the planner-adjusted prior.

---

### F7 - Deep MaxN exhibits information-set strategy fusion

**Severity:** Major
**Classification:** correctness defect - hidden-information leak in opponent model
**Owner:** `engine/crates/catan-search/src/depth.rs`; `engine/crates/catan-search/src/shared.rs:1-11`

`shared.rs` already documents the current limitation: observation-safe opponent mixtures are experimental and are not enabled.

Searchers are constructed with:

```text
observation_safe_root: None
```

#### Mechanism

Each belief particle is a full determinization.

At simulated decision nodes, MaxN chooses the action maximizing the acting player's utility within that fully specified world.

Although candidate priors can be observation-safe, the child evaluation sees hidden hands.

A simulated player can therefore choose different strategies depending on a third party's cards that the acting player cannot know.

#### Release-mode reproduction

Two valid states had:

```text
same observation_hash for actor 3
same actor 3 legal actions
same public position
```

Only player 1's hidden three-resource composition differed.

State A:

```text
chosen: MaritimeTrade 4 ore -> lumber
value(lumber) ~= 0.292196
value(brick)  ~= 0.292196
```

State B:

```text
chosen: MaritimeTrade 4 ore -> brick
value(brick)  ~= 0.282909
value(lumber) ~= 0.275919
```

Actor 3 cannot observe the hidden change that caused the policy change.

#### Expected behavior

All states within an acting player's information set must induce the same decision policy, with uncertainty integrated into expected values rather than resolved before the actor chooses.

#### Why it matters

This distorts opponent forecasting and therefore root-player values, particularly for:

- races;
- robber timing;
- resource denial;
- trades;
- Monopoly;
- blocking;
- endgame turns.

#### Alternative explanations ruled out

The two states have the same observation identity and legal-action set. The only causal input is hidden third-party information.

#### Minimal fix direction

Use an observation-keyed opponent decision node shared across indistinguishable particles, or an explicit observation-safe stochastic opponent policy integrated across posterior worlds.

#### Repair proof

Permutation/hidden-state tests must enforce:

```text
same actor observation
-> same simulated policy distribution
```

even when other players' hidden cards differ.

---

### F8 - The engine can EndTurn into a guaranteed next-turn loss despite having a verified blocking settlement

**Severity:** Major
**Classification:** strategic-model defect
**Owners:** `engine/crates/catan-search/src/threats.rs:337-363`, `engine/crates/catan-search/src/depth.rs:651`, `engine/crates/catan-search/src/eval.rs`

#### Standard 10-VP reproduction

Valid three-player state:

```text
Opponent:
  9 public VP
  3 cities
  1 settlement
  Largest Army
  enough resources for one winning settlement

Current player:
  settlement resources available
  several legal settlement sites
```

The threat detector itself returns:

```text
ImmediateMainPhaseWin
```

For contested vertex 0:

```text
action_blocks_threat(BuildSettlement vertex0) = true
```

After applying the block:

```text
detected opponent threats = []
opponent next-main-phase settlement/city actions = []
```

This is not subjective strategic advice. The move removes a concretely represented immediate win.

#### Production result

Settlement priors:

```text
vertex 15  33.026
vertex 14  24.707
vertex 12  12.290
vertex 0    6.100  <- forced blocker
```

At branch cap 8, the blocker disappears.

Production search:

```text
chosen = EndTurn
all surviving root values ~= 0
```

#### Wider search falsification

The same state was increased to:

```text
depth 6
branch 32
64,000 nodes
```

The blocking settlement now survives root selection.

The result is still:

```text
chosen = EndTurn
```

Branch truncation is therefore not the whole explanation.

#### Evaluator evidence

Before block:

```text
strategic utility root ~= 18.217
strategic utility leader ~= 83.071
evaluate ~= [~0, 1.0, ~0]
```

After the strategically mandatory low-production settlement:

```text
root utility ~= 8.646
leader utility ~= 83.853
evaluate ~= [~0, 1.0, ~0]
```

The evaluator penalizes spending resources and taking a weak vertex enough that the guaranteed-loss blocker looks worse.

#### Existing unused mechanism

`force_threat_blocking_actions()` already exists, but its comment says it is disabled in production pending posterior aggregation and post-apply verification.

The synthetic case supplies exactly that post-apply verification.

#### Expected behavior

When a legal action is proven to remove an otherwise forced immediate opponent win, ordinary static utility differences should not allow `EndTurn` to dominate it.

#### Minimal fix direction

Do not hard-code a generic "blocking is good" bonus.

Elevate a posterior-safe invariant:

```text
if non-blocking actions permit a near-certain forced win
and a candidate demonstrably removes it,
preserve/search the blocker and value survival lexicographically
before ordinary strategic utility.
```

#### Repair proof

The reproduced position should:

1. detect the threat;
2. prove the candidate removes it across required posterior support;
3. preserve the candidate before root truncation;
4. choose it over EndTurn.

---

### F9 - Production root width 8 is materially prior-sensitive; more nodes cannot recover a pruned action

**Severity:** Significant
**Classification:** search-budget weakness / intentional approximation with measured consequence
**Owner:** `engine/crates/catan-search/src/shared.rs:24-28`, `engine/crates/catan-search/src/depth.rs:649-698`

#### Release-mode reproduction

State hash:

```text
2ce1d7ba37219fd3
turn 9
actor 0
hand [1,1,2,0,5]
103 generated legal roots
```

Production:

```text
depth 4 / branch 8 / 4k
chosen = MaritimeTrade 4 ore -> grain
value ~= 0.190885
```

Increasing only node budget:

```text
8k   same
16k  same
32k  same
```

The omitted candidate cannot be recovered.

At branch 12:

```text
chosen = OfferTrade 2 ore -> 1 grain
value ~= 0.192745
```

The same domestic root remains best at branch 16, 24, and 32, including depth 6.

#### Expected behavior

This does not prove the branch-12 trade is objectively superior. The value difference is modest and there is no ground-truth oracle proving it wins more games.

The demonstrated issue is that production outcome is root-prior/width-sensitive, not node-budget-sensitive.

#### Why it matters

This interacts directly with F6: domestic trades are exactly the class not receiving the whole-turn planner prior that was meant to protect useful continuation roots.

#### Minimal fix direction

Do not automatically increase every live branch cap. First repair planner priors and legal candidate generation, then benchmark adaptive/progressive root widening.

#### Repair benchmark

Measure recommendation crossover and live latency across:

```text
branch 8 / 10 / 12 / 16
4k / 8k / 16k nodes
```

on recorded difficult positions.

---

## 3. Minor correctness issue

### F10 - Opponent `playedDevelopmentThisTurn` is always reconstructed as false

**Severity:** Minor
**Classification:** state reconstruction correctness; user-visible recommendation flip not reproduced
**Owner:** `src/worker/deep-search.ts:755`

`src/worker/deep-search.ts:755` sets:

```text
playedDevelopmentThisTurn =
  isOwn && board.ownDevelopmentCards.hasPlayedThisTurn
```

Every opponent therefore enters Rust with the flag false.

Rust correctly enforces one development-card play per turn, but only if this flag is accurate.

This is a genuine state reconstruction error. Its live user-facing reach is narrower than F3 because most displayed root decisions happen on the user's turn, when opponents' previous turn flags have naturally reset. No displayed recommendation flip was reproduced, so it is not classified as Major.

---

### F11 - Recovered turn 54 proves same-turn search-order budget starvation can choose a dominated root

**Severity:** Blocker
**Classification:** recommendation correctness - search traversal / node-budget fairness
**Owner:** `engine/crates/catan-search/src/depth.rs:198-377`, especially the globally shared `self.nodes` / `node_limit` inside same-turn decision recursion

A real captured turn was recovered and replayed against the reviewed `aebe9ed` engine.

Public state:

```text
turn 54
root player: Rolo#5575
hand: 0 lumber, 4 brick, 1 wool, 3 grain, 1 ore
trade ratios: lumber 4, brick 4, wool 2, grain 4, ore 4
visible VP: root 2, Kyle 4, Clerc 3, Gall 2
```

The historical deep recommendation was `MaritimeTrade brick -> lumber ratio 4`. Current packaged-WASM replay reproduces that recommendation at the live 4k/350ms profile, at 8k/700ms, at 16k/1.4s after the deadline no longer fires, at 32k, and at depth 6 / 250k. Timeout is therefore not the causal explanation.

Local evaluation points the other way. On a representative state:

```text
root prior: BuyDevelopment > maritime
one-step normalized eval: BuyDevelopment >> maritime
whole-turn planner at 100k nodes: BuyDevelopment slightly > maritime
```

The key invariant is stronger than an evaluator comparison. For every possible development-card draw (`Knight`, `VictoryPoint`, `RoadBuilding`, `YearOfPlenty`, `Monopoly`):

```text
Maritime(brick -> lumber) -> BuyDevelopment -> Resolve(card)
==
BuyDevelopment -> Resolve(card) -> Maritime(brick -> lumber)
```

The resulting `GameState`, `state_hash`, and evaluation are identical. Buying first additionally reveals the development card before deciding whether the maritime trade is still useful, so the buy-first order has a strict information advantage and cannot rationally be valued lower solely because of action order.

The failure localizes inside same-turn recursive search. After `BuyDevelopment` and a draw, the same maritime action is legal, improves immediate evaluation, and remains inside a wide root, yet its backed-up value becomes exactly `0.0` while an earlier domestic-trade subtree consumes the shared node prefix. Suppressing domestic trades on that same post-draw state restores a positive maritime value above `EndTurn`.

#### Root cause

Root actions receive explicit quotas, but siblings inside a same-turn decision node do not. `Searcher::visit()` walks ranked actions against one shared `self.nodes < self.node_limit` budget. Earlier high-branching domestic-trade continuations can exhaust that budget before later legal siblings receive comparable search.

#### Minimal fix direction

Allocate fair local sibling budgets, or otherwise make same-turn recursive expansion order-insensitive enough that a high-branching earlier action cannot zero an unevaluated later action.

#### Repair proof

- exact turn-54 replay;
- commutative action-order invariant above;
- permutation test over same-turn sibling ordering;
- later siblings must retain bounded search support when earlier domestic-trade branches expand heavily.

---

### F12 - Public-board fallback can collapse the resource posterior to zero on midgame attach/reload

**Severity:** Major
**Classification:** state reconstruction / availability of recommendation path
**Owner:** `src/content/overlay.ts:stateFromPublicBoard()` and `reconciledState()`; `src/worker/deep-search.ts` zero-world guard

When no durable tracker session is available, `stateFromPublicBoard()` creates 16/24 particles with the local hand exact and every opponent hand empty. `reconciledState()` then enforces public hand sizes.

The recovered turn-54 public board has hand sizes:

```text
Rolo 9
Clerc 8
Kyle 8
Gall 7
```

Replaying the fallback path creates 24 worlds and public-resource reconciliation reduces them to zero because every opponent was seeded with zero cards. The deep adapter then throws:

```text
Deep Search has no resource worlds consistent with public evidence
```

This is deterministic for a normal midgame attach/reload where opponents publicly hold cards; it is not a sampling miss.

#### Minimal fix direction

Seed fallback particles from public hand sizes and resource conservation, or repair unknown opponent compositions from the public pool before exact-hand-size reconciliation.

#### Repair proof

A midgame snapshot with nonzero opponent hand sizes and no prior log/session must produce at least one conservation-consistent world and reach packaged WASM.

---

### F13 - Decision-source telemetry can mislabel deep-selected actions

**Severity:** Minor
**Classification:** observability / auditability
**Owner:** TypeScript final-action source tagging in the overlay

Two recovered traces looked like deep-result arbitration failures until the action payloads were compared directly:

```text
turn 29 robber:
  deep move-robber target/victim == final target/victim
  finalActionSource = placement-heuristic

turn 29 main:
  deep maritime wool -> ore ratio 2 == final trade-builder payload
  finalActionSource = coach-goal
```

The labels were wrong, but the first action itself matched deep exactly; the second failed during execution rather than being replaced by a different strategic choice. These traces therefore do **not** prove a general deep-arbitration override.

F4 remains the concrete authority exception: the accepted-trade fast-confirm path can execute `ConfirmTrade` even when Rust exact authority selected `CancelTrade`.

#### Minimal fix direction

Derive source labels from the actual mapped final action and authority transition, not from the rendering/helper path that happened to construct the same action.

---

### F14 - The live 12-particle strategic coreset can change the final action family relative to the full posterior

**Severity:** Major
**Classification:** recommendation correctness - belief compression / strategic approximation
**Owners:** `engine/crates/catan-search/src/shared.rs:67-298`, `select_strategic_particles()`; `engine/crates/catan-search/src/depth.rs:616-623`

#### Concrete reproduction

A deterministic 24-world belief fixture was constructed with:

- every `GameState::validate()` returning `Ok(())`;
- all 24 worlds sharing the same root-player `observation_hash(0)`;
- only hidden opponent resource composition varying;
- the root player owning a playable Monopoly card.

With the complete 24-world posterior, Deep MaxN chooses the Monopoly family. With the production 12-particle strategic coreset, it chooses `EndTurn`.

The split persists when the node budget is increased:

| Nodes | Full 24-world posterior | Production 12-world coreset |
| ---: | --- | --- |
| 8,000 | `PlayMonopoly(Grain)` | `EndTurn` |
| 16,000 | `PlayMonopoly(Grain)` | `EndTurn` |
| 32,000 | `PlayMonopoly(Grain)` | `EndTurn` |
| 64,000 | `PlayMonopoly(Grain)` | `EndTurn` |
| 128,000 | `PlayMonopoly(Grain)` | `EndTurn` |

At 128k nodes, the full-posterior search values were approximately:

```text
PlayMonopoly(Grain)  0.966289
EndTurn               0.939317
```

The regret of the coreset-selected `EndTurn` under the full-posterior search is therefore about `0.0270` normalized value in this fixture.

The exact Monopoly family solver also disagrees after compression:

```text
full posterior exact Monopoly target     Grain
12-particle coreset exact Monopoly target Ore
```

In the compressed production search the root chooses `EndTurn`, so the later exact-family refinement is never invoked and cannot rescue the omitted Monopoly family.

#### Alternative explanations ruled out

- Increasing nodes through 128k does not remove the split.
- Giving the 24-world search more total nodes to equalize per-particle compute does not remove the split.
- All worlds are valid and root-observation-identical.
- A temporary copy of the engine with the separate duplicate-Monopoly-root bug removed still produces the same `Monopoly` versus `EndTurn` family split.

#### Root cause

`select_strategic_particles()` preserves a compact signature and then assigns every omitted world's mass to its nearest retained representative. The signature preserves several coarse strategic properties, but it does not preserve the full opponent resource distribution or per-resource Monopoly payoff. In this fixture that reassignment changes the sufficient statistics needed to value the Monopoly family strongly enough to remove it as the chosen root.

#### Minimal fix direction

Do not rely on a 12-state geometric coreset for decisions whose value depends on posterior-wide sufficient statistics that can be computed cheaply. Preserve the relevant resource/development statistics explicitly, or evaluate exact/high-impact action families against the full posterior before allowing compression to remove the family from the strategic root.

#### Repair proof

A regression should compare production compression against the full posterior on same-observation fixtures and require bounded root regret. The Monopoly fixture above must not change from a materially winning Monopoly line to `EndTurn` solely because of strategic particle compression.

---

### F15 - Monopoly root replacement can duplicate one identical action and produce impossible legal support

**Severity:** Minor
**Classification:** search-root bookkeeping defect; recommendation impact not independently isolated
**Owner:** `engine/crates/catan-search/src/depth.rs:656-672`

#### Concrete reproduction

On a fully valid one-particle state with posterior mass exactly `1.0`, the depth report contained:

```text
PlayMonopoly(Grain) legal_weight = 2.0
```

`legal_weight` is posterior probability mass and therefore cannot exceed `1.0`.

The root initially contains multiple parameterized `PlayMonopoly(resource)` actions. The code solves the exact Monopoly family and replaces the first Monopoly entry with the exact best action, but it does not remove another root entry that may already equal that best action. The same action is then searched twice and accumulated into one `DepthActionValue`.

A temporary-copy deduplication patch changed the reproduced `legal_weight` from `2.0` to `1.0`. The chosen action in that simple control remained Monopoly, so a user-visible recommendation flip caused solely by F15 has not been established.

#### Why it matters

The duplicate consumes two root budget shares, corrupts `legal_weight`/availability telemetry, and can crowd another action out of a branch-capped search. It also contaminated early coreset diagnostics, although removing it did not remove F14.

#### Minimal fix direction

When collapsing Monopoly to one exact family representative, remove every existing Monopoly parameterization and insert exactly one selected action while preserving root order/cap semantics.

#### Repair proof

Assert root action uniqueness and `0 <= legal_weight <= 1` for every belief depth candidate. A one-particle Monopoly state must expose exactly one Monopoly root representative after family collapse.

---

## 4. Research and benchmark findings

### R1 - Strategic coreset signatures omit additional strategically distinct tails

F14 now proves that the production coreset can change the final action family, so coreset fidelity is no longer only a theoretical benchmark concern.

`particle_signature()` preserves:

- settlement affordability;
- city affordability;
- hidden VP;
- Monopoly ownership;
- coarse ore/grain concentration;
- near-victory status.

It does not explicitly preserve:

- road affordability;
- development purchase affordability;
- Knight/Largest-Army readiness;
- Road Building or Year of Plenty tactical identities;
- per-resource opponent totals needed for Monopoly target/value;
- all race/blocking distinctions that can matter to downstream search.

Separate tail probes found that a distinct road-affordable `[1 lumber, 1 brick]` opponent tail can disappear at 4% posterior mass, and that a rare hidden-Knight tail can disappear when an opponent sits at two played Knights. These examples help explain the compression risk; F14 supplies the direct recommendation-level reproduction.

### R2 - Hidden-development sampling is seed-sensitive

The same semantic public board and hand were evaluated with hidden development-card identities resampled using eight deterministic seeds.

Across eight seeds:

```text
BuyDevelopment       5 / 8
domestic trade root  3 / 8
```

All runs completed the full 4,000 nodes with no deadline.

This is genuine particle sensitivity, not timing noise.

Before increasing live search depth, stratified hidden-development sampling or exact marginal handling is likely higher leverage.

### R3 - Current decision traces are insufficient for a causal "why did this root disappear?" replay

`src/core/decision-trace.ts` records useful information:

- replay state/board;
- chosen;
- candidate means;
- lower values;
- legal support;
- visits;
- seed;
- effective particle count;
- final action/source.

It does not persist:

- pre-truncation root candidates;
- planner-adjusted prior;
- root quota score;
- per-root node allocation;
- representative particle mapping;
- threat/safety override reason;
- exact/tactical principal-line provenance;
- depth/node data for each candidate.

Stored replay worlds are also truncated to the top 128.

The WASM depth report currently serializes candidate `prior: 0.0` even though prior ordering was used before truncation.

The historical turn-54 state was later recovered from persisted runtime evidence and replayed, producing F11. The stored decision trace still did not expose the pre-truncation roots, planner priors, or same-turn sibling budget consumption needed to explain the reversal; custom native/TypeScript probes were required to localize it.

---

## 5. Recovered `4 brick -> 1 lumber` versus development-card case

The original live position was recovered after the earlier hand-only synthetic experiment. That earlier synthetic control remains useful because it showed the hand itself was insufficient to cause the bad recommendation; the full board, belief, deck, and search context were necessary.

Recovered turn-54 facts:

```text
hand: 0 lumber, 4 brick, 1 wool, 3 grain, 1 ore
brick maritime rate: 4:1
wool maritime rate: 2:1
public bank: [13, 15, 9, 13, 13]
visible VP: root 2, Kyle 4, Clerc 3, Gall 2
```

The current `deep-maxn-v9` replay reproduces the historical `4 brick -> 1 lumber` recommendation. Increasing time/nodes through 32k and depth 6 / 250k does not change it, so this is not primarily a 350ms timeout artifact.

The decisive comparison is action order:

```text
A = Maritime(brick -> lumber)
B = BuyDevelopment
```

For every development-card identity, `A -> B -> draw` and `B -> draw -> A` reach the same state and evaluation. `B` first is at least as informative because the draw is known before deciding whether to spend four brick. Yet the recursive search undervalues the maritime continuation after `B` when earlier domestic-trade branches consume the shared same-turn node budget.

When domestic-trade continuations are suppressed only for that post-draw probe, the maritime continuation becomes positive again and outranks `EndTurn`. That isolates the failure to same-turn sibling budget starvation described in F11, not to the development-card evaluator or a hidden trade cost.

**Conclusion:** the original live recommendation is reproduced and the root cause is localized. This case is no longer unresolved.

---

## 6. Synthetic strategic cases

| Case | Result | Classification |
| --- | --- | --- |
| Accepted trade hard evidence | Wrong posterior can flip Confirm -> Cancel | Correctness defect |
| Incoming trade orientation | Posterior points at opposite resource | Correctness defect |
| Empty real dev deck | WASM chooses phantom BuyDevelopment | Correctness defect |
| Rust Cancel vs overlay confirm | UI can execute opposite action | Correctness defect |
| Colonist 3-for-1 domestic offer | Lower-level transition accepts it; generated root omits it | Correctness defect |
| Trade -> settlement planner | Trade roots never complete | Strategic-model defect |
| Same-observation hidden-card swap | Simulated opponent action flips | Hidden-information defect |
| Forced next-turn block | EndTurn despite verified blocker | Strategic-model defect |
| Recovered turn 54 | `4 brick -> lumber` persists; same-turn node starvation violates action-order dominance | Correctness defect |
| Midgame public-board fallback | 24 worlds collapse to zero against public hand sizes | Correctness defect |
| Full 24-world posterior vs live 12-world coreset | `PlayMonopoly(Grain)` -> `EndTurn`, stable through 128k nodes | Correctness defect |
| One-particle Monopoly root | `legal_weight = 2.0` from duplicate identical root | Bookkeeping defect |
| Hand-only 4B -> lumber synthetic | BuyDevelopment stable | Pass; proved hand alone was not causal |
| Port 6-pip vs non-port 8-pip | Port can win | Pass |
| Productive vs dead road | Strong separation | Pass |
| Robber: 9-VP city-ready leader vs stronger raw-pip target | Leader correctly targeted | Pass |
| Knight-heavy / VP-heavy / progress-heavy dev decks | Value changes appropriately | Pass |
| Branch 8 vs 12 | Recommendation changes only after root widening | Prior-sensitive |
| Hidden-dev deterministic seeds | 5/8 dev, 3/8 trade | Particle-sensitive |
| Rare Knight/road belief tail | Can disappear in 12-world coreset | Approximation |

---

## 7. Development-card model assessment

The underlying Rust development valuation is one of the stronger parts of the engine.

Measured `marginal_development_value()` responses:

```text
Knight-heavy deck                         ~= 0.1865
VP-heavy deck                             ~= 7.4000
progress-card-heavy deck                  ~= 0.6176
near-empty Knight-only deck               ~= 0.1754
Knight-heavy with player at 2 Knights     ~= 0.3936
```

The evaluator therefore does not use a fixed generic development-card value.

It responds to:

- remaining deck composition;
- hidden VP probability;
- Largest Army opportunity;
- endgame horizon;
- action-card congestion.

The end-to-end rating is nevertheless Incorrect because F3 can feed it the wrong deck.

---

## 8. Strategic evaluator assessment

The evaluator explicitly contains meaningful terms for:

- victory points;
- hidden victory points;
- production;
- resource scarcity;
- number diversity;
- resource diversity;
- dynamic hand value;
- build tempo;
- expansion paths;
- expansion survival;
- expansion portfolio;
- ports;
- Longest Road acquisition/retention;
- Largest Army acquisition/retention;
- development cards;
- discard/seven exposure;
- speculative-road debt;
- robber effects;
- bank shortages;
- opponent race progress.

This is materially richer than raw pip counting.

The problem is therefore not that the evaluator lacks Catan strategy.

The clearest evaluator defect is narrower and more serious: **immediate survival is not lexicographically dominant enough in F8.**

---

## 9. External Catan strategy and AI comparison

External material should be treated as supporting evidence, not as an oracle.

Catan AI systems such as Catanatron use strong search plus handcrafted value models, reinforcing the importance of evaluator quality without implying that their weights transfer directly to this engine.

JSettlers separates opening strategy, robber strategy, Monopoly strategy, player tracking, build planning, and negotiation. That structure illustrates how much Catan strength comes from maintaining coherent strategic state rather than one generic score.

Research on Catan AI also treats the game as a multiplayer stochastic imperfect-information problem, which supports the importance of information-set-consistent opponent decisions rather than determinization-specific policies.

### Principle classification

| Strategic principle | Current engine |
| --- | --- |
| Ore/wheat/sheep city + dev strategy | **Explicitly modeled** |
| Wood/brick expansion strategy | **Explicitly modeled** |
| Production quality | **Explicitly modeled** |
| Number diversity | **Explicitly modeled** |
| Resource diversity | **Explicitly modeled** |
| Resource scarcity | **Explicitly modeled** |
| Port specialization/liquidity | **Explicitly modeled** |
| Avoid pointless roads | **Explicitly modeled** |
| Road -> settlement continuation | **Explicitly modeled** |
| Longest Road timing | **Explicitly modeled** |
| Largest Army timing | **Explicitly modeled** |
| Development-deck composition | **Explicitly modeled**, but end-to-end state can be wrong |
| Robber production denial | **Explicitly modeled** |
| Robber leader urgency | **Explicitly modeled** |
| Discard / seven risk | **Explicitly modeled** |
| Expansion races | **Partially modeled** |
| Hard blocking of imminent wins | **Modeled incorrectly** |
| Domestic trade acceptance | **Partially modeled** |
| Large-bundle Colonist domestic offers | **Modeled incorrectly** |
| Recipient-specific trade strategy | **Open product-contract question; not a confirmed defect** |
| Multiple completed domestic trades in one turn | **Open product-contract question; not a confirmed defect** |
| Trade diplomacy/history | **Partially modeled** |
| Opponent archetype/style | **Partially modeled** |
| Information-set-consistent opponent strategy | **Modeled incorrectly** |
| Table reputation / social politics | **Not modeled** |
| Feeding a leader | **Partially modeled**, with explicit hard vetoes |
| Late-game race-to-win survival | **Modeled incorrectly in adversarial case** |

---

## 10. Required coverage matrix

| Area | Rating | Reason |
| --- | --- | --- |
| Board topology | **Strong** | Full graph transferred and validated |
| Vertices | **Strong** | Adjacencies/buildings/open sites retained |
| Roads | **Adequate** | Connectivity/blocking and frontier value modeled |
| Blocking | **Incorrect** | F8 |
| Ports | **Strong** | Correct ratios and strategic value |
| Resource production | **Strong** | Pip probability, cities, robber, bank shortage |
| Resource scarcity | **Adequate** | Dynamic evaluator support, but belief errors contaminate opponent scarcity |
| Bank card counts | **Adequate** | Strong conservation/visible bank handling, downstream beliefs can be corrupt |
| Opponent card counting | **Incorrect** | F1/F2 |
| Hidden-hand probabilities | **Incorrect** | F1/F2 |
| Development deck | **Incorrect** | F3 |
| Opponent hidden development cards | **Weak** | Sampling, seed sensitivity, coreset distortion, turn flag |
| Strategic belief compression | **Incorrect** | F14 changes final action family relative to full posterior |
| Dev-card purchase value | **Incorrect end-to-end** | Core evaluator strong; deck input can be false |
| Robber | **Strong** | Standard imminent-win targeting probe passed |
| Trade | **Incorrect** | F1/F2/F4/F5/F6 |
| Longest Road | **Adequate** | Correct graph length/race outlook; bounded horizon |
| Largest Army | **Adequate** | Knight value/race state modeled |
| Discard risk | **Strong** | Nonlinear composition-aware loss + configurable limit |
| Settlement races | **Weak** | Arrival/survival modeled, but F8 hard race fails |
| Whole-turn plans | **Weak** | Builds work; domestic-trade roots fail completion propagation; F11 shows same-turn traversal starvation |
| Opponent strategy | **Incorrect** | F7 information-set leak |
| Root/search bookkeeping | **Incorrect** | F11/F15 |
| Late-game race urgency | **Incorrect** | F8 |

---

## 11. Recommendation stability

| Position | Classification | Evidence |
| --- | --- | --- |
| Hand-only 4B -> lumber synthetic | **Stable control** | BuyDevelopment unchanged through 250k/depth6; hand alone not causal |
| Recovered turn 54 | **Incorrect and budget-order-sensitive** | maritime remains selected through 250k/depth6; continuation restored when earlier domestic-trade branches are suppressed |
| Turn 9 `2ce1d7...` | **Prior/root-width-sensitive** | nodes 4k -> 32k unchanged; branch 8 -> 12 flips root |
| Hidden-dev fixture | **Particle-sensitive** | 5/8 dev vs 3/8 trade |
| Full 24 vs strategic 12 particles | **Incorrect compression** | Monopoly -> EndTurn persists from 8k through 128k nodes |
| Forced blocker | **Evaluator/survival-dominated** | branch32/depth6 still passes |
| Accepted active trade | **Tactical/exact** | exact answer flips when belief support is corrupted |
| Phantom empty dev deck | **State correctness** | illegal candidate manufactured before strategic comparison |
| Duplicate Monopoly root | **Bookkeeping defect** | one-particle legal support reaches 2.0; chosen flip not isolated |

One timing suspicion was explicitly refuted during the review. A native debug build initially exhausted the cooperative deadline before meaningful search, but the actual packaged WASM completed a 4,000-node depth-4 trade-heavy request in roughly 68 ms on this WSL machine with `deadline=false`; a 64k/depth6/branch32 run was roughly 260 ms. The debug timing artifact is therefore not a reported defect.

---

## 12. Decision trace and reproducibility status

The trace system is directionally good but cannot yet satisfy a causal "why did the engine choose this?" standard.

A bad live recommendation can often be reconstructed at the state level, but the trace cannot currently answer:

```text
Why was candidate X absent?
What was its planner value?
What prior pushed Y inside branch cap 8?
Which representative particle removed a strategic tail?
Did a threat override execute?
Was the final action exact, tactical, or ordinary MaxN at candidate level?
```

Minimum additional telemetry:

```text
complete posterior fingerprint
selected strategic particle indexes + weights
pre-truncation root list
policy prior
planner value/completed flag
quota score
post-truncation root list
per-root node allocation
exact/tactical authority reason
safety override reason
candidate value before/after override
final TypeScript authority transition
```

That telemetry should be added before broad evaluator tuning.

---

## 13. What is already working well enough not to fix by intuition

The review did not support claims that these areas are fundamentally absent:

- topology;
- port valuation;
- resource scarcity;
- city vs settlement economics;
- dynamic development-deck value;
- Largest Army;
- Longest Road graph logic;
- dead-road penalties;
- robber leader targeting;
- seven/discard exposure;
- expansion portfolios;
- state-validated board execution.

The recovered turn-54 `4 brick -> lumber` case is now direct evidence of a current search defect, but its root cause is same-turn budget starvation rather than a generic failure to value development cards.

Broad retuning of evaluator constants now would risk masking the actual correctness failures.

---

## 14. Causal groups for implementation-plan discussion

This section is **not** an implementation plan. It groups defects by dependency so the repair order can be decided after reviewing the report together.

### A. Information-state correctness

```text
F1 posterior-preserving resampling
F2 trade orientation and evidence idempotence
F3 complete public development-play conservation
F10 opponent development-turn reconstruction
F12 midgame public-board fallback
```

Search cannot be stronger than the state it receives.

### B. Colonist action/authority contract

```text
F4 Rust exact trade veto must remain authoritative through the overlay
F5 Colonist-legal large domestic bundles must be representable
F6 domestic-trade continuations must participate in whole-turn planning
```

The unproven questions about multiple completed domestic trades per turn and recipient-specific offers remain outside this confirmed repair set until Colonist behavior is verified.

### C. Strategic-search correctness

```text
F7 opponent policy must be information-set-consistent
F8 verified forced-loss blockers must dominate losing pass actions
F11 same-turn sibling search must not depend catastrophically on traversal order
F14 strategic belief compression needs a bounded-regret/full-posterior contract
F15 root-family collapse must preserve unique actions and probability support
```

F9 root-width sensitivity and R2 hidden-development seed sensitivity should be re-benchmarked after these correctness defects are removed, before deciding whether larger live budgets are necessary.

### D. Auditability

```text
F13 final action source must identify the actual authority path
R3 traces need enough root/planner/particle provenance to explain a bad recommendation without custom probes
```

---

## Final answer to the primary question

**Does `deep-maxn-v9` consistently preserve, search, and correctly value the actions a strong Catan player should seriously consider?**

**Not yet.**

It frequently does. The strategic model genuinely understands production, ports, development composition, expansion, roads, trophies, robber pressure, hand safety, and coherent builds.

But the current recommendation cannot be treated as reliably authoritative because there are reproducible positions where:

```text
the posterior is mathematically altered;
incoming trade evidence means the opposite of what happened;
publicly exhausted dev cards reappear in the deck;
a midgame fallback can collapse every hidden-resource world;
a Colonist-legal large player trade never enters search;
an opponent strategy uses information that player cannot know;
a verified block of an imminent win loses to EndTurn;
same-turn traversal order makes a dominated action order win on captured turn 54;
12-particle compression changes a full-posterior Monopoly decision into EndTurn;
Rust says CancelTrade while UI can execute ConfirmTrade.
```

Those failures identify where incorrect preferences originate. They are not generic algorithm weakness.

No implementation code was changed as part of this review.
