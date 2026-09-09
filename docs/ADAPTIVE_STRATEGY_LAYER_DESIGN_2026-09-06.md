# Adaptive strategy layer for Colonist Assistant

## Latest clarification: resource saving and GPU routing — 2026-09-08

New [three-player matched pilot](ADAPTIVE_STRATEGY_3P_PILOT_2026-09-08.md): 12 terminal games, zero cutoffs; baseline and adaptive each won 1/3 in both trade settings. M2 ran on 743 decisions but admitted zero challengers. This supplies no strategy-strength improvement evidence and makes a specific resource-saving causal scenario the next task. Reproduction runner: `scripts/run-adaptive-strategy-pilot.mjs`.

This dated note takes precedence over older backend-promotion wording below. Soundness and robustness govern acceptance; the previous 2× speedup threshold is superseded. Current uncommitted `src/background/index.ts` prefers compatible exact CUDA MaxN for eligible ordinary Deep Search requests. Opening placement and explicit strategy-policy requests retain CPU/WASM ownership. The locally inspected `dist/background.js` still contains the disabled production-promotion flag, so these source edits do not establish that an installed extension uses GPU. Confirm the rebuilt artifact and a decision's actual runtime/algorithm before claiming live GPU execution. Exact CUDA parity is a same-policy correctness objective; experimental `gpu-root-rollout` is a different algorithm.

### Reported symptom and evidence boundary

Hamza reports that the engine spends resources on immediate actions instead of saving for a future settlement, city or development card. This is a concrete strategic-quality concern, but no specific midgame decision supplied with this report yet proves the causal owner. Existing bounded lookahead and resource/economy features do not guarantee good long-term planning. The opening opponent-model repair does not resolve this midgame concern.

### Bounded next investigation and repair criteria

1. **Reproduce the decision.** Retain the public board, own hand, beliefs, trade setting, canonical request, build identity, chosen spend, alternative EndTurn and their search diagnostics in an existing replay fixture. Select examples where saving is useful and counterexamples where immediate spending is useful. Do not infer optimal play solely from the final game result.
2. **Separate candidate coverage from valuation.** Inspect whether saving/EndTurn survives root admission (`depth.rs`, `strategy.rs`), whether search reaches our next meaningful decision, and whether that continuation can complete the intended build. Use existing per-root completed-wave and controlled-next-decision diagnostics. A missing alternative, truncated horizon and poor leaf valuation require different repairs.
3. **Compare complete funding paths.** Use existing `economy.rs`, `planner.rs` and `eval.rs` owners to compare spend-now with retain-now toward a named legal target. Account for the entire build cost, starting resources, production, bank/port conversion, discard/robber exposure, lost tempo and opponent occupation. Only extend the demonstrated deficient owner; do not add a parallel strategy engine or a blanket resource-hoarding bonus.
4. **Keep plans conditional.** Reconsider saving when a target becomes blocked, a winning action appears, the hand changes or an opponent threatens to win. Saving is an alternative evaluated by common search, not a rule that overrides legal or mandatory action authority. Existing policy-led future-self continuation may need deeper investigation; merely retaining a named goal cannot prove better decisions.
5. **Both trade settings.** With our domestic trades disabled, require self-production or legal bank/port funding. With trades enabled, distinguish available accepted trades from uncertain future cooperation. Do not assume an opponent will supply a missing resource. Opponents retain their own permitted trading behavior.
6. **Verify causally, then measure strength.** A regression must demonstrate the diagnosed failure and preserve counterexamples where spending now is correct. Compare baseline and repair under matched work, board/chance seeds and rotated seats, with both trade settings. Report terminal games, wins, cutoffs and confidence intervals on boards excluded from tuning. Recheck CPU/exact-CUDA parity for any shared search/evaluator change before claiming equivalent behavior.

Implementation status: this section is an investigation/acceptance plan; no resource-reservation policy or evaluator change is implemented by this documentation update. Milestones 3–5 below remain unfinished. Existing historical 3-player results (508/600 against weighted agents and 201/600 against alpha-beta agents) do not measure the current build. The clean v14 three-game all-MaxN CPU/CUDA smoke checks parity, not candidate playing strength.

Status updated 2026-09-08: **implemented through Milestone 2, not through the full roadmap.** Milestone 0 is the pre-strategy baseline repair (`c21e4ae`); Milestone 1 adds shadow evidence (`06b14f8`); Milestone 2 adds opt-in candidate admission (`1420496`), with review corrections in `15f5051`. Thus two strategy-layer milestones are implemented in addition to the Milestone-0 prerequisite. Implementation does not mean demonstrated strength or production promotion.

Milestone 2's `adaptive-candidate-admission-v1` policy admits at most three evidence-backed challengers by replacing only unprotected baseline roots inside the existing cap. Every retained root enters the same common search/evaluator. Missing policy remains baseline-authoritative. The small matched pilot supplied no evidence of improved strength: no admitted challenger became the common-search winner.

**Milestones 3–5 are not started. Milestone 6 is partially implemented and not accepted for promotion.** The later [Engine stabilization and CPU/GPU decision contract](ENGINE_STABILIZATION_AND_CPU_GPU_PLAN_2026-09-07.md) takes priority: the current candidate uses `deep-maxn-v14` on CPU/WASM. Experimental native rollout rejects M2; exact CUDA supports explicit M2 in parity tooling but remains unpromoted. The repaired v14 native gate passed nine fixed-work CPU/CUDA comparisons plus timed cutoff and cancellation checks. Historical v13 artifacts do not certify all v14 behavior, and the recorded v13 GPU smoke failed the 2× retention gate (1.370× for 3P; 1.272× for 4P). Packaged browser execution and held-out strength evidence remain outstanding. See [the acceptance report](ENGINE_STABILIZATION_ACCEPTANCE_2026-09-07.md) for the exact verification scope.

Date: 2026-09-06. Source investigation began at `d80e4b40601411b6cb848471978dbe1799c2045a` and continued against the current working tree. This document does not claim demonstrated playing-strength improvement.

## 1. Objective

Allow the engine to consider several competing routes to victory and revise them as the game changes. Inputs include player count, seat order, victory target, dice model, production, legal building opportunities, resource and development-card beliefs, and opponent pressure.

The engine should answer:

> Which legal first action best preserves and advances my chance of reaching the victory target before the other players, given what I can currently know?

A strategy is a generator of concrete plans and alternatives. Examples are securing a contested settlement, growing city production, pursuing development-card points, acquiring an award, and completing a winning turn. A strategy does not own its own rules engine or directly click the UI.

This document began as a design and now tracks implementation through Milestone 2. Remaining milestones require separately reviewed, bounded plans and their own evidence.

## 2. Existing behavior and the gaps this addresses

The following findings are based on source inspection, not new gameplay experiments:

| Existing component | Verified behavior | Design implication |
| --- | --- | --- |
| `src/worker/deep-search.ts` | Reconstructs development-card worlds; carries player order, victory target, dice identity and effort settings | Reuse its validated observation contract |
| `catan-core/src/state.rs::chance_weight` | Weights dice outcomes, development draws and steals | Strategies must consume the authoritative probability model |
| `catan-search/src/eval.rs` | Values production, deck composition, expansion arrival races, trophies and resource deficits | Reuse these calculations; avoid counting their value twice |
| `catan-search/src/planner.rs` | Compares bounded action sequences within the current turn | Extend existing plan representation where useful |
| `catan-search/src/root_impact.rs` | Promotes important spatial actions into search without adding synthetic utility | Use this distinction between candidate coverage and final value |
| `catan-search/src/depth.rs` | At the investigated baseline, observation-safe recursion used the same top-three prior-weighted mixture for every actor. Milestone 0 now carries `controlled_player`; future controlled-player decisions take one observation-ranked continuation while opponents retain the mixture. The feature-gated CUDA-exact belief-search implementations mirror this rule and renormalize the selected controlled continuation to unit mass | Role correctness is repaired in the working tree across CPU and CUDA-exact belief search; full contingent optimization remains Milestone 5 |
| `catan-search/src/cuda/sim.cu` | At the investigated baseline, native rollouts sampled every actor from the same weighted policy machinery. Milestone 0 now derives the controlled player from each root's pre-action base state and uses deterministic highest-weight policy choices only on that player's later decision turns | Native future-self stochastic dilution is removed without changing generic simulation, opponents or chance sampling |
| `catan-search/src/model.rs` + `model_weights.rs` | Learned value/policy heads are wired in but inactive: runtime strategic feature schema is v2, bundled weights declare v1, and both promotion flags are false | Live strategic evaluation/policy currently falls back to hand-written logic; do not assume the learned heads are contributing |
| `catan-search/src/rollout_cutoff.rs` | Native GPU nonterminal rollout comparison uses a dedicated hand-written strategic cutoff score | A retrained CPU value head alone would not replace native rollout reasoning |
| `catan-wasm/src/native_gpu.rs` | Implements `gpu-root-rollout` with separate root admission and comparison | CPU integration alone does not change native CUDA decisions |
| `src/core/decision-trace.ts` | Records candidate, authority and search-stage evidence | Extend this trace instead of creating another logging system |

The inspected evaluator and current-turn planner do not explicitly establish whether all remaining attainable point sources can reach the target. They estimate strategic value through local features and bounded search. Long-term reachability is therefore a proposed addition, not a proven diagnosis of any specific losing game.

### 2.1 Milestone 0 implemented in the working tree: controlled-player continuation

Before measuring the strategy layer, the baseline needed one semantic repair: distinguish the player whose move the engine is planning from simulated opponents after the root action.

At the investigated baseline, the observation-safe CPU path deliberately avoided per-particle perfect-information maximization by backing up a prior-weighted mixture of the top three observation-ranked actions. That same mixture was used when the acting player was the original player being advised. Native CUDA rollouts likewise sampled a weighted policy action for every acting player, including the original player on later turns. This was information-safe, but it modeled our future behavior as another stochastic policy rather than as a controlled decision maker.

Milestone 0 implements the bounded repair in the current working tree. CPU belief search now carries `controlled_player = observer`; when that player acts recursively it follows the highest-scoring action from the observation-safe policy domain **before** action-family quota ordering, while opponents keep the existing quota-ordered top-three mixture. The feature-gated CUDA-exact belief-search implementations carry the same observer identity and use that same pre-quota observation-safe argmax with unit backup mass. Native root rollouts derive the controlled player from the pre-action base root state and switch the existing weighted policy machinery into deterministic highest-weight selection only on that player's later decision turns. Native rollout scoring is a different policy implementation from CPU MaxN scoring, but it already has argmax rather than quota-slot semantics. Chance phases remain stochastic, and generic non-root CUDA simulation keeps its previous stochastic policy behavior.

That matters whenever a root action is valuable only if we make a good follow-up decision. Development-card draws, conditional award races, route pivots after an opponent blocks a site, and buy-now/build-later plans all have this shape. A strategy layer can improve root coverage while still having those roots undervalued by the comparator.

Keep this as a separate baseline reasoning repair, not as part of strategy scoring:

1. Carry an explicit `controlled_player`/root-actor identity through continuation search and rollout evaluation. **Implemented.**
2. Opponents keep observation-safe stochastic/profile policies unless a separately reviewed opponent-search experiment changes them. **Preserved.**
3. Future decisions by the controlled player use a deliberate bounded observation-safe policy rather than the opponent-style stochastic mixture. CPU uses the top observation-ranked action; native root rollouts use the highest-weight action from their existing policy machinery. Full belief-contingent optimization remains a later experiment. **Implemented at bounded Milestone-0 scope.**
4. Never maximize independently inside each hidden particle and average afterward. If two hidden worlds are indistinguishable to the controlled player at that decision, the policy must not intentionally condition on unseen identities. **Invariant preserved; full observation-history grouping remains Milestone 5.**
5. Preserve chance-node semantics and authoritative stochastic transitions exactly; this repair changes decision ownership, not the dice/development/steal laws. **Preserved.**
6. Evaluate this repair independently before strategy admission changes. The repaired baseline becomes the comparison point for later strategy experiments. **Still pending.**

CPU tree backup and native GPU rollout kernels use different mechanics, but now share the same role invariant: **we may model opponents, but we should not model our own future choices as if they were merely another opponent-policy sample.**

### 2.2 Learned-model status is important, but not the first repair

The repository already contains learned value and policy infrastructure, but it is not active production authority. `features.rs` declares strategic feature schema v2 while the bundled checkpoint declares schema v1; `VALUE_MODEL_PROMOTED` and `POLICY_MODEL_PROMOTED` are both false. Existing engineering records also retain negative evidence for the prior 52-feature candidate rather than claiming a promotion.

Therefore do **not** flip the promotion flags or treat retraining as the immediate fix. First stabilize decision semantics and the strategy evidence contract. A later schema-v2 training effort should use the repaired baseline/teacher semantics and fresh held-out GPU evidence before promotion. This keeps model training from learning around a continuation-policy defect or being credited for improvements caused elsewhere.

### 2.3 Decision-failure taxonomy required before action-changing strategy work

For every scenario used to justify the strategy layer, trace enough evidence to classify a bad recommendation as one primary failure type:

| Failure class | Meaning |
| --- | --- |
| Coverage failure | The strategically correct first action never entered the retained comparison set |
| Valuation failure | The action was compared at adequate common depth/horizon but still scored too poorly |
| Horizon failure | The action's advantage appears only after work the live budget did not complete |
| Continuation failure | Future own/opponent decisions were represented in a way that distorted the root action's value |
| Belief/model failure | Card, dice, resource, opponent, legality or public-state evidence was wrong or materially incomplete |

This taxonomy prevents strategy code from being used to patch valuation or continuation defects. The trace should record baseline rank, proposal support, admission/pruning reason, completed common horizon, and the evidence that made a point source necessary or a site/award scarce.

Some passages in [STRATEGIC_ENGINE_V3.md](STRATEGIC_ENGINE_V3.md) disagree with each other and with current source about recursive opponent behavior, root width and budget allocation. Use the source observations above for this design. Reconcile those passages when an implementation changes the documented engine contract.

CPU/WASM MaxN and native **rollout** CUDA are distinct search algorithms. The exact CUDA path introduced by the stabilization contract is different: it is a computation backend for the same `deep-maxn-v14` candidate policy and must pass fixed-work action/value parity. The three-action opponent mixture is the CPU belief-search mechanism; `gpu-root-rollout` uses its own weighted rollout continuation, while `deep-maxn-cuda-exact-fixed-work-v1` mirrors the MaxN continuation contract. See [CPU_GPU_MREF_CONTRACT.md](CPU_GPU_MREF_CONTRACT.md).

## 3. Architectural options

| Option | Advantages | Costs and limitations |
| --- | --- | --- |
| **Shared strategy proposals evaluated by existing search — recommended** | Reuses rules, beliefs, exact solvers and execution; strategies remain independently testable; one comparable value model per backend | Candidate selection can improve before longer-term continuation quality improves |
| Separate complete engine for each strategy, followed by an arbiter | Easy to run a fixed strategy as an experimental opponent | Duplicates planning and budget consumption; independently scored outputs are difficult to compare fairly |
| Learned strategy selector | Could learn nonlinear interactions among inputs | Requires representative training data and held-out evidence; can learn artifacts of weak strategies or incomplete observations |

Choose the first option. Use a small compile-time Rust catalog. No external plugin loader, remote model, rules DSL, service, or new dependency is needed for the initial implementation.

Keep multiple strategies eligible. Do not select one permanent archetype at game start. Player count influences competition and response opportunities; it does not imply a rule such as “four players always buy development cards.”

## 4. Scope and invariants

- Initial rules scope is the existing 2–4 player base game with its supported configurable victory target. Reject unsupported rules or player counts through the existing validation path. Five to eight players require the separate migration described in [COLONIST_5_8_PLAYER_SUPPORT.md](COLONIST_5_8_PLAYER_SUPPORT.md).
- Keep the current `deep-maxn-v14` candidate on CPU/WASM. The new strategy policy remains experimental until promotion evidence exists. Follow the stabilization routing contract: rollout GPU cannot substitute for MaxN, and exact CUDA remains unpromoted until its independent backend gates pass.
- Preserve public observation boundaries, exact local hand knowledge and honest uncertainty about opponents. No sampled hidden world becomes an actor's knowledge.
- Preserve mandatory actions, exact-family ownership, existing verified safety arbitration, trade exclusions and legal-state validation.
- Execute only the authoritative first action. Every subsequent click or decision must satisfy the existing state-signature and legal-target checks.
- Preserve the local-only extension model and `https://colonist.io/*` scope. No account automation, interception, remote code or analytics.
- Retain the nominal 2,000 ms live WASM decision budget, existing bounded exceptions, and the separate cold packaged-WASM smoke under one second on the reference machine. Additional strategy work consumes the existing budget.
- Initial strategy infrastructure changes proposal coverage and diagnostics. Probability-law changes, evaluator changes and continuation-policy changes must be separately identifiable in experiments.
- The controlled-player continuation repair in Section 2.1 is a pre-strategy baseline change. Freeze and identify that repaired baseline before attributing any later gain to strategy proposals.
- Keep learned value/policy heads disabled until a schema-compatible checkpoint independently clears its own promotion evidence; strategy work must not silently activate them.

## 5. Decision flow

```mermaid
flowchart TD
    A[Validated observation and weighted beliefs] --> B[Mandatory and exact tactical decisions]
    B -->|Strategic comparison required| C[Shared strategy context]
    C --> D[Bounded strategy proposals]
    C --> E[Existing baseline candidates]
    D --> F[Deduplicate and admit candidates]
    E --> F
    F --> G[Search competing plans with common semantics]
    G --> H[Existing authority and safety arbitration]
    H --> I[One validated first action and explanation]
    I --> J[New public observation]
    J --> A
```

Strategies influence what receives consideration. The backend's shared comparator determines the recommendation. Several strategies may support the same action; that action receives one search value and multiple explanatory tags, never several additive bonuses.

Opening retains the public snake-order solver. The initial strategy layer covers normal strategic decisions; it may report opening-derived opportunities, but it does not replace setup, discard, robber, or incoming-trade owners.

## 6. Shared inputs and proposed contracts

These names describe proposed internal contracts; they are not existing APIs.

### StrategyContext

Construct once per decision in Rust from the validated request and posterior. Reuse immutable derived calculations across strategies.

| Input group | Required information and interpretation |
| --- | --- |
| Identity | Observation/evidence fingerprint, actor, canonical player order, phase, turn, rules and strategy-policy versions |
| Game rules | Active player count, victory target, piece supply, allowed trades, discard limit, validated map and development-deck composition |
| Own state | Exact resources and development cards, cards bought this turn, buildings, roads, awards and legal proposals |
| Public rivals | Score, pieces, roads, hand/card totals, played cards, turn order and observable trade restrictions |
| Beliefs | Weighted feasible hidden worlds, posterior provenance and any approximation/support limitations |
| Chance | Requested stochastic model, current public-history posterior and transition-aware probability access |
| Shared economics | Build deficits, production, ports, bank availability, arrival estimates and discard exposure |
| Shared threats | Existing immediate-win assessments, route cuts, contested sites and award exposure |
| Effort | Backend identity, root capacity, deadline, remaining work and completed comparison coverage |

A strategy consumes actor-visible facts and aggregates over the existing root posterior. It does not receive one determinization and treat opponent card identities as known. Opponent predictions use their information boundaries, not the root's private hand.

### StrategyProposal

Each catalog function returns zero, one or two proposals. A proposal contains:

- Stable strategy ID and goal identity, such as a settlement vertex or award.
- A legal first action from the existing actor proposal domain.
- A bounded continuation or reference to an existing `TurnPlan`, when available.
- Preconditions and invalidation conditions, including the required evidence fingerprint.
- Relevant point sources, resource costs, production delay and opponent response windows.
- Evidence status for each claim: rule-derived bound, posterior estimate, heuristic estimate or unknown.
- A reason for search coverage, plus a compact explanation of the opportunity being compared.

Admission priority and predicted arrival time are not win probabilities. A proposal cannot assert that it wins merely because its strategy-specific score is high.

### StrategyAssessment

After comparison, record the evaluated action, supporting strategies, completed horizon, posterior coverage, backend comparator components, uncertainty/limitations, and admission or rejection reason. Retain the raw search winner and the final authority after exact/safety replacements.

An empty proposal list is valid and leaves baseline candidates available. An invalid optional proposal is discarded and diagnosed. Invalid authoritative observations fail through the existing recovery path; they must not be hidden by a strategy fallback.

## 7. Initial strategy catalog

These are proposal families, not mutually exclusive play modes.

| Strategy | When it generates alternatives | Plans it compares | What can invalidate it |
| --- | --- | --- | --- |
| Production growth | Legal city/settlement investment can improve access to needed resources | Invest now versus spend on immediate points or scarce opportunities | Site loss, changed bank/robber state, insufficient remaining game horizon |
| Expansion race | A reachable useful settlement site has a competing claimant or corridor risk | Settle now, complete a route, defend access, or take an alternate site | Occupation, distance-rule exclusion, route cut, changed resource/turn advantage |
| Development access | Purchases are available and offer points, army progress or useful action-card outcomes | Buy now versus city/build now and buy later | Deck exhaustion, revised card beliefs, production opportunity cost, action-card congestion |
| Award race | Longest Road or Largest Army is realistically contestable or vulnerable | Acquire, defend, or abandon the award for another point source | Rival progress, cut risk, piece limits, insufficient knights or time |
| Closeout and recovery | A winning sequence is near, a necessary point source is threatened, or a prior plan fails | Complete a current-turn win, preserve an alternate route, or pivot after a loss | Changed legality, hidden-outcome update, opponent win or unavailable prerequisite |

Defensive and trade-safety checks remain shared constraints across the catalog. Trading and resource conversion are common plan steps, not a separate engine that can negotiate independently of the selected plan.

Adding another strategy requires a catalog entry, bounded generator, explicit evidence inputs, paired decision scenarios, and an ablation showing its contribution. It should not require modifying the UI, executor or core rules.

## 8. Remaining routes to victory

Represent potential point sources using their identities and prerequisites:

- Existing public and exact own hidden points.
- Additional settlement sites and city upgrades.
- Award acquisition and retention.
- Remaining development-card VP opportunities under the posterior.

Distinguish three concepts:

1. **Hard optimistic upper bound:** a rule-derived ceiling that deliberately overestimates attainable points. Only a ceiling below the target can prove insufficiency.
2. **Feasible candidate route:** a bounded sequence or compatible set of opportunities with modeled costs and prerequisites. It is not a guarantee that opponents permit completion.
3. **Practical route estimate:** likely completion time and vulnerability under current production, chance and opponent assumptions.

Count a city upgrade as one additional point over an existing settlement. Account for settlement pieces returned by upgrades. Do not add mutually exclusive sites, reuse the same resources in two simultaneous plans, count an existing award again, or treat opponent-controlled points as permanently unavailable without justification.

For a cheap hard upper bound, relaxing spatial conflicts and resource costs is acceptable because it overestimates capacity. Omitting a possible site, recovered piece, award or card source is unsafe. If the supported rules and evidence do not establish a sound bound, return unknown. A low survival heuristic or absence from sampled worlds is never proof of impossibility.

Compare routes with and without scarce sources. If a sound optimistic ceiling excluding future development VPs is below the target, that establishes that some development VP access is necessary under those assumptions. It does not establish that buying this turn is optimal: timing still competes with city production, opponent purchases and opponent finish time.

The Milestone-1 serialized diagnostic therefore uses **optimistic-bound** rather than feasibility language. A ceiling at or above the victory target means only that the target is `not ruled out by the optimistic bound`; it does not prove a legal route exists. Development-deck min/max/expected fields are explicitly aggregations over the supplied positive-weight sampled belief worlds, not claims about every logically compatible hidden world.

Introduce hard reachability diagnostics in the first shadow milestone, not as a later action-changing feature. Initially use them only as diagnostics and candidate-coverage evidence. Any influence on leaf value is a separate experimental revision to the shared evaluator, so its effect can be measured without double-counting existing points or expansion terms.

## 9. Probability and player adaptation

### Development cards

For each feasible world, weight draw outcomes by remaining card counts, then aggregate with posterior weights. Report the probability and its evidence assumptions. An exact remaining composition is possible only when public and own-card accounting establishes it.

“Four cards left” does not imply a VP probability by itself. A constructed state with three VP cards among four remaining cards implies 75% for the next draw. That example must be embedded in a card-conserving fixture; it is not an assertion that any observed four-card deck has that composition.

Compare draw-dependent continuations. A VP draw and an action-card draw may justify different next actions. Decisions after an observed draw may differ; decisions before it must share a policy.

### Dice

Use the core chance law as the single probability owner. For fair IID dice, history does not make a number due. For `mref-colonist-linked-2024-v1`, condition and advance the existing public/reference posterior after simulated rolls. Zero-posterior nominal outcomes must be removed before transition application and surviving positive outcomes renormalized; the tactical solver carries a regression where a fourth recent `8` is impossible under Mref and must never be transitioned. Mref remains a named hypothesis, not knowledge of the server's hidden mechanism.

Near-term forecasts must use transition-aware distributions. Repeatedly applying the root distribution to every future roll is invalid for a history-dependent model. Existing fixed-pip production estimates may remain labeled long-term heuristics until a separately validated forecast replaces them.

Use full distributions when needed for build readiness and seven/discard risk; expected resource totals alone lose correlation and threshold effects. If a bounded forecast cannot resolve an arrival estimate, expose unknown or a labeled approximation.

### Player count and opponents

Derive response opportunities from canonical seat order and phases. More players changes the number of possible buyers, expansion competitors and actions before our next turn. Two-player denial has different consequences from three- or four-player denial because another rival may benefit.

Reuse available session trade evidence and public opponent behavior. Do not infer a durable personality from one action. Initial proposals use existing opponent assumptions and uncertainty; learned per-player behavior is a later experiment requiring support counts, shrinkage toward defaults and held-out verification. No cross-account profiling is introduced.

## 10. Admission, comparison and changing strategy

### Initial implementation: bounded proposal coverage

1. Run existing authoritative validation and mandatory/exact handling.
2. Produce the repaired-baseline candidate ordering, shared context, reachability diagnostics and failure-classification fields before any strategy proposal changes authority.
3. Generate at most two proposals per catalog strategy; deduplicate by canonical first action.
4. Preserve existing mandatory/safety reservations, required EndTurn coverage and the baseline ordering leader. This leader is the best candidate before deeper comparison, not a separately searched baseline winner.
5. Admit at most three additional distinct strategy challengers into the configured root cap, replacing only unprotected candidates. Keep the total within that backend's existing capacity.
6. Rank challenger coverage by evidence: sound necessary-source constraints, directly contested opportunities, then estimated utility of bounded current-turn plans. For award-race proposals, `ContestedOpportunity` currently requires a rule-backed immediate award transfer by the visible candidate action (for example, the road claims Longest Road or the Knight claims Largest Army); merely not owning an award is not evidence of a race. Tie-break with the existing baseline ordering and canonical action identity. These ranks allocate consideration, not final action authority.
7. If protected candidates consume capacity, omit optional proposals and record why. Never claim that all strategies were evaluated when some were excluded.
8. Use the existing complete comparison/fallback discipline. Partial new work cannot replace a completed authoritative result merely because a challenger was visited first.
9. When a scenario still recommends the wrong action, classify it as coverage, valuation, horizon, continuation or belief/model failure before adding another strategy rule or evaluator weight.

The three-challenger limit is an initial experimental resource bound, not a measured optimum. The first revision retains a fixed admitted set per decision. This allows candidate-coverage effects to be isolated.

### Subsequent experiment: reconsidering excluded candidates

Retain omitted proposals in a bounded queue. At completed wave boundaries, admit a challenger when new search evidence undermines a leading plan or leaves a necessary source uncovered. A new roster requires a complete common comparison table before replacing the previous roster's result. Do not compare a deep incumbent value with a shallow challenger value as if they shared a horizon. If the deadline prevents that comparison, retain the previous result and report the untested challenger.

### Subsequent experiment: fully optimizing our continuations

The pre-strategy repair in Section 2.1 only establishes role correctness: our future decisions stop being treated as opponent-style stochastic behavior. It does not claim to solve the full imperfect-information contingent-planning problem.

A later stronger experiment replaces any bounded deterministic/greedy controlled-player continuation with belief-contingent optimization. Group equivalent observable histories and choose one continuation for each group using its conditional posterior. Retain relevant public history/belief state in the grouping key when the current visible board alone is insufficient.

Never maximize independently within each hidden world and average afterward. That would allow incompatible choices based on unseen cards. Opponent responses remain modeled policies unless a separate opponent-search experiment justifies a change.

This experiment requires its own detailed algorithm design and evidence before activation. The initial strategy catalog does not claim to solve it.

### Switching after new observations

Recompute eligibility and compare plans after resource changes, rolls, trades, development purchases/plays, buildings, road cuts, award changes, or corrected evidence. The selected first action determines the active explanatory strategy tags.

Do not add an artificial switching penalty to action value. When alternatives are indistinguishable at available resolution, use the backend's deterministic tie-break; retain a previous display label only if it still supports the selected action. Past investments do not justify continuing an invalid plan.

## 11. Comparison semantics and budget

All strategies evaluated in a backend share its terminal handling, strategic evaluator, horizon definition and safety arbitration. Strategy-local scores may order proposals; they cannot be compared as final utilities or vote the winner into office.

Use expected ability to finish before opponents as the objective. At nonterminal cutoffs this remains a model estimate. Expected VP, shortest build ETA and route feasibility are evidence, not interchangeable definitions of winning.

Keep horizon units explicit: action count, completed player turns and complete table rounds differ. Compare alternatives at equivalent absolute game progress, including buy-first versus build-first sequences. Count opponent response windows separately.

All proposal generation and comparison share one cooperative decision budget. Reuse route maps, economy summaries and exact-family outputs. Skip optional proposal work when a complete baseline comparison is at risk. Do not launch five independent two-second engines or repeat exact solvers for each strategy.

Record time spent on context, proposals, admission and comparison, plus effective candidate count and completed horizon. Report budget exhaustion and omitted work. A timeout must not silently become evidence against the strategy evaluated last.

## 12. CPU, WASM and native integration

Keep strategy code in `catan-search`; it is shared Rust logic, with backend adapters deciding how admitted candidates receive search work.

| Proposed responsibility | File/module boundary |
| --- | --- |
| Pre-strategy controlled-player continuation semantics | Existing `depth.rs`, `cuda_sim.rs`, `cuda/sim.cu`, and native adapter boundaries; no strategy module owns this |
| Catalog, context and bounded proposals | New `engine/crates/catan-search/src/strategy.rs`, exported by existing `lib.rs` |
| Point-source bounds and route evidence | New `engine/crates/catan-search/src/reachability.rs` when introduced |
| Existing economic/spatial calculations | Reuse `eval.rs`, `economy.rs`, `resilience.rs`, `root_impact.rs` |
| Current-turn sequence reuse | Extend `planner.rs` only where a proposal needs its existing sequence machinery |
| CPU admission and later full contingent-continuation experiments | `depth.rs` and shared admission helpers |
| Learned-model lifecycle | Existing `model.rs`, `model_weights.rs`, feature/training pipeline; remain separately promoted from strategy policy |
| Request/response and strategy-policy identity | `engine/crates/catan-wasm/src/lib.rs`, `src/worker/deep-search.ts` |
| Native capability and host-side candidate admission | `engine/crates/catan-wasm/src/native_gpu.rs` and existing native protocol/routing owners, verified during implementation planning |
| User explanation and local evidence | `src/core/engine.ts`, `src/core/decision-trace.ts`; overlay consumes the authoritative rationale |
| Offline evaluation | Existing `catan-arena` tactical corpus and benchmark machinery |

The initial candidate-admission layer can run on the native Rust host before CUDA evaluation. It does not require each strategy to be reimplemented as a kernel. Changes to rollout policy, leaf semantics or packed state do require the corresponding CUDA implementation and parity work.

Introduce an explicit experimental strategy-policy identity, separate from search algorithm, stochastic model and protocol identity. A missing field means the existing baseline for backward compatibility. An explicit unknown requested strategy identity must be rejected or routed to a backend that supports it; it cannot be silently ignored. Responses echo the identity actually used.

Production M2 remains disabled/opt-in and ordinary requests therefore stay baseline-authoritative. Explicit M2 requests use CPU/WASM in production; exact CUDA can execute the same proposal contract only in parity/experimental tooling while its backend promotion gate remains closed. Different algorithms such as MaxN versus `gpu-root-rollout` need not choose identical moves, but CPU MaxN versus exact CUDA **must** match under fixed work within the declared tolerance.

Cache identity includes observation and belief evidence, rules, strategy version, search algorithm, stochastic identity and relevant effort/horizon. Explanations and results are revoked together when their evidence becomes stale.

## 13. Explanation contract

Provide one concise reason tied to the final authoritative action and one material tradeoff. For example:

> Buy development: current card evidence makes a VP draw likely, and delaying risks losing access to a point source this plan needs. Building the city would improve production sooner.

Only show a numerical draw probability when it is actually calculated. Only say a point source is necessary when a sound bound establishes that claim. Otherwise say the model estimates it is important.

If safety arbitration replaces the search winner, the explanation must describe the replacement. Never keep a “build a city” strategy label beside a highlighted development-card purchase.

The detailed local trace records strategy IDs, goal targets, evidence status, proposed/admitted/pruned actions, shared comparator results, completed horizons, budget usage, previous-plan invalidation and authority replacements. Reuse existing retention/export controls. No new remote logging is introduced.

## 14. Verification and promotion design

These remain the promotion requirements for strategy work. Stabilization has since added focused regressions, exact-backend parity artifacts, replay fidelity checks and a product-route takeover smoke, but those results do not promote M2 or establish playing-strength improvement.

### Paired decision scenarios

| Scenario pair | Required distinction |
| --- | --- |
| VP-rich versus VP-poor depleted deck | Odds and development proposal evidence change according to conserved card counts |
| Same scarce source with a ready buyer versus no near-term buyer | Delay risk changes; no universal buy-first expectation |
| Verified below-target building ceiling versus sufficient alternate points | Necessary-source evidence appears only in the former |
| Rival can settle first versus rival lacks required resources | Race urgency changes with complete costs and turn order |
| Same board with 2, 3 and 4 supported players | Response opportunities and competing claimants are derived correctly |
| Fair IID versus reference-history dice | Correct model identity and transition-conditioned probabilities are preserved |
| City secures immediate win versus attractive development gamble | Existing exact winning authority remains intact |
| Road route cut or award lost after planning | Old proposal invalidates and the next legal decision replans |
| Same observed history with hidden-world ordering changed | Proposals, admitted candidates and action policy remain invariant within defined numeric tolerance |
| Previously hidden draw becomes visible | Continuation may change after observation, never before it |
| Budget expires during challenger evaluation | Last complete authoritative comparison survives |
| Several strategies propose one action | One action value, no duplicated utility or budget allocation |
| Older native companion receives explicit experimental request | Capability routing/rejection preserves requested semantics |
| State changes before execution | Existing stale-result rejection still prevents clicks |

Construct fixtures through legal transitions or validate card/piece conservation explicitly. Expected-action assertions need a proven tactical answer or a separately established high-budget reference. Where the optimum is uncertain, assert evidence and candidate coverage rather than prescribing a favorite strategy.

### Evaluation sequence

1. Record traces for the current baseline, then evaluate the controlled-player continuation repair by itself. If accepted, freeze that repaired baseline and use it for every subsequent strategy comparison.
2. On the repaired baseline, record failure classification plus hard reachability diagnostics and high-budget diagnostic references on a frozen scenario corpus. Label oracle information separately from observation-limited references.
3. Run strategy generation in offline shadow mode: record proposals without changing choices.
4. Compare candidate admission alone against the repaired baseline, with identical shared evaluator and controlled-player continuation semantics.
5. Ablate each strategy and the reachability signal. Separately evaluate candidate reconsideration, probability-aware economic forecasts and full contingent continuation optimization.
6. Run matched arena blocks at 2, 3 and 4 players, rotating every seat and matching board/chance seeds. Stratify by supported dice mode and victory target. Keep tuning seeds separate from held-out seeds.
7. Validate packaged WASM behavior, native semantic compatibility and latency before any consensual live trial.

Do not combine learned-model promotion with these comparisons. A schema-compatible learned checkpoint is a separate experiment with its own frozen teacher semantics and held-out evidence.

Report terminal wins, cutoffs, paired scenario regret where a trustworthy reference exists, critical-candidate coverage, failure rates, latency distribution and completed work. Use confidence intervals clustered by matched block. Do not drop cutoffs or combine different backend budgets in a way that manufactures improvement. Report both equal-effort algorithm comparisons and actual live-budget results.

Before a held-out run, freeze the candidate revision, corpus split, primary strength metric, per-stratum tolerances, sample-size rationale and stopping rule in the benchmark plan. Promotion requires all correctness gates, compliance with the existing latency gates, and held-out evidence meeting those preregistered criteria. This design deliberately makes no claim that a few scenario wins demonstrate general strength.

Keep simulator strength, live integration reliability and displayed win-estimate calibration as separate evidence categories.

## 15. Delivery sequence and stopping boundaries

The ROI order is intentionally asymmetric. Fix the comparator's model of our own future behavior first, then instrument reachability and failure type, then change candidate coverage. Deeper forecasting and full contingent planning come only after evidence shows they are the remaining bottleneck.

| Milestone | Reviewable deliverable | Authority boundary |
| --- | --- | --- |
| **0. Baseline reasoning repair** | **Committed at `c21e4ae`:** controlled-player identity carried through CPU/GPU continuation; future self no longer uses opponent-style stochastic behavior; opponents remain observation-safe modeled policies | Build/static checks complete; the reduced-effort matched arena screen is directionally favorable in 2p/3p/4p but too small for promotion; independent review and a larger preregistered held-out run remain separate gates |
| **1. Reachability + shadow strategy evidence** | **Implemented:** shared player-count/response-window context, optimistic sampled-world bounds, five proposal families and baseline rank/retention evidence. Stabilization separates observable proposal status from causal attribution, which remains unknown without a controlled reference/intervention | Shadow-only: no strategy-driven root admission, evaluator bonus, or final-action authority change |
| **2. Experimental candidate admission** | **Implemented:** explicit `adaptive-candidate-admission-v1` admits at most three distinct evidence-ranked challengers into the existing root cap; mandatory/forced-loss safety roots, EndTurn, the retained baseline leader, and retained spatial/closeout protections cannot be displaced; admitted actions enter the same common search/evaluator and diagnostics record selection, displacement, omission, and search entry | Opt-in only; missing policy is baseline-authoritative; native rollout GPU rejects/reroutes the policy; exact CUDA can reproduce it only as the same-policy backend under parity tooling and remains unpromoted; bounded matched pilot is neutral/non-positive, so there is no default promotion |
| **3. Adaptive candidate reconsideration** | **Not started:** bounded queue of omitted proposals with common-horizon re-entry only at completed comparison boundaries | First demonstrate a consequential omitted candidate; independent ablation, no shallow-vs-deep comparisons |
| **4. Transition-aware economic forecasting** | **Not started:** Mref/fair chance-consistent build-readiness and opponent-response forecasts where static pips are inadequate | First demonstrate a comparison that static forecasts get wrong; separate experiment, no silent chance-law or evaluator change |
| **5. Full contingent continuation** | **Not started:** observation-equivalent-history grouping with conditional-posterior optimization for future controlled-player decisions | Stronger than Milestone 0's bounded argmax; independent algorithm design/review and evidence required |
| **6. Backend integration and promotion** | **Partial:** protocol-7 capabilities, `analyze-exact`, topology handling and arena routing are implemented; focused v14 fixed-work parity passes. Full current-candidate acceptance, performance and packaged execution remain incomplete | Exact CUDA remains unpromoted. Recorded v13 speedup failed ≥2×; current v14 performance is unmeasured. Packaged recommendation → click → confirmed transition and held-out strength evidence remain missing. Learned-model promotion stays separate |

Milestone 2 keeps evidence ranking confined to coverage: necessary-source evidence ranks ahead of contested/scarce opportunity evidence, which ranks ahead of bounded current-turn/planner evidence; baseline order and canonical action order break remaining ties. Multiple strategies supporting one action still consume one root slot. The admitted roster is frozen for the decision, so no omitted challenger re-enters after deeper work; reconsideration remains Milestone 3.

The first bounded matched Milestone-2 pilot used four blocks per player-count stratum, fixed 2,000-node MaxN waves, depth 3, root cap 12, four belief/strategic particles, no search time limit, no player trades, and matched seat rotations/board/chance seeds (`9100001`, `9101001`, `9102001`). It produced:

| Players | Candidate / baseline wins | Mean rank | Mean VP | Cutoffs | Policy decisions | Admission decisions | Challengers admitted / displaced / evaluated | Admitted challenger common-search wins | Candidate / baseline mean decision latency |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2 | 4/8 / 4/8 | 1.500 / 1.500 | 7.500 / 7.500 | 0 | 898 | 1 | 2 / 2 / 2 | 0 | 134.684 / 134.196 ms |
| 3 | 4/12 / 8/24 | 2.000 / 2.000 | 7.583 / 7.583 | 0 | 1,298 | 0 | 0 / 0 / 0 | 0 | 238.770 / 239.594 ms |
| 4 | 4/16 / 12/48 | 2.500 / 2.500 | 7.250 / 7.250 | 0 | 1,267 | 2 | 2 / 2 / 2 | 0 | 336.926 / 338.068 ms |

Across 3,463 policy decisions, only three decisions admitted any challenger (about 0.087%); four challenger actions were admitted and evaluated, all four displaced baseline actions, none won common search, and search depth/nodes were unchanged within each matched stratum. This small pilot is deliberately reported without tuning or rerunning its seeds. It supports semantic/latency feasibility but supplies no evidence that Milestone-2 admission improves playing strength.

Milestone 1 remains the evidence source for proposal support. Current diagnostics record `omitted`, `searched-not-selected`, `budget-limited`, or `selected`; none alone proves coverage, valuation, horizon, continuation, or belief-model causation. Causal attribution remains unknown without discriminating evidence. Milestone 3 candidate reconsideration has not been implemented.

The next roadmap work is to finish the applicable stabilization evidence and evaluate M2 against the repaired baseline with frozen settings. Only a demonstrated omitted-candidate failure justifies starting M3. M4 and M5 remain separate later experiments, not work implicitly completed by backend integration. Backend speedup alone cannot establish strategy strength.

Do not start by training or enabling the learned heads. Their current checkpoint is schema-incompatible and unpromoted, and native rollout cutoff reasoning is separately hand-written. Revisit learning only after the baseline decision semantics and teacher/evidence contracts are stable.

Remaining empirical questions are which proposals improve decisions, whether point reachability improves value beyond coverage, how often dynamic root re-entry matters, whether transition-aware resource forecasts change ordering under the live budget, and how much full contingent optimization fits that budget. They are experimental questions with the evaluation path above, not assumptions to encode as facts.

## 16. Stage-1 isolated planning validation — 2026-09-08

Question: does bounded MaxN systematically spend immediately when retaining
resources for a future settlement, city, or development card would be better?
Method: synthetic post-roll Main-phase fixtures (own domestic trading masked,
product own-player-only semantics), decided through the production iterative
wave entry (1500 nodes/wave, depth 3, baseline) and a fixed-work deep
reference (40k nodes, depth 5). A save fixture holds one ore short of a city
on an ore-rich board (seed 3: 13 ore pips 2p / 9 pips 3p) with a dev purchase
competing for the same grain/ore. Regressions live in
`engine/crates/catan-search/src/midgame_save_spend_tests.rs`.

| Fixture | Production entry | Deep reference | Standing |
| --- | --- | --- | --- |
| 2p save (dev vs EndTurn) | EndTurn | EndTurn | Agree: saves. Regression. |
| 2p city affordable now | BuildCity 0.9995 | BuildCity | Agree: spends. Counterexample regression. |
| 3p save (dev vs EndTurn, seed 3) | BuyDevelopment 0.8020 | EndTurn 0.8196 | Disagree: ignored diagnostic, neither side is ground truth. |
| 2p road materials (seed 3) | EndTurn +0.0035 | Road | Disagree inside noise: ignored diagnostic. |
| EndTurn ranked + retained | all 7 shapes (2p/3p) | — | Coverage invariant regression. |

Classification per the Section 2.3 taxonomy:

- **Coverage: ruled out.** EndTurn is ranked (~#5–6 of 8–9) and never pruned
  on any fixture, and is protected once retained. A future save-blindness
  must come from valuation or horizon quality, not admission.
- **Reachability: holds through the production entry.** `EndTurn`
  continuations reach our next decision with mass 1.0 on the tested fixtures
  (2p and 3p, 1 and 8 particles). A single-wave 1500-node Global budget does
  starve EndTurn (mass 0, value collapse 0.86 → 0.57), but no live path uses
  the Global entry; production waves re-budget per depth. That starvation is
  a harness artifact, not a live defect, and was not repaired.
- **Valuation across work budgets: open question.** The two flips differ with
  no mechanism beyond node count (both roots reach; margins 0.004–0.06, in
  both directions across fixtures). Thin-margin ranking instability between
  work budgets is expected search approximation, not demonstrated
  save-blindness. No hoarding bonus, budget reshuffle, or evaluator change
  is justified by this evidence; each is explicitly banned as a substitute
  for a demonstrated cause.

No production source changed in this stage. No benchmark campaign was run:
with no repair there is nothing to measure, and the pilot seed 2026090901
stays out of future held-out evaluation regardless.

Unresolved causal question: the complaint arose in real midgame positions
with hidden hands, VP pressure, robber/discard exposure, and timed budgets
that these sterile fixtures (empty opponent hands, zero VPs, post-setup
boards) do not reproduce. To discriminate valuation from noise requires a
recorded live decision where saving was clearly better: an investigation-tab
export (game key, decision record showing own hand, chosen spend, retained
EndTurn alternative, and search diagnostics) of one such moment, plus its
spend-now counterexample. Until that evidence exists, Milestones 3–5 remain
unstarted and M2 stays opt-in.

## 17. Stage-2 contested two-player planning — 2026-09-09

Accepted Stage 1 (no repair justified; EndTurn retained everywhere, so
admission was not reopened). Controlled synthetic 2p pairs, one factor at a
time, through the production wave entry plus the deep reference. Fixtures and
regressions in `engine/crates/catan-search/src/midgame_save_spend_tests.rs`.

### Contested settlement (demonstrated mechanism, regression locked)

Seed-15 pair sharing one board: our hand is one 4:1 bank trade from settling
a target vertex (funding path verified by applying the trade, not assumed).
Variant B roads the opponent to the target and funds their settlement cost;
variant A leaves them empty. The modeled opponent takes the target in B
(BuildSettlement v8, verified by deciding for the opponent directly — the
disproof gate; had it not taken the site, no flip would be required).

| Variant | Production entry | Deep reference |
| --- | --- | --- |
| A uncontested | bank+settle 0.9993, EndTurn 0.9880 | bank+settle 0.9873, EndTurn 0.9854 |
| B contested | bank+settle 0.9382, EndTurn 0.4924 | bank+settle 0.9172, EndTurn 0.8037 |

Waiting is near-free in A and destroys the site in B, and both budgets price
it: EndTurn craters exactly where the site is contested (regression asserts
B-EndTurn + 0.05 below A-EndTurn; observed gap 0.50). A's exact choice is
unasserted (0.002–0.011 margins are noise). Opponent-response modeling,
not admission or reachability, carries this: mass-next-decision is 1.0
throughout. No repair needed; the mechanism works.

Robustness of the mechanism: identical A/B choices and EndTurn values
through the timed entry (mask on), and the A→B EndTurn crater persists with
our domestic trades enabled (fixed 0.88→0.48, timed same direction). With
offers present, B's winning spend flips between bank (timed) and a lumber→ore
offer (fixed) — offer-axis instability across entries, recorded as
approximation-sensitive and out of scope for save-vs-spend.

### Exposure, immediate win (no defect; outcomes recorded, probes removed)

- Wealth-matched discard exposure (9 vs 7 cards, same city distance, same
  legal set): both budgets spend (road) in the high-exposure state; the low
  state repeats the known thin-margin live-vs-deep flip family. No
  exposure-blindness demonstrated.
- Immediate win: 9 VPs plus affordable city selects BuildCity (existing
  `maxn_converts_an_immediate_win` covers the authority; guard probe agreed).

### Live-decision replay: road4311 D14 (no production change)

Source: a 4-player bot game export supplied from Downloads
(`colonist-evidence-road4311-*-648Z.txt`, build 0.9.1
main@401956f425b3+dirty). Turn 10, our seat (P2) holds [brick1, wool1,
grain2, ore1] with two settlements (city legal at neither: ore 1 of 3),
healthy bank, robber on our ore-6 hex. The live engine bought a development
card at completed depth 0 (27 nodes, deadline during root scoring, 22
particles, ~100 roots); runner-up was an ore-seeking offer 0.095 behind.

Reconstruction (`engine/crates/catan-search/src/road4311_d14_tests.rs`):
EXACT geometry/pieces/hands (all 22 belief worlds)/bank/robber/VPs/full dev
deck; dice observations exact but the Mref posterior digest does not match
(brute-forced constructions miss), so the live belief's observation list is
not recoverable — a labeled replay boundary. Staged CPU-search code is
unchanged from the recording build. NOTE: an earlier revision of this note
compared the wrong value component (P0 instead of the acting P2); all values
below are the decision-relevant value[2], and the arbitration rule is
max-value with forced-loss escape (`depth.rs` final arbitration).

Reconciled single ranking (full search, recorded effort, fixed work):
offer 0.48 wins, BuyDevelopment 0.39 second, EndTurn 0.31. The save
hypothesis is FALSIFIED for D14: completed search ranks saving below the
recorded spend, and the starved floor agrees (dev 0.44 > offer 0.35 > EndTurn
0.31). The dev-vs-offer flip across budgets is a separate trade-optimism
question, out of scope for save-vs-spend, with acceptance uncertainty on the
offer side.

Fidelity (committed regression): the starved replay reproduces the live
choice AND its values exactly (dev 0.4444, runner-up gap 0.095), so the
reconstruction captures the live depth-0 mechanism; dice differences do not
move these floor values. The Mref digest gap therefore does not block the
floor finding, but it means the full-search counterfactual is established
only in reconstruction (Mref-approx and M0 agree), not proven for the exact
live posterior.

Fallback handling: the no-wave condition is exposed (`floorComplete`,
attempted-vs-completed depth, per-stage timings, and the evidence string
naming depth 0) and handled by design (a guaranteed complete one-ply floor
decides rather than an arbitrary fallback). A time-aware planner trim was
implemented, verified coverage-clean, then REVERTED: it merely shifts time
between stages, and release-timed runs show the depth-1 wave itself exceeds
small slices — unproven benefit against a real shared-path blast radius.
The justified next question is floor quality (how the initial one-ply
comparison is supported under the same allowance), not stage budgeting, and
it needs its own mandate: nothing here supports a hoarding bonus, weight
retune, or strategy-layer change for D14.

Close-out verdict: saving hypothesis FALSIFIED for D14 — completed search
ranks EndTurn below the recorded dev purchase, and the starved floor agrees
(dev > offer > EndTurn). No demonstrated production defect; no production
change. M2 remains experimental. A review of this investigation caught a
wrong-player value comparison (P0's component read for P2's decision); the
starved-floor regression now derives the utility index from the acting
player so the mistake cannot return.

### Stage-3 validation (fresh-seed current-behavior block)

`scripts/run-adaptive-strategy-validation-stage2.mjs`, seed 2026090801 (the
pilot seed stays held out), one matched 3-seat block per trade setting,
baseline MaxN vs two weighted opponents. No production change precedes this
run, so it is additional validation of current behavior, not an improvement
benchmark. Global no-trades rows are arena-restricted lanes, not product own-player-only evidence.

| Domestic trades | Policy | Wins / 3 | Cutoffs | Policy decisions / admitted |
| --- | --- | --- | --- | --- |
| Enabled | Baseline | 2 | 0 | — |
| Enabled | Adaptive M2 | 2 | 0 | 511 / 0 |
| Globally disabled | Baseline | 2 | 0 | — |
| Globally disabled | Adaptive M2 | 2 | 0 | 300 / 0 |

Twelve terminal games, zero cutoffs or invariant failures. Adaptive again
admitted zero challengers (identical trajectories within each trade
setting), confirming the pilot's coverage finding on fresh boards: candidate
admission changes nothing here. This does not establish a live win rate.

### Live-decision replay boundary — 2026-09-09

A bounded search of local evidence found no replayable live spend-vs-save
decision. Audited fixtures carry inputs without recorded choices (and use
abstract boards without production); takeover corpora carry full midgame
states but no recorded decision, diagnostics, or complaint; pasted live logs
cover opening/dice phases only. Per the investigation protocol this stops at
the evidence boundary: the next step needs one user-supplied
Investigation-tab export of a midgame spend that should have been saved
(chosen action, intended alternative and future build identified). No
synthetic campaign was substituted.

### Verdict

No demonstrated correctness failure in Stage 2. One demonstrated mechanism
(race pricing, regression-locked), two approximation-sensitive flip families
(ignored diagnostics), zero production changes. Timed and fixed-work entries
agree on all mechanism facts. The original live complaint remains
unreproduced: with hidden hands, VP pressure, and timed budgets still absent
from these fixtures, a recorded live decision is still the missing evidence.

## 18. M2 admission usefulness — 2026-09-09

Question: M2 admitted zero challengers across 1,554 policy decisions (743
first pilot + 811 Stage-3 validation with identical trajectories). Is that a
malfunction or the correct output?

Mechanism (source): every generator proposes only actions already in the
ranked baseline domain (`strategy.rs` producers read `ranked_actions`;
nothing proposes EndTurn, offers, or bank trades), and admission can only
rescue ranked-but-truncated actions into a fixed cap with an EndTurn
reserve. M2 is structurally a second chance for truncated roots.

Evidence (rust-level admission audits through the production entry):
- Save fixtures, race pair: 0–1 proposals, all already retained → zero
  candidates. The race settle needs bank funding first, so it never enters
  the ranked domain at all.
- Live D14 (heaviest real truncation: 12 retained, 86 pruned): every
  truncated action is an offer; dev and EndTurn are retained; the sole
  proposal (dev) is already retained. Winners identical with/without M2.
- Award-takeover hunt (low-prior +2VP road under offer flood, both sides at
  5-chains): the award road is retained, not truncated; the deep reference
  prefers ore-seeking offers anyway. No omission.
- One out-of-scope observation, not pursued: D14's deep-preferred ore×3
  offers were truncated from baseline search. That is offer-family coverage
  (root caps/quotas) compounded with acceptance optimism — the
  dev-versus-offer floor question, explicitly out of scope here. M2's
  families cannot propose offers by design, so no admission change would
  touch it.

Decision (step 3, first branch): baseline already covers every useful
action M2 is empowered to rescue. Keep M2 experimental; more admission
machinery has no demonstrated value. No benchmark follows a non-change
(step 4 correctly skipped). If truncation policy for offers is ever
revisited, that is a quotas/caps investigation, not M2 promotion evidence.

### Trade-acceptance realism: D14 dev-vs-ore-offer (no repair)

D14's completed search prefers asking brick+wool for 2 ore (0.48) over
buying dev (0.39) over saving (0.31). Bounded audit of where the offer's
value comes from, same fixture and 22 worlds:
- Acceptance is gated by availability within the model: exactly 0.000 in all 15
  no-payer worlds, 0.16–0.39 where a recipient holds 2 ore (committed
  invariant `d14_offer_acceptance_gated_by_availability`).
- Rejection is modeled (accept/reject mixture at response nodes), not
  assumed away; the floor prices pre-offer, pending, and post-reject
  states identically (0.3527), so the cutoff floor adds no premium of its own.
- Failed-offer memory exists: `domestic_trade_count` damps repeat-offer
  priors 1.0→0.42→0.16, embargo mechanics exist, counts reset next turn;
  post-reject the turn continues with dev/EndTurn still available, so
  opportunity cost is priced, not ignored.
- Value decomposition within the model: base ~0.37 (dev fallback) + haggling/counter
  option value in all worlds (~0.43 with no payer able to accept) +
  accept lottery toward an immediate city in payer worlds (~0.55).
  Removing the payer worlds does not collapse the offer because recipients
  holding 1 ore can counter inside the bounded neighborhood and our
  controlled continuation accepts only good ones.
- One structural gap noted, not repaired: repetition cost lives in priors
  (allocation) but not in backed-up values, so fully-searched haggling
  never discounts repetitiveness itself. Whether real acceptance decays
  with repetition is unanswerable without live acceptance data, and
  discounting values to intuition would be anecdotal retuning. Offer
  coverage/quotas stay untouched until acceptance calibration is evidenced.

Verdict: no acceptance-modeling defect demonstrated; the offer's premium
is accounted for within the model. That accounting is explicitly not live
validation: the mechanics (availability gating, counter negotiation,
rejection fallback) explain the score, but the assigned probabilities
(16–39% acceptance, counter rates, repeat-offer behavior) are uncalibrated
against live play. Calibrating them needs player-visible offer/outcome
records with predicted-vs-observed comparison across repeated interactions,
which no local record supplies.

## 19. Source references

- [Live request adapter](../src/worker/deep-search.ts)
- [Decision entry point](../src/worker/analyze.ts)
- [Dice evidence routing](../src/core/dice-history.ts)
- [Core transitions and chance weights](../engine/crates/catan-core/src/state.rs)
- [CPU belief search](../engine/crates/catan-search/src/depth.rs)
- [Shared evaluator](../engine/crates/catan-search/src/eval.rs)
- [Current-turn planner](../engine/crates/catan-search/src/planner.rs)
- [Spatial candidate promotion](../engine/crates/catan-search/src/root_impact.rs)
- [Learned-model gate and inference](../engine/crates/catan-search/src/model.rs)
- [Bundled learned checkpoint metadata](../engine/crates/catan-search/src/model_weights.rs)
- [Native GPU rollout host](../engine/crates/catan-search/src/cuda_sim.rs)
- [Native GPU rollout policy/kernel](../engine/crates/catan-search/src/cuda/sim.cu)
- [Native GPU cutoff evaluator](../engine/crates/catan-search/src/rollout_cutoff.rs)
- [Native GPU owner](../engine/crates/catan-wasm/src/native_gpu.rs)
- [Decision trace](../src/core/decision-trace.ts)
- [Schema-v2 learning evidence](SESSION_ENGINEERING_SUMMARY_2026-09-03.md)
- [Existing threat-strategy design](latent-threat-strategy.md)
- [Benchmark methodology](BENCHMARKS.md)
- [Milestone 0/1 empirical screen](MILESTONE_0_1_EMPIRICAL_SCREEN_2026-09-06.md)

Source navigation used Codebase Memory with direct-source verification. Satori's earlier publication reported pending source changes, so stale publication line ranges were not treated as current source authority. Recorded graph coverage had no reported gaps for the relied-on engine paths; that is a best-effort signal, not proof of complete behavioral verification.
