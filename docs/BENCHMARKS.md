# Benchmarks

Last updated: July 28, 2026.

## Method

- Standard four-player base-game simulator.
- Candidate occupies every seat once on each board block.
- Every seat rotation in a block shares the same board and chance seed.
- Blocks use independent deterministic seeds.
- Invariants are checked after every state transition.
- A turn cutoff is reported separately rather than silently counted as a
  normal win.
- Confidence intervals are 95% block bootstraps, preserving the matched-board
  grouping.
- Both agents can use bounded player-to-player negotiation.
- Arena agents receive the complete simulator state. The live extension instead
  aggregates legal hidden-card particles, so these results measure search and
  evaluation quality—not end-to-end card-inference accuracy.

## Tuning results

These runs selected the production engine and are not held-out estimates.

| Candidate | Opponents | Games | Wins | Win share | Blocked 95% CI |
|---|---|---:|---:|---:|---:|
| PUCT, 80 iterations | 3 weighted heuristics | 100 | 66 | 66% | 57–76% |
| PUCT, 80 iterations | 3 paranoid alpha-beta | 100 | 10 | 10% | 5–16% |
| MaxN, depth 3 | 3 weighted heuristics, pre-trade-generator | 100 | 73 | 73% | 63–83% |
| MaxN, depth 3 | 3 weighted trading heuristics | 100 | 79 | 79% | 70–87% |
| MaxN, depth 3 | 3 paranoid alpha-beta | 100 | 26 | 26% | 19–33% |
| Paranoid alpha-beta, depth 3 | 3 MaxN | 100 | 26 | 26% | 20–32% |

The symmetric 26% cross-results are consistent with MaxN and paranoid
alpha-beta being close peers in this simulator. These results selected MaxN
for the pre-v3 release; they are retained as the locked historical baseline.
Deep MaxN remains the production default. Belief-aware PUCT stays available as
an experimental engine until a held-out promotion run beats MaxN or AlphaBeta
without sacrificing the interactive latency budget.

## Locked held-out evaluation

| Candidate | Opponents | Games | Wins | Win share | Blocked 95% CI | Cutoffs |
|---|---|---:|---:|---:|---:|---:|
| MaxN, depth 3 | 3 weighted trading heuristics | 1,000 | 778 | 77.8% | 75.1–80.5% | 0 |
| MaxN, depth 3 | 3 paranoid alpha-beta agents | 1,000 | 257 | 25.7% | 23.8–27.8% | 0 |
| MaxN, depth 3 (3 players) | 2 weighted trading heuristics | 600 | 508 | 84.7% | 81.8–87.5% | 0 |
| MaxN, depth 3 (3 players) | 2 paranoid alpha-beta agents | 600 | 201 | 33.5% | 31.0–36.2% | 0 |

The weighted baseline is not a skilled-human proxy. This result shows a large
and reproducible improvement over the project’s heuristic policy inside the
same rules engine. It does **not** imply a 77.8% win rate on Colonist or against
human players.

The alpha-beta result is statistically consistent with fair share in a
four-player field. It is the more demanding comparison: unlike the weighted
baseline, every opponent performs depth-3 exact-chance search on every
decision.

## Reproduction

```bash
cd engine
cargo run --release -p colonist-catan-arena -- \
  --players 4 --blocks 250 --threads 16 --validate --quiet \
  --candidate maxn --baseline weighted --seed 9100001
```

On the benchmark machine (Ryzen 7 3700X, 16 logical CPUs), the weighted
held-out run completed in 42.4 seconds at 23.6 games/second; the alpha-beta run
completed in 98.3 seconds at 10.2 games/second.
