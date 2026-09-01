# GPU search acceleration

The current CUDA benchmark accelerates the value/policy network only. It does not run `GameState::legal_actions`, state transitions, belief MaxN, or the handcrafted `evaluate()` function on the GPU.

Use:

```bash
npm run benchmark:gpu:parity -- --input <expert-corpus.jsonl>
```

to run one fixed value/policy corpus on CPU and CUDA with identical seeds, splits, model shape, optimizer settings, and batches. The command fails when any reported quality metric differs by more than `1e-5`. Throughput is reported separately.

## Exact GPU acceleration

Tasks 4-12 are integrated at `7c91f69`. Treat that search behavior as the CPU semantic reference for the exact GPU path.

Use:

```bash
npm run benchmark:gpu:exact-feasibility -- --states 512 --repeats 32 --no-player-trades
```

to measure the corrected handcrafted evaluator against the non-evaluator node primitive on deterministic 3-player and 4-player states. The benchmark also exposes a benchmark-only breakdown of route-map construction, expansion-arrival preparation, and the remaining utility calculation.

The measured no-player-trades result on the RTX 3070 Ti host was:

| Format | `evaluate()` | Evaluator share of node primitive | Full-evaluator infinite-speed ceiling | Evaluator speedup needed for 2x node |
| --- | ---: | ---: | ---: | ---: |
| 3P | 17.71 us/state | 96.1% | 26.0x | 2.08x |
| 4P | 28.06 us/state | 97.4% | 38.7x | 2.05x |

This makes a full exact evaluator GPU backend worthwhile in principle. The non-evaluator work (`legal_actions`, clone/apply, and hash in the benchmark primitive) is small enough that Amdahl's law does not block a useful speedup.

A narrow route/race-only port is not enough. Route-map plus arrival-score preparation accounted for about 25.4% of 3P evaluator time and 16.9% of 4P evaluator time in the canonical run. Even at infinite speed, offloading only that subset caps the node primitive at about 1.32x for 3P and 1.20x for 4P. The exact path therefore needs to port a substantial part of `strategic_utility_with_routes_and_knowledge()` as well as its route/race inputs.

The implementation contract is:

1. Preserve `GameState`, legal actions, MaxN semantics, priors, node budgets, information-set behavior, authority, and backed-up values.
2. Keep the CPU evaluator as the semantic oracle.
3. Batch exact leaf evaluation across independent roots, belief particles, and arena games.
4. Require CPU/GPU evaluator parity on a fixed state corpus before using GPU values in search.
5. Require chosen-action and terminal-outcome parity on deterministic 3P/4P benchmark seeds.
6. Keep the backend only if the end-to-end 3P and 4P arena benchmark is at least 2x faster.

The current handcrafted `evaluate()` is not a dense kernel. It includes route maps, expansion races, discard optimization, trophy outlooks, development-card value, ports, production, hand composition, build tempo, and other state-dependent terms. Implementing Option 1 is therefore a full evaluator backend, not a small CUDA wrapper.

For hardware headroom only, the existing width-512 neural parity benchmark reproduced CPU quality within about `1.2e-7` while CUDA value inference was about 40.5x faster on the 3P corpus and 33.1x faster on the 4P corpus. Those numbers do not prove an exact evaluator speedup, but they show that the GPU has substantially more raw batched throughput than the roughly 2.1x evaluator acceleration needed to clear the exact-path 2x node gate.

## Neural GPU search

After the corrected engine demonstrates a stable strength improvement, evaluate a promoted neural value/policy search. The goal is to reduce node count by giving MaxN better priors and leaf values, while preserving or improving win rate.

For large batched arena/self-play workloads, a realistic engineering target is **10-50x end-to-end throughput** compared with the current CPU benchmark. **20-100x** is possible only after aggressive batching/vectorization and materially fewer searched nodes. Unlike the exact path, this changes search behavior and requires fresh game-strength evidence.

CPU/CUDA numerical parity from `benchmark-gpu-zoom.py` proves only that the same neural calculation is reproduced on both devices. It does not prove that a neural search preserves the old handcrafted engine's decisions.
