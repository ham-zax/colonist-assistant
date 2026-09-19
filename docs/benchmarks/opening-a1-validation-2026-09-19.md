# Opening A1 evaluator validation — 2026-09-19

## Scope

This note records Agent A, Mission A1 validation for the opening evaluator
architecture rebuild. The implementation changes deterministic evaluator
semantics only; it adds no Jev or network dependency and does not calibrate
against the hill6758 outcome.

## Implemented semantics

- Multiplayer completed-opening leaves use the root player's setup-aware own
  value plus causal denial. They no longer subtract a fixed fraction of the
  strongest rival. Two-player leaves retain the direct own-minus-rival
  zero-sum comparison.
- Opponent snake-draft branches are unchanged: every opponent continues to
  maximize that opponent's own setup-aware value.
- Causal denial removes one root settlement at a time, finds opponent setup
  sites made newly legal by that removal, and values only the best opportunity
  loss relative to the opponent's best actually legal site. The lost weighted
  production gap is converted to the opening production scale and shared among
  the non-root seats.
- Port value is a breadth-adjusted sum of improvements in complete road,
  settlement, city, and development-card affordability at 0, 18, and 36 roll
  horizons, multiplied by after-port conversion efficiency. A printed ratio
  change that advances no complete build receives no value.
- Expansion realization uses the exact road-plus-settlement project cost. An
  already funded project realizes fully; otherwise its raw site and portfolio
  value is multiplied by project conversion efficiency divided by
  `1 + ETA / 18`.

## Focused and corpus validation

Commands were run from `engine/`.

```text
cargo test -p colonist-catan-search task9783_equal_pips_complete_portfolio_beats_speculative_repair_in_both_trade_modes
cargo test -p colonist-catan-search hill6758_multiplayer_objective_keeps_own_economy_and_causal_denial_separate
cargo test -p colonist-catan-search hand2325_d1_does_not_sacrifice
cargo test -p colonist-catan-search concentrated_port_engine_can_beat
cargo test --release -p colonist-catan-search opening::recorded_tests:: -- --test-threads=1 --nocapture
```

Results:

- task9783 passed with player trades disabled and enabled. The equal-pip
  all-five portfolio `[5, 4, 5, 4, 3]` remains preferred over the historical
  `[5, 11, 0, 5, 0]` root.
- hill6758 passed with player trades disabled and enabled. The fixed rival
  multiplier is zero in multiplayer, and each candidate's terminal value is
  its own value plus its explicit causal-denial term.
- hand2325 rejects the five-pip generic-port root whose ratio improvement only
  advances one complete-build family.
- The concentrated 2:1 conversion fixture still beats balanced 4:1 access
  when the port advances multiple complete-build families.
- Recorded opening corpus: 27 passed, 0 failed, 1 intentionally ignored
  research experiment; elapsed time 160.00 seconds in the release profile.
- Final `colonist-catan-search` release suite: 219 unit tests passed, 15
  intentionally ignored diagnostics, and all 3 integration tests passed.

The no-player-trade hill6758 evidence probe reported:

| Root | Production pips | Own value | Causal denial | Final value |
| --- | --- | ---: | ---: | ---: |
| historical `v:2,-2,1` | `[2, 5, 0, 8, 5]` | 8.6093 | 0.1219 | 8.7312 |
| all-five `v:0,0,0` | `[2, 5, 2, 4, 5]` | 8.7679 | 0.1584 | 8.9262 |
| 2:1 brick port `v:-1,3,0` | `[0, 5, 3, 4, 5]` | 7.6954 | 0.1368 | 7.8321 |

The solver's selected root was a different all-five portfolio,
`v:1,-1,0`, with final value 9.0646. The historical zero-wool root therefore
no longer wins because unrelated rival strength is subtracted from a better
own position. The brick-port root has positive explicit complete-build port
gain (0.0683), but its exact road-plus-settlement realization is only 0.2885;
the evaluator does not grant full speculative repair credit.

## Bounded matched replay

The three original forced hill6758 roots were replayed on the original
controlled future stream with board seed 9196758, chance seed 6758001,
continuation seed 9001001, depth 3, 24,000 ordinary nodes, and player trades
disabled. Each arm forced only the first settlement; the rebuilt evaluator
controlled subsequent setup choices.

| Forced root | Historical result | A1 replay result |
| --- | --- | --- |
| zero-wool historical | seat 3 win, turn 96 | seat 3 win, turn 84 |
| all-five | seat 3 win, turn 92 | seat 3 win, turn 116 |
| 2:1 brick port | seat 3 win, turn 84 | seat 3 win, turn 116 |

This single-stream ordering reversed after the downstream setup policy
changed. It is a falsification result, not a tuning target: it does not
override the causal static regression, and it is insufficient to establish a
universal root ranking. Independent review and broader Wave 3 matched/held-out
validation should treat the changed terminal ordering as an explicit review
focus.

The replay preceded the final observation-safety correction that prevents
public opponent prospective-port evaluation from consulting hidden resource
identities. That correction can change later main-game search choices even
though it does not change the exact hill6758 setup-root evidence. At the
user's direction, the expensive terminal replay was not repeated; therefore
84/116/116 is development-branch evidence, not a terminal result verified on
the exact final commit.
