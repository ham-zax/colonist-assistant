# Strategic Threat Modeling for Catan Search

Status: Working design note

This document consolidates the current investigation into three related strategic weaknesses in the decision engine:

1. adversarial road-network play: blocking, cutting, Longest Road disruption, route resilience, and expansion denial;
2. latent tactical threats from hidden information: Monopoly, Road Building, Year of Plenty, Knight, hidden victory points, resource inference, and malicious trade continuations;
3. hostile-table behavior: repeated robber targeting, trade denial, pairwise embargoes, coordinated blocking, and opponents who may accept local opportunity cost to reduce our chance of winning.

The goal is not to encode a fixed Catan strategy or social script. The goal is to make the existing belief search reliably consider high-impact opponent continuations that are currently easy to omit under a live search budget, including bounded stress cases where opponents behave more adversarially toward the root player than ordinary MaxN assumes.

### Research inputs and evidence status

This revision incorporates two hostile-table strategy memos supplied during the design investigation, plus an external verification pass against current CATAN rules/tournament rules and the April 2026 36,000-game heuristic simulation discussed in the second memo. The memos are useful as sources of tactical hypotheses, especially around trade denial, ports, development-card pivots, repeated robber pressure, coordinated blocking, threat perception, number diversity, closeout timing, and economic self-sufficiency.

Treat the claims in four evidence classes:

| Class | Examples | Use in this design |
| --- | --- | --- |
| Rules/mechanical | ports reduce maritime ratios; Knight moves the robber; hidden development cards change visible threat; roads and settlements can block expansion | safe to model through `GameState` transitions |
| Existing engine evidence | hidden-hand particles, development-card sampling, expansion ETA, trade embargo state, MaxN/Paranoid search | safe to use as current architectural facts |
| Strategic hypothesis | ports become more valuable under trade denial; dev-heavy play is more resilient to physical blocking; visible leadership increases targeting | add counterfactual benchmarks before production heuristics |
| Social/meta tactic | calling out another leader, appearing humble, persuading a coalition, intentionally looking harmless | do not encode directly in the decision engine without measurable behavioral evidence |

The memo contains prescriptive statements that may be true only for particular tables or rulesets. This document converts them into testable engine questions rather than treating them as universal Catan laws.

## 1. Design principle

The engine should not prefer "Longest Road" or "web-like roads" as strategies in isolation. It should estimate the value of a move under plausible opponent responses.

For roads, the useful concept is **resilient route option value**:

- expansion gained;
- Longest Road equity gained or protected;
- opponent expansion denied;
- choke points controlled;
- alternate routes and bypasses preserved;
- expected damage if an opponent cuts or contests the route;
- recovery cost after disruption.

For hidden development cards, the useful concept is **latent tactical threat mass**:

- how much posterior probability supports a dangerous hidden continuation;
- how much that continuation changes the game if exploited;
- whether our candidate action increases, decreases, or leaves that threat unchanged.

The search should answer:

> How does this action change the opponent's best plausible tactical opportunity before our next meaningful action?

This is deliberately different from applying static penalties whenever an opponent might hold a dangerous card.

## 2. Strategy research: stable conclusions

The external strategy material supports several principles that are useful for the engine, but it does not justify hard-coded strategic archetypes.

### 2.1 Production is necessary but not sufficient

High-probability numbers are valuable because production creates tempo. Wheat and ore are often especially important because they support cities and development cards. However, the correct opening still depends on resource scarcity, number distribution, ports, expansion lanes, turn order, and the opponents' placements.

The current opening evaluator already reflects much of this through:

- production pips;
- dynamic resource value;
- resource diversity;
- duplicate-number exposure;
- build coverage;
- expansion portfolio;
- scarcity alignment;
- port option value;
- robber concentration.

Therefore this work should not replace the opening model with a fixed OWS rule.

### 2.2 Longest Road is contextual

Community and strategy-guide advice consistently warns against accumulating roads without a concrete payoff. Roads are valuable when they:

- secure an expansion site;
- seize or protect a choke;
- block a rival;
- complete or defend Longest Road;
- create a decisive tempo swing.

A branch by itself does not improve Longest Road. Official rules count the single longest continuous road. Arbitrary branching can therefore spend pieces without improving the trophy.

The more useful structural distinction is:

- **fragile chain**: one critical intersection can materially reduce route value;
- **redundant network**: a cycle, bypass, or alternate path preserves meaningful connectivity after disruption.

The engine should reward the latter only when the redundancy has actual strategic value.

### 2.3 Blocking is an opportunity-cost decision

A block is strong when the opponent's lost expansion, production, trophy equity, or tempo exceeds our opportunity cost. A block is weak when the opponent has an equivalent detour or we sacrifice a much stronger build.

This argues for modeling blocking as a relative search consequence rather than a categorical rule.

### 2.4 Hidden development cards create surprise tempo

Development cards matter strategically because their identities are hidden and several can invalidate visible-board ETA assumptions.

Examples:

- **Monopoly** can turn an apparently safe concentrated hand into a large transfer and immediate build sequence.
- **Road Building** can compress a two-road race into a single hidden-card action.
- **Year of Plenty** can convert a hand that appears two resources short into an immediate city, settlement, road, or development purchase.
- **Knight** can move the robber before production, steal a resource, and trigger Largest Army.
- **Victory Point** cards can make public score understate the true winning threat.

The correct response is posterior reasoning, not assuming the worst card exists.

### 2.5 Economic independence matters when trade access is unreliable

The hostile-table research adds a useful conditional principle: ports and self-conversion become more valuable when opponents refuse to trade with us.

This should not become "always take a port over production." The value of a 3:1 or 2:1 port depends on:

- our expected surplus production;
- the resource we are short;
- the quality of the port settlement itself;
- the probability that domestic trades will actually be available at acceptable terms;
- whether expansion to the port costs roads and tempo that outweigh the conversion benefit.

The engine should therefore treat **economic independence as state-dependent resilience**. If domestic trade is disabled, embargoed, or empirically unlikely, port conversion and resource self-sufficiency should gain relative value through the normal search consequences.

Port specialization also creates counter-risk. A 2:1 engine often encourages holding a large surplus of one resource, which can increase discard exposure and Monopoly exposure. The correct decision is therefore not "specialize under hostility" but "compare independent conversion value against concentration risk and the cost of reaching the port."

### 2.6 Development-card play can be resilient under board hostility

The research also argues that a development-heavy path can remain viable when opponents repeatedly block roads or deny trades. The useful engine interpretation is narrower than the social claim that development cards look "innocuous."

Development cards can provide value that is harder to stop with road blocking alone:

- Knights relocate the robber and progress Largest Army;
- Road Building compresses expansion tempo;
- Year of Plenty repairs short-term resource deficits;
- Monopoly converts opponent holdings into immediate tempo;
- hidden victory points reduce the information available to opponents.

This does not justify a blanket "pivot to dev cards when targeted" rule. The engine should compare the actual development-card conversion path against cities, settlements, ports, roads, and the current deck composition.

### 2.7 Robber defense is also offensive denial

Repeated robber targeting has two costs: immediate steals and suppressed production. A Knight can therefore have compound value when it:

- removes the robber from our important producer;
- moves it onto a high-value rival producer;
- steals from a strategically relevant victim;
- advances or captures Largest Army;
- changes a near-term build race after the steal or production restoration.

The current strategy work should value this complete transition, not only the stolen card or played-Knight count.

### 2.8 Hostile-table behavior is not ordinary MaxN behavior

A coordinated table can make moves that are individually suboptimal but collectively damaging to one player:

- refuse otherwise profitable trades with the target;
- repeatedly place the robber on the target;
- spend roads to close the target's lane even when another expansion is locally better;
- choose a low-production settlement because it cuts the target's Longest Road;
- tolerate another opponent's growth while suppressing the perceived leader.

This is a different modeling problem from hidden cards. Hidden cards are uncertainty about **state**. Hostile-table play is uncertainty about **opponent objective and policy**.

The engine should preserve that distinction.

### 2.9 Threat perception is a model input, not a strategy objective

The research contains useful social observations about appearing non-threatening, keeping hidden development cards ambiguous, and redirecting attention toward the actual leader. Most of that should not become direct engine utility.

The mechanically useful part is that human opponents often target visible threat signals such as:

- public victory points;
- Longest Road or Largest Army ownership;
- obvious expansion lanes;
- visible production concentration;
- number of unplayed development cards;
- recent rapid growth.

If opponent-policy modeling later needs a targeting model, these can become **public threat-salience features** that predict robber, blocking, or embargo behavior. We should not reward "looking weak" for its own sake unless measured opponent behavior shows that the lower salience changes actual outcomes.

### 2.10 Research claims that require validation rather than hard-coding

The hostile-table memo is valuable as a hypothesis source, but several prescriptions are context-dependent:

- sacrificing strong production to secure a port;
- delaying development-card purchases until after one or two cities;
- treating Wheat/Ore/Sheep as a universal opening priority;
- intentionally specializing in one resource to appear less threatening;
- assuming a development-heavy pivot is always best under hostility.

These should become benchmark scenarios, not constants.

The engine must also remain ruleset-aware. Do not bake standard-score, discard, robber, or trading assumptions into strategy code when `GameState` already carries `victory_target`, `card_discard_limit`, `friendly_robber`, `player_trades_enabled`, `domestic_trade_disabled`, and `domestic_trade_embargoes`.

### 2.11 External empirical evidence: the 36,000-game heuristic simulation

The second memo cites a real April 2026 simulation of 36,000 Catan games. It is useful evidence, but it must be interpreted at the level the experiment supports.

The simulation used four fixed heuristic agents across 3-player and 4-player games and several board-generation modes. Its author explicitly states that:

- the agents are heuristics, not optimal search agents;
- player-to-player trading uses simplified acceptance logic;
- robber play is individually leader-aware but not collectively coordinated;
- negotiation, bluffing, alliances, and adaptive human pivots are not modeled;
- the results describe interactions among those strategies, not ground truth for optimal human play.

Within those limitations, several results are good calibration hypotheses for our engine:

- Longest Road appeared in roughly 56-61% of simulated wins and Longest-Road-path games ended earlier;
- winners reached their third settlement roughly 6.7-8.7 individual turns earlier than losers;
- wheat had the largest winner/loser production gap in the reported experiment;
- OWS was competitive but not the best fixed agent in every configuration;
- starting pip count had little winner/loser separation once placements were reasonable;
- some 20-21-pip placements performed poorly, showing that composition and topology can dominate raw pip total;
- PortRusher behavior lost roughly 10 percentage points on randomized boards, with the author explicitly warning that the bot may chase bad ports in ways a human would not.

These findings support the direction already taken by `opening.rs`: production should matter, but number diversity, build coverage, port alignment, route options, and robber concentration also matter. They do **not** justify hard-coded rules such as "always rush Longest Road," "always build the third settlement before a city," or "ports are bad on random boards."

The most useful next step is to reproduce the *questions* with our stronger engine and matched seeds rather than import the original agents' coefficients.

### 2.12 Rules and evidence corrections from the second memo

Several statements in the second memo are mechanically useful after correction.

#### Coordinated leader suppression is not synonymous with prohibited collusion

Current CATAN Championship rules explicitly allow strategic cooperation/collusion when it advances the participating players' own positions. They prohibit intentionally giving another player a win or tournament advancement without direct or overall personal benefit.

For this design, use terms such as **coalition pressure**, **coordinated leader suppression**, or **anti-root policy** unless discussing the tournament rule itself. The engine problem is not to detect cheating. It is to remain strong when several self-interested opponents temporarily share an incentive to hurt us.

#### Robber bargaining rules are narrower than the memo states

Under current Championship rules, while a robber action is unresolved, players may not negotiate resource trades. An attempted resource-trade negotiation can send the robber to the desert with no steal. However, discussion about **where to place the robber is explicitly allowed**.

Therefore do not model tournament robber procedure as "no negotiation before placement." If social behavior is ever modeled, distinguish robber-placement persuasion from prohibited in-action resource-trade negotiation.

#### Trade probing before Monopoly is explicitly legal

The official CATAN base-game FAQ explicitly allows a player to offer trades to learn about likely holdings and then play Monopoly. Opponents are not required to provide truthful statements about what they hold.

This upgrades Dirty Monopoly from a community anecdote to a rules-supported tactical continuation. The exact resource yield is still state-dependent; claims such as "often 8-14 cards" are not a design invariant.

#### Card counting is a posterior, not exact omniscience

Visible production and announced trades provide strong evidence, but hidden hands remain uncertain after random robber steals, hidden development draws, and other unobserved card identities. Tournament rules also require hands and development identities to remain hidden.

The engine's weighted resource/development worlds are therefore the correct abstraction. Do not replace them with deterministic "mental accounting" that assumes exact opponent hand composition.

#### Knight retaliation is delayed until our own turn

A ready Knight can be played before rolling on our own turn, but not immediately during an opponent's turn after they rob us. A defensive model should measure the production/steal damage that can occur before our next action, then credit the Knight for clearing or redirecting the robber at the next legal opportunity.

The same correction applies to claims that a Knight creates an instantaneous deterrent. Deterrence is opponent-policy behavior; the mechanical effect occurs only when the card can legally be played.

#### Bank scarcity is real, but intentional starvation is not automatically good

Resource supply is finite, and production can fail when the bank lacks enough cards. The engine already represents bank counts and discounts production when a public bank is depleted. Holding a scarce resource can therefore impose a real externality on opponents.

Do not add a generic hoarding bonus. Hoarding also increases discard risk, Monopoly exposure, and opportunity cost. Let bank depletion enter through the actual state and compare the resulting opponent production loss against those costs.

#### "Stealth spike" should mean response-window compression, not artificial sandbagging

A same-turn closeout can be strategically strong because opponents receive no intervening decisions after the winning sequence becomes executable. Hidden VP, a Knight/Largest-Army swing, Longest Road, and a final city or settlement can combine into such a closeout.

The engine should not intentionally suppress public VP or reject good builds merely to "look weak." Translate the useful idea into a search quantity: **how many opponent decision windows remain before a reachable win, and how robust is that closeout across the posterior?**

## 3. What the engine already does well

The current architecture is a strong base for this work.

### 3.1 Road and award rules are mechanically correct

`engine/crates/catan-core/src/state.rs` already models the important rules correctly:

- `longest_road_length()` stops traversal through an opponent building;
- `update_longest_road()` recomputes the holder and applies the two-point award;
- building a road or settlement updates Longest Road;
- ties preserve the incumbent according to the current rules implementation.

Therefore road interruption does not require new rules machinery.

### 3.2 Hidden resource worlds already exist

`src/core/tracker.ts` maintains weighted hidden-resource worlds from:

- physical resource supply;
- exact own hand;
- public hand sizes;
- observed production and spends;
- accepted trades;
- rejected trades and counters as softer evidence;
- optional public bank information.

This is already enough to estimate whether an opponent can afford a tactical continuation across the posterior rather than from one guessed hand.

### 3.3 Hidden development cards are sampled

`src/worker/deep-search.ts` constructs development-card worlds from the base deck composition and public evidence.

The sampler:

- subtracts publicly played cards;
- keeps our exact development hand;
- uses each opponent's public hidden-card count;
- distinguishes cards bought this turn from already-playable cards;
- samples hidden card identities without replacement.

This means belief particles can contain worlds in which an opponent holds Monopoly, Road Building, Year of Plenty, Knight, or hidden VP.

### 3.4 Search respects information sets

`GameState::observed_state(observer)` hides other players' exact resources and development identities from the acting player's policy.

`depth.rs` uses observation-safe recursive action selection in production belief search. The simulator can apply the selected action to the exact particle while the action policy is conditioned only on what that simulated player should know.

This distinction must be preserved in all new threat logic:

- our root decision may integrate exact hidden states across our posterior;
- simulated opponent policies must not condition on hidden information they could not observe.

### 3.5 Exact progress-card parameter search exists

The engine already has dedicated handling for Monopoly, Year of Plenty, and Road Building parameter choices.

`engine/crates/catan-search/src/exact.rs` can rank compact action families across the weighted belief. Monopoly also receives special post-action scoring so an immediately spendable stolen hand is not incorrectly punished as if the turn had already ended.

This work should reuse those transitions where practical rather than implement duplicate card semantics.

### 3.6 Expansion ETA already considers opponent affordability

`engine/crates/catan-search/src/eval.rs` has useful infrastructure:

- `road_distances()` computes required roads while respecting opponent roads and buildings;
- `expansion_arrival_score()` estimates build ETA;
- `expansion_site_survival()` compares our ETA with rival ETA;
- `expansion_option_value()` values the best site plus a portfolio of alternatives.

This can support road-safety and choke analysis.

### 3.7 Production search does not currently depend on the lossy strategic coreset

`BeliefDepthConfig::strategic_particle_limit` documents the finite coreset as a legacy arena/benchmark experiment. Production bounded-search entry points use `usize::MAX`, leaving only exact-identical coalescing active.

Therefore the next repair should focus on action/threat coverage, not on redesigning particle compression.

### 3.8 Rules state already represents trade hostility and configurable game conditions

`GameState` already exposes several conditions that matter to the hostile-table strategy problem:

- `victory_target`;
- `card_discard_limit`;
- `friendly_robber`;
- `player_trades_enabled`;
- `domestic_trade_disabled`;
- directed `domestic_trade_embargoes`.

This is useful because economic-resilience strategy can be evaluated through real rules transitions rather than a synthetic "hostile mode."

The tracker also maintains opponent policy evidence for expansion, city/development preference, trade flexibility, and trade resistance. That is a useful base for opponent-style inference, but those traits are currently player-global rather than pair-specific hostility toward the root player.

### 3.9 The search code already contains two opponent-objective models

Production MaxN treats each acting opponent as maximizing its own value component. That is a reasonable default for ordinary competitive play.

The search code also contains a Paranoid model where non-root players minimize the root player's component. This approximates a fully coordinated anti-root coalition.

Neither extreme should automatically replace the other:

- MaxN can underestimate targeted spite, embargoes, or coordinated blocking;
- Paranoid can overestimate coalition discipline and make the engine excessively defensive.

The existing distinction provides a useful mechanism for **hostility stress testing** and, later, possibly a mixed objective model if evidence supports it.

### 3.10 Opening resilience and bank scarcity are already partially modeled

The second memo's number-dispersion and robber-resilience claims do not reveal a blank area in the current opening evaluator.

`engine/crates/catan-search/src/opening.rs` already includes:

- total production;
- `unique_strike_ways` across distinct roll numbers;
- `duplicate_number_exposure`;
- `shared_hex_exposure` when both buildings depend on the same hex;
- resource diversity;
- build-access timing;
- port option value weighted by production share;
- `opening_robber_concentration`.

This means the implementation question is **calibration and hostile-policy sensitivity**, not "add number diversity" from scratch.

Similarly, `engine/crates/catan-search/src/eval.rs::production_pips()` already discounts publicly depleted bank resources and blocked robber hexes. Deliberate bank-pressure tactics can therefore affect leaf value through the real state. Before adding any new scarcity heuristic, verify whether search actually exploits that consequence when doing so is useful.

## 4. Demonstrated strategic gaps

### 4.1 Longest Road valuation ignores topology vulnerability

`engine/crates/catan-search/src/eval.rs::longest_road_outlook()` mostly reasons from:

- current road length;
- best rival road length;
- distance to the qualifying threshold;
- approximate acquire/retain probabilities;
- rough additional road cost.

It does not explicitly price:

- critical cut vertices;
- Longest Road loss after a legal opponent settlement;
- route bypassability;
- opponent reach to the cut point;
- opponent affordability across belief particles;
- recovery cost after a cut.

As a result, two road networks with the same current Longest Road length can receive similar trophy outlooks despite radically different resilience.

### 4.2 Road frontier priors reward progress but not adversarial exposure

`road_frontier_value_with_context()` rewards a hypothetical road for improved expansion value and trophy outlook.

It does not subtract expected:

- settlement cuts;
- extension denial;
- choke loss;
- Longest Road award transfer;
- bypass/recovery cost.

That matters because root width is finite. A strategically robust road may never receive enough prior support to enter deep search if its value is primarily defensive.

### 4.3 Opponent settlement and road priors underweight disruption

CPU action priors favor settlement production quality and road expansion value. A low-production settlement that cuts a rival's eight-road chain can therefore rank below a normal high-production settlement.

Deep search can discover the tactical value if the action survives ranking and receives enough budget, but the live engine cannot rely on that happening consistently.

The CUDA continuation policy has the same issue more clearly:

- `vertex_policy_score()` emphasizes pips, resource diversity, and ports;
- `road_policy_score()` emphasizes endpoint and nearby expansion prospects.

Neither has a strong public-board disruption component for:

- cutting an opponent's road;
- taking or stripping Longest Road;
- denying a critical expansion site;
- seizing a choke.

Running more GPU simulations before fixing that proposal bias can produce a precise answer under a continuation policy that rarely attempts the strongest disruptive move.

### 4.4 Explicit opponent threat detection is too narrow

`engine/crates/catan-search/src/threats.rs` currently recognizes near-term threats such as:

- direct main-phase build wins;
- one-road Longest Road wins;
- Largest Army outlook;
- hidden VP wins;
- production-enabled wins;
- trade-enabled wins.

The threat layer is strongest when the opponent is already close to a literal immediate victory.

It does not comprehensively enumerate playable progress-card continuations before evaluating the opponent's next build sequence.

Consequences include missing explicit proofs for:

- Year of Plenty -> city/settlement/win;
- Road Building -> choke/settlement/Longest Road/win;
- Knight -> robber move/steal/Largest Army/build sequence;
- Monopoly -> resource transfer/build/win;
- development purchase -> stochastic VP win at nine points where applicable.

General search can still encounter these lines, but the dedicated tactical safety layer does not guarantee them coverage.

### 4.5 Trade safety ignores development-card follow-ups

This is the clearest concrete gap from the hidden-card investigation.

`engine/crates/catan-search/src/trade_safety.rs` evaluates whether a domestic trade newly enables dangerous opponent builds. Its bounded continuation explores only:

- `BuildRoad`;
- `BuildSettlement`;
- `BuildCity`.

It does not explore a beneficiary playing:

- Monopoly;
- Road Building;
- Year of Plenty;
- Knight.

Therefore an incoming trade can pass the dedicated hard-safety checker even in posterior worlds where the trader can immediately exploit the exchange with a ready progress card.

#### Dirty Monopoly example

Suppose an opponent has a ready Monopoly in a meaningful fraction of the posterior. We hold several ore. They offer another ore in exchange for something cheap. Accepting increases our ore concentration. They can then play Monopoly on ore and recover the offered ore along with the rest of our stockpile.

The current domestic-trade safety proof does not model that same-turn progress-card continuation.

This does not mean every such trade should be rejected. It means the expected malicious continuation must reach strategic search instead of being absent from the safety model.

### 4.6 Monopoly risk is not equivalent to seven-risk

`expected_discard_loss()` already prices the danger of carrying a large hand through future seven rolls.

Monopoly exposure is different. It is resource-specific and opponent-card-specific.

A diversified seven-card hand and a seven-card hand concentrated in one resource can have similar discard exposure while having very different expected loss against a likely Monopoly.

The engine currently lacks an explicit posterior metric for this distinction.

### 4.7 Simulated opponents only have a crude belief about our resources

Our tracker has rich evidence about opponent hands, including gains, spends, and trade behavior.

A skilled human opponent also card-counts us. This affects:

- Monopoly target selection;
- robber victim selection;
- trade decisions;
- assessment of our next build;
- blocking urgency.

Observation-safe GPU Monopoly targeting currently estimates opponent resource composition primarily from public hand size and public production mix. This avoids hidden-information cheating, which is correct, but it can be materially weaker than a human who tracked visible gains and spends.

This is a second-order improvement. The first priority is ensuring dangerous public or owner-known actions enter search at all.

### 4.8 MaxN can underestimate targeted coalition behavior

In production MaxN, each opponent chooses actions according to its own value component. That means a move with poor personal payoff may be underexplored even when it severely damages us.

A hostile table can violate that assumption. Examples include:

- accepting lower production to settle on our critical road vertex;
- refusing a mutually beneficial trade specifically because it helps us;
- repeatedly robbing us while another player is slightly stronger;
- spending roads to close our lane without an immediate personal expansion payoff.

The existing disruption-prior gap makes this worse: even before objective modeling, some anti-root actions are not proposed strongly enough.

The design should therefore separate two questions:

1. **Can the opponent see and propose the damaging action?** Fix this first through observation-safe disruption priors.
2. **How willing is the opponent to sacrifice its own value to hurt us?** Model this as opponent-policy uncertainty or a bounded stress case, not as a universal assumption.

### 4.9 Trade resistance is not the same as pairwise hostility

The tracker learns broad trade-flexible and trade-resistant tendencies, and the rules state can represent explicit pairwise embargoes. What is not yet represented as a strategic belief is a probability that a particular opponent will discriminate specifically against us even when normal trade utility would favor acceptance.

This matters for:

- port and maritime-trade value;
- whether a resource bottleneck can reasonably be solved through domestic trade;
- whether an apparently strong build plan depends on cooperation that is unlikely to arrive;
- whether offering a trade exposes information without realistic acceptance probability.

Do not solve this by assuming all trades fail. A future model, if needed, should infer pair-specific cooperation from observed accept/reject/embargo history.

### 4.10 Public threat salience is only weakly represented

Some policies already react to public victory points and board value, but there is no explicit model of "the table is now focusing us" as a latent state.

The hostile-table research suggests a measurable hypothesis: visible leadership may increase robber pressure, trade denial, and blocking beyond what pure self-interest predicts.

If game recordings contain enough evidence, this can later be modeled as a posterior over **targeting intensity**, conditioned on public signals and recent opponent behavior. Until then it should remain a research hypothesis, not a production penalty for being visibly ahead.

## 5. Proposed architecture: Latent Tactical Threat Outlook

The road work and hidden-card work should become one focused search-support layer rather than separate strategic modes.

A working name is `LatentTacticalThreatOutlook`. The final module name can be chosen during implementation planning based on ownership boundaries.

The layer should answer two related questions:

1. **What damaging tactical continuations are plausible before our next meaningful action?**
2. **How does each candidate root action change their probability or consequence?**

### 5.1 Threat families

At minimum, evaluate:

#### Public board / road threats

- settlement cut of our existing road network;
- rival road taking a critical edge or choke;
- settlement stealing an intended expansion endpoint;
- rival extension taking Longest Road;
- opponent settlement breaking our current Longest Road;
- route recovery/bypass requirement after disruption.

#### Hidden development-card threats

- Monopoly resource transfer followed by builds;
- Road Building followed by a settlement, trophy swing, or choke capture;
- Year of Plenty followed by immediate conversion;
- Knight followed by robber denial/steal and Largest Army consequences;
- hidden VP reaching the target;
- development purchase with a remaining immediate-win VP probability.

#### Trade-created threats

- a trade funding a direct build;
- a trade funding an award swing;
- a trade funding a contested settlement;
- a trade increasing Monopoly payoff;
- a trade feeding a Road Building/YOP/Knight continuation;
- a trade disclosing enough information to materially change an opponent's likely target, if opponent-belief modeling is later added.

#### Hostility / coalition threats

- repeated robber targeting beyond normal leader-sensitive policy;
- trade refusal or embargo directed at us;
- opponent willingness to spend tempo for a road cut or choke denial;
- multiple opponents choosing compatible blocking actions;
- reduced reliability of plans that assume domestic trade access.

### 5.2 Keep tactical threat and hostility uncertainty separate

`LatentTacticalThreatOutlook` should remain about concrete reachable consequences in a state or belief particle. Hostility is better represented as an uncertainty over **opponent action policy/objective**.

Conceptually:

```text
World uncertainty
  resources
  development cards
  bank/deck where hidden

Policy uncertainty
  ordinary self-interested play
  trade-resistant play
  expansion-denial tendency
  root-targeting / coalition pressure
```

Do not multiply these into an unconstrained Cartesian product at every node. A practical first use is to evaluate serious root candidates under the normal policy and one bounded hostile stress policy, then inspect how sensitive the decision is.

The existing MaxN/Paranoid distinction can help construct that stress test, but full Paranoid behavior should not become the default production policy without evidence.

### 5.3 Economic resilience should emerge from reachable alternatives

When domestic trading is unavailable or unreliable, the engine should value plans that retain independent conversion paths:

- maritime 4:1;
- 3:1 port access;
- relevant 2:1 port access;
- production diversity;
- build sequences that spend a surplus before discard or Monopoly exposure becomes costly.

Prefer evaluating those alternatives through actual acquisition rates, trade ratios, and legal actions. Avoid introducing an abstract "independence bonus" unless the existing search cannot express the consequence within budget.

## 6. Road Network Outlook

The road-specific component should compute exact structural damage where possible.

A conceptual representation:

```text
RoadNetworkOutlook
  longest_length
  longest_road_equity

  best_expansion_value
  expansion_portfolio_value

  critical_vertices[]
    vertex
    cut_loss
    award_swing
    opponent_access_mass
    protection_eta
    bypass_roads

  critical_edges[]
    edge
    expansion_loss
    opponent_access_mass

  maximum_cut_loss
  expected_cut_loss
  minimum_bypass_cost
  route_redundancy
```

These are design concepts, not a required public API.

### 6.1 Critical-vertex calculation

For each relevant open vertex on or adjacent to our road network:

1. determine whether an opponent settlement can legally occupy it under some plausible continuation;
2. hypothetically block traversal through that vertex;
3. recompute our exact Longest Road length;
4. recompute any Longest Road holder/VP consequence;
5. recompute relevant expansion reachability;
6. calculate the best bypass or recovery route.

Do not use a simplistic "two own roads touch this vertex" test. The correct damage is the actual path consequence after the settlement.

### 6.2 Route redundancy

Do not reward generic branching.

Reward redundancy only when it preserves something that matters after an opponent response, for example:

- the same settlement site remains reachable;
- Longest Road remains held;
- a choke remains controlled;
- the bypass materially reduces recovery cost.

Cycles and alternate paths often provide real resilience. Dead-end forks often do not.

### 6.3 Opponent access probability

Use existing legality and arrival machinery where possible.

Across weighted particles, estimate whether an opponent can reach and occupy the critical point before we can protect it.

This can include:

- current exact sampled hand from our posterior;
- ordinary paid road/settlement sequence;
- hidden Road Building continuation;
- turn order;
- currently available alternative routes.

The exact particle may be used for our posterior integration. Any simulated opponent action selection must remain observation-safe.

## 7. Hidden-card tactical grammar

Do not perform an unconstrained full-turn search inside every heuristic call. Enumerate a small tactical grammar that covers the high-impact surprise mechanisms.

Conceptually, for each opponent and weighted particle:

```text
No progress card
  -> direct builds / awards

Knight
  -> robber move
  -> steal chance
  -> Largest Army update
  -> builds

Monopoly(resource)
  -> transfer all matching resources
  -> builds / development purchase / awards

YearOfPlenty(first, second)
  -> receive resources subject to bank
  -> builds / development purchase

RoadBuilding(edge1, edge2)
  -> topology update
  -> settlement / Longest Road / choke consequences

BuyDevelopment
  -> chance outcome when an immediate VP draw is strategically relevant
```

Respect:

- card ownership in the particle;
- bought-this-turn restrictions;
- one progress-card play per turn;
- bank availability;
- normal build legality;
- information-set constraints.

## 8. Threat outputs

Rather than returning one arbitrary heuristic score, expose interpretable consequences that root ordering, safety logic, and search can use.

Possible metrics:

| Metric | Meaning |
| --- | --- |
| `immediate_win_mass` | Posterior mass where opponent can win before our next meaningful action |
| `forced_loss_mass` | Posterior mass where a candidate root action leaves a verified forced loss |
| `monopoly_loss_ev` | Expected resources lost under the opponent's best plausible Monopoly target |
| `monopoly_conversion_mass` | Posterior mass where Monopoly enables a meaningful immediate conversion |
| `road_building_cut_mass` | Posterior mass where Road Building reaches a damaging choke/cut |
| `road_award_swing_mass` | Posterior mass where Longest Road changes hands through the continuation |
| `yop_conversion_mass` | Posterior mass where YOP completes a meaningful build or win |
| `knight_award_mass` | Posterior mass where Knight changes Largest Army ownership |
| `robber_production_loss` | Expected production loss from the best plausible Knight/robber action |
| `dev_purchase_win_mass` | Chance-weighted immediate win probability from buying a development card |
| `expansion_denial_mass` | Posterior mass where our best expansion is removed or materially delayed |
| `expected_cut_loss` | Posterior-weighted Longest Road / route damage |
| `recovery_cost` | Expected cards/roads/tempo needed to restore the lost option |
| `trade_access_sensitivity` | Value lost when plausible domestic trade availability is removed |
| `hostile_policy_regret` | Root-action regret under a bounded anti-root stress policy versus the normal opponent model |
| `targeting_pressure` | Optional inferred probability of repeated robber/block/embargo focus, only if supported by recordings |
| `one_turn_close_mass` | Posterior mass where the player can complete a winning sequence before another opponent decision |
| `response_windows_to_close` | Expected opponent decision opportunities before the strongest reachable closeout |

The exact set should be minimized during implementation planning. This table is the threat vocabulary, not an instruction to create every field immediately. `hostile_policy_regret` and `targeting_pressure` are research-facing diagnostics first; they do not need to become production evaluation fields. `one_turn_close_mass` and `response_windows_to_close` should first be treated as planning/search diagnostics because existing tactical and whole-turn search may already encode much of the value implicitly.

## 9. Use the threat layer to improve search, not replace it

The strongest implementation is a four-layer design.

### Layer 1: root posterior risk and opportunity

For each serious root candidate, compare threat consequences before and after the action.

Use the **delta**:

```text
DeltaThreat(action) = Threat(after action) - Threat(before action)
```

This prevents existing unavoidable danger from automatically penalizing every move.

Examples:

- a road that reduces `road_building_cut_mass` is defensively valuable;
- a trade that sharply increases `monopoly_loss_ev` is strategically suspicious;
- a settlement that removes the opponent's only Road Building choke can be promoted;
- spending a concentrated hand can be valuable when it reduces Monopoly and seven exposure at the same time.

### Layer 2: opponent disruption priors

Add public-board disruption value to CPU and GPU opponent action priors so strong malicious moves receive search coverage.

Settlement prior should recognize public consequences such as:

- cutting a rival road;
- stripping Longest Road;
- denying a contested site;
- taking a choke.

Road prior should recognize:

- blocking a rival route;
- winning Longest Road;
- enabling a high-impact settlement;
- denying a critical edge.

Progress-card continuation policy should similarly prioritize consequential legal uses rather than generic resource weights alone.

These priors must use information visible to the simulated acting player plus that player's own private hand.

### Layer 3: hostile-policy stress check for sensitive roots

For a small set of serious root candidates, optionally compare the ordinary opponent model against a bounded anti-root stress model.

The purpose is not to choose the minimax-safe move automatically. The purpose is to detect decisions whose value collapses if one or more opponents are willing to target us.

Useful outputs include:

- ordinary expected value;
- hostile-stress value;
- regret between the two;
- which opponent action creates the collapse;
- whether a modest defensive alternative removes most of the sensitivity.

Use this selectively when there is evidence of hostility, an explicit embargo, repeated robber focus, a critical shared choke, or a close endgame. Do not spend a second full search budget on every routine decision.

### Layer 4: hard tactical safety only for near-forced cases

Hard vetoes or mandatory blocker promotion should remain narrow.

Use hard safety when the posterior supports a verified immediate or forced tactical loss, for example:

- opponent wins next turn unless we block;
- accepting a trade creates a near-certain immediate winning continuation;
- a candidate road leaves an unavoidable Longest Road cut that directly loses the game.

Ordinary strategic exposure should remain a soft search consideration.

## 10. Monopoly-specific defensive reasoning

Monopoly deserves explicit threat outputs because it interacts with hand shape, trades, ports, and build timing.

### 10.1 Resource-specific exposure

For each opponent with posterior Monopoly mass, estimate the best resource they would name from information available to them.

From our root perspective, integrate across exact resource particles to estimate actual consequence.

Distinguish:

- total hand size;
- concentration by resource;
- whether that concentration comes from a deliberate 2:1-port surplus strategy;
- whether the stolen cards complete the opponent's build;
- whether losing the cards destroys our planned build;
- whether spending, maritime conversion, or building now removes exposure.

### 10.2 Dirty Monopoly / malicious trade continuation

Trade evaluation should explicitly allow:

```text
trade
-> Monopoly(resource)
-> immediate spend
```

where legal in the particle.

The result should usually enter strategic scoring rather than become an unconditional rejection rule.

### 10.3 Opponent belief about our hand

Later, if benchmarks show meaningful remaining error, improve the simulated opponent's estimate of our resources using public event history rather than only production mix and hand total.

Do not make this the first implementation wave. It is more complex and is not necessary to fix the currently demonstrated omission of progress-card continuations.

## 11. Year of Plenty-specific reasoning

Treat a ready YOP as two conditional resources subject to bank availability.

The threat is not the intrinsic value of two cards. The threat is the best resulting conversion:

- city;
- settlement;
- road + settlement sequence where legal;
- development purchase;
- award setup;
- immediate win.

Threat detection should evaluate the relevant resource pairs rather than relying on direct visible affordability alone.

## 12. Road Building-specific reasoning

Road Building invalidates ordinary road ETA assumptions.

When the posterior contains a ready Road Building card, road-race analysis should consider:

- two free roads before our next action;
- Longest Road acquisition or retention;
- reaching a settlement site earlier than paid-road ETA predicts;
- seizing a choke;
- creating a cut route;
- bypassing a block.

This must integrate with the Road Network Outlook rather than become a separate static Road Building bonus.

## 13. Knight-specific reasoning

Knight threats combine three mechanisms:

1. robber relocation;
2. random steal;
3. Largest Army progression.

A ready Knight can also invalidate the assumption that an opponent's important production remains blocked through their next roll.

Threat search should consider:

- best public robber denial target;
- self-unblocking value;
- victim selection based on legal/public information;
- Largest Army ownership after the play;
- immediate build consequences after the steal where tractable.

Do not assume the exact stolen resource when evaluating an opponent policy. Treat the steal as chance from the opponent's information set.

## 14. Development-purchase win probability

When an opponent is within one hidden VP of the target and can buy development, explicit threat analysis should include the chance that the draw is an immediate winning VP card.

The probability comes from the remaining development-deck posterior, not from a fixed constant.

This only needs special treatment when the chance can change the root decision. It should not expand every ordinary threat calculation into a full development-deck search.

### 14.1 Closeout compression and opponent response windows

The useful part of the "stealth spike" strategy is not cosmetic hidden scoring. It is **action compression**: assembling a line that reaches the target with few or zero opponent decisions in between.

Examples include:

- Knight -> Largest Army -> city -> hidden VP closeout;
- Road Building or paid roads -> Longest Road -> settlement closeout;
- Monopoly -> immediate resource conversion -> winning build;
- Year of Plenty -> exact missing resources -> winning build;
- development-card purchase -> chance of immediate VP closeout.

The engine already proves some same-turn wins. The strategic extension is to recognize when a non-terminal setup move materially increases the posterior mass of a short, robust closeout while a superficially similar move exposes the plan to one or more opponent response windows.

Do not award hidden points or low public score directly. Compare actual reachable sequences, their posterior success mass, and the number of intervening opponent decisions.

## 15. GPU parity requirements

GPU verification is only meaningful if the GPU continuation policy proposes the relevant adversarial actions.

Before using large rollout counts as evidence for this feature, ensure CUDA policy can represent at least the same public disruption concepts as CPU search.

High-priority parity work:

- settlement disruption score;
- road/choke disruption score;
- Longest Road cut/award consequence;
- better Road Building pair selection for tactical topology changes;
- progress-card use weighted by immediate strategic conversion rather than generic resource value alone.

Avoid expensive full posterior analysis inside each CUDA rollout step. A practical split is:

- CPU/Rust root: posterior-aware latent-threat analysis;
- GPU continuation: public/owner-known disruption-aware action proposals;
- GPU simulation: exact rules and sampled hidden state.

## 16. Verification strategy

Do not begin by running only ordinary full games. First verify the tactical mechanisms with counterfactual states where the expected behavior is interpretable.

### 16.1 Road-defense corpus

Include paired states such as:

- fragile exposed road chain vs protected chain;
- same geometry with opponent cut affordable in low, medium, and high posterior mass;
- straight route vs one-road bypass/cycle;
- immediate Longest Road winning road;
- vanity Longest Road extension vs high-value settlement route;
- hidden Road Building threat at a choke;
- real choke block vs apparent block with an equivalent detour.

Track:

- chosen action;
- opponent cut rate;
- Longest Road loss/transfer;
- expansion denial;
- recovery cost;
- eventual win value/regret.

### 16.2 Hidden-card corpus

Include:

- dirty Monopoly accept vs reject;
- concentrated vs diversified hand at equal total cards;
- same state with Monopoly present vs absent in the opponent particle;
- YOP exactly completing a city;
- YOP exactly completing a winning settlement;
- Road Building crossing a two-road choke;
- Road Building taking Longest Road;
- Knight unblocking a critical producer before rolling;
- Knight taking Largest Army for the win;
- opponent development purchase at nine VP under different remaining VP-card counts;
- negative controls where the hidden card exists but cannot materially improve the opponent's position.

### 16.3 Hostile-table / coalition corpus

Add counterfactual states that isolate the claims from the hostile-table research:

- normal trade availability vs `player_trades_enabled = false` with the same board and hand;
- a targeted pairwise embargo vs ordinary trade access;
- high-production settlement vs slightly weaker port settlement under persistent trade denial;
- ordinary MaxN opponent response vs a damaging low-production road-cut settlement;
- ordinary robber targeting vs repeated root-targeting;
- one opponent blocking us at small personal cost vs two opponents making compatible blocks;
- visible Longest Road lead vs equivalent hidden development-card progress;
- development-heavy recovery after physical expansion is closed;
- negative controls where a port or defensive pivot costs more than the hostility it mitigates;
- equal-total-pip openings with concentrated vs diversified roll numbers;
- equal-production openings where one pair shares a high-value robber hex and the other does not;
- third-settlement-first vs city-first positions under matched resources to test whether the 36k timing correlation survives stronger search;
- wheat-heavy vs ore-heavy matched-production positions, varying the actual reachable build plan rather than assigning a global resource rank;
- normal randomized-board port opportunities vs deliberately bad PortRusher-style port chasing;
- bank-scarcity positions where holding a resource harms opponents enough to matter, plus negative controls where spending the hoard is better;
- one-turn closeout vs equal-VP multi-turn closeout with an opponent response window between scoring actions.

Track not only chosen action and win value but also:

- maritime-trade usage;
- failed/declined domestic-trade dependence;
- robber turns spent on the root player;
- expansion lanes lost;
- value under normal vs hostile opponent objectives;
- opportunity cost of defensive pivots.

These scenarios should establish whether the hostile-table adaptations are real engine improvements before adding any learned targeting model.

### 16.4 GPU counterfactual budget

For each tactical state, use enough GPU continuations per root to stabilize the comparison. A useful experimental range is roughly 256-1024 continuations per root, adjusted for state variance and runtime.

This is a benchmark design recommendation, not a production runtime requirement.

### 16.5 Whole-game matched A/B after tactical validation

After the tactical corpus behaves correctly, compare current engine vs latent-threat engine on matched games.

Use:

- same boards/seeds/chance streams where supported;
- seat rotation;
- P3 and P4 separately;
- zero or tightly bounded truncation;
- trade-enabled, no-player-trade, and targeted-embargo cohorts when economic-resilience behavior is being changed;
- a normal-opponent cohort plus a bounded hostile-policy stress cohort when opponent-objective modeling is being evaluated.

A reasonable first campaign is at least:

- 100 matched board/chance blocks in P3, rotated across all seats;
- 100 matched board/chance blocks in P4, rotated across all seats.

This produces roughly 300 P3 and 400 P4 candidate-seat games per variant.

The benchmark should report not only win rate but also tactical event deltas:

- harmful trades accepted;
- successful opponent road cuts;
- Longest Road surprise losses;
- progress-card-enabled opponent wins;
- robber concentration on the candidate seat;
- domestic-trade acceptance/denial by seat pair;
- port/maritime conversion usage;
- normal-policy vs hostile-policy action regret;
- defensive opportunity cost;
- time/turn to third settlement and first city;
- distinct-number exposure and shared-hex robber exposure;
- bank-denial value versus discard/Monopoly cost;
- posterior one-turn closeout mass and opponent response windows;
- decision latency and deadline/truncation rate.

## 17. Implementation staging

This section is intentionally a working sequence, not yet a file-level implementation plan.

### Stage A - Define tactical consequences and reuse rules transitions

Establish the minimum internal result types and helper boundaries for:

- road cut consequence;
- opponent progress-card continuation consequence;
- posterior aggregation.

Prefer existing `GameState::apply()`, Longest Road logic, exact progress-card machinery, and expansion helpers over duplicate rules.

### Stage B - Fix search coverage before tuning values

First make sure high-impact malicious opponent actions can enter search:

- CPU settlement/road disruption priors;
- CUDA settlement/road disruption priors;
- progress-card tactical continuation coverage.

This is more important than fine-tuning a static threat score.

### Stage C - Extend opponent immediate-threat proof

Add bounded progress-card-aware continuations to the threat layer where they can create an immediate or forced result.

Keep this proof small and tactical. Do not turn `threats.rs` into a second general search engine.

### Stage D - Extend trade safety

Evaluate newly enabled progress-card continuations after domestic trades, especially Monopoly.

Continue using posterior thresholds for hard vetoes. Ordinary uncertain malicious-trade risk should flow into strategic scoring instead of becoming a blanket rejection.

### Stage E - Add road-network resilience deltas to root ordering

Use exact road cut/award/expansion consequences to promote robust roads and defensive settlements when they materially change expected outcomes.

Do not add a generic "build a web" preference.

### Stage F - Add hostility stress evaluation only after action coverage is sound

Use the existing MaxN/Paranoid distinction, or the smallest equivalent policy mixture, to measure whether serious root choices are brittle to anti-root play.

Do not make Paranoid the default. First use it as a diagnostic/stress signal and compare against recorded hostile-table behavior where available.

### Stage G - Tactical GPU verification

Run the targeted counterfactual corpus, including trade-denial and hostile-blocking cases, and fix continuation-policy blind spots before treating large rollout batches as evidence.

### Stage H - Whole-game matched campaigns

Only after targeted behavior is correct, measure whether the added defense improves overall win rate without excessive conservatism or latency.

## 18. Anti-goals

Do not implement any of the following without new evidence:

- fixed OWS strategy mode;
- fixed expansion strategy mode;
- fixed Longest Road strategy mode;
- generic "web-shaped road" reward;
- assume every hidden development card is the worst possible card;
- hard-veto trades because Monopoly is merely possible;
- opponent policies that inspect hidden information unavailable to that opponent;
- a large pile of manually tuned defensive penalties replacing search;
- expensive full tactical search at every evaluation leaf;
- broad opponent-psychology or table-politics simulation before the demonstrated tactical omissions are fixed;
- hard-coded "appear weak" or sandbagging bonuses;
- social-chat manipulation or leader-calling scripts as part of the decision engine;
- replacing MaxN with full Paranoid play by default;
- assuming trade denial merely because a player is ahead;
- fixed port-over-production rules for hostile games;
- deterministic opponent-hand reconstruction after hidden steals/discards;
- off-turn Knight retaliation that the rules do not allow;
- generic resource-hoarding bonuses for bank starvation;
- intentionally suppressing public VP as a standalone strategy objective;
- treating the 36,000-game heuristic simulation as an optimal-play oracle.

## 19. Success criteria

This strategic work is successful when the engine can demonstrate all of the following:

1. It distinguishes a fragile road chain from a resilient route with the same nominal road length.
2. It searches low-production opponent settlements when they cause high-value road cuts or denial.
3. CPU and GPU continuation policies both propose consequential disruptive road/settlement actions often enough for search to evaluate them.
4. A posterior Road Building threat changes road-race ETA when it should, but not when the card cannot create a useful continuation.
5. A posterior Monopoly threat values hand concentration and malicious trade continuations contextually rather than categorically.
6. YOP and Knight can appear in immediate opponent-win proofs when they genuinely complete the winning sequence.
7. Trade safety recognizes progress-card-enabled tactical continuations without rejecting ordinary uncertain trades by default.
8. All opponent policies remain information-set safe.
9. Targeted counterfactual benchmarks show lower tactical regret before whole-game win-rate claims are made.
10. Matched P3/P4 campaigns show that the added anticipation improves or preserves overall strength without unacceptable decision latency.
11. Port and maritime-trade value rises appropriately when domestic trade is actually disabled or embargoed, without causing unnecessary port chasing under normal trade access.
12. The engine can identify root choices that are unusually brittle under targeted anti-root behavior without defaulting to universally paranoid play.
13. Strategy remains correct under configurable `victory_target`, discard limit, friendly-robber, and trading rules rather than relying on one standard ruleset.
14. Any future targeting-pressure model is justified by recorded behavior and improves prediction of robber/block/embargo choices on held-out games.
15. Opening evaluation preserves the useful empirical signal from number diversity and robber resilience without sacrificing clearly superior production merely to diversify.
16. Hidden-hand reasoning remains probabilistic after random steals and other hidden events; no strategy path assumes exact opponent cards without evidence.
17. Knight/robber defense respects the legal response window: a Knight can clear the robber on our next legal turn, not during the opponent's turn.
18. Same-turn and short-horizon closeouts are valued because they remove opponent response windows, without adding an artificial reward for low visible VP.
19. External 36k-simulation findings are reproducible or falsifiable in our own matched GPU/CPU benchmark corpus before they influence production coefficients.

## 20. Current recommended direction

The strategic improvement should be framed as:

> **Posterior-aware adversarial anticipation over public topology, inferred resources, hidden development cards, and bounded opponent-policy uncertainty, used to improve tactical coverage and root ordering while leaving final choice to search.**

The highest-priority demonstrated gaps are:

1. opponent settlement/road priors do not sufficiently value disruptive public-board actions;
2. explicit opponent threat detection omits several progress-card-enabled continuations;
3. domestic trade safety explores only direct builds and therefore misses same-turn progress-card exploitation, with dirty Monopoly as the clearest example;
4. road valuation does not explicitly price cut vulnerability, bypassability, and posterior opponent access;
5. ordinary MaxN does not represent opponents who accept personal opportunity cost to target the root player, while full Paranoid search is too strong an assumption to use indiscriminately;
6. economic-resilience decisions do not yet have an explicit planning/verification framework for trade denial, embargoes, and repeated targeting.

The implementation order should remain causal: first guarantee that damaging actions enter CPU/GPU search, then extend concrete tactical threat proofs, then measure whether a bounded hostility model adds value beyond ordinary MaxN. Social signaling and table-politics behavior remain outside the production engine unless recordings demonstrate a predictable, actionable effect.

## 21. External evidence reviewed

This working design used the following external evidence to separate rules, empirical observations, and strategic hypotheses. These sources are inputs to benchmark design; they are not substitutes for repository-level verification or our own GPU/CPU experiments.

### Current CATAN Championship Tournament Rules

Current tournament rules were used to verify:

- strategic cooperation that benefits the participants is not automatically prohibited;
- intentionally giving another player a win/advancement without personal benefit is prohibited;
- resource-trade negotiation is prohibited while the robber action is unresolved;
- bargaining about robber placement is allowed;
- resource/development identities must remain hidden;
- a newly purchased VP development card may immediately produce the winning point;
- trade/build actions may be repeated in the action phase;
- bank shortages can suppress production.

### Official CATAN base-game FAQ

The base-game FAQ was used to verify:

- a player may make trade offers to probe likely resource holdings before playing Monopoly;
- opponents are not required to provide truthful resource information in that discussion;
- resource-card identities remain hidden while hand totals are public;
- a Knight may be played before rolling on its owner's turn;
- robber placement can be discussed even though the resource transfer itself must wait until the robber action resolves.

### April 2026 36,000-game heuristic simulation

The simulation is useful as a large exploratory experiment, not as an optimal-play oracle. Its own methodology notes fixed heuristic agents, simplified trading, no collective robber coordination, and no negotiation/bluffing/alliance model.

The results used here only as calibration hypotheses are:

- Longest Road frequency/timing in simulated wins;
- third-settlement timing gap;
- wheat winner/loser production gap;
- weak marginal signal from raw starting pip totals after reasonable placement quality;
- context dependence of fixed OWS/Road/Port/Balanced agents;
- the warning that naive port chasing can reduce win rate on random boards.

Before any of these findings changes production coefficients, reproduce or falsify it using our own engine, rules model, opponent policies, and matched-state benchmarks.
