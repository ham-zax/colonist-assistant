# Road-intent F1 admission validation — 2026-09-20

## Status

**REJECT/HOLD. Do not admit the deadline-limited road-intent override to F2.**

Two independent admission requirements fail:

1. The exact hill6758 D27 trigger does not reproduce on the candidate carried onto current local main. With deterministic node budgets and zero wall-clock cutoff, the reconstructed D27 root prefers Knight rather than the historical road `e:2,-2,2`. The real candidate road `e:1,-1,2` also has a worse current-main lower-confidence value than `e:2,-2,2`, so it fails the gate's LCB guard even if the historical road is treated as the current winner.
2. Four forced-root common-random-number continuation pairs are mixed rather than causally dominant. Edge 71 produces one candidate-only P3 win, edge 53 produces one baseline-only P3 win, one pair is a P3 win under both roots, and one pair is a P3 loss under both roots. Mean candidate-minus-baseline terminal actor VP is +0.5, but the mission explicitly disallows admission on this split/tied pattern.

F1 made no production-strategy change, did not tune the candidate, did not rebuild frontend/WASM packaging, and did not modify the preserved `wip/road-intent-deadline` worktree.

## Scope and revisions

- Agent: **F**
- Mission: **F1 — Road-Intent Admission Validation**
- Validation worktree: `/home/hamza/repo/colonist-road-intent-validation`
- Branch: `agent/f1-road-intent-validation`
- Starting candidate HEAD: `2d2912538b869ea7efa5c599a1e0a43113ba557c`
- Current local-main base: `9aee018fe1c8522870ed9961740946a943f75109`
- Preserved original candidate: `wip/road-intent-deadline @ 2592cafdaa31711974753755d0ba5d610ccd688a`
- Primary live evidence: `/mnt/c/Users/Hamza/Downloads/colonist-evidence-hill6758-2-2026-09-19T07-49-29-720Z (2).txt`

The candidate at `2d29125` already contains the road-intent production override and an inherited generated WASM binary. F1 treated that binary as stale packaging and did not rebuild or validate it. The only F1 source mutation is the offline `jev-strategy-lab` research harness needed to represent a frozen midgame information set, force one root, preserve event-family CRN streams, and emit terminal route metrics.

## D27 reconstruction fidelity

The reconstruction freezes frame 89, after P3 rolled 8 and before the road phase at frame 90 and historical road placement at frame 91.

Exact public/current-state evidence recovered from the export:

- decision: D27
- turn: 31
- actor: P3
- phase: Main
- player trading: disabled
- domestic-trade disabled mask used by the lab: all four seats
- roll: 8
- robber: `h:0,-2`
- bank: `[14,16,16,13,16]`
- P3 private hand: `[1,1,0,2,2]`
- public VP: `[2,3,4,2]`
- public hand counts: P0/P1/P2/P3 = 5/5/4/6
- P3 playable development card: one Knight
- board: 11 settlements, 13 roads, no cities
- settlements by player: `[2,3,4,2]`
- roads by player: `[3,3,4,3]`
- 24 pre-D27 Balanced-Dice gameplay rolls using `mref-colonist-linked-2024-v1`; the prefix ends with P3's roll of 8
- 23 B17 resource-belief worlds, with exported weights and hand vectors preserved
- nine legal BuildRoad roots:
  `e:1,1,2`, `e:0,2,0`, `e:1,1,0`, `e:2,-1,2`, `e:2,-1,0`, `e:2,-2,2`, `e:1,0,2`, `e:1,0,1`, `e:1,-1,2`
- Knight and EndTurn are also legal roots

Internal edge mapping from the exported board is important:

- historical road `e:2,-2,2` = edge 53
- `e:1,0,1` = edge 69
- requested candidate `e:1,-1,2` = edge 71
- target `v:1,-1,1` = vertex 53

The existing candidate regression test uses edge 69 as its synthetic "stronger" road. It therefore exercises the gate predicates but does **not** reproduce the requested D27 candidate coordinate, which is edge 71.

Recorded live road-intent evidence:

| Root | Edge | Target | Roads left | ETA | Survival | Target value | Portfolio | Frontier | Ordering |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| historical `e:2,-2,2` | 53 | `v:1,-1,1` | 0 | 14.4 | 0.995 | 8.7885 | 3.7085 | -0.5078 | 1.5809 |
| candidate `e:1,-1,2` | 71 | `v:1,-1,1` | 0 | 14.4 | 0.995 | 8.7885 | 5.7278 | 0.6028 | 3.2822 |

The candidate also carries recorded `critical-expansion-protection` evidence.

The export does not make every hidden variable recoverable. P0 has one hidden development card, but its identity is not preserved. The lab keeps the exact combined hidden development-card pool and re-determinizes opponent development identity exchangeably before search instead of inventing a fixed card type. Opponent policy-profile bytes are reconstructed from the exported B17 archetype posteriors using the production quantization. These are explicit reconstruction limitations.

The live evidence itself is marked partial and not benchmark-eligible because earlier card history was unavailable and the tracker recorded a state warning. `unmatchedRelevant=0`. The export contains 25 indexed gameplay rolls in total, but the 25th is P0's roll of 5 on turn 32 after D27; the exact D27 prefix is the first 24 rolls, ending with P3's 8 at log index 161. F1 initially included that post-root roll, detected the mismatch during replay, corrected `reconstruct_d27.mjs` from `.slice(0, 25)` to `.slice(0, 24)`, regenerated the frozen state, and invalidated all evidence produced from the contaminated reconstruction.

Research reconstruction artifacts:

- `benchmark-results/jev-lab/road-intent-f1/reconstruct_d27.mjs`
- `benchmark-results/jev-lab/road-intent-f1/hill6758-d27-state.json`
  SHA-256 `2afc8f023631b95218e14b5b3009b7fb0ec9a152940cbf4e0f0d102a90553605`
- `benchmark-results/jev-lab/road-intent-f1/hill6758-d27-state-summary.json`
  SHA-256 `1fbfdb29cbb1eedcd223981f9039ba0e798997638a9a6392b44e750239f77112`

## Gate reproduction

### Gate mechanics

The focused candidate regression passes:

```text
cargo test -p colonist-catan-search hill6758_d27_deadline_limited_search_uses_same_target_road_dominance -- --nocapture

1 passed; 0 failed; 239 filtered out
```

That test proves the implementation's synthetic gate behavior:

- deadline + completed depth 1 can select a same-target road with no-worse legality/LCB/roads/ETA/survival/target value and strictly better portfolio/frontier;
- no deadline leaves the completed search winner authoritative;
- completed depth > 1 leaves the completed search winner authoritative.

It does not reproduce the true edge-71 D27 case because its synthetic stronger action is edge 69.

### Current-main D27 deterministic replay

The corrected D27 information set was searched with a deterministic 4,740-node budget, zero wall-clock cutoff, branch cap 10, 23 exported posterior resource worlds, and all 23 strategic particles.

Artifact:
`benchmark-results/jev-lab/road-intent-f1/corrected-v3/d27-root-4740.jsonl`
SHA-256 `2019c7fbde207325d7cde497627a905030c9422fc2ffdabe08d10585c0867261`

Result:

- state hash: `af247102fe7f32c6`
- nodes: 4,734
- completed depth: 1
- deadline reached: false
- search winner: `PlayKnight { hex: 12, victim: Some(2) }`
  = live coordinate `h:0,-1`, victim P2
- candidate edge 71 actor value: 0.01204099
- candidate edge 71 LCB: 0.00060604
- historical edge 53 actor value: 0.00260694
- historical edge 53 LCB: 0.00183326

The required historical baseline choice `e:2,-2,2` is therefore **not reproduced**. Because the current winner is Knight, the road-intent override is not eligible to fire. Even if edge 53 is treated as the provisional winner, edge 71 has a materially lower current-main LCB and fails the no-worse-LCB predicate.

Current-main road features remain directionally similar to the live evidence but drift slightly:

| Root | Portfolio | Frontier | Ordering |
| --- | ---: | ---: | ---: |
| edge 53 | 3.67350 | -0.50568 | 1.58025 |
| edge 71 | 5.72785 | 0.62421 | 3.31103 |

ETA, survival, target, roads remaining, and target value remain equal between the two roads. The changed search winner and LCB are the admission-relevant differences.

**Gate reproduction verdict: failed.** F1 cannot truthfully claim that current-main reproduces the exact D27 deadline/depth-1 baseline followed by an edge-71 override.

## Forced-root matched continuation

Only the root differs within each pair:

- baseline: force `BuildRoad { edge: 53 }`
- candidate: force `BuildRoad { edge: 71 }`
- same corrected 24-roll pre-root information set
- normal depth-3 continuation policy afterward
- branch cap: 10
- strategic particles: 24
- ordinary node budget: 2,000
- opening node budget: 2,000
- trade-response node budget: 1,200
- opening and trade-response wall-clock budgets: 0
- max turn: 220
- independent continuation seeds: 9417001, 9417002, 9417003, 9417004
- separate deterministic CRN streams for roll, development-card draw, and steal event families

All eight authoritative runs completed without a cutoff. They are isolated under:

`benchmark-results/jev-lab/road-intent-f1/corrected-v3/`

`SHA256SUMS` in that directory freezes the eight arm files, three root diagnostics, and aggregate summary.

P3 terminal outcomes:

| Seed | Edge 53 baseline | Edge 71 candidate | Candidate minus baseline |
| ---: | --- | --- | --- |
| 9417001 | loss; P1 wins T93; P3 VP 6; LR length 8, holder P0 | **win; P3 wins T99; VP 10; LR length 5, holder P1** | VP +4; LR -3 |
| 9417002 | **win; P3 wins T135; VP 10; LR length 9, holder P0** | loss; P1 wins T121; P3 VP 8; LR length 5, holder P0 | VP -2; LR -4 |
| 9417003 | **win; P3 wins T143; VP 10; LR length 12, holder P3** | **win; P3 wins T103; VP 10; LR length 4, holder P0** | VP 0; 40 turns earlier; LR -8 |
| 9417004 | loss; P1 wins T121; P3 VP 9; LR length 5, holder P0 | loss; P1 wins T121; P3 VP 9; LR length 7, holder P0 | VP 0; LR +2 |

Aggregate:

- same winner: 2/4 pairs
- candidate-only P3 wins: 1
- baseline-only P3 wins: 1
- both P3 win: 1
- both P3 lose: 1
- baseline P3 results: L/W/W/L
- candidate P3 results: W/L/W/L
- mean candidate-minus-baseline P3 terminal VP: +0.5

The candidate has one clear win conversion in seed 9417001 and reaches the same win 40 turns earlier in seed 9417003. The baseline has the opposite clear win conversion in seed 9417002. Route outcomes are also mixed: edge 71 ends with a shorter longest-road length in the first three seeds and a longer one in seed 9417004. This is not causal domination.

**Matched-continuation verdict: mixed, not a coherent advantage and not causal domination.** The mission explicitly disallows admission on a split/tied four-stream result.

Aggregate artifact:
`benchmark-results/jev-lab/road-intent-f1/corrected-v3/summary.json`
SHA-256 `07641c6da3526f6d47e6aa4f886813ff4422a8e3f7983818ec27dbee96c0aa1b`

The earlier `corrected-v2/` continuation set is invalidated and excluded because its state reconstruction accidentally consumed P0's post-D27 turn-32 roll. No result from that set contributes to the admission decision.

## Stronger-search diagnostic

Two bounded, zero-wall-clock diagnostics used the corrected D27 reconstruction and current-main semantics.

At 48,000 nodes:

- artifact: `benchmark-results/jev-lab/road-intent-f1/corrected-v3/d27-root-48000.jsonl`
- SHA-256: `d59a47473636161970d791310e71dbadc86b000e2de4d8b191ac442062e859c4`
- nodes: 47,998
- completed depth: 1
- deadline reached: false
- winner: **edge 35**, live coordinate `e:0,2,0`
- edge 35 actor value: 0.02112973
- Knight actor value: 0.01381683
- edge 71 actor value: 0.01285699
- edge 53 actor value: 0.00260694

At 160,000 nodes:

- artifact: `benchmark-results/jev-lab/road-intent-f1/corrected-v3/d27-root-160000.jsonl`
- SHA-256: `0cf11f71ad74e26772679653647467b733d1fc805692bcbafd8ea3aca9dbeb7b`
- nodes: 159,981
- completed depth: **2**
- deadline reached: false
- winner: **edge 69**, live coordinate `e:1,0,1`
- edge 69 actor value: 0.01222179
- Knight actor value: 0.01125916
- edge 71 actor value: 0.00725085
- edge 53 actor value: 0.00716892
- road-intent replacement: none

The 160k run forces `EndTurn` only after the root search so the harness stops after that diagnostic decision; the search winner remains edge 69. The completed depth-2 result is authoritative under the candidate's own guard and does not converge toward edge 71.

## Negative controls

### D17 mechanically weaker road

The preserved D17 state is available in the prior C1 research artifacts at state hash `c107acdb5a6406dd`, turn 8, Main phase, actor P1.

The completed non-deadline depth-3 search uses edge 6 as its search winner. The proposed edge 21 is mechanically worse:

| Feature | edge 6 | edge 21 |
| --- | ---: | ---: |
| target vertex | 6 | 29 |
| roads remaining | 0 | 1 |
| ETA | 14.4 | 15.6522 |
| survival | 0.9950 | 0.6653 |
| target value | 6.4950 | 4.3801 |
| portfolio | 3.3188 | 3.5380 |
| frontier | 2.1401 | 0.1458 |

Edge 21 fails same-target, roads-remaining, ETA, survival, and target-value predicates. Its slightly higher portfolio cannot make it pass.

Preserved sources:

- `/home/hamza/repo/colonist-opening-validation/benchmark-results/jev-lab/c1-wave3/prior-evidence/cf-mid-d17-road6-control.jsonl`
- `/home/hamza/repo/colonist-opening-validation/benchmark-results/jev-lab/c1-wave3/prior-evidence/cf-mid-d17-road21-replay2.jsonl`

### Guard properties

Direct source inspection plus the focused regression establishes:

- worse LCB cannot pass;
- worse ETA cannot pass;
- worse survival cannot pass;
- different target cannot pass;
- more roads remaining cannot pass;
- lower target value cannot pass;
- no deadline cannot invoke the override;
- completed depth > 1 remains authoritative.

No generic road-ranker behavior was introduced or required by F1.

## Admission decision

**REJECT/HOLD.**

The candidate does not satisfy the F1 admission contract:

- exact D27 gate trigger on current-main: **failed**
- coherent forced-root continuation advantage or causal domination: **failed**
- negative controls: **pass**
- generic road-ranker expansion required: **no**

Because the first two required conditions fail independently, there is no F2 admission.

### F2 scope

**None. F2 is not authorized by this validation.** Do not tune or repair the candidate under the F1 result.

If this question is reopened later, the prerequisite is a new bounded validation that first recovers a current-main D27 root where the historical edge-53 search winner and deadline/depth condition can be reproduced without inventing hidden state. That would be a new mission, not continuation of this admission.

## Research tooling and verification

F1 extended only `engine/crates/catan-arena/src/bin/jev-strategy-lab.rs` so a research run can:

- load a midgame state spec;
- restore public Mref roll history;
- preserve exact exported resource-belief worlds;
- keep unknown opponent development identity exchangeable;
- configure deterministic branch/particle budgets;
- force one root;
- expose road-intent replacement provenance;
- record terminal route/build metrics.

Verification performed:

```text
cargo build -p colonist-catan-arena --bin jev-strategy-lab
Finished dev profile successfully.

cargo build --release -p colonist-catan-arena --bin jev-strategy-lab
Finished release profile successfully.

cargo test -p colonist-catan-search hill6758_d27_deadline_limited_search_uses_same_target_road_dominance -- --nocapture
1 passed; 0 failed.

rustfmt --check --edition 2024 engine/crates/catan-arena/src/bin/jev-strategy-lab.rs
passed.
```

A repository-wide `cargo fmt --all -- --check` is not clean on this candidate branch because pre-existing files, including candidate production/test files, differ from rustfmt output. F1 did not reformat those files because production mutation is outside mission authority. The only F1 Rust file passes the targeted formatter check.

## Evidence limitations

- The live export is explicitly partial because older card history was unavailable and a tracker-state warning was present.
- P0's one hidden development-card type is unrecoverable; the corrected lab samples that hidden identity exchangeably from the exact combined hidden pool.
- Opponent policy-profile bytes are reconstructed from exported B17 archetype posteriors rather than directly serialized bytes.
- The live source identifies itself as `main@71a8f37c4dd6+dirty`; current-main candidate semantics are not bit-identical to that dirty live build. Small road-feature drift and the changed search ordering are observed rather than papered over.
- The 48k diagnostic remained at depth 1; the separate 160k zero-wall-clock diagnostic completed depth 2 and chose edge 69 rather than either contested road.
- The four-stream continuation sample is intentionally bounded; the mission forbids treating mixed results as sufficient, so no additional streams were justified.

These limitations all bias toward HOLD, not toward inventing a positive admission result.
