# Strategic engine v3

Colonist Assistant uses one local decision pipeline:

```text
Colonist observations
  → weighted hidden-state filter
  → exact mandatory/current-turn solver
  → whole-turn plan generator
  → belief-aggregated multiplayer MaxN
  → learned value, policy, opponent, and trade models
  → state-validated executor
```

The Rust rules engine remains the source of truth for legality. TypeScript
extracts the live board, maintains evidence, renders advice, and executes only
the first validated action of the authoritative plan.

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

A tactical line is marked proven only when the same observable first action
wins in every materially weighted world without exhausting its proof bound.

## Whole-turn plans

Main-phase actions are compared as complete endpoints rather than isolated
clicks. Plans can include a development-card play, domestic or maritime trade,
road, settlement, city, development-card purchase, and end turn. The planner
deduplicates equivalent endpoints, prevents conversion loops, removes
dominated trades, and preserves candidates from every relevant strategic
class.

Roads are valued by reachable settlement sites, arrival races, ports, blocking
value, Longest Road probability, and cut risk. A road without a useful
destination, block, trophy line, or hand-safety purpose has little value.

## Strategic search

The production engine is deterministic multiplayer MaxN aggregated over
weighted hidden worlds. It searches complete turn plans with a bounded global
node budget and backs up a win-race value vector. At each simulated player
node, that player's component selects the continuation.

Action-class quotas prevent settlements, expansion roads, cities, hand-safety
conversions, trades, trophies, or end turn from being silently removed by one
global cutoff. Root actions are evaluated over the full posterior, including
rejection or illegality branches.

Belief PUCT remains available as an experimental shared information-set search
with weighted particle sampling, progressive widening, and tree reuse.
Paranoid AlphaBeta is a selectable defensive peer. Search is reproducible from
the canonical observation hash, configuration, stable ordering, and
deterministic random streams.

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
empirical guarantees. The UI regularizes early extremes and rate-limits changes
that are not accompanied by material public progress.

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

## Learned models

Expert Iteration records deeper PUCT visit distributions and terminal winners
from a league of PUCT, MaxN, alpha-beta, UCT, weighted, and style-varied
opponents. A heterogeneous graph represents hexes, vertices, edges, players,
the bank, development deck, and parameterized legal actions.

Checkpoints are promoted only when game-grouped held-out tests show:

- value log loss and Brier score beat a uniform player prior;
- policy cross-entropy beats a uniform legal-action prior;
- trade acceptance log loss and Brier score beat the training base rate.

Cutoff games never receive fabricated winners. Training and validation never
split states from the same board/chance seed pair.

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

Every decision trace records the deep candidates, particle support, latency,
model versions, final source, whether anything executed before deep search,
and the outcome of the state-validated click. The replay tool can compare the
live policy, MaxN-only, larger-budget MaxN, complete-information oracle, and
ablation configurations.
