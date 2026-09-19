# Agent A — Mission A2: Repair R1 Opening Evaluator Findings

## Role

Role: implementation repair  
Review independence: independent re-review required  
Session: continue in the existing Agent A chat  
Workspace: `/home/hamza/repo/colonist-opening-architecture`

A1 is complete at `15b2365`. Do not reopen unrelated A1 work. Repair only the two R1 blocking findings and directly affected contracts.

## Review source

R1 status: blocking findings.

Original reviewed range:

`a169b28765f37d772ee1b8cf0d71dab5ec8d93af..15b236584dfa55417dc2815945b1d2ac57827224`

Candidate:

`15b2365 Rebuild opening evaluator architecture`

## Blocking finding F1 — temporal causality of opening denial

R1 found that `causal_opening_denial_value()` computes denial from the completed final board and can credit a root settlement with denying opponents who had already finished their setup placements.

Four-player settlement order is:

`P0, P1, P2, P3, P3, P2, P1, P0`.

Therefore:

- P0's second settlement cannot deny any opponent setup settlement;
- P1's second settlement can only affect P0's still-future settlement;
- more generally, denial credit must respect the decision-time setup state and only opponents whose relevant settlement decision occurs after the root placement.

R1 also found that `actual_best` is taken from sites still open on the completed board rather than sites available at the opponent's relevant decision point.

### Repair obligation

Implement denial from the decision-time setup state, not the completed board correlation.

At minimum:

- only future opponent settlement decisions after the root placement may receive denial credit;
- compare legal opponent opportunities with and without the root placement at that time;
- preserve a non-double-counting aggregation rule;
- do not award denial for opponents already done with setup;
- add a temporal negative control proving P0's final four-player settlement earns no denial against completed opponents.

## Blocking finding F2 — duplicated expansion ETA penalty

R1 found that the road-plus-settlement project ETA is effectively discounted twice for unfunded opening expansion.

The pre-existing `expansion_arrival_score()` / accessibility path already contributes:

`1 / (1 + ETA / 18)`

to `expansion.value` for the root player's exact-hand path.

A1 then multiplies by `complete_build_conversion_value()`, whose unfunded realization is:

`conversion_efficiency / (1 + ETA / 18)`.

That yields an unintended effective factor:

`conversion_efficiency / (1 + ETA / 18)^2`

before the independent race/survival term.

### Repair obligation

Apply the road-plus-settlement self-funding ETA discount exactly once.

Acceptable directions include:

- derive an opening-specific raw expansion value without the existing economic-accessibility ETA factor, then apply the A1 realization factor once; or
- retain the existing ETA accessibility and apply only the non-duplicated remainder of the realization semantics.

Preserve the opponent-relative survival/race calculation independently.

Add a regression validating the final expansion term algebraically, not only the exposed `realization` field.

## Required preservation

Do not regress the R1 non-blocking passes:

- multiplayer blanket strongest-rival subtraction remains removed;
- two-player own-minus-rival remains correctly scoped;
- opponent self-maximization remains intact;
- complete-build prospective-port semantics remain intact;
- hand2325 weak generic-port negative remains passing;
- concentrated 2:1 positive control remains passing;
- public-opponent hidden-hand invariance remains passing;
- CPU/CUDA source-semantic parity remains aligned where touched.

## Validation authority

The user authorized focused regressions and bounded simulations.

Run the smallest focused set capable of disproving the repairs. Required focused validation:

- new temporal denial negative control;
- existing denial positive/negative controls;
- new final expansion algebra regression;
- existing complete-project funding regression;
- hill6758 both trade modes;
- task9783 both trade modes;
- hand2325 weak generic-port control;
- concentrated 2:1 port control;
- public hidden-hand invariance if `eval.rs` changes;
- two-player endpoint semantics;
- `git diff --check`.

Do not broaden into D31, road-intent WIP, B1/B2 calibration, GPU runtime routing, or unrelated test campaigns.

## Review gate

Commit the repair on `agent/a1-opening-architecture` and return the exact new commit.

A1 integration remains blocked until Agent R re-reviews the A2 repair as R2 and reports pass.

## Finish report

Return:

1. status: complete / blocked / needs decision;
2. Agent A, Mission A2;
3. branch/worktree and repair commit;
4. exact F1 repair and temporal-causality test;
5. exact F2 repair and single-ETA algebra test;
6. focused validation actually performed;
7. whether any directly related behavior changed beyond the two findings;
8. unresolved blockers for R2.
