# M1 midgame build-transition counterfactual study — 2026-09-20

Status: **complete**

## Authority and workspace

- Mission: Agent M — M1, causal diagnosis only.
- Verified base: `9aee018fe1c8522870ed9961740946a943f75109`; local `main` and `origin/main` matched before worktree creation.
- Worktree: `/home/hamza/repo/colonist-midgame-transition-study`.
- Branch: `agent/m1-midgame-transition-study`.
- Production strategy mutation authority: none.
- Player trades: OFF. No natural trades-ON position met the pilot's fidelity and 3–8 VP constraints without broadening the mission.

No forced-root terminal outcome was inspected before freezing the state and root manifests. Discovery used current-main deterministic trajectories capped at turn 50. The ignored authority files are:

- `benchmark-results/jev-lab/m1-build-transition/state-selection-manifest-v1.json`;
- `benchmark-results/jev-lab/m1-build-transition/root-manifest-v1.json`;
- `benchmark-results/jev-lab/m1-build-transition/protocol-v1.json`.

## Frozen primary states and roots

| State | Exact source | Actor state | Why selected | Frozen roots |
| --- | --- | --- | --- | --- |
| A — road / settlement / save | seed `9520001`, game 0, decision 101, hash `88b38976489011f5` | turn 50, actor 1, 3 VP, hand `[1,2,1,2,0]` | Planner-backed road competes with an immediately legal settlement and resource preservation. | `BuildRoad(53)`; `BuildSettlement(22)`; `EndTurn` |
| B — settlement / development / city-save | seed `9520001`, game 1, decision 61, hash `0be5bbcb3aaac817` | turn 30, actor 1, 3 VP, hand `[1,1,2,1,2]` | Settlement and development are legal while the actor is one ore short of a city. | `BuildSettlement(6)`; `BuyDevelopment`; `EndTurn` |
| C — city hard negative | seed `9521001`, game 0, decision 75, hash `b527c2578b98c565` | turn 48, actor 3, 5 VP, hand `[3,0,0,2,3]` | Affordable city should protect legitimate immediate spending; Road Building provides a distinct expansion/trophy transition. | `BuildCity(15)`; `PlayRoadBuilding(16,22)`; `EndTurn` |
| D — development / city conversion / save | seed `9521001`, game 1, decision 67, hash `d525e5639887a3eb` | turn 41, actor 0, 3 VP, hand `[0,0,1,3,1]` | Development is affordable; saving retains a 12-roll city path; 3:1 grain-to-ore tests an apparent conversion that may be illusory. | `BuyDevelopment`; `MaritimeTrade(grain,ore,3)`; `EndTurn` |

Resource order in compact hands is `[lumber, brick, wool, grain, ore]`.

## Frozen protocol

- Exactly 4 states x 3 roots x 3 seeds = **36 planned terminal arms**, with no automatic expansion.
- Continuation seeds: `9531001`, `9532003`, `9533007`.
- Search: depth 3, 24 belief particles, 12 strategic particles, branch cap 12, 12,000 ordinary nodes, 12,000 opening nodes, and 2,000 trade-response nodes.
- Every wall-clock search budget is zero. Continuation policy is current-main baseline after forcing only the root.
- Rolls, development draws, and steals use separate common-random-number streams keyed by continuation seed.
- Maximum terminal turn is 220; cutoffs remain explicit evidence and are never silently treated as losses.
- Every replay must assert the frozen actor observation hash before applying its root.

## Checkpoint log

### Checkpoint 1 — workspace, states, and roots

Complete. Four distinct board states and exactly three roots per state were frozen before forced-root terminal outcomes. Baseline verification before research changes passed `npm test` (500 passed, 5 skipped) and `cargo test -p colonist-catan-arena --bin jev-strategy-lab` (1 passed).

### Checkpoint 2 — deterministic root diagnostics

Complete. Every selected state replayed to its frozen observation hash and accepted its current-engine root. All four diagnostics used the frozen node budgets and reported `deadlineReached=false`. Search depth is the deepest completed depth under the fixed 12,000-node cap, not a wall-clock result.

| State / root | Actor value | Lower confidence | Prior | Immediate strategic delta | Resource-only delta | Key deterministic mechanism |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| A road 53 (engine) | 0.000018 | 0.000002 | 0.2040 | -2.667 | -2.351 | Planner value 36.221, decisive completion mass 1.0, zero response windows. |
| A settlement 22 | 0.001509 | 0.000172 | 0.1678 | +5.100 | -3.648 | Search winner and immediate production/VP gain; planner value 26.880. |
| A EndTurn | 0.000042 | 0.000004 | 0.0400 | -2.262 | 0 | Keeps the settlement-affordable hand but exposes three response windows. |
| B settlement 6 (engine) | 0.088198 | 0.008447 | 0.5163 | +3.253 | -3.125 | Immediate settlement/production; decisive completion mass 1.0. |
| B BuyDevelopment | 0.041289 | 0.002345 | 0.1383 | -1.326 expected | -2.775 | Consumes the city-transition ore/grain/wool and leaves three response windows. |
| B EndTurn | 0.014671 | 0.000746 | 0.1202 | -1.263 | 0 | Preserves a one-ore-short, 7.2-roll city path but has expected discard loss 0.142. |
| C city 15 (engine) | 0.995628 | 0.992873 | 0.1005 | +7.460 | -2.057 | Converts the already-affordable city immediately and improves persistent production. |
| C Road Building 16/22 | 0.994569 | 0.991646 | 0.7243 | +0.009 | 0 | Resource-free expansion improvement; the city remains affordable afterward. |
| C EndTurn | 0.672287 | 0.469207 | 0.0005 | -4.308 | 0 | Keeps the city hand but carries expected discard loss 1.798. |
| D BuyDevelopment (engine) | 0.003627 | 0.001489 | 0.6298 | +0.553 expected | -0.924 | Immediate development conversion is locally positive. |
| D grain-to-ore 3:1 | 0.000288 | 0.000100 | 0.0711 | -0.429 | -0.429 | City ETA stays 12 rolls while development ETA worsens from 0 to 7.2 rolls. |
| D EndTurn | 0.000406 | 0.000141 | 0.1979 | -0.602 | 0 | Preserves both immediate development affordability and the 12-roll city path with zero discard exposure. |

The four selected search records completed at depths A=2, B=2, C=1, and D=1 with 12,000, 11,995, 12,000, and 11,996 nodes respectively. Root counts/pruning were A 17/5, B 11/0, C 5/81, and D 6/0. This pilot therefore compares decisions at an identical work cap, not an identical completed depth.

The deterministic mechanisms already supply two important guards. State C is a strong spend-now hard negative: EndTurn preserves the same city hand but adds large discard exposure, while City creates immediate production and VP. State D falsifies a superficial “trade toward City” story before terminal play: converting all three grain to one ore does not shorten the city ETA and destroys the immediately available development conversion.

### Checkpoint 3 — terminal arms

Complete: **36/36 admitted arms validated**. Each file has one lab header, the exact frozen continuation seed/root/state hash, the expected chosen root at the frozen decision, and exactly one `gameEnd`. No arm cut off at turn 220. Four outputs interrupted during an earlier detached batch and six empty files produced by an incorrect output invocation were never admitted; each affected planned tuple was rerun once correctly, and only its single valid terminal file is present.

The SHA256 manifest for the 36 terminal files and three frozen authority manifests is `benchmark-results/jev-lab/m1-build-transition/artifact-sha256.txt` (manifest SHA256 `1e3936c970fad68fb57b1d5e8aeb506e3b67d774d12e91fdf6c0d0d012f40951`). Benchmark output remains ignored as required by repository policy.

### Checkpoint 4 — interpretation

Complete. **NO GENERAL DEFECT ESTABLISHED.** The pilot found local ordering behavior worth retaining as diagnostics, but no common causal mechanism met the six-part general-defect bar in two unrelated states. In particular, the nominal save-for-City paths often did not convert to City before another spend, and the hard-negative state correctly attached severe discard exposure to waiting.

## Per-seed terminal outcomes

`VP` is the actor's final actual VP. `R/S/C/D` are rolls from the forced root to the actor's first road, settlement, city, and development purchase; `-` means none before terminal. A star marks an actor win.

| State | Root | Seed | Winner | Actor result | Terminal turn | R/S/C/D |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| A | road 53 | 9531001 | 3 | VP 8 | 96 | 0/8/-/28 |
| A | road 53 | 9532003 | 2 | VP 5 | 99 | 0/12/-/- |
| A | road 53 | 9533007 | 2 | VP 7 | 119 | 0/16/68/36 |
| A | settlement 22 | 9531001 | 0 | VP 5 | 93 | 12/0/-/28 |
| A | settlement 22 | 9532003 | 1 | **VP 10★** | 94 | 8/0/-/8 |
| A | settlement 22 | 9533007 | 3 | VP 8 | 112 | 12/0/-/16 |
| A | save | 9531001 | 3 | VP 8 | 100 | 12/4/-/32 |
| A | save | 9532003 | 0 | VP 7 | 113 | 4/12/56/- |
| A | save | 9533007 | 3 | VP 8 | 112 | 4/16/-/20 |
| B | settlement 6 | 9531001 | 0 | VP 8 | 73 | 12/0/-/4 |
| B | settlement 6 | 9532003 | 1 | **VP 10★** | 74 | 12/0/20/8 |
| B | settlement 6 | 9533007 | 2 | VP 5 | 67 | 12/0/-/12 |
| B | Buy Development | 9531001 | 2 | VP 6 | 67 | 0/12/-/0 |
| B | Buy Development | 9532003 | 2 | VP 6 | 91 | 0/8/32/0 |
| B | Buy Development | 9533007 | 2 | VP 6 | 79 | 0/44/-/0 |
| B | save | 9531001 | 2 | VP 6 | 71 | 20/4/12/24 |
| B | save | 9532003 | 1 | **VP 10★** | 78 | 8/4/28/8 |
| B | save | 9533007 | 2 | VP 5 | 63 | 4/16/16/12 |
| C | city 15 | 9531001 | 2 | VP 5 | 83 | 4/-/0/- |
| C | city 15 | 9532003 | 2 | VP 5 | 79 | 15/-/0/16 |
| C | city 15 | 9533007 | 0 | VP 7 | 81 | 4/-/0/24 |
| C | Road Building | 9531001 | 2 | VP 5 | 87 | 0/36/0/20 |
| C | Road Building | 9532003 | 2 | VP 9 | 87 | 0/8/0/36 |
| C | Road Building | 9533007 | 3 | **VP 10★** | 80 | 0/8/0/28 |
| C | save | 9531001 | 2 | VP 6 | 87 | 8/36/20/36 |
| C | save | 9532003 | 2 | VP 7 | 87 | 8/16/24/12 |
| C | save | 9533007 | 0 | VP 3 | 73 | 4/-/-/16 |
| D | Buy Development | 9531001 | 3 | VP 8 | 104 | 8/48/36/0 |
| D | Buy Development | 9532003 | 3 | VP 9 | 100 | 24/32/20/0 |
| D | Buy Development | 9533007 | 3 | VP 4 | 84 | 8/-/20/0 |
| D | grain-to-ore | 9531001 | 3 | VP 4 | 76 | 15/-/-/4 |
| D | grain-to-ore | 9532003 | 3 | VP 6 | 104 | 24/32/16/8 |
| D | grain-to-ore | 9533007 | 3 | VP 4 | 84 | 8/28/-/8 |
| D | save | 9531001 | 3 | VP 6 | 84 | 19/40/36/4 |
| D | save | 9532003 | 3 | VP 6 | 84 | 24/-/8/4 |
| D | save | 9533007 | 3 | VP 4 | 76 | 12/28/-/4 |

Win counts are screening evidence only: A road 0/3, settlement 1/3, save 0/3; B settlement 1/3, development 0/3, save 1/3; C city 0/3, Road Building 1/3, save 0/3; D all roots 0/3. No conclusion below relies on a 2–1 comparison.

## State-by-state causal diagnosis

### A — planner-backed road versus settlement and save

The engine selected road 53 even though the ordinary actor-value ranking strongly favored settlement 22 (`0.001509` versus `0.000018`). The identifiable owner is the planner overlay: road 53 had planner value `36.221`, decisive completion mass `1.0`, and zero response windows, versus settlement's `26.880`. The forced road immediately triggered a grain-to-lumber maritime trade and another road in seed 9531001, and delayed the first settlement to 8/12/16 rolls. Immediate settlement converted at roll 0 and was the only A root to win an arm.

This is a local road-sequence ordering concern, not evidence for a save-transition defect. EndTurn already held the legal settlement cost, suffered a discard before building in seed 9531001, and in the other two seeds spent on roads before finally settling at 12/16 rolls. Saving did not outperform either by a stable matched mechanism. The A finding should remain a minimal reproducer for planner-versus-search arbitration, without reopening the rejected road-intent production override.

### B — settlement versus development versus nominal City save

The engine preferred settlement with the strongest actor value (`0.088198`), lower-confidence value (`0.008447`), prior (`0.5163`), and an immediate strategic gain of `+3.253`. That root converted production and VP immediately and won the same seed (`9532003`) in which save won. Buy Development consumed the ore/grain/wool needed by the prospective City path, was correctly assessed at `-1.326` expected immediate utility, and did not win.

EndTurn did eventually reach City in 12/28/16 rolls, but it was not a clean City transition: settlement occurred first in seeds 9531001 and 9532003, a road occurred first in seed 9533007, and two seeds discarded before the next build. Thus the saved hand was redirected before City in all three arms. The terminal evidence does not contradict the engine's immediate-settlement mechanism; it demonstrates why a generic save-for-City bonus would mislabel the path.

### C — immediate-spend hard negative

The engine preferred City by actor value (`0.995628` versus Road Building `0.994569` and save `0.672287`) despite Road Building's much larger prior (`0.7243`). City delivered `+7.460` immediate strategic utility, including persistent production; EndTurn carried expected discard loss `1.798`. All three save arms in fact discarded before a meaningful build, City was delayed to 20/24 rolls or never built, and the actor never won.

Road Building was strategically legitimate rather than a straw alternative: it preserved the City hand and the continuation built City in the same root turn in all three arms. It also won seed 9533007 and ended with Longest Road there. That is a useful local ordering counterexample—free roads plus same-turn City can create value that a simple City ETA misses—but the near-tied root values already represented that value, and one 1/3 versus 0/3 result does not establish a defect. Most importantly, this state protects the invariant that waiting must not receive generic credit when discard exposure makes immediate conversion valuable.

### D — development versus false City conversion

The engine preferred Buy Development through a positive expected immediate delta (`+0.553`) and prior `0.6298`. The 3:1 grain-to-ore action did not improve the 12-roll City ETA and worsened development ETA from 0 to 7.2 rolls; its `-0.429` evaluation explicitly captured that opportunity cost. Terminal continuations agreed: after the trade, development came first at 4/8/8 rolls and City arrived only once (16 rolls). After EndTurn, development still came first at 4 rolls in every seed; City arrived at 36/8 rolls or never. The supposed saved City resources therefore converted to development first in all three save arms.

No D root won. That is not evidence for or against the root ordering because actor 0 was a low-probability player and player 3 won every matched arm. The causal result is mechanical: neither the trade nor EndTurn instantiated the claimed direct City transition, while the evaluator correctly recognized immediate development affordability and the trade's destroyed opportunity.

## Hypotheses and hard negatives

| Hypothesis | Result | Evidence |
| --- | --- | --- |
| H1 nearest-build tunnel vision | Not established | B's nearest settlement immediately improved production and matched/save terminal evidence did not expose a systematic lost City transition; C's immediate City avoided real discard risk. |
| H2 save-without-conversion hallucination | Supported as a guard, not as an engine defect | B save spent on settlement/road before City; D save bought development first in all seeds; A save often bought roads before settlement. The current engine generally rejected these saves. |
| H3 action-family immediacy bias | Not established | C assigned the free-road line a 0.7243 prior and near-tied actor value; B explicitly penalized the immediate development spend. |
| H4 transition double counting | No evidence | No repeated root ordering was traced to duplicate ETA/expansion/build-tempo credit. |
| H5 missing opportunity cost | Falsified in the clearest test | D's diagnostic explicitly showed unchanged City ETA plus destroyed development affordability; B also assigned Buy Development negative immediate utility. |
| H6 discard/trophy/race override | Supported | C save had high predicted discard loss and all three arms discarded; Road Building's Longest Road conversion in one seed showed real non-ETA value. B/A save arms also exposed discard or race delays. |

The required hard negative is state C: spending on City was mechanically justified over EndTurn, and the alternative Road Building line showed that immediate non-resource tempo can also be legitimate. State D is a second counterexample to generic saving because the nominal City path consistently converted to development first.

## Decision and recommended next experiment

**NO GENERAL DEFECT ESTABLISHED.** No mechanism appeared in two unrelated states with a shared owning term and directionally consistent matched evidence. The evidence instead rejects a generic save/spend heuristic: conversion identity, discard exposure, action ordering within the same turn, and intervening spends matter more than static ETA alone.

The next bounded experiment should target **same-turn action-order equivalence**, using naturally occurring states where two legal roots can both execute the same set of builds before EndTurn (for example Road Building then City versus City then Road Building). Freeze pairs only when both sequences are legal and resource-equivalent, then compare whether search values differ because one root exposes the follow-up action more faithfully. State C is the minimal seed for that question. This is distinct from a save bonus, development-card penalty, or road-intent override.

No M2 production repair scope is proposed from M1.

## Verification

- Terminal matrix audit: 36/36 exact planned filenames; 36/36 files with one header and one `gameEnd`; zero cutoffs; continuation-seed and frozen-root-hash pairing valid.
- `cargo test -p colonist-catan-arena --bin jev-strategy-lab`: 7 passed.
- `cargo test -p colonist-catan-arena`: arena library 2 passed, main binary 11 passed, lab binary 7 passed, no failures.
- `npm test`: 42 files passed; 500 tests passed and 5 skipped.
- `git diff --check`: passed before the final report commit.
- Changed tracked paths relative to base are limited to this report and the research-only `engine/crates/catan-arena/src/bin/jev-strategy-lab.rs`; no production evaluator, search, strategy, WASM, worker, or frontend file changed.
