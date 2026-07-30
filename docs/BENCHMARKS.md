# Benchmarks

Last updated: July 30, 2026.

## Current method

- The native arena simulates standard base-game Catan for 2–4 players.
- A matched block rotates the candidate through every seat. Every rotation in
  that block reuses the same board and chance seeds; later blocks use distinct
  deterministic seed pairs.
- By default, arena MaxN, experimental belief PUCT (the `puct` token),
  AlphaBeta, and UCT receive the same deterministic observer-consistent
  weighted belief particles.
  `--perfect-information` is a separately labelled oracle mode and must not be
  mixed with weighted-belief results.
- Arena particles are synthetic determinizations. They do not reproduce the
  live event-conditioned belief posterior and therefore do not measure
  end-to-end card-inference quality.
- `--validate` checks invariants after every transition and aborts on the first
  failure.
- Cutoffs are reported separately. Wins, calibration, and the block bootstrap
  use terminal games only.
- `--trajectory-output trajectory.jsonl` records per-turn trajectory samples
  for every matched-block seat rotation: public and actual victory points,
  production pips, expansion outlook, development-card queues, award holders,
  offer/accept counts, and estimated win value. Use these to distinguish
  concealed strength from weak early infrastructure plus late tactical rescue.
- Reported confidence intervals are 95% block bootstraps. They become useful
  only with many independent matched blocks; a single block produces a
  degenerate interval.
- Weighted and random policies are engineering baselines, not skilled-human
  proxies.

The packaged live Strategist is not the arena policy named `puct`. Production
uses weighted-belief Deep MaxN. In the native arena, `maxn` and its `deep`
alias are the closest comparison. `puct` selects experimental belief PUCT, and
`strategist` remains a compatibility alias for `puct`. Results must name the
actual arena policy instead of transferring the user-facing Strategist brand
to it.

## Historical experimental belief-PUCT behavioral smoke

The following runs used the native arena's experimental belief-PUCT policy.
They are one deliberately repeated historical diagnostic block, not a result
for the current live Deep MaxN core and not a strength evaluation. Each stage
used:

- four players and one matched block;
- arena `puct` (experimental PUCT) in every seat once against three
  weighted policies;
- seed `9200001`, with identical board/chance seeds across seat rotations;
- weighted-belief mode with 32 particles;
- 112 iterations and 72 rollout actions;
- invariant validation;
- four terminal games and zero cutoffs.

| Revision stage | Wins | Mean roads | Mean settlements | Mean cities | Mean dev buys | Mean offers | Mean counters | Mean decision latency |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Before the core fixes | 0/4 | 8.75 | 2.25 | 1.00 | 1.25 | 45.75 | 80.50 | 441.7 ms |
| After the core fixes | 1/4 | 4.75 | 3.25 | 0.50 | 5.50 | 3.00 | 2.25 | 872.4 ms |
| Experimental PUCT, after development congestion | 2/4 | 6.00 | 4.00 | 1.00 | 3.50 | 2.75 | 1.75 | 839.9 ms |

These paired experimental smokes expose behavioral pathologies and verify that
each revision can finish a validated seat rotation. They show the removal of
the offer/counter loop and the intended reduction in development-card
congestion. They do **not** estimate win rate, prove that each revision is
stronger than the last, justify promotion, or validate the current live search.
Four games are dominated by board, dice, and trajectory variance; the
one-block bootstrap is not informative.

An earlier three-player pre-fix experimental PUCT diagnostic used 320
iterations. It won 1/3, averaged 11.67 roads, and averaged 1,497.8 ms per
decision. It is retained only as evidence of the road-spam regression that the
later work targeted. Its player count and budget differ, so it is superseded
and is not directly comparable with the table above. It was never a
production-latency measurement.

A post-fix independent three-player smoke used seed `9300001`, one matched
block, 112 iterations, 72 rollout actions, and 32 weighted-belief particles.
All three games were terminal with zero cutoffs; they averaged 139.7 turns and
690 actions and took 270.3 seconds in total. Experimental PUCT went 0/3. Like
the four-player table, this tiny block is useful for termination and invariant
coverage only. It is not evidence that the policy is weaker or stronger than
the baseline, is not evidence about live Deep MaxN, and is recorded here to
avoid selecting only favorable diagnostic seeds.

The final paired smoke can be rerun with:

```bash
cd engine
cargo run --release -p colonist-catan-arena -- \
  --players 4 --blocks 1 --threads 4 --validate --quiet --json \
  --candidate puct --baseline weighted \
  --iterations 112 --rollout-actions 72 --belief-particles 32 \
  --seed 9200001
```

Archive the exact source revision with any result. The arena output records the
configuration but does not by itself make results from changed code
comparable.

## Current packaged Deep MaxN latency regression

The packaged cold-adapter test crosses the real generated WASM boundary,
constructs the normal live request, and requires a legal weighted-belief Deep
MaxN result in less than one second.

The normal live configuration uses depth 4, a branch cap of 8, a 4,000-node
strategic limit, a cooperative 350 ms deadline, and a compact representative
particle subset for MaxN. Exact solvers still see the fuller posterior. Setup
uses a larger dedicated draft budget. Trade responses and pondering use
separately bounded node profiles. The deadline is checked between bounded
search slices. If it expires partway through a hidden-world action row, that
entire row receives the same structured fallback so earlier actions cannot
retain an unfair deep-search advantage. Mandatory actions and the small exact
parameter solvers remain separately bounded and are intentionally outside this
strategic-search flag.

In the pre-release timing ablation, the 350 ms profile replayed 366 request
states from two headed games. Every replay returned in less than 410 ms on the
reference machine. Compared with a 600 ms profile, 360 of 366 first actions
were identical; the six differences were constructive or trade/end-turn
choices rather than legality failures. These archived-state measurements are
latency and stability evidence, not a universal guarantee or a strength
estimate. The one-second adapter assertion is the release ceiling, while the
one-second warning and twelve-second client cutoff exist only to fail closed.

## Current Deep MaxN negotiation-friction smoke

A same-seed behavioral smoke compared the current Deep MaxN against the
weighted heuristic before and after adding a small search cost for domestic
offers and counteroffers. Trades that reduce an above-threshold hand receive
only one quarter of that cost. Both runs used seed `9500001`, 32 iterations,
40 rollout actions, four belief particles, matched chance streams, and seat
rotation:

| Players | Revision | Games | Terminal | Cutoffs | Candidate wins | Offers/game | Accept rate | Candidate VP |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 3 | Before | 3 | 3 | 0 | 1 | 35.33 | 1.89% | 6.00 |
| 3 | Current | 3 | 3 | 0 | 1 | 5.67 | 29.41% | 6.00 |
| 4 | Before | 4 | 4 | 0 | 2 | 31.50 | 7.14% | 6.75 |
| 4 | Current | 4 | 4 | 0 | 2 | 5.00 | 20.00% | 7.25 |

The change removed most unproductive negotiation while preserving the observed
win count and hand-safety offers in this tiny sample. Seven games are enough
for a behavioral regression, not a strength claim. A separate experimental
PUCT-versus-MaxN four-player rotation did not finish before its 210-second
external harness limit, which is additional throughput evidence for keeping
PUCT out of the live path; it is not an arena cutoff or a completed result.

## Current 0.8.0 headed Chromium release smoke

The packaged 350 ms release profile completed one headed four-player Base game
against three Colonist Hard bots on July 30, 2026:

- terminal result: rank 1, recognized from Colonist's exact victory heading;
- game clock: 10:29 and 115 turns; harness wall time: 637.9 seconds;
- validated autonomous DOM/canvas clicks: 321;
- recorded decisions executed before a Strategist result: 0;
- local-fallback traces: 0;
- 171 completed packaged-WASM searches had 89.1 ms median and 880.1 ms p95
  trace latency;
- no service request exceeded five seconds; among the five requests logged for
  more than one second end to end, service time was at most 860 ms;
- eight volatile incoming-trade counter workflows failed closed: four because
  Colonist did not commit the counter and four because the state signature or
  legal target set changed. Every one replanned and the game continued;
- the trace audit found zero repeated offers, impossible trades, stale rolls,
  robber moves to the current hex, unsafe end-turns with a deterministic hand
  conversion, or postgame actions.

Four trace lifecycles exceeded five seconds, all around those superseded
incoming trades; none was a five-second engine-service request or a committed
stale action. The immediately preceding release-candidate game also reached
Colonist's victory screen at 11 points after 12:32 and 108 turns, but exposed a
harness and bridge bug: the exact terminal matcher accepted `Victory!` but not
Colonist's `Victory!!!`. That run was classified as a harness failure after the
already-finished game sat for 90 seconds. The matcher now accepts repeated
terminal punctuation while still rejecting in-game labels such as
`Victory Points!!!`, and the clean rerun above exercises the repair.

These games validate end-to-end progress, fail-closed execution, terminal
handling, and interactive service latency. One or two wins cannot estimate the
bot's win rate against Hard bots, strong humans, or any broader population.

The final release-screenshot pass then completed a separate four-player Base
game against three Easy bots at rank 1 on July 30, 2026. It finished in 8:30
and 87 turns (520.9 seconds wall time) with 285 validated clicks and no harness
failure. The run captured real 1280 by 800 packaged views for settings,
table-card beliefs, an incoming-trade counteroffer, and an exact five-card
discard before the first discard click. Its deliberate Autopilot pauses make
its trace-lifecycle counts unsuitable for latency comparison. This single Easy
game is release-artifact and deadlock evidence only, not a win-rate result.

## Historical live Chromium integration smoke

A headed Chromium run on July 30, 2026, before the current live Deep MaxN
budget was established, completed one normal four-player Base game against
three Colonist Hard bots:

- authoritative runtime: packaged Strategist WASM;
- terminal evidence: the Colonist log named the local player as the winner and
  the final score parser reported 10 points;
- duration: 12:30 of game time and 111 turns;
- result: rank 1;
- validated autonomous clicks: 277;
- decisions executed before a deep result: 0;
- among requests logged because total latency exceeded one second, service
  time was 2.822 s median, 3.099 s p95, and 3.243 s maximum;
- end-to-end trace latency, including queueing: 5.544 s p95 and 10.641 s
  maximum, below the 12-second client safety limit.

Ten attempted interactions failed closed and replanned: six trade responses
were not committed before Colonist changed the offer, and four state
signature/legal-target checks changed before execution. None produced a stale
continuation or stopped the game.

This is historical end-to-end reliability evidence, not a current latency
benchmark or a win-rate estimate. One game cannot establish strength against
Hard bots or humans. During this validation, the harness also exposed and
fixed two real integration bugs: setup-road pondering used the wrong actor's
settlement anchor, and a substring check mistook the persistent
`Victory Points` heading for a terminal game state. Both now have focused
regressions.

## Historical legacy-search results

The tables below were recorded on older search/evaluation revisions. They
selected an earlier Deep MaxN implementation and are retained only as
historical context. Current `puct` and legacy `strategist` arena names resolve
to experimental belief PUCT, while `deep` resolves to MaxN. The old PUCT rows
still cannot be reproduced merely by substituting those names because the
implementation has changed. These numbers are not evidence for the current
packaged Strategist.

### Historical tuning runs

| Candidate | Opponents | Games | Wins | Win share | Blocked 95% CI |
|---|---|---:|---:|---:|---:|
| Legacy PUCT, 80 iterations | 3 weighted heuristics | 100 | 66 | 66% | 57–76% |
| Legacy PUCT, 80 iterations | 3 paranoid alpha-beta | 100 | 10 | 10% | 5–16% |
| MaxN, depth 3 | 3 weighted heuristics, pre-trade-generator | 100 | 73 | 73% | 63–83% |
| MaxN, depth 3 | 3 weighted trading heuristics | 100 | 79 | 79% | 70–87% |
| MaxN, depth 3 | 3 paranoid alpha-beta | 100 | 26 | 26% | 19–33% |
| Paranoid alpha-beta, depth 3 | 3 MaxN | 100 | 26 | 26% | 20–32% |

### Historical evaluation previously treated as held out

| Candidate | Opponents | Games | Wins | Win share | Blocked 95% CI | Cutoffs |
|---|---|---:|---:|---:|---:|---:|
| MaxN, depth 3 | 3 weighted trading heuristics | 1,000 | 778 | 77.8% | 75.1–80.5% | 0 |
| MaxN, depth 3 | 3 paranoid alpha-beta agents | 1,000 | 257 | 25.7% | 23.8–27.8% | 0 |
| MaxN, depth 3 (3 players) | 2 weighted trading heuristics | 600 | 508 | 84.7% | 81.8–87.5% | 0 |
| MaxN, depth 3 (3 players) | 2 paranoid alpha-beta agents | 600 | 201 | 33.5% | 31.0–36.2% | 0 |

The old report recorded 42.4 seconds for the four-player weighted run and
98.3 seconds for the alpha-beta run on a Ryzen 7 3700X. Those figures describe
different code and are not current Strategist throughput expectations. The
historical tables also predate the current uniform weighted-belief comparison
mode.

## Strength evidence still required

The packaged weighted-belief Deep MaxN Strategist has no statistically useful
held-out strength result yet. A promotion-quality comparison should freeze the
source revision and untouched seed list, use hundreds of matched blocks in
both three- and four-player games, rotate every seat, preserve a common
information mode, and report terminal games, cutoffs, confidence intervals,
rank, victory points, behavioral metrics, and latency. Native results must
remain separate from live Colonist reliability and from claims about human
opponents. Experimental belief-PUCT smoke runs cannot satisfy this requirement
or be relabelled as live Strategist evidence.
