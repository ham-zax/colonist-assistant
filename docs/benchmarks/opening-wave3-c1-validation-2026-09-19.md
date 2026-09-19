# Opening Wave 3 C1 validation — 2026-09-19

## Scope and authority

Agent C independently validated the reviewed A1+A2 opening implementation at
`0eda726cf0fbd1eba46e3e351fde8c278ed20bb0`, which includes:

- `70e1461 Rebuild opening evaluator architecture`
- `8496200 Repair temporal opening evaluation`

This mission made no production evaluator or search-semantic changes and made
no Jev or other API calls. Commit `3e2268f` adds only deterministic research
tooling and an ignored recorded-state exporter. Commit `8146fe5` adds explicit
research-harness node/time controls and their parser regression test.

## Recorded corpus

From `engine/`:

```text
cargo test --release -p colonist-catan-search opening::recorded_tests:: -- --test-threads=1 --nocapture
```

Result: **27 passed, 0 failed, 1 ignored, 208 filtered out**. Rust reported
169.65 seconds; `/usr/bin/time` reported 216.86 seconds including the cold
release build. No failure was attributable to the reviewed evaluator.

## Frozen generated cohort

A timing-only pilot used excluded seed `8800001` with trades disabled. Wall
times were 26.59 seconds for 2P (including its initial release build), 11.85
seconds for 3P, and 10.88 seconds for 4P.

The cohort was then frozen before outcome inspection. The final manifest is
`frozen-manifest-v2.json` (SHA-256
`4ec53e8c06fe20c3bb4b0a7db3060bb36e67c2386daebf1e33e06f638142441b`):

| Split group | Seed | Partition |
| --- | ---: | --- |
| `generated-seed-9311001` | 9311001 | calibration |
| `generated-seed-9311002` | 9311002 | holdout |
| `generated-seed-9311003` | 9311003 | calibration |
| `generated-seed-9311004` | 9311004 | holdout |
| `generated-seed-9311005` | 9311005 | calibration |
| `generated-seed-9311006` | 9311006 | holdout |

Each group contains all six variants: 2P, 3P, and 4P, each with player trades
off and on. The 36-case cohort therefore has six cases in every player-count
and trade-mode cell. Board seed is the split unit, so rule variants cannot
cross calibration and holdout. The deterministic opening probe evaluates the
initial actor (`rootPlayer=0`); it does not expose arena-style seat rotation,
so this cohort makes no seat-rotation claim.

The bounded probe used 12,000 maximum nodes, root width 24, opponent width 4,
and no wall-clock deadline. All 36 selected settlement roots and all 36 setup
roads were present, authoritative, endpoint-complete, and had the required
objective, production, port, expansion, realization, and denial evidence.
Setup-road reports were exhaustive. Settlement reports were explicitly
non-exhaustive at the bounded cap, with 1,585–3,450 completed setups and 24–54
authoritative endpoint-complete candidates; this is retained as
`complete=false`, not promoted to exhaustive status. No deadline fired.

Semantic checks found zero invalid 2P rival weights, zero invalid multiplayer
rival weights, zero negative causal-denial values, and zero missing required
evidence on selected actions. The cohort contains 344 evidence-bearing
candidates with positive prospective-port gain, 768 with discounted expansion
realization, 143 positive-denial candidates in 3P, and 195 in 4P. Nine of the
18 matched trade-off/on board-and-player-count pairs selected different roots.
These are deterministic diagnostics only; no engine value is a label.

## Matched terminal continuation

The original terminal run plan was frozen before valid outcomes in
`matched-run-plan-v2.json` (SHA-256
`888b57c996defa30ee1e94309405e0dfa0407e25aab085e23f3bf63e99b8401a`).
Each arm reconstructs the exact pre-root state, forces only decision 0, and
uses the normal depth-3 policy with 24,000 ordinary nodes thereafter. Roll,
development-card, and steal event families share common random numbers within
each continuation seed. The independent continuation seeds are `9401001`,
`9402003`, and `9403007`; the original plan covered both player-trade modes
with a 220-turn cap.
Setup uses 12,000 nodes and trade responses 2,000 nodes, both with a zero
wall-clock budget.

After nine valid hill6758 trades-off arms completed, the user capped terminal
work at 12–15 arms. The admitted subset is therefore frozen in
`matched-run-plan-capped-v3.json` (SHA-256
`6eb01582f470cd46927c26c0ecf76269d782347732d35ccf9d843a9c10769e8f`):
the nine completed hill6758 trades-off arms plus six task9783 trades-off arms,
for exactly 15. Because this capped v3 subset was created after those nine
valid outcomes already existed, its provenance correctly records
`frozenBeforeTerminalOutcomes=false`; the scope reduction was user-directed
and did not change any underlying arm, seed, or outcome. Eight incomplete
hill6758 trade-on arms were stopped and quarantined under
`user-cap-excluded-partials/`; no partial is evidence. Trade-on terminal
direction and the port hard negative remain undecided.

The superseded v1 plan exposed hard-coded 1,200 ms setup and 350 ms trade
response caps in the research harness. Its five completed and nine partial
arms were quarantined under `timed-budget-excluded-20260919T1600Z/` and never
entered an aggregate. Research tooling commit `8146fe5` exposes
explicit time controls; v2 preserves every generated seed, split group,
recorded state, terminal arm, and causal RNG seed while fixing both time
budgets at zero before admitting a valid v2 outcome.

### Admitted terminal outcomes (trades off)

The user-directed cap admits only trades-off streams. `W`/`L` is the forced
root actor's terminal result; the number is the terminal turn. All 15 arms
completed without a cutoff.

| Case | Seed | Historical / repair-dependent | All-five / complete | Brick port |
| --- | ---: | --- | --- | --- |
| hill6758 | 9401001 | L / 89 | **W / 68** | L / 89 |
| hill6758 | 9402003 | L / 82 | L / 79 | L / 90 |
| hill6758 | 9403007 | **W / 96** | **W / 92** | L / 103 |

| Case | Seed | Complete all-five | Historical repair-dependent |
| --- | ---: | --- | --- |
| task9783 | 9401001 | **W / 126** | L / 124 |
| task9783 | 9402003 | L / 84 | L / 76 |
| task9783 | 9403007 | **W / 90** | L / 132 |

The frozen aggregator produces four pairwise comparisons, all
`inconclusive` under the predeclared three-stream 95% interval rule:

| Case / pair | Stream directions | Mean direction | 95% interval |
| --- | --- | ---: | --- |
| hill historical → all-five | `[-1, 0, -1]` | -0.667 | [-2.101, 0.768] |
| hill historical → brick port | `[0, 0, 1]` | 0.333 | [-1.101, 1.768] |
| hill all-five → brick port | `[1, 0, 1]` | 0.667 | [-0.768, 2.101] |
| task complete → historical | `[1, 0, 1]` | 0.667 | [-0.768, 2.101] |

Thus the capped terminal evidence is directionally favorable to the all-five
roots in some streams, especially task9783, but supplies **zero conclusive
labels**. It must not be used as a universal ranking or as Jev ground truth.

The user cap excludes all trade-on terminal arms and the hand2325 terminal
pair. Their static/generated evidence remains available, but their terminal
direction is not claimed.

Aggregate direction is reported separately from per-stream results. Direction
is `+1` when the left arm wins and the right does not, `-1` for the reverse,
the earlier win when both arms win, and `0` when both actor arms lose or either
run reaches the cap. A pair is conclusive only when the paired 95% interval
excludes zero. One stream is never treated as a universal ranking.

## Post-D2 qualification

The integrated strategy branch now contains reviewed D1+D2 at `a4bb9e6` and
`35815e3`. D2 changed persistent-production weighting inside main-game
`strategic_utility` and its exact CUDA parity: persistent board production now
uses the fixed base resource weights rather than transient hand-scarcity
weights. D2 did not edit `opening.rs` or `economy.rs`. The C1/default live
opening solver uses `rollout_count = 0`, and `frozen-manifest-v2.json` likewise
freezes `rolloutCount: 0`, so the recorded opening corpus and the 36-case
generated opening evidence remain evidence about the dedicated A1+A2 opening
evaluator. `opening.rs` retains an optional non-default rollout leaf that can
call `eval::evaluate()` if rollouts are enabled; that path was not active in C1.

All 15 matched terminal arms were generated from repository commit
`0eda726cf0fbd1eba46e3e351fde8c278ed20bb0`, before D2. They are therefore
**pre-D2 continuation evidence**, not final-semantics terminal labels. All four
pairwise comparisons were already inconclusive, and `b2-handoff-v1.json` has
`admittedLabels: []`; no stale pre-D2 terminal comparison is admitted as B2
ground truth.

A post-D2 terminal rerun is not required to preserve C1's current conclusion.
It is required only if the project specifically wants final-semantics terminal
directionality. Under the user cap: static opening validation is complete; the
15-arm terminal campaign is complete but inconclusive; conclusive B2 terminal
labels are zero; and a universal opening ranking is not established.

## Hard-negative status

Only a hard negative with a satisfied matched-terminal label contract enters
metric totals.

| Family | Concrete case | Status | Metric eligible |
| --- | --- | --- | --- |
| attractive-but-bad port | `hand2325-attractive-bad-port` | instantiated, terminal run excluded by user cap | no |
| all-five, low throughput | `generated-five-resource-low-throughput-9311002-p2-on-v26` | instantiated, unlabeled | no |
| self-unfundable repair | `generated-self-unfundable-repair-9311003-p2-off-v51` | mechanically instantiated | no |
| unreachable repair | `generated-unreachable-repair` | `generation_required` | no |
| non-causal denial | `opening-non-causal-denial-regressions` | mechanically validated by two focused tests | no |
| dominated setup road | `generated-mechanically-dominated-setup-road-9311002-p2-on-e65` | mechanically instantiated | no |

The low-throughput case has all five resources but only 12 total pips versus
24 for its frozen comparator. The self-unfundable case records a one-road
repair with 48-roll project ETA and 0.3333 conversion/realization. The denial
regressions
`player_zero_final_settlement_cannot_deny_completed_opponents` and
`multiplayer_denial_exists_only_for_a_site_the_root_actually_blocks` both
passed. None of those mechanical facts is promoted to terminal ground truth.
The setup-road case compares edge 65 with edge 54 after frozen settlement 42:
both lead to vertex 18 with one remaining road, 14.4-roll project ETA, full
realization, and no prospective-port gain, while edge 65 has the lower
expansion term (2.9411 versus 3.1293). It is mechanically dominated but remains
outside metrics because no terminal pair was frozen.

Raw D17 files were recovered as prior provenance, but they record a turn-8
`Main`-phase road decision, one historical stream, and nonzero wall-clock
budgets. D17 is therefore retained as midgame context only, not misrepresented
as the required opening setup-road label.

## B2 handoff

B2 should consume the ignored directory
`benchmark-results/jev-lab/c1-wave3/` as follows:

- `frozen-manifest-v2.json`: immutable cohort, split, and label contracts;
- `generated/*.json`, `generated-summary.json`, and
  `generated-analysis.json`: unlabeled deterministic candidate evidence;
- `states/*.json` and `recorded-evidence/*.json`: exact pre-root state and
  reviewed A1+A2 opening-evaluator feature provenance;
- `matched-run-plan-capped-v3.json`, `matched/raw/*.jsonl`, `matched.sha256`, and
  `matched-summary-v1.json`: forced-root terminal authority, streams, and
  retained intermediate actor diagnostics;
- `hard-negative-status-v1.json`: admission status and blockers;
- `prior-evidence/*.jsonl` and `prior-evidence.sha256`: excluded D17 context;
- `b2-handoff-v1.json`: machine-readable input inventory and admitted labels.

B2 must preserve every `splitGroup` across all player counts, trade modes,
continuation seeds, root arms, and later Jev perturbations. It must exclude
`unlabeled_deterministic_diagnostic`, `generation_required`, mechanical-only,
prior-context, and inconclusive pairs from accuracy totals. The remaining
opening-corpus blockers are an auditable unreachable-repair construction,
trade-on terminal comparisons, and a terminal label for the attractive-port
case. D17 is available only as excluded midgame context. These limitations do
not block consumption of the completed, explicitly provenanced trades-off
subset, but B2 must not infer the omitted results.

No production defect was discovered.
