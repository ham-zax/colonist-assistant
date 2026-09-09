# Engine stabilization evidence — 2026-09-07

These artifacts support the gates in `../../ENGINE_STABILIZATION_ACCEPTANCE_2026-09-07.md`. The focused evaluator/search/native-host/takeover artifacts record the v13 candidate and do not by themselves certify v14. A separate clean revision-matched v14 exact arena smoke from `ff7b9b7` is retained here for the end-to-end parity/performance gate. None of these artifacts is a browser win-rate estimate or evidence of playing-strength improvement. The historical v13 smoke measured 1.370× for 3P and 1.272× for 4P; the clean v14 smoke measured 1.455× and 1.375× respectively. Hamza clarified that soundness and robustness govern acceptance: the former 2× threshold is superseded, and these timings are informational.

## Artifacts

- `exact-gpu-evaluator-parity.json` — current-reference handcrafted evaluator parity on 69 deterministic states spanning 2P/3P/4P, Mref/fair-related state coverage and supported strategic fixtures.
- `exact-gpu-search-parity.json` — 15 fixed-work CPU/explicit CUDA MaxN cases covering 2P/3P/4P, development cards, road races, player-trade settings, Mref and 15-point targets.
- `native-host-exact-parity.jsonl` — production-shaped serialized request comparison between packaged CPU/WASM and protocol-7 native `analyze-exact`, plus Mref/invalid-evidence/cancellation integration checks.
- `native-exact-takeover-smoke.jsonl` — one frozen arena takeover snapshot using protocol-7 exact MaxN for the first root, followed by random continuation so the artifact verifies the decision route without pretending to be a strength campaign.
- `exact-gpu-arena-smoke.json` — historical v13 matched 3P/4P exact CPU/CUDA end-to-end smoke originally used for the now-superseded ≥2× retention gate.
- `exact-gpu-arena-smoke-v14-ff7b9b7.json` — clean revision-matched v14 smoke using the same gate protocol; parity passed and elapsed speedups were 1.455× (3P) and 1.375× (4P), so promotion still fails.

Checkpoint files created by the matched arena smokes live under `exact-gpu-arena-smoke.checkpoints/` and `exact-gpu-arena-smoke-v14-ff7b9b7.checkpoints/`; they are benchmark support material rather than a separate acceptance surface. The v14 report preserves the absolute `/tmp` checkpoint paths from the original execution; the sibling retained checkpoint directory contains the exact copied checkpoint bytes.

## Reproduction commands

Evaluator and fixed-work search parity:

```bash
cd engine
cargo run --release -p colonist-catan-arena --features cuda-exact --bin exact-gpu-parity
cargo run --release -p colonist-catan-arena --features cuda-exact --bin exact-gpu-search-parity
```

Production-shaped native exact parity and cancellation:

```bash
node scripts/verify-mref-native.mjs
```

Matched exact-backend retention smoke:

```bash
npm run benchmark:gpu:exact -- --smoke \
  --output docs/benchmarks/engine-stabilization-2026-09-07/exact-gpu-arena-smoke.json
```

The frozen takeover artifact uses the first snapshot from `benchmark-results/recommendation-v9-e0-862e311-frozen-takeover-corpus.jsonl`, protocol-7 `analyze-exact`, and random continuation after the initial exact root. Schema-1 snapshots default player trades to enabled because their embedded source revision predates the no-player-trades option.

## Interpretation

Correctness parity is a prerequisite for treating exact CUDA as an accelerator for the Deep MaxN policy. `gpu-root-rollout` is a different algorithm and is not represented as a parity accelerator by these artifacts. The clean v14 arena smoke confirms matched game outcomes on its recorded revision. No minimum-speedup gate applies. Production stays on CPU/WASM pending the remaining robustness and packaged-execution checks.
