# Engine stabilization evidence — 2026-09-07

These artifacts record the v13 candidate's correctness and routing gates in `../../ENGINE_STABILIZATION_ACCEPTANCE_2026-09-07.md`. They do not certify the current v14 implementation. They are not a browser win-rate estimate and do not establish playing-strength improvement. The recorded arena smoke failed the 2× retention threshold: 1.370× for 3P and 1.272× for 4P.

## Artifacts

- `exact-gpu-evaluator-parity.json` — current-reference handcrafted evaluator parity on 69 deterministic states spanning 2P/3P/4P, Mref/fair-related state coverage and supported strategic fixtures.
- `exact-gpu-search-parity.json` — 15 fixed-work CPU/explicit CUDA MaxN cases covering 2P/3P/4P, development cards, road races, player-trade settings, Mref and 15-point targets.
- `native-host-exact-parity.jsonl` — production-shaped serialized request comparison between packaged CPU/WASM and protocol-7 native `analyze-exact`, plus Mref/invalid-evidence/cancellation integration checks.
- `native-exact-takeover-smoke.jsonl` — one frozen arena takeover snapshot using protocol-7 exact MaxN for the first root, followed by random continuation so the artifact verifies the decision route without pretending to be a strength campaign.
- `exact-gpu-arena-smoke.json` — matched 3P/4P exact CPU/CUDA end-to-end smoke used for the pre-existing ≥2× retention gate.

Checkpoint files created by the matched arena smoke live under `exact-gpu-arena-smoke.checkpoints/` and are benchmark support material rather than a separate acceptance surface.

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

Correctness parity is a prerequisite for treating exact CUDA as an accelerator for `deep-maxn-v13`. `gpu-root-rollout` is a different algorithm and is not represented as a parity accelerator by these artifacts. Production stays on CPU/WASM unless the exact backend also satisfies the performance and packaged-execution gates.
