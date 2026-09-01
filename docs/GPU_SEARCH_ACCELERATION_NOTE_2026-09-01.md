# GPU search acceleration

The current CUDA benchmark accelerates the value/policy network only. It does not run `GameState::legal_actions`, state transitions, belief MaxN, or the handcrafted `evaluate()` function on the GPU.

Use:

```bash
npm run benchmark:gpu:parity -- --input <expert-corpus.jsonl>
```

to run one fixed value/policy corpus on CPU and CUDA with identical seeds, splits, model shape, optimizer settings, and batches. The command fails when any reported quality metric differs by more than `1e-5`. Throughput is reported separately.

## Exact GPU acceleration

Do not build the CUDA search backend against the pre-Task-12 implementation. Tasks 4-12 change root budgeting, information-set behavior, particle handling, and authority contracts that the accelerator must preserve.

After Tasks 4-12 land:

1. Freeze the corrected CPU search as the semantic reference.
2. Keep `GameState`, legal actions, MaxN semantics, priors, and backed-up values unchanged.
3. Batch exact leaf evaluation across independent roots, belief particles, and arena games.
4. Require CPU/CUDA parity on chosen actions and terminal outcomes, with a narrow floating-point tolerance for evaluator values.
5. Keep the exact GPU backend only if the 3P/4P end-to-end benchmark is at least 2x faster.

A realistic RTX 3070 Ti target is about **2-6x end-to-end**. Higher gains require leaf evaluation to dominate runtime and GPU batches to stay full. This path is a performance migration, not a new strategy.

The current handcrafted `evaluate()` is not a dense kernel. It computes route maps, expansion races, discard optimization, trophy outlooks, development-card value, ports, production, hand composition, and other state-dependent terms. Porting it exactly to CUDA is therefore a substantial implementation and should target the final post-Task-12 search contract rather than the code that Agent A is replacing.

## Neural GPU search

After the corrected engine demonstrates a stable strength improvement, evaluate a promoted neural value/policy search. The goal is to reduce node count by giving MaxN better priors and leaf values, while preserving or improving win rate.

For large batched arena/self-play workloads, a realistic engineering target is **10-50x end-to-end throughput** compared with the current CPU benchmark. **20-100x** is possible only after aggressive batching/vectorization and materially fewer searched nodes. Unlike the exact path, this changes search behavior and requires fresh game-strength evidence.

CPU/CUDA numerical parity from `benchmark-gpu-zoom.py` proves only that the same neural calculation is reproduced on both devices. It does not prove that a neural search preserves the old handcrafted engine's decisions.
