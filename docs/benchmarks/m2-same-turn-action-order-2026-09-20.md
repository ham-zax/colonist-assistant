# M2 same-turn action-order equivalence study — 2026-09-20

Status: **complete**

## Authority and workspace

- Mission: Agent M — M2, search-representation and action-order diagnosis only.
- Parent result: M1 concluded **NO GENERAL BUILD-TRANSITION DEFECT**; M2 is a separate diagnostic experiment, not a production repair.
- Worktree: `/home/hamza/repo/colonist-midgame-transition-study`.
- Branch: `agent/m1-midgame-transition-study`.
- Starting commit verified: `ea254763fc2a3cd27623076c61b3a3a22f8976b1`.
- Production mutation authority: none.
- Player trades: OFF, matching the recorded M1 trajectories.

Ignored authority artifacts live under `benchmark-results/jev-lab/m2-action-order/`. The candidate set was selected from a search-free mechanical scan of four deterministic recorded trajectories. No M2 root-value asymmetry was inspected before the manifest was frozen.

## Checkpoint 1 — frozen candidate pairs

Four states were frozen in `candidate-manifest-v1.json`. Only two natural cross-family states existed in the scanned trajectories, so two ordinary-road pairs from unrelated boards/actors are retained as same-family controls rather than manufacturing more cross-family examples.

| ID | Exact source | Actor state | Sequence A | Sequence B | Why it should commute |
| --- | --- | --- | --- | --- | --- |
| C — City / Road Building | board `9521001`, chance `73977621840940`, game 0, decision 75, observation `b527c2578b98c565` | turn 48, actor 3, actual/public 5 VP, hand `[3,0,0,2,3]` | City 15; Road Building 16/22 | Road Building 16/22; City 15 | Independent City resources and held card; neither action introduces chance or changes the other's legality. |
| E — road / City | board `2663955770`, chance `73977890275525`, game 1, decision 59, observation `8eeced913c26d351` | turn 29, actor 0, 3 VP, hand `[1,1,0,2,3]` | road 19; City 15 | City 15; road 19 | Jointly affordable paid builds with independent route/building legality. |
| F — two-road control | board `2663955770`, chance `73977890275525`, game 1, decision 47, observation `0497e37ec88eb8ff` | turn 24, actor 3, 4 VP, hand `[4,3,1,0,3]` | road 0; road 1 | road 1; road 0 | Both roads are legal from the root and remain legal after the other. |
| G — two-road control | board `2663956770`, chance `73977890276397`, game 1, decision 52, observation `4cdc3f108479b82c` | turn 30, actor 1, actual/public 3/2 VP, hand `[4,2,0,1,1]` | road 17; road 20 | road 20; road 17 | Unrelated board/actor control with two independently legal roads. |

Resource order is `[lumber, brick, wool, grain, ore]`.

## Checkpoint 2 — mechanical equivalence audit

Complete from the search-free scanner. Every pair is valid for M2: both first actions were legal at the shared root; each second action remained legal; no chance/opponent node occurred between actions; exact `GameState` equality held; the field-level snapshot reported no differing fields.

| ID | Pre full hash | Post full hash A/B | Post observation hash A/B | Post public hash A/B | Exact equality |
| --- | --- | --- | --- | --- | --- |
| C | `c10a0434e34bcbab` | `c38c8b0fb8b8ca67` | `b22a4dfd17e81ad1` | `b22a4dfd17e81ad1` | yes; zero differing fields |
| E | `1769f1c2b34727ed` | `8be7659b77bf4b38` | `5969440c9e9c0d1c` | `5969440c9e9c0d1c` | yes; zero differing fields |
| F | `7f9f8ef563aad199` | `b7eb3aa178226023` | `a536ed12c8b20f95` | `53c8afe1e1db4139` | yes; zero differing fields |
| G | `6192ebb42e62e2f0` | `effb2b468d95695e` | `2d730c5132597672` | `f669cde10d35e910` | yes; zero differing fields |

The audited snapshot covers full, observation, and public hashes; actor hand; all players' exact and public counters; roads; settlements/cities; bank; development deck and played-card state; per-player played-development flag; Largest Army; Longest Road; robber; current player; phase; turn; free roads; sorted legal-action set; and strategic utility. Exact `GameState` equality additionally covers all remaining internal rule state.

## Frozen search protocol

- Depth caps: 1, 2, and 3.
- Fixed ordinary budget: 48,000 nodes; zero wall-clock cutoff.
- 24 belief particles, 12 strategic particles, branch cap 12, baseline policy.
- Identical root action set and trajectory seed at every depth for a state.
- Standalone searches from each post-first-action state use the same
  configuration and record whether the paired second action is legal,
  admitted, ranked, selected, pruned, or planner-backed.
- One deterministic post-sequence continuation sanity seed: `9542001`; terminal campaigning is out of scope.

## Checkpoint 3 — root search diagnostics

The values below are the native search values of the two *first actions*. They
are not values of forced two-action macro sequences: after the first action,
normal search may select the paired second action or a different continuation.
`A-B` is positive when the first action in Sequence A ranks above the first
action in Sequence B.

| State | Depth cap | Completed depth | Nodes | A value / LCB | B value / LCB | A-B | Engine root |
| --- | ---: | ---: | ---: | --- | --- | ---: | --- |
| C | 1 | 1 | 120 | 0.997069 / 0.993221 | 0.996756 / 0.992121 | +0.000313 | City 15 |
| C | 2 | 2 | 33,362 | 0.955805 / 0.891599 | 0.948395 / 0.891903 | +0.007410 | City 15 |
| C | 3 | 3 | 47,990 | 0.922367 / 0.815481 | 0.917997 / 0.817111 | +0.004370 | City 15 |
| E | 1 | 1 | 372 | 0.376830 / 0.006531 | 0.218971 / 0.002162 | +0.157859 | road 19 |
| E | 2 | 2 | 41,520 | 0.289901 / 0.005646 | 0.165333 / 0.002759 | +0.124569 | road 19 |
| E | 3 | 2 | 48,000 | 0.290163 / 0.005986 | 0.144557 / 0.002701 | +0.145606 | road 19 |
| F | 1 | 1 | 408 | 0.931528 / 0.808538 | not admitted | n/a | maritime trade |
| F | 2 | 2 | 47,163 | 0.694473 / 0.567358 | not admitted | n/a | road 31 |
| F | 3 | 1 | 47,991 | 0.931528 / 0.808538 | not admitted | n/a | maritime trade |
| G | 1 | 1 | 492 | 0.055132 / 0.003861 | 0.052047 / 0.003633 | +0.003084 | maritime trade |
| G | 2 | 2 | 40,510 | 0.042491 / 0.004686 | 0.027230 / 0.002345 | +0.015261 | road 56 |
| G | 3 | 2 | 47,998 | 0.033366 / 0.004143 | 0.024452 / 0.001834 | +0.008913 | road 56 |

### Root admission and arbitration

- C: both roots were retained at all depths. City was pre-truncation rank 1
  with 9,612 allocated nodes at the deeper cap; Road Building was rank 3 with
  9,600. Neither decisive-plan, exact-family, nor safety arbitration replaced
  the search result. City's prior was only 0.1005 versus Road Building's
  0.7243, so the small City mean advantage was not caused by the prior.
- E: both roots were retained. City was pre-truncation rank 2 with 8,388
  allocated nodes; road 19 was rank 6 with 3,204. No arbitration replacement
  occurred. Road 19 therefore did not win because it received more root work.
- F: road 0 was retained at pre-truncation rank 12 with 960 allocated nodes,
  while road 1 was rank 15 and pruned as `BranchTruncated` at every depth. This
  is a real root-admission limitation, but it yields no paired value comparison
  and does not recur in the unrelated pairs.
- G: both roots were retained. Road 17 was rank 3 with 8,388 allocated nodes;
  road 20 was rank 8 with 3,204. No arbitration replacement occurred.

The Road Building exact family was applicable in C and identified the frozen
16/22 action, but `exactFamilyReplacement` remained null. It therefore did not
override City or explain City's small mean advantage.

### Paired follow-up exposure

| State/order | Paired second action | Legal/admitted? | Rank by cap 1/2/3 | Selected by normal continuation? |
| --- | --- | --- | --- | --- |
| C, City first | Road Building 16/22 | yes/yes | 2 / 2 / 2 | no; EndTurn selected |
| C, Road Building first | City 15 | yes/yes | 1 / 1 / 1 | yes at every cap |
| E, road 19 first | City 15 | yes/yes | 3 / 3 / 3 | no; City 23 selected |
| E, City 15 first | road 19 | yes/yes | 2 / 2 / 2 | no; EndTurn selected |
| F, road 0 first | road 1 | yes/yes | 9 / 11 / 5 | no |
| F, road 1 first | road 0 | yes/yes | 9 / 10 / 5 | no |
| G, road 17 first | road 20 | yes/yes | 5 / 5 / 8 | no |
| G, road 20 first | road 17 | yes/yes | 3 / 2 / 2 | no |

Every paired second action was visible to the standalone post-first-action
search. Production recursion for the controlled player retains its single top
observation-ranked continuation, so selection—not mere admission—determines
whether the frozen second action is exposed inside the native first-root
subtree. C is the only pair with one-sided exposure: City is selected after
Road Building, while Road Building is rank 2 behind EndTurn after City. In E,
normal continuation chose a different City vertex after road first and EndTurn
after City 15 first. E's large root gap therefore compares two differently
continued subtrees rather than two forced orderings of the same action pair.

### Utility boundary and deterministic continuation

| State | Utility after A first | Utility after B first | Common post-sequence utility | Post-state search identical? |
| --- | ---: | ---: | ---: | --- |
| C | 58.611275 | 51.160690 | 59.000790 | yes |
| E | 32.589775 | 40.163548 | 39.186195 | yes |
| F | 42.891869 | 42.891869 | 42.345821 | yes |
| G | 30.435383 | 30.242640 | 30.092056 | yes |

The evaluator gives exactly one common utility at each identical post-sequence
state. From each shared post-state, the deterministic continuation search with
seed `9542001` produced byte-for-byte equal structured results for A and B:
same state/observation hashes, recommendation, ranked candidates, values,
priors, planner evidence, node count, completed depth, and pruning counts.
This is a bounded search reproducibility check, not a terminal campaign.

### State C diagnosis

The canonical pair is mechanically equivalent: both orders end at full hash
`c38c8b0fb8b8ca67`, observation/public hash `b22a4dfd17e81ad1`, and utility
59.000790. The M1 root gap (City 0.995628 versus Road Building 0.994569) is
consistent with the small mean gaps in this fixed-budget rerun, but it is not a
hidden-follow-up failure:

- both roots are retained with essentially equal deep node allocation;
- both paired follow-ups are legal and admitted by standalone follow-up search;
- Road Building first exposes City as rank 1 and selects it at every depth,
  while City first does not expose rank-2 Road Building in the native
  top-one controlled continuation;
- the higher Road Building prior does not force the final root;
- City has only a 0.000313 depth-1 mean advantage, and the lower-confidence
  ordering reverses in Road Building's favor at depths 2 and 3;
- no planner/exact/safety replacement changes the chosen root.

The mean gap changes from +0.000313 to +0.007410 to +0.004370 rather than
shrinking monotonically. The one-sided exposure mechanism favors representing
Road Building then City, yet the root mean weakly favors City first; at depths
2 and 3 the LCB ordering favors Road Building. The observed M1 preference is
therefore not caused by hiding City after Road Building. It is small relative
to the confidence spread and is best classified as local finite-budget subtree
valuation, not a material action-order defect.

## Checkpoint 4 — interpretation

### Ownership findings

No common defect owner was found.

- Evaluator: not the owner. Each pair has exact `GameState` equality and one
  common strategic utility after the second action.
- Depth horizon: not the owner in these states. Same-turn recursion in
  `catan-search/src/depth.rs` increments depth only when the turn/current player
  changes, while separately bounding actions in a turn.
- Controlled-continuation ranking is the owner of C's one-sided sequence
  exposure. `recursive_observation_best_policy_action` selects the top
  observation-ranked action and truncates to one: this includes City after Road
  Building but excludes rank-2 Road Building after City. No second unrelated
  pair reproduced that one-sided frozen-sequence exposure, and its direction
  does not explain City's small root-mean lead.
- Root prior: not a common owner. In C, the much larger Road Building prior
  lost by a tiny mean margin; in E and G, allocation and value ordering do not
  reduce to the prior alone.
- Planner/exact-family arbitration: not a common owner. No frozen root was
  selected through decisive-plan, exact-family, or safety replacement. The
  exact Road Building result in C was diagnostic evidence, not an override.
- Normal recursive continuation is the main explanation for E: native root Q
  values follow the search-selected continuation and did not follow either
  frozen E sequence. This is expected search semantics, not evidence that two
  identical post-sequence states are valued differently.
- Branch truncation owns the local F coverage failure: one of two equivalent
  parameterized roads was rank 15 under a branch cap of 12. Because the paired
  follow-up remained admitted in both directions and no unrelated state showed
  the same failure, this is a local diagnostic limitation rather than the
  required repeated action-order mechanism.

### General-defect bar

| Requirement | Result |
| --- | --- |
| Same mechanism in at least two unrelated valid states | **fail** — only C has one-sided frozen-follow-up exposure; E uses different continuations, F is root truncation, and G is parameter-specific road ordering. |
| Identical post-sequence full states | pass in all four states |
| Common search/planner owner | **fail** |
| Materially above normal numerical/search uncertainty | **fail as a repeated property** — only E is large, and it does not compare the frozen completed sequences. |
| Greater depth consistently reduces or explains the gap | **fail** — C, E, and G are non-monotonic; F remains pruned. |
| Valid correctly handled hard negative/control | pass — C's represented Road Building→City order does not receive a material or stable advantage; its deeper LCB ordering reverses the small mean result. |

The fact that A's mean exceeds B's in C, E, and G is not a shared direction:
the manifest's A label denotes City, road 19, and road 17 respectively. It has
no common action-family or sequence-position meaning.

### Hypotheses falsified or bounded

- Falsified for this corpus: identical post-sequence states acquire different
  evaluator values or different deterministic continuation results.
- Falsified for State C: the City-first preference exists because the City
  follow-up after Road Building is hidden by depth, admission, or exact-family
  arbitration. City is the top continuation after Road Building at every cap.
- Not supported: a generic first-action-family advantage, a Development Card
  ordering penalty, or a systematic prior-driven action-order bias.
- Locally observed but not generalized: branch truncation can exclude one
  parameterization of a commutative same-family pair (F).
- Not tested by native root Q alone: the value of a *forced* two-action macro
  sequence. Native root values intentionally allow different follow-ups, as E
  demonstrates.

### Decision

**NO GENERAL ACTION-ORDER DEFECT ESTABLISHED**

The four mechanically valid states satisfy the equivalence premise, and the
post-boundary evaluator/search behavior is exactly reproducible. Before that
boundary, the differences do not repeat under a common mechanism, do not
consistently shrink with depth, and include one large comparison whose normal
continuations are not the frozen pair. M2 therefore does not justify an action-
order heuristic, evaluator change, planner change, or production M3.

### Recommended next experiment

If this question is revisited, the next narrow experiment should be a
sequence-conditioned search-backup study, not a terminal campaign: add a
research-only macro-root probe that forces the two concrete same-turn actions,
then backs up the identical post-state under the same particles and node
budget. Compare that conditional value with the native optimized first-action
Q in C and E plus one newly observed cross-family pair. This directly measures
representation loss without conflating it with the alternative continuations
seen in E. F's branch-cap sensitivity should remain a separate parameterized-
action coverage question.

### Artifacts and verification

- Frozen manifest: `benchmark-results/jev-lab/m2-action-order/candidate-manifest-v1.json`.
- Four search-free discovery logs and 12 fixed-depth diagnostic logs are in the
  same ignored artifact tree.
- `artifact-sha256.txt` records SHA-256 checksums for the manifest, discovery
  logs, diagnostics, and source-research note.
- Harness unit tests: `cargo test -p colonist-catan-arena --bin jev-strategy-lab`
  — 8 passed.
- Release harness build: `cargo build --release -p colonist-catan-arena --bin
  jev-strategy-lab` — passed.
- All 12 diagnostic files passed assertions for one header/decision/audit,
  frozen depth and budget, frozen pre/post hashes, exact equality, zero differing
  fields, and identical matched continuation structures.
- `sha256sum --check` passed for all 18 entries in `artifact-sha256.txt`.

Only the research lab binary and this report changed from the M2 starting
commit. No production strategy, evaluator, search-policy, WASM, worker, or
frontend file changed.
