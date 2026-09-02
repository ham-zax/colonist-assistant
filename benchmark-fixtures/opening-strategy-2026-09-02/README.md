# Opening strategy baseline — 2026-09-02

This fixture freezes the CUDA-assisted whole-game cohort used to evaluate the opening-port repair and generate hypotheses for the future ETW/Best-Build-Plan work.

## What this run is

- 4-player native arena.
- 50 matched blocks, 4 seat rotations per block: 200 scheduled games.
- Candidate: `maxn`.
- Baseline seats: `weighted`.
- Seed: `20261000`.
- CUDA exact evaluator on an NVIDIA GeForce RTX 3070 Ti.
- Fast discovery search: depth 1, branch cap 6, 128 MaxN nodes, one belief particle, one strategic particle.
- Opening budget: 1,000 nodes.
- Maximum 160 turns.
- Player-to-player trading remained enabled.
- 186 games reached a terminal result; 14 hit the turn cutoff.

The temporary arena used for this run removed the normal all-MaxN CUDA CLI restriction because `choose_action` already dispatches CUDA only for MaxN decisions. It also added `policyProfiles`, `developmentBought`, and `maritimeTrades` to trajectory records. Production arena source was not modified for those diagnostics.

The arena summary reports `buildGitSha: "unknown"` because the instrumented arena was compiled as a standalone temporary crate. Its `colonist-catan-core` and `colonist-catan-search` dependencies pointed at the active repository. The opening-port repair from `engine/crates/catan-search/src/opening.rs` was therefore part of this run.

## Frozen files

- `summary.json` — final arena summary and CUDA/search timing.
- `checkpoints.jsonl.gz` — one cumulative checkpoint per completed game, including `lastGame` outcomes.
- `trajectories.jsonl.gz` — 21,950 per-turn state snapshots.
- `analysis.txt` — output from `scripts/analyze-opening-strategy.py` at turn 48.

## Reproduce

Run the benchmark without patching production source:

```bash
node scripts/run-opening-strategy-benchmark.mjs \
  --profile fast \
  --blocks 50 \
  --seed 20261000 \
  --threads 4 \
  --max-turns 160 \
  --output benchmark-results/opening-strategy-replay
```

Analyze this frozen corpus directly:

```bash
python3 scripts/analyze-opening-strategy.py \
  benchmark-fixtures/opening-strategy-2026-09-02/checkpoints.jsonl.gz \
  benchmark-fixtures/opening-strategy-2026-09-02/trajectories.jsonl.gz
```

## Findings to carry forward

This is a hypothesis-generating baseline, not final strategy-strength evidence. The search is deliberately shallow and the three weighted opponents are engineering baselines, not skilled-human proxies.

Among the 185 terminal candidate games that were still alive beyond turn 48:

- winners began with 15.75 combined setup production pips on average versus 13.83 for non-winners;
- candidates below 12 setup pips won 7.9%, while candidates at 18+ pips won 38.9%;
- candidates with at least one city by turn 48 won 45.9%;
- candidates with an extra settlement but no city won 37.7%;
- candidates with a city and at least one development-card purchase won 50.0%;
- candidates that bought at least two development cards while adding neither a city nor a settlement won 9.1%;
- candidates making at least three maritime trades while adding neither a city nor a settlement won 7.1%.

These correlations support using production as a strong opening signal and treating cities, expansion, development cards, and ports as parts of a build plan rather than independent static bonuses. They do not establish causal strategy weights.

## Intended next use

Keep this corpus fixed while the GPU-resident roadmap lands. Once GPU-native search and the batched whole-game GPU arena are available, rerun the same seed/config first as a before/after bridge. Use the later massive campaign infrastructure for the real strategy search rather than tuning this shallow hybrid benchmark into a permanent policy.
