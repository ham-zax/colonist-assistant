# Agent D — Mission D1: D31 Resource-Liquidity Causal Diagnosis

Status: **complete — general evaluator defect established (H1)**

Workspace: `/home/hamza/repo/colonist-d31-liquidity`  
Branch: `agent/d1-d31-liquidity`  
Reviewed base: `1b2a0c2`, containing reviewed A1+A2 integration `70e1461` + `8496200`.

D1 does not implement a production repair. The source changes in this mission are behavior-neutral research diagnostics and focused research tests only.

## Result

D31 does **not** justify a generic development-card penalty. The corrected causal-event-family continuation evidence still has BuyDevelopment winning the actor branch in 3/4 preserved matched streams.

D1 does establish a separate general evaluator defect:

> A pure hand-only transition can increase the strategic-utility contribution of unchanged board production because `dynamic_resource_weights` depends on the hand and `strategic_utility` multiplies those hand-dependent weights by persistent production.

The stronger observable violation on the reconstructed D31 state is:

> With no discard exposure and no board change, removing one grain card increases total `strategic_utility` by 0.104158.

That is not merely a decomposition preference. The player has strictly fewer resources and no compensating state improvement, yet the evaluator says the state is better because unchanged production is reweighted upward.

The production owner is the `weighted_production` term in:

- `engine/crates/catan-search/src/eval.rs` — `strategic_utility_with_routes_and_knowledge`;
- `engine/crates/catan-search/src/cuda/exact_eval.cu` — CUDA `strategic_utility`.

Do **not** replace `dynamic_resource_weights` globally. Dynamic weights remain useful for hand liquidity, candidate city/settlement value, and other genuinely marginal choices. D2 should only separate persistent production value from transient hand scarcity, preserving CPU/CUDA parity.

## Exact D31 reconstruction on reviewed A1+A2

The replay reconstructs decision 31 at observation hash `0406a78fa49053c6`:

- turn 19, actor 0;
- hand, in canonical `[lumber, brick, wool, grain, ore]` order: `[0, 1, 1, 1, 1]`;
- production pips: `[0, 5, 6, 12, 0]`;
- no native lumber;
- no native ore;
- all maritime ratios `4:1`;
- no player trades;
- the ore is the just-imported bottleneck from the recorded D31 history.

Current-base root search:

| Search | BuyDevelopment | EndTurn | Root coverage |
| --- | ---: | ---: | --- |
| depth 1 | 0.087150 | 0.026289 | 2 ranked, 0 pruned |
| depth 3 | 0.072054 | 0.032799 | 2 ranked, 0 pruned |

At depth 3, both roots have legal weight 1.0. Search allocated 19,092 nodes to BuyDevelopment and 19,068 to EndTurn. EndTurn reaches the controlled player's next decision with posterior mass 1.0. The disagreement is therefore not root omission, pruning, or starvation.

Research artifacts:

- `benchmark-results/jev-lab/d1/d31-current-depth3.jsonl`
- `benchmark-results/jev-lab/d1/d31-decomposition-a1a2-v2.jsonl`

These live under an ignored research path and are not release artifacts.

## H1 — persistent production revaluation

### Exact D31 spend-only decomposition

The resource-only state substitutes the hand after paying the development cost while leaving the board, production, trophies, and development inventory unchanged.

| Term | Before | After cost only | Delta |
| --- | ---: | ---: | ---: |
| weighted production | 2.601153 | 3.602191 | **+1.001038** |
| hand | 0.997875 | 0.145318 | -0.852557 |
| build tempo | 0.782000 | 0.581571 | -0.200429 |
| expansion best | 1.494171 | 1.484179 | -0.009991 |
| expansion portfolio | 0.499495 | 0.442407 | -0.057089 |
| closed economy | 0.133776 | 0.048776 | -0.085000 |
| discard penalty | 0 | 0 | 0 |
| total utility | 21.917250 | 21.713226 | **-0.204023** |

Production is exactly unchanged. The +1.001038 weighted-production increase is created only by the hand changing the dynamic weights:

- before: `[1.968624, 0.756000, 0.492750, 0.713700, 2.031480]`;
- after cost: `[1.968624, 0.756000, 0.744600, 1.078480, 3.069792]`.

Without that persistent-production revaluation, the same measured spend-only decomposition is approximately `-1.205061`, not `-0.204023`. This arithmetic removes only the demonstrated +1.001038 production artifact; it is not a simulated D2 result.

### Stronger monotonicity reproducer

On the exact D31 state, remove only the grain card:

- no board change;
- no production change;
- no discard risk before or after;
- total utility changes by **+0.104158**;
- weighted production changes from 2.601153 to 3.345304 (**+0.744152**);
- build tempo falls from 0.782000 to 0.678500;
- hand value falls from 0.997875 to 0.422612.

The evaluator therefore prefers the strictly poorer hand because the persistent production term is revalued upward.

### Synthetic positive case and counterexample

The focused H1 probe uses seed 71 with unchanged board production `[7, 3, 0, 5, 0]`.

Removing hand resources increases the production term from 2.039475 to 2.397110, delta **+0.357635**, with both player trades OFF and ON.

A counterexample disables settlement/city/development targets and removes ore. Production stays unchanged and the production term remains exactly 2.397110. The defect is therefore contextual: hand-dependent target deficits trigger the revaluation; not every spend changes it.

### Semantic cross-checks already in source

Two nearby owners already encode the intended separation:

- `hand_transition_value` freezes `dynamic_resource_weights` across a prospective hand transition and states that board position, trophies, and production cancel.
- `public_strategic_utility_with_routes` already values persistent production with `BASE_RESOURCE_WEIGHTS`, not a sampled/hidden hand-dependent weight.

Those are consistent with a narrow persistent-production fix rather than deleting dynamic resource valuation.

## H2 — imported bottleneck opportunity cost

No separate missing “imported resource penalty” is established.

D31's ore has zero native production and a 4:1 replacement path. Before spending it, the dynamic ore weight is 2.031480. Removing only that ore produces:

- total utility delta: **-0.452051**;
- weighted production: unchanged at 2.601153 because ore production is zero;
- hand: 0.997875 -> 0.308756;
- build tempo: 0.782000 -> 0.616818;
- closed economy: 0.133776 -> 0.048776;
- expansion best: 1.494171 -> 1.841741;
- expansion portfolio: 0.499495 -> 0.639173.

The expansion increase is a plausible marginal reaction: once ore is missing, an ore-producing future site is more useful. It does not revalue production the player already owns. The net result still strongly penalizes losing the imported ore.

Exact deterministic build-ETA diagnostics further show the opportunity cost:

| Build | Before | After full dev cost |
| --- | ---: | ---: |
| road | 9 rolls | 12 rolls |
| settlement | 12 | 15 |
| city | 18 | 24 |
| development | **0** | **15** |

For the ore-only removal, development ETA rises from 0 to 12 rolls and city ETA from 18 to 21.6 rolls.

Thus hand liquidity, maritime conversion, next-build timing, and closed-economy access already price the bottleneck. The full development-cost spend looks artificially cheap mainly because H1 adds +1.001038 of unrelated persistent-production value.

## H3 — development-card valuation

No generic BuyDevelopment penalty is supported.

The next-card belief is observation-safe and uses the public/own exchangeable standard pool:

- Knight 56%;
- Victory Point 20%;
- Road Building 8%;
- Year of Plenty 8%;
- Monopoly 8%.

At D31 the actor has no queued development cards, so queue congestion is not the cause. `development_utility` already applies cross-card queue and newly-bought-card diminishing value. `observed_marginal_development_value` uses the canonical observation-safe deck rather than opponent/deck hidden identities.

The immediate resolved outcomes relative to the pre-buy state are:

| Draw | Weight | Utility delta |
| --- | ---: | ---: |
| Knight | 0.56 | -0.009510 |
| Victory Point | 0.20 | +7.195978 |
| Road Building | 0.08 | +0.534597 |
| Year of Plenty | 0.08 | +0.373219 |
| Monopoly | 0.08 | +0.332872 |

The expected development-card value after paying the cost is 1.737149. The full expected immediate BuyDevelopment delta is +1.533127 because the resource-only spend is currently only -0.204023.

If only the demonstrated H1 production artifact is removed arithmetically, the immediate expected delta remains about **+0.532089**. D31 therefore still does not imply that buying development is intrinsically overvalued.

Largest Army is not driving D31: the largest-army utility term is only 0.036390 and is unchanged by the resource-only spend. The Victory Point branch is the dominant local positive outcome, but the actor is at 2/10 VP, so this is not a closeout artifact.

## H4 — search arbitration

H4 is falsified as a root cause on the reviewed base.

- EndTurn is legal with weight 1.0.
- It is ranked and retained.
- `prunedRootCount = 0`.
- Depth 1 already prefers BuyDevelopment.
- Depth 3 gives both roots essentially equal search work.
- No safety replacement changes the winner.

The previous result that same-turn actions do not consume search depth does not need to be reopened.

## Preserved matched continuation evidence

D1 reused the corrected causal-event-family common-random-number streams already preserved under `/home/hamza/repo/colonist-assistant/benchmark-results/jev-lab/`. They are used to constrain the interpretation of D31, not to prove H1.

| Continuation seed | BuyDevelopment | EndTurn |
| --- | --- | --- |
| 31001 | actor wins, turn 91, actor VP 10 | actor loses, turn 87, actor VP 7 |
| 31002 | actor loses, turn 102, actor VP 6 | actor wins, turn 115, actor VP 10 |
| 31003 | actor wins, turn 64, actor VP 10 | actor loses, turn 77, actor VP 8 |
| 31004 | actor wins, turn 79, actor VP 10 | actor loses, turn 87, actor VP 6 |

Actor wins: BuyDevelopment **3/4**, EndTurn **1/4**.

These streams predate A1+A2 and were not rerun because D1 did not need new stochastic evidence to discriminate H1: the current reviewed base deterministically reproduces the invariant violation at the exact root. A1 changed prospective-port semantics, so these preserved terminal streams should not be described as fresh A1+A2 benchmark results. Their valid role is to prevent the stale one-stream EndTurn conclusion from being reused.

An attempted replay with player trades enabled from game start diverged mechanically at decision 12, so it was rejected rather than treated as a matched D31 comparison. The H1 synthetic invariant probe was run in both trade modes and reproduced the same +0.357635 persistent-production revaluation.

## Required counterexamples

The focused current-base probe preserves five cases where spending now must remain valid:

| Counterexample | Result |
| --- | --- |
| immediate/near-immediate VP closeout | BuyDevelopment chosen; terminal-value root |
| meaningful Largest Army race | Buy 0.992347 vs EndTurn 0.931713 |
| excess non-bottleneck resources | Buy 0.953537 vs EndTurn 0.890076 |
| dev purchase improves build transition (Road Building only) | Buy 0.939823 vs EndTurn 0.888326 |
| saving has no realistic conversion path | Buy 0.219393 vs EndTurn 0.192910 |

These are D2 acceptance guards. D1 did not mutate the production evaluator to run a candidate repair through them.

## Narrow D2 scope

D2 should own only the persistent-production invariant:

1. In CPU `strategic_utility_with_routes_and_knowledge`, stop multiplying already-owned `production_pips` by hand-dependent `dynamic_resource_weights`.
2. Mirror the same semantic change in CUDA `strategic_utility`.
3. Preserve dynamic weights for hand value, prospective expansion/city valuation, robber/action priors, and other genuinely marginal uses unless a separate defect is independently demonstrated.
4. Add a focused regression that unchanged board production has unchanged production contribution across a pure hand spend, including the exact D31-like positive case and a counterexample.
5. Preserve CPU/CUDA evaluator parity.
6. Verify the five spend-now counterexamples plus D31 root decomposition in both trade modes where the fixture is meaningful.

D2 must not add a generic BuyDevelopment penalty, an imported-ore special case, or a road/search arbitration change.

## Verification performed

- Exact D31 replay on A1+A2 base at depth 1 and depth 3.
- Behavior-neutral strategic-utility decomposition.
- Single-card spend diagnostics on the exact D31 state.
- Deterministic per-build ETA diagnostics.
- Focused H1 test in player-trades OFF and ON modes.
- Focused five-case spend-now counterexample probe.
- Reuse and inspection of the four corrected event-family CRN streams.

Commands used for focused tests:

```bash
cd /home/hamza/repo/colonist-d31-liquidity/engine
cargo test -p colonist-catan-search d1_h1_unchanged_production_probe -- --nocapture
cargo test -p colonist-catan-search d1_required_development_counterexamples_probe -- --nocapture
```

No production strategy repair was implemented in D1.
