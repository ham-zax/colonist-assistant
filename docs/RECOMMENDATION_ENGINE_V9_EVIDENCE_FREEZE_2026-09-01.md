# Recommendation Engine v9 Evidence Freeze

Date: 2026-09-01

This report records the pre-correctness `deep-maxn-v9` evidence freeze required by `RECOMMENDATION_ENGINE_BEFORE_AFTER_EVIDENCE_PLAN_2026-09-01.md`. No production recommendation-semantic source was changed before this freeze completed.

## Build and live search identity

- Source SHA: `862e3110069524fbcda654b4f702ac4726774c0f`
- Engine revision: `deep-maxn-v9`
- Build dirty state: `true` because the measurement/snapshot tooling was uncommitted while the evidence was captured
- Information mode: `weighted-belief`
- MaxN depth: `4`
- Branch cap: `8`
- Node budget: `4,000`
- Time budget: `350 ms`
- Arena belief particles: `24`
- Rust strategic-particle limit: `12`
- Tracker world limit: `4,096`
- TypeScript source-world cap: `96`
- Interactive WASM particle cap: `24`
- Tactical search: depth `14`, nodes `900`

The arena snapshot/replay smoke restored a saved PreRoll position to the identical state hash and restored the recorded chance/policy RNG states before the full corpus was frozen.

## Whole-game v9 baseline

All completed games validated and none cut off.

| Players | Baseline | MaxN wins | Win share | Fair share | Blocked 95% CI | Mean rank | Mean VP | Cutoffs |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 3 | weighted | 50 / 201 | 24.9% | 33.3% | 19.4%–30.3% | 2.276 | 5.985 | 0 |
| 3 | AlphaBeta | 64 / 201 | 31.8% | 33.3% | 27.9%–35.8% | 2.027 | 6.876 | 0 |
| 4 | weighted | 51 / 200 | 25.5% | 25.0% | 19.5%–31.5% | 2.713 | 6.325 | 0 |
| 4 | AlphaBeta | 52 / 200 | 26.0% | 25.0% | 20.5%–31.0% | 2.505 | 6.430 | 0 |

The first three rows are the original completed `benchmark-local.mjs` legs. The original fourth child was terminated by `SIGTERM` at 58/200 before the wrapper could emit its top-level matrix report. The 4-player AlphaBeta row is therefore a replacement **pre-fix** run from the same SHA, revision, seed `95100001`, and production profile; it is not represented as a continuation of the interrupted prefix.

The replacement 4-player AlphaBeta run also retained its final detailed candidate metrics: mean search nodes `2620.7`, mean depth `3.747`, posterior particles `24.0`, strategic particles `11.616`, search-deadline share `42.9%`, mean root actions `5.841`, and mean decision latency `254.4 ms`.

The original wrapper held the first three legs' detailed aggregate search/trade/build metrics only in memory. They were lost when that wrapper terminated. Those three legs were not rerun merely to backfill the missing aggregates; their original checkpoint/challenge streams and outcome statistics remain the baseline evidence.

## Frozen takeover corpus

The immutable corpus contains 150 stable PreRoll positions selected before corrected-engine outcomes existed:

- 50 three-player states, target-seat counts `17/17/16`
- 100 four-player states, target-seat counts `25/25/25/25`
- source streams: completed MaxN-versus-weighted games for each player count
- all states satisfy the fixed turn/behind/evaluator/uniqueness criteria
- every replay outcome was verified against the corpus by snapshot ID, state hash, order, source SHA, and engine revision

The assembled control and v9 outcome files each contain exactly 150 unique records with zero cutoffs.

| Arm | Target wins | Win share | Mean rank | Mean final VP | Mean VP gained | Mean target latency | Mean deadline share |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Original control policy | 40 / 150 | 26.7% | 2.397 | 6.813 | 4.220 | 71.1 ms | 12.6% |
| v9 MaxN takeover | 49 / 150 | 32.7% | 2.210 | 7.240 | 4.653 | 258.1 ms | 45.9% |

Across all 150 states, v9-wins/control-loses occurred 26 times and control-wins/v9-loses occurred 17 times, a raw net of +9 win flips. Across the 109 states whose source target was not already MaxN, the counts were 24 versus 14, raw net +10; mean rank changed by `-0.257` and mean final VP by `+0.523` in v9's favor.

These raw paired results are **not** a noise-free causal estimate. The runtime calibration below materially limits that interpretation.

## Same-policy runtime-noise calibration

Forty-one frozen target seats already used MaxN in the source game. On those states, the control and v9 arms use the same target engine, restored state, chance RNG, policy RNGs, and production search limits.

Despite identical target semantics:

- 21 / 41 pairs diverged in final game state
- 5 / 41 pairs disagreed on target win/loss
- v9-wins/control-loses: 2
- control-wins/v9-loses: 3
- net win flips: -1

The differing runs also expanded different search node counts and crossed different 350 ms deadline boundaries. Production wall-clock search therefore has a measurable replay-noise floor even under common random-number restoration.

Use the takeover layer as production-runtime evidence, and calibrate later v9/v10 rescue/regression counts against this same-policy disagreement rate. Do not use raw takeover win flips as correctness proof. Finding-specific deterministic reproductions remain the correctness authority.

## Persisted artifacts

The generated evidence remains under the repository's ignored `benchmark-results/` directory:

- `benchmark-results/recommendation-v9-e0-862e311.checkpoints/`
- `benchmark-results/recommendation-v9-e0-862e311-challenges/`
- `benchmark-results/recommendation-v9-e0-862e311-frozen-takeover-corpus.jsonl`
- `benchmark-results/recommendation-v9-e0-862e311-frozen-takeover-corpus.manifest.json`
- `benchmark-results/recommendation-v9-e0-862e311-takeover-control.jsonl`
- `benchmark-results/recommendation-v9-e0-862e311-takeover-v9.jsonl`
- `benchmark-results/recommendation-v9-e0-862e311-summary.json`
- `benchmark-results/recommendation-v9-e0-862e311-summary.md`

The detailed implementation choices and compromises made while creating this evidence are recorded in `RECOMMENDATION_ENGINE_IMPLEMENTATION_DECISIONS_2026-09-01.md`.

## Freeze status

E0/E1 pre-fix evidence is frozen. The next allowed production-semantic change is Task 1: remove posterior-changing resource resampling and prove the F1 regression before continuing through the implementation plan.
