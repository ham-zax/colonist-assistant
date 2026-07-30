# Strategist architecture

Colonist Assistant uses one local decision pipeline:

```text
Colonist observations
  → weighted hidden-state filter
  → exact local mandatory and parameter solvers
  → bounded whole-turn candidates and structured action ordering
  → setup: belief-aggregated MaxN draft search
  → normal play: bounded weighted-belief Deep MaxN
  → structured evaluator + opponent/trade signals
  → state-validated first-action executor
```

The Rust rules engine remains the source of truth for legality. TypeScript
extracts the live board, maintains evidence, renders advice, and executes only
the first validated action of Strategist's authoritative result. Experimental
belief PUCT, AlphaBeta, and UCT remain diagnostic policies in the native arena;
replay tooling also exposes selected diagnostic budgets. Users cannot select
them as live action authorities. The bundled learned policy and value heads are
unpromoted and disabled.

## Beliefs

Every hidden world carries a posterior weight. Unknown steals branch in
proportion to the victim's possible card counts; discard branches use a
plan-aware discard policy. Public trades, offers, accepts, rejections,
counters, development-card use, hand totals, the bank, and the user's exact
visible cards update the posterior.

The filter reports effective sample size and uses deterministic stratified
resampling with support-preserving rejuvenation. Representative search
particles preserve strategically different worlds, including city readiness,
settlement races, trade feasibility, Monopoly totals, and hidden victory-point
threats.

## Exact decisions

The engine enumerates legal outcomes for:

- discards;
- robber placement and victim;
- Year of Plenty pairs;
- Monopoly resources;
- coherent Road Building pairs;
- incoming trade accept, reject, and counter continuations;
- completed trades with multiple possible partners;
- forced current-turn wins.

Here, exact means that the bounded local action family is completely
enumerated. Its strategic utility and forced continuation tail are still model
estimates; the result is not an exact solution of the full game. A line is
reported as tactically proven only when the bounded solver establishes the same
observable first action across every materially weighted world without
exhausting its proof limit.

Robber actions are compound `(hex, victim)` choices. Their exact comparison
uses opponent production denial, public race threat, the belief-weighted
stolen-card tail, and the acting player's blocked production. A self-only hex
is treated as dominated when an empty or opponent target is legal; the
structured global value cannot override that local protocol score.

## Opening search

Initial settlements and roads use a dedicated bounded snake-order solver rather
than the normal turn-depth horizon. It follows the first settlement through a
pruned set of opponent picks, the best surviving second settlement, its
starting resource payout, and both settlement-anchored road directions.

The endpoint combines future road reach with the opening factors supported by
Guhe and Lascarides' JSettlers ablation: total production, different roll
numbers, an extra penalty for putting both settlements on the same hex, and
weighted access to the road, settlement, city, and development-card resource
combinations. Strategist routes setup through belief-aggregated MaxN with a
separate node budget. Public board geometry dominates setup, but the
implementation still preserves the weighted information set instead of
silently switching to perfect information.

## Whole-turn plans

Bounded main-phase candidates consider useful endpoints rather than scoring
only the next click. Candidate plans can include a development-card play,
domestic or maritime trade, road, settlement, city, development-card purchase,
and end turn. The plan generator deduplicates equivalent endpoints, prevents
conversion loops, removes dominated trades, and supplies structured ordering
signals to the bounded MaxN search. It does not exhaustively enumerate every
possible turn sequence.

Roads are valued by reachable settlement sites, arrival races, ports, blocking
value, Longest Road probability, and cut risk. A road without a useful
destination, block, trophy line, or hand-safety purpose has little value.

## Strategic search

Strategist is the sole production engine. During normal play it runs
vector-valued Deep MaxN over observer-consistent weighted hidden-state
particles. Root priors are averaged over the weighted posterior before
action-class quotas are applied, so candidate survival cannot depend on which
hidden particle happens to be listed first. Every retained root action receives
a fair bounded slice in every materially weighted world; illegal hidden-world
instances retain their no-action baseline rather than disappearing from the
denominator. Posterior-weighted values and legal support are then aggregated
at the observable root.

Within a simulated world, each acting player maximizes its own component of the
race-value vector. Chance nodes use their rules-engine probabilities. Depth
advances on completed turns, while a separate in-turn action bound prevents
unbounded trade/build sequences. This is preferable to paranoid AlphaBeta's
assumption that every opponent forms one coalition against the root player, but
it is still a bounded model of opponent behavior rather than a solved
multiplayer equilibrium.

Action-class quotas preserve representative settlements, expansion roads,
cities, hand-safety conversions, trades, trophies, development-card families,
and end-turn choices in the ordered candidate pool. Exact parameter solvers can
replace a generic family representative, such as selecting Monopoly's resource
over the complete belief before the strategic budget is divided. The normal
live request uses depth 4, branch cap 16, a 4,000-node limit, and a cooperative
350 ms strategic-search deadline. Setup uses the same deadline with its
dedicated draft solver; trade responses and pondering use separate bounded
node profiles.

The browser and native search share the same wall-clock deadline. Search checks
it between bounded slices and once more before returning. If time expires in
the middle of one hidden world's root-action row, the complete row is replaced
with a uniform structured fallback, so action order cannot decide which
candidates received deep values. Setup similarly falls back across the
complete observable root. `deadlineReached` therefore describes the strategic
MaxN or opening phase. Mandatory actions and exact tactical family solvers have
their own small limits and deliberately remain outside that flag.

Domestic offers and counteroffers carry a small search-time negotiation cost
so a superficially neutral exchange cannot consume a turn through repeated
low-probability proposals. When an offer reduces an above-threshold hand, that
cost is quartered to retain hand-safety conversions. In a same-seed seven-game
behavioral smoke, this reduced offers from roughly 31–35 per candidate game to
5–6 and increased observed acceptance from roughly 2–7% to 20–29%, without
reducing the candidate's win count. This is a regression signal, not a
statistically useful strength estimate.

The packaged cold-adapter regression crosses the generated WASM boundary and
must remain below one second. An archived-state pre-release ablation replayed
366 requests at the 350 ms profile, returned every one in less than 410 ms on
the reference machine, and matched the 600 ms profile's first action in 360
cases. This is a release latency/stability regression, not a universal timing
guarantee or strength result. The one-second UI warning and twelve-second
client cutoff are fail-closed containment, not performance targets.

Stable observation hashing, action ordering, and node allocation make the
bounded MaxN path reproducible for the same build and request. Experimental
belief PUCT, UCT, and paranoid AlphaBeta remain native-arena comparisons; no
PUCT result is production evidence merely because the arena retains
`strategist` as a compatibility name. Arena `deep` resolves to MaxN.

## Race-to-win value

The value function combines:

- public and believed victory-point race;
- current production and bank scarcity;
- context-dependent resource deficits;
- nonlinear expected loss to a seven before the next spend;
- expansion-site survival and complete road-plus-settlement cost;
- marginal development-card utility and play congestion;
- probability of acquiring and retaining Longest Road and Largest Army;
- robber denial, ports, piece inventory, and opponent threat.

Displayed shares are labelled `WIN` but remain stabilized model estimates, not
empirical guarantees or calibrated probabilities. The presentation blends
Strategist's root utility with an independent public build-time prior,
regularizes early extremes, and rate-limits changes that are not accompanied
by material public progress. The display model never chooses an action.

## Trading

Offers are generated from the deficits of top build plans. The bounded
neighborhood includes one-for-one, two-for-one, mixed bundles, one-for-two,
two-for-two, and hand-safety conversions. Offers worse than a known maritime
conversion, unrelated to a plan, repeated unchanged after rejection, or likely
to enable a leader are suppressed.

Acceptance is estimated per recipient from their observation, production,
ports, hand size, threat, likely plan, history, and opponent profile. The
creator waits for all responses and selects the accepting partner with the best
resulting race value. Responses update the belief filter.

Before accepting, countering, confirming, or offering a domestic exchange, a
bounded safety proof checks whether the transferred cards newly enable an
opponent's immediate win, award swing, or capture of a visibly contested
settlement. It becomes a hard veto only at 99% weighted-posterior support;
uncertain threats remain available to normal strategic search.

## Learned models

The Expert Iteration tooling can record diagnostic search targets and terminal
outcomes from a league of MaxN, experimental belief PUCT, AlphaBeta, UCT,
weighted, and style-varied opponents. A heterogeneous graph represents hexes,
vertices, edges, players, the bank, development deck, and parameterized legal
actions.

Checkpoints are promoted only when game-grouped held-out tests show:

- at least 20 independent validation seed groups;
- value log loss beats a uniform player prior by at least 0.002;
- value Brier score beats a uniform player prior by at least 0.001;
- policy cross-entropy beats a uniform legal-action prior by at least 0.001;
- trade acceptance log loss and Brier score beat the training base rate.

Those gates measure predictive quality, not playing strength. A checkpoint also
needs a separately powered arena evaluation before it can support a policy
promotion claim.

The bundled checkpoint has only two validation state groups and misses the
required evidence threshold. `VALUE_MODEL_PROMOTED` and
`POLICY_MODEL_PROMOTED` are both false. Neither learned head contributes to
live action ordering or leaf values; structured priors and the structured
strategic evaluator remain authoritative. A later training run must pass the
appropriate gates, followed by a separately powered arena evaluation, before
generated weights can enable either learned head.

Cutoffs are marked `terminal: false`, carry zero outcome vectors, and are
excluded from arena win and calibration statistics. The raw arena
`winner` field on a cutoff is an evaluator adjudication, not a terminal label,
and training must not use it as one. Training and validation never split states
from the same board/chance seed pair.

## Executor authority

The live priority is:

1. mandatory protocol action;
2. belief-wide proven tactical action;
3. current deep-plan first action;
4. explicit engine-error state with no policy substitution.

Heuristics cannot click while a valid deep request is pending. Multi-click
workflows assert game, turn, phase, state signature, and legal targets before
every continuation. A build-button click retains the searched board target
through Colonist's placement transition; stale road plans cannot substitute a
different target.

The decision-trace schema records deep candidates, particle support, latency,
model versions, final source, whether anything executed before Strategist, and
the outcome of the state-validated click when those fields apply. The replay
tool can rerun diagnostic MaxN and experimental PUCT budgets and compare
per-particle counterfactuals. Those hypothetical particle worlds are not a
complete-information oracle unless the actual hidden state is independently
known.
