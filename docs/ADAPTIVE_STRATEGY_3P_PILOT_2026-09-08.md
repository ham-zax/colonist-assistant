# Three-player adaptive strategy pilot

## Frozen protocol

Run `cargo build --release --manifest-path engine/Cargo.toml -p colonist-catan-arena`, then `node scripts/run-adaptive-strategy-pilot.mjs`.

This is a diagnostic pilot, not a promotion or win-rate estimation campaign. Each of four configurations plays one matched block: three seats on the same board/chance seeds, for 12 games total. Compare baseline MaxN and explicit `adaptive-candidate-admission-v1`, each against two weighted agents, with domestic trades enabled and globally disabled. Seed: 2026090901. CPU evaluator; observation-consistent synthetic arena beliefs; eight belief/strategic particles; depth cap 3; root cap 12; 1,500-node search waves; 12,000 opening nodes; 1,500 trade-response nodes; all cooperative time limits disabled; 600-turn game cap; transition validation enabled.

The arena's global no-trades mode differs from the extension's own-player-only restriction. Neither configuration reproduces live event-conditioned beliefs or establishes a Colonist human win rate. One matched block produces an uninformative block-bootstrap interval; do not treat the arena's numeric interval as statistical confidence. The dirty build includes the current opening repair but these are three-player games, outside the new two-player opponent-portfolio branch.

Raw commands, JSON summaries, checkpoint records and stderr are retained locally under `benchmark-results/adaptive-3p-pilot/`, which is intentionally ignored by Git. The runner stops on a failing arena process. Use a different output-directory argument to preserve a previous run.

## Implementation decision rule

1. Use candidate-only strategy metrics to check proposal admission, completed comparison and admitted-challenger wins. The `strategyPilot.baseline` field aggregates two weighted opponents; it is not the paired baseline MaxN run.
2. If admission is rare or admitted proposals never win, do not infer that merely increasing proposal count helps. Reproduce a resource-saving decision and separate omitted alternatives from short search, future-self policy and valuation failures.
3. Start with a focused scenario where spending resources now competes with retaining them for a legal settlement, city or development card. Include a counterexample where spending now is correct. Use the shared economy and search owners, and keep EndTurn in the comparison.
4. Compare completed future build opportunities under chance-consistent forecasts, bank/port conversion, opponent occupation and discard exposure. No unconditional resource reserve or assumed future player cooperation.
5. Implement only the demonstrated owner: candidate admission/reconsideration for missing roots; continuation search for missed follow-ups; economic forecasting/evaluation for a demonstrated ranking error. Require the same-policy exact-CUDA check for any shared search/evaluator repair.
6. After a causal regression passes, run multiple fresh matched blocks and both trade settings with explicit confidence intervals and game cutoffs before claiming improved strength. Keep the pilot seed out of subsequent held-out evaluation.

Detailed roadmap: [adaptive strategy design](ADAPTIVE_STRATEGY_LAYER_DESIGN_2026-09-06.md). No new production strategy behavior is introduced by this pilot.

## Completed results

Build: `deep-maxn-v14`, `2afc708f5e944a4d5b3f562ad92d2e1def82631d`, dirty working tree. All four runs exited successfully and all 12 games reached a terminal outcome; zero game cutoffs and zero search-deadline share. Transition validation remained enabled.

| Domestic trades | Candidate policy | Wins / games | Mean VP | Mean rank | Mean completed search depth | Policy decisions | Challengers admitted / selected |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Enabled | Baseline | 1 / 3 | 9.000 | 1.833 | 1.412 | 0 | 0 / 0 |
| Enabled | Adaptive M2 | 1 / 3 | 9.000 | 1.833 | 1.412 | 500 | 0 / 0 |
| Globally disabled | Baseline | 1 / 3 | 7.333 | 2.000 | 1.780 | 0 | 0 / 0 |
| Globally disabled | Adaptive M2 | 1 / 3 | 7.333 | 2.000 | 1.780 | 243 | 0 / 0 |

Candidate decisions were 512 per trades-enabled arm and 255 per trades-disabled arm. Mean search nodes were 1,495.301 and 1,496.737 respectively, identical between policies within each trade setting. Candidate mean decision latency was 583.320/555.505 ms (baseline/adaptive, trades enabled) and 200.525/192.228 ms (trades disabled). Sequential timing differences are informational and do not establish a speed improvement. Whole-arm elapsed time was 124.094, 119.425, 19.215 and 18.507 seconds respectively.

The adaptive policy was exercised on 743 decisions, with zero challenger candidates, admissions, displacements or admitted-challenger wins. Thus this pilot found no additional root coverage or playing-strength gain from M2. Equal aggregate outcomes are not a proof of identical actions at every step. The sample is one independent board block per trade setting and cannot establish a reliable 33.3% population win rate.

The focused strategy unit suite also passed: 15 tests, zero failures (`cargo test --release --manifest-path engine/Cargo.toml -p colonist-catan-search --lib strategy`). This establishes the tested admission/evidence rules, not long-term planning quality.

## Immediate next implementation task

Start with the resource-saving diagnostic from the main design document, rather than implementing all remaining milestones. Its acceptance artifact is a reproducible canonical save-versus-spend decision plus a discriminating reference comparison. Determine whether EndTurn and the useful future build are already represented. If they are, M3 candidate re-entry is not the demonstrated owner: inspect completed horizon, controlled-player follow-up and economic valuation. Change production only after that comparison identifies a concrete failure, and preserve scenarios where immediate spending wins. The current aggregate pilot cannot by itself assign that cause.
