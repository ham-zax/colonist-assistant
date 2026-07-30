# Strategist architecture

Colonist Assistant uses one local decision pipeline:

```text
Colonist observations
  → weighted hidden-state filter
  → exact local mandatory and parameter solvers
  → opponent immediate-win threat retention
  → relevance-conditional root candidates
  → adaptive node allocation over a compact particle subset
  → setup: public snake-order opening oracle
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

Production MaxN evaluates particles after an observer-consistent root. Deeper
simulated opponents select with an observation-safe public utility so identical
observations yield identical strategies. Shared observation-keyed helpers also
exist for the eventual ISMCTS successor. Experimental belief PUCT remains a
diagnostic arena policy and is not the live action authority.

## Beliefs

Every hidden world carries a posterior weight. Unknown steals branch in
proportion to the victim's possible card counts; discard branches use a
plan-aware discard policy. Public trades, offers, accepts, rejections,
counters, expirations, development-card use, hand totals, the bank, and the
user's exact visible cards update the posterior.

Durable tracker events include `trade-offered`, `trade-accepted`,
`trade-rejected`, `trade-countered`, and `trade-expired` in addition to
completed trades. Those events update hand feasibility, offer/reject propensity,
and recipient-specific negotiation history. Live Colonist active-trade panel
diffs emit them; the public game log alone does not reliably surface offers,
rejects, counters, or expirations.

The filter reports effective sample size and uses deterministic stratified
resampling with support-preserving rejuvenation. Representative search
particles preserve strategically different worlds, including city readiness,
settlement races, trade feasibility, Monopoly totals, and hidden victory-point
threats. Exact mandatory and tactical solvers see the fuller posterior; the
strategic MaxN layer then searches a compact representative subset.

## Exact decisions

The engine enumerates legal outcomes for:

- discards;
- robber placement and victim;
- Year of Plenty pairs;
- Monopoly resources;
- coherent Road Building pairs;
- incoming trade accept, reject, and counter continuations;
- completed trades with multiple possible partners;
- forced current-turn wins;
- opponent immediate-win threats on their next main phase.

Here, exact means that the bounded local action family is completely
enumerated. Its strategic utility and forced continuation tail are still model
estimates; the result is not an exact solution of the full game. A line is
reported as tactically proven only when the bounded solver establishes the same
observable first action across every materially weighted world without
exhausting its proof limit.

Opponent threat detection looks for immediate settlement/city conversion wins,
Longest Road and Largest Army swings, hidden victory-point thresholds, and
production-enabled wins. Blocking settlements, roads, and robber hexes are
forced into the strategic root before ordinary relevance quotas apply.

Robber actions are compound `(hex, victim)` choices. Their exact comparison
uses opponent production denial, public race threat, the belief-weighted
stolen-card tail, and the acting player's blocked production. A self-only hex
is treated as dominated when an empty or opponent target is legal; the
structured global value cannot override that local protocol score.

## Opening search

Initial settlements and roads use a dedicated public snake-order solver rather
than the normal turn-depth horizon. Setup is board-driven; the live path no
longer describes it as belief-aggregated particle search.

The solver statically scores every legal first click, then spends the deep draft
budget preferentially on the strongest candidates. Opposing seats greedily
maximize their own opening features over a pruned candidate set. The endpoint
combines multi-road expansion portfolio value, port flexibility, board-wide
resource scarcity alignment, robber concentration, and the JSettlers-style
production, number-diversity, shared-hex, and build-coverage terms.

Live setup uses a larger cumulative budget than ordinary turns, and not-my-turn
pondering can continue opening analysis while opponents place. A wall-clock
cutoff preserves completed deep values for candidates that already finished a
draft leaf instead of rewriting the entire root row to the same static score.

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
relevance-conditional quotas are applied, so candidate survival cannot depend
on which hidden particle happens to be listed first.

Root ordering prefers spatial coverage over unconditional family quotas: two
settlements, three route-distinct roads, two cities, material trades, one or
two relevant development lines, trophy/hand-safety when active, and end turn.
Threat-blocking actions are inserted first. The live branch cap is eight.

Node allocation is adaptive rather than uniform: about 70% of each particle's
budget goes to the leading four root actions, 20% to challengers, and 10% to
the remaining uncertainty-sensitive tail. Strategic MaxN also searches about
twelve representative particles rather than diluting 4,000 nodes across thirty-
two near-duplicate worlds. Exact safety checks still use the fuller posterior.

Within a simulated world, the protected root still maximizes its private
race-value component. Opposing actors follow a prior-weighted mixture over the
top observation-ranked actions rather than privately maximizing over hidden
cards, so worlds that look identical to an opponent share one strategy while
still covering more than a single greedy line. Chance nodes use their
rules-engine probabilities. Depth advances on completed turns, while a separate
in-turn action bound prevents unbounded trade/build sequences.

The normal live request uses depth 4, branch cap 8, a 4,000-node limit, and a
cooperative 350 ms strategic-search deadline. Setup uses a larger dedicated
draft budget and deadline; optional post-draft rollouts remain off by default
until held-out opening regret justifies enabling them. Trade responses and
pondering use separate bounded node profiles.

The browser and native search share the same wall-clock deadline. Search checks
it between bounded slices and once more before returning. If time expires in
the middle of one hidden world's root-action row, the complete row is replaced
with a uniform structured fallback, so action order cannot decide which
candidates received deep values. Setup preserves completed deep values and
falls back only for unevaluated candidates. `deadlineReached` therefore
describes the strategic MaxN or opening phase. Mandatory actions and exact
tactical family solvers have their own small limits and deliberately remain
outside that flag.

Domestic offers and counteroffers carry a small search-time negotiation cost
so a superficially neutral exchange cannot consume a turn through repeated
low-probability proposals. When an offer reduces an above-threshold hand, that
cost is quartered to retain hand-safety conversions.

The packaged cold-adapter regression crosses the generated WASM boundary and
must remain below one second. This is a release latency/stability regression,
not a universal timing guarantee or strength result. The one-second UI warning
and twelve-second client cutoff are fail-closed containment, not performance
targets.

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
- expansion-site survival plus a top-three expansion portfolio;
- settlement/city readiness tails (probability ready within 1–2 own turns);
- phase-conditioned weights for production, expansion, liquidity, and trophies;
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
resulting race value. Responses update the belief filter through durable
offer/accept/reject/counter events as well as completed trades.

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
