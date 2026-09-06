# Milestone 0/1 empirical screen — 2026-09-06

## Purpose

This is an early matched-seed gameplay screen, not promotion evidence.

It answers two separate questions:

1. Does the Milestone-0 controlled-player continuation repair show a directional gameplay-strength signal against the pre-repair engine?
2. Does Milestone 1 remain shadow-only while exposing reachability/strategy diagnostics through the packaged engine contract?

Milestone 1 does not alter root admission, evaluator utility, or final action authority, so no win-rate gain is attributed to Milestone 1. Its acceptance evidence is diagnostic correctness plus authority preservation. The arena A/B below measures the Milestone-0 continuation change; the current binary also contains Milestone-1 shadow diagnostics, but all search time budgets are disabled and the shadow work runs after the baseline search winner is known.

## Revisions

- Pre-Milestone-0 baseline: `d80e4b40601411b6cb848471978dbe1799c2045a`
- Milestone-0 checkpoint: `c21e4ae36b274e1dc1cea5841cc5426a7f1a6739`
- Current arena binary: built from the `c21e4ae` working tree with uncommitted Milestone-1 shadow diagnostics (`buildDirty=true`)

A detached worktree at the pre-M0 revision was compiled into a separate target directory so both revisions could be run with identical arena arguments.

## Arena protocol

Shared settings:

- simulator: `colonist-native`
- candidate: `maxn`
- baseline: `weighted`
- matched blocks: 4 per player-count stratum
- seat rotation: arena default matched-block behavior
- validation: enabled
- player trades: disabled
- maritime trades: enabled
- MaxN depth: 3
- MaxN branch cap: 12
- MaxN node budget: 2,000
- belief particles: 4
- strategic particles: 4
- max turns: 300
- opening nodes: 2,000
- opening time budget: 0
- trade-response nodes: 500
- trade-response time budget: 0
- MaxN time budget: 0
- threads: 2 per revision while the two revisions ran side-by-side

All decision-affecting search budgets in this screen are node-based rather than wall-clock based. The profile is deliberately smaller than the live/default quality budget so 2-, 3-, and 4-player matched screening is practical. It is a diagnostic strength screen, not a replacement for the preregistered held-out arena required for promotion.

Seeds:

- 2 players: `9100001`
- 3 players: `9101001`
- 4 players: `9102001`

## Results

| Players | Games / revision | Pre-M0 MaxN wins | Current MaxN wins | Pre-M0 win share | Current win share | Pre-M0 mean MaxN rank | Current mean MaxN rank | Pre-M0 mean MaxN VP | Current mean MaxN VP | Pre-M0 mean search depth | Current mean search depth |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | 8 | 3 | 5 | 0.375 | 0.625 | 1.625 | 1.375 | 6.875 | 8.500 | 2.012 | 2.239 |
| 3 | 12 | 2 | 6 | 0.167 | 0.500 | 2.250 | 1.708 | 7.000 | 8.167 | 1.769 | 1.908 |
| 4 | 16 | 4 | 6 | 0.250 | 0.375 | 2.469 | 2.094 | 7.000 | 7.000 | 1.736 | 1.928 |
| **Descriptive total** | **36** | **9** | **17** | **0.250** | **0.472** | — | — | — | — | — | — |

Per-run blocked 95% intervals reported by the arena:

- 2p pre-M0: `[0.000, 0.750]`; current: `[0.250, 1.000]`
- 3p pre-M0: `[0.000, 0.333]`; current: `[0.167, 0.667]`
- 4p pre-M0: `[0.250, 0.250]`; current: `[0.125, 0.625]`

There were zero cutoffs in all six final runs.

The descriptive aggregate fair-share expectation across these three equally-sized block strata is 12 wins out of 36 games: four expected wins in each stratum after seat rotation. Current MaxN won 17/36; pre-M0 MaxN won 9/36. This aggregate is descriptive only; no pooled confidence interval is claimed because the experimental unit is the matched block and player-count strata have different fair shares.

## Interpretation

The direction is favorable in all three player-count strata:

- more MaxN wins in 2p, 3p, and 4p;
- better mean MaxN rank in every stratum;
- higher mean MaxN VP in 2p and 3p, tied in 4p;
- deeper mean completed search under the same node budget in every stratum.

The deeper-search result is consistent with the intended Milestone-0 mechanism: future controlled-player decision nodes no longer spend their branch budget on an opponent-style action mixture. It is supporting causal evidence, not proof that every additional depth unit is beneficial.

The sample is too small for promotion. Four matched blocks per stratum produce wide blocked intervals, and the current/pre-M0 intervals overlap. The correct conclusion is **positive pilot evidence with no observed player-count regression**, not “Milestone 0 is proven stronger.”

## Milestone-1 evidence

Milestone 1 is shadow-only. Focused verification completed before this arena screen:

- `cargo check -p colonist-catan-search`
- `cargo check -p colonist-catan-search --features cuda-exact`
- `cargo check -p colonist-catan-wasm`
- `cargo check -p colonist-catan-wasm --features native-gpu`
- `npm run check`
- 3 focused reachability unit tests passed
- 4 focused strategy-shadow unit tests passed
- packaged WASM built successfully
- `tests/deep-search-adapter.test.ts`: 26/26 passed, including the strategy-shadow provenance boundary

The shadow contract records player count/opponent count, response windows, optimistic reachability, bounded strategy proposals, baseline rank/retention, and evidence-backed coverage/valuation/horizon classification. It does not change candidate admission or final authority.

## Human gameplay replay

Real gameplay logs are useful as an additional evaluation lane, but they should not replace matched arena self-play.

The repository already has `scripts/replay-decisions.mjs` and `scripts/replay-engine.ts`. A trace containing `replayState`, `replayBoard`, and `rootPlayer` can be reconstructed through the current TypeScript-to-WASM request path and rerun at multiple particle/search budgets. The replay engine already computes action-family agreement, reference regret, root admission/rank evidence, and optional seed stability.

This is the right use of real human games: measure counterfactual recommendations at recorded decision states and discover realistic failure cases. It cannot prove the downstream result of an action the human did not actually take, because after the first counterfactual move the historical trajectory is off-policy. Whole-game causal strength should therefore remain an arena/self-play question; human logs are a realism and decision-regret corpus.

## Next evidence gate

Before Milestone 2 changes action coverage, use this pilot to define a larger preregistered held-out arena with enough matched blocks to narrow the player-count-stratified intervals. Add real decision-trace replay when representative logs are available. Keep those two evidence categories separate.
