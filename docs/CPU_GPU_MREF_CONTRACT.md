# CPU reference and CUDA Mref integration

**User requirement clarified — 2026-09-08:** acceptance is governed by soundness and robustness, not a minimum speedup. The former 2× threshold is superseded. Preserve same-policy parity, evidence validity, cancellation/recovery, stale-result rejection, safe trade execution, and packaged recommendation-to-confirmed-transition verification. Report performance as diagnostic evidence; sub-2× speedup is not a failure or a reason by itself to block a backend. This clarification does not itself enable production routing or establish playing strength.

## Reference baseline

The current stabilization candidate policy is `deep-maxn-v14`. The saved acceptance artifacts describe the earlier `deep-maxn-v13` working tree based on `d77935fda984e3006b8a8ad9d65eb58a6d162eaf`; they do not certify v14. The older R6 pin `950c1ae9af224f802ad266f268fe7c2f9ff2fe38` remains historical provenance only.

`deep-maxn-v13` keeps the existing `catan-core` stochastic mathematics and public-history posterior contract, while incorporating the repaired controlled-player continuation, current root/admission semantics, and the shared fixed-work/deadline contract. Any future semantic change to those decision rules requires a new reference identity and rerunning the affected parity gate.

Identities are deliberately separate:

| Component | Identity |
| --- | --- |
| Current CPU candidate policy | `deep-maxn-v14` |
| Exact CUDA backend for the same policy | `deep-maxn-cuda-exact-fixed-work-v1` |
| Experimental native rollout algorithm | `gpu-root-rollout` |
| Legacy stochastic model | `m0-fair-iid-2d6-v1` |
| Balanced-Dice reference hypothesis | `mref-colonist-linked-2024-v1` |
| Public-history posterior policy | `public-history-belief-v1` |
| Native messaging protocol / state schema | `7` / `3` |
| Internal CUDA state/reduction ABI | `3` |

Mref is a named public/reference hypothesis, not a reconstruction of Colonist's
hidden server deck or proof of current live-server tuning.

## Ownership and routing

`GameSession.diceHistory` owns public roll evidence. The live constructor and
search adapter bind stochastic actors to the same canonical player ordering.
Unusable Balanced history fails closed; it does not become an M0 request.

Rust resolves the public evidence into one canonical posterior before creating
private resource-world states. `CudaSimPackedState` packs that full posterior
into each resident state. CUDA prepares and conditions it with every simulated
roll. It does not draw every future roll from a static root distribution, sample
a hidden controller into actor-facing authority, or multiply TypeScript resource
worlds by a separate dice-world population.

Production browser decisions route eligible midgame Deep MaxN requests to `deep-maxn-cuda-exact-fixed-work-v1` when an installed companion satisfies capability checks, with CPU/WASM remaining the packaged default, opening owner, and fallback. Companion availability cannot substitute `gpu-root-rollout`; that algorithm remains explicit experimental tooling. Opening remains unsupported by exact CUDA and therefore stays on the same-policy CPU opening path.

Native `hello` is bounded to 2,000 ms. Silent initialization and transport
disconnects use the same-policy CPU/WASM path; semantic/protocol/model mismatches
remain errors. Decision identities include tab, document, frame, and client
request ID, with a separate native correlation ID. One request owns native
search at a time; concurrent requests use WASM with their stochastic authority
preserved. A weighted request in another document cannot release the active
native owner's port. Cancellation received during initialization prevents that
request from subsequently starting native search.

Readiness queries have a separate 12-second safety limit and can retry after
failure. A delayed readiness response cannot overwrite the evidence gate or
runtime result of a newer board decision. These are transport/lifecycle fixes,
not new CUDA parity or live win-rate evidence. See the
[September 17 follow-up review](ENGINE_TIMEOUT_AND_GPU_REVIEW_2026-09-17.md#follow-up-review-and-fixes)
for regression coverage and the remaining discard-state limitation.

For offline CPU/exact-MaxN parity, explicit `effort.decisionTimeMs: 0` (or legacy `timeBudgetMs: 0` when no effort object is supplied) disables the wall-clock deadline and evidence escalation. Node and depth limits remain active, and native cancellation remains supported. Positive decision times retain the 50–10,000 ms normalization; omitted effort retains the existing defaults. Live requests supply positive budgets. Experimental rollout and other modes retain their positive time floor. Parity tooling must assert the returned zero decision time and matching effective effort before comparing fixed work.

For timed MaxN, the search budget is a cooperative target, not a promise of an
exact elapsed-time ceiling. On 2026-09-08 Hamza explicitly prioritized robust,
fully arbitrated results over discarding completed work at that target. Root
preparation and the complete one-ply table may overrun it; final safety/exact-family
arbitration must still finish and report `deadlineReached=true` if the total
target expired. Explicit cancellation remains authoritative. The browser's
separate 12-second safety limit, supersession and board-signature checks still
reject late/stale work. This does not increase search depth/node budgets or
promote a larger production time profile. Requests that never establish valid
decision evidence can still fail closed.

Exact CUDA accepts the same M0/Mref posterior input, root exclusions, trade rules and explicit `adaptive-candidate-admission-v1` identity as CPU MaxN. The rollout parser still rejects the adaptive strategy policy rather than silently pretending support. An old rollout-only companion cannot satisfy the protocol-7 exact capability check.

## Mechanical parity versus strategic parity

The original parity gate establishes **mechanical parity**: shared rules and state
transitions, stochastic/Mref law, terminal semantics, packed-state integrity, and
the required observation boundary. That remains necessary, but it is not enough to
show that CUDA is functioning as an acceleration backend for the same Strategist.

**Algorithm/backend parity** is the production acceleration contract for CPU MaxN versus exact CUDA. With identical canonical particles and fixed work it requires the same legal/root domain, retained roots and priors, continuation semantics, node schedule, final authority and chosen action, with backed-up values equal within the declared numerical tolerance. A substantive disagreement blocks exact-backend promotion.

**Strategic parity** remains the diagnostic contract for comparing different algorithms such as CPU MaxN versus `gpu-root-rollout`. For one identical adapter observation and joint posterior, compare the strongest common semantic layer rather than requiring bitwise equality between different search algorithms:

- identical observation and stochastic-model identity;
- comparable posterior/effective particle populations;
- baseline candidate-domain and retained-root overlap;
- protected/promoted/admitted roots and their provenance;
- controlled-player continuation meaning: deterministic best action under each
  backend's observation-safe policy, while opponents retain weighted modeled play;
- root ordering and chosen-action agreement/disagreement;
- completed depth/rollout work, deadline/cutoff state, and any exact/safety final
  replacement;
- the explicit strategy-policy identity.

CPU Deep MaxN and native GPU rollouts still use different policy/evaluator value scales and search mechanics. A numeric value correlation is therefore not part of the rollout comparison. Persistent root-coverage or action-direction disagreement under the same observation is a strategic-parity finding even when mechanical parity is green; exact action equality on every rollout decision is not required. This looser rule does **not** apply to exact CUDA, whose fixed-work parity gate requires the same MaxN computation within tolerance.

Historical v13 exact evidence is frozen under `docs/benchmarks/engine-stabilization-2026-09-07/`: evaluator parity passed 69 states across 2P/3P/4P with maximum absolute error `2.9802322e-7`; fixed-work search parity passed 15/15 action comparisons with identical node/depth work and maximum absolute error `5.9604645e-7`; the production-shaped native-host lane matched M0+trades, Mref and explicit M2 requests within `3.8743019e-7`, while also rejecting invalid evidence and recovering from cancellation. These results do not certify the current v14 orchestration. The recorded v13 arena smoke measured 1.370× for 3P and 1.272× for 4P, below a historical threshold that is now superseded. None of these results establishes playing strength or authorizes promotion.

### Current decision-pipeline map

The table below intentionally compares the distinct rollout algorithm to CPU MaxN and the resident rollout benchmark. Exact CUDA is **not** a fourth strategy column: `analyze-exact` is required to reproduce the packaged CPU/WASM MaxN column under the algorithm/backend parity contract, with GPU evaluation as a computation backend. Its currently declared exceptions are opening placement and promotion/robustness status, both of which route or remain on CPU.

| Surface | Packaged CPU/WASM Deep MaxN | Native GPU rollout diagnostic (browser routing disabled) | `gpu-sim-agent-benchmark` |
| --- | --- | --- | --- |
| Input information | Adapter actor-visible state plus joint hidden-world posterior | The same serialized request/posterior for eligible native decisions | One resident authoritative `GameState`; no live posterior reconstruction |
| Posterior handling | Rust belief particles; weighted belief MaxN | Posterior normalization/allocation across resident CUDA states | None: one realized state per simulated game |
| Root generation | Observation-safe actor proposal domain plus exact/mandatory handling | Observation-safe proposal domain across posterior particles | Resident benchmark root sampling |
| Root admission | Baseline quota/root cap; optional `adaptive-candidate-admission-v1` only when explicitly requested | Exact-family collapse, verified blockers, spatial/closeout promotion and bounded native admission; adaptive strategy policy unsupported | Benchmark-specific sampled roots; no production admission contract |
| Safety filters | Exact mandatory/tactical ownership, root exclusions, exact-family and final safety arbitration | Exact mandatory/tactical ownership, forced-loss checks, domestic-trade hard vetoes, exact-family and final safety arbitration | Rules legality only; no live trade-hard-veto/final safety pipeline |
| Controlled-player continuation | Pre-quota argmax of the observation-safe policy score | Greedy maximum weight in native CUDA rollout policy | Searched candidate roots followed by fixed-step `gpu-weighted` continuations |
| Opponent continuation | Top observation-safe quota mixture | Weighted observation-safe native rollout policy | Lightweight `gpu-weighted` opponents/continuations |
| Chance/Mref | Authoritative chance nodes; M0 and Mref supported | Authoritative packed chance law; M0 and Mref supported | Resident simulator's benchmark stochastic stream; no live Mref posterior input |
| Horizon/budget | Iterative bounded MaxN depth/node/deadline effort | Rollout budget/horizon, posterior allocation, adaptive racing/escalation, deadline | Fixed benchmark root samples × rollouts × continuation steps |
| Opening | CPU/WASM owner | Routed away from native GPU | Simulated by benchmark policy/search rather than the extension's opening route |
| Domestic/maritime trade | Full rules plus adapter exclusions/safety and final arbitration | Full rules plus native domestic-trade hard-veto path and arbitration | Controlled by benchmark trade toggle and resident policy; not live trade safety |
| Final arbitration | Exact-family/safety replacement after search | Exact-family, forced-loss/safety, and safer-EndTurn arbitration after GPU evidence | Benchmark search winner; no live post-search arbitration |
| Strategy policy | Baseline plus explicit M2 experimental admission | Baseline only; M2 request rejected/routed to CPU/WASM | No adaptive-strategy policy contract |
| Output owner | Packaged WASM `analyze` response and current browser recommendation | Native companion `analyze` response used by diagnostic/benchmark tooling | Arena benchmark JSON, not a browser recommendation |

`scripts/benchmark-gpu-strategic-strength.mjs` now exercises packaged WASM and the
native production analyzer on the same constructed request and emits a
`same-observation-cpu-gpu-strategic-parity-diagnostic` block. It reports an
adapter observation digest, posterior identity/counts, candidate and retained-root
overlap, ordering, chosen actions, work/deadline state, and final replacements.
The report marks `productionEquivalent: false` and `browserWinRateEstimate: false`:
it is a production-pipeline decision diagnostic on one captured fixture, not a
whole-game strength estimate.

The A2 D68 diagnostic found a real strategic-parity gap under the current adapter
request. CPU/WASM and native GPU saw the same 12-root candidate domain and the same
8-particle posterior/effective sample size, but only 4 of 12 roots retained the same
rank; mean absolute rank delta was 1.5 and the maximum was 4. At a 96-step native
rollout horizon, CPU chose the strong eastern road while native GPU chose the
historical weak road. At 48 rollout steps, both chose the strong road. Native GPU
reported `deadlineReached` in both probes, so the deadline flag alone does not
explain the reversal. The persistent ranking difference is source-backed: CPU
belief-root ordering includes plan-adjusted priors plus quota-score aggregation,
while native GPU begins from plain observation-safe shared priors before its own
promotion/admission path. The 48/96 result shows that this ordering mismatch is not
sufficient by itself to force action disagreement; native rollout horizon/valuation
also materially affects D68. The exact internal cause of the 96-step reversal
remains an unresolved strategic-parity question, not evidence that GPU is stronger.

The protocol-7 native `hello` response reports `stochasticModels` and an explicit algorithm capability set. `exactMaxn` identifies `deep-maxn-cuda-exact-fixed-work-v1`, availability, fixed-work-only promotion status, cancellation/deadline support and opening support. Production-facing exact clients require that capability; an old protocol-6/rollout-only companion is not silently treated as an exact accelerator. Native clients reject absent/different Mref identity, unknown models and unusable evidence.

## CUDA posterior contract

The existing game-state fields 0 through 403 are unchanged. Fields 404 and 405
hold the stochastic model and particle count; field 406 starts 64 controller slots
of 28 words each. The total resident layout is 2,198 `u32` words per lane.
M0 has zero-valued additional fields and preserves its previous two-die RNG draws.

Each particle stores its 64-bit mass, remaining total counts, deck size, recent
five totals, initialized-player mask, seven counts, seven-streak owner/count, and
prepared actor. Particle masses sum to `2^32`.

`engine/crates/catan-search/src/cuda/mref.cuh` implements the same rational law
as `engine/crates/catan-core/src/dice.rs`.
CUDA uses 128-bit integer intermediates and largest-remainder normalization.
Outcome ties use ascending total; particle ties use canonical controller order.
Preparation and conditioning coalesce equivalent controllers and preserve the
CPU's floor-before-particle-renormalization behavior. Retired slots are zeroed.
The full public posterior survives descendant state cloning.

`dice_distribution_kernel` exposes probability vectors for verification. Normal
search uses the same distribution and transition functions. The CUDA ABI probe
checks the version, state width, action width, and 12-word reduction contract
before resident buffers are used.

`build-cuda-ptx.py` and the Rust build script hash `sim.cu`, `rollout_cutoff.cuh`,
`mref.cuh`, and the NVRTC options together. Rebuild PTX after editing any of those
sources. Native Mref requires NVRTC device-128-bit support when rebuilding the
artifact; running the packaged companion uses the embedded PTX and driver.

## Post-integration live-game repair

The later live-game repair changes native root arbitration and its rollout
horizon. It does not change the pinned CPU rules, M0/Mref probabilities, packed
state ABI, or strategic leaf formula. The earlier independent GPU verdict
applies to its reviewed commit range, not automatically to this repair.

Strategic root evaluation advances to a common absolute turn relative to the
pre-action state. The existing effort setting and exported horizon retain their
legacy units: `ceil(horizon / 4)` is the number of completed turns. Building or
trading before ending a turn cannot reduce opponents' simulated opportunities.
Cancellation remains checked between groups of at most four turns; a runaway
micro-action sequence is an error, not a successfully evaluated shorter leaf.
The zero-horizon CPU/CUDA leaf comparison still applies only the root action.

Terminal bounds include the entire unresolved outcome mass plus a bounded
sampling interval. Global evidence tiers govern both racing and final ranking;
overlapping terminal bounds defer to the strategic leaf and VP margin. Fewer
observed terminal losses cannot break an otherwise equal strategic comparison.
These are search uncertainty bounds, not calibrated live win probabilities.

Indexed session events are replayed in log order; synthetic observations keep
their observed log watermark. Semantic hydration updates the same event slot.
Bot board observations and returning log rolls merge only when the indexed
prefix proves their common ordinals; contradictions and unresolved alignment
keep Balanced Dice unavailable. Compact records retain source/index metadata
in `@eventEvidence`, keyed by the existing event anchors.

Trade workflows share retry deadlines: missing controls resolve or fail within
about 1.2 seconds at that step, step completion is bounded at four seconds, and
the transaction is bounded at twelve seconds. Failure cancels the workflow and
requests a fresh board/replan.

## Verification

Run from the repository root, on a CUDA-capable machine:

```sh
python3 scripts/build-cuda-ptx.py --check
cargo run --release --manifest-path engine/Cargo.toml \
  -p colonist-catan-search --features cuda-sim --bin cuda-sim-mref-parity
cargo run --release --manifest-path engine/Cargo.toml \
  -p colonist-catan-search --features cuda-sim --bin cuda-sim-parity
cargo run --release --manifest-path engine/Cargo.toml \
  -p colonist-catan-search --features cuda-sim --bin cuda-sim-generated-parity
npm run build:companion
node scripts/verify-mref-native.mjs
npm run check
npm run build
```

`cuda-sim-mref-parity` compares every packed descendant field and exact dice
vectors across 2-, 3-, and 4-player M0/Mref states, including uncertain posteriors,
reshuffle crossings, 15 VP, Friendly Robber, and discard limit 9. It also executes
strategic root expansion, 96-step rollouts, and reduction.

The native integration gate constructs real adapter requests on the D68 board
fixture with explicitly synthetic dice evidence. It checks protocol capabilities,
M0 and Mref strategic execution, CPU/WASM posterior digest agreement, rejection of
unknown provenance, cancellation, and post-cancellation host responsiveness.
It is not a historical D68 recommendation benchmark.

Observed on the RTX 3070 Ti during implementation:

| Gate | Result |
| --- | --- |
| Mixed M0/Mref full-state transitions | 11,520 matched |
| Exact dice vectors | 3,088 matched |
| Multi-particle lanes in that gate | 12 |
| M0 direct transitions | 16,384 matched |
| M0 GPU-generated transitions | 20,544 matched |
| Native M0 search | 384 rollouts; about 4.05 seconds |
| Native complete-history Mref search | 312 rollouts; about 4.26 seconds |
| Native 64-particle suffix Mref search | 280 rollouts; about 4.03 seconds |

Rollout counts and elapsed time depend on deadline scheduling. Identical final
strategic actions are not the mechanical parity criterion: CPU Deep MaxN and GPU
rollouts remain different search policies over the same game-model semantics.
Strategic parity is assessed separately with the same-observation contract above;
a disagreement must be classified as expected algorithmic variation, root/admission
mismatch, continuation-policy mismatch, deadline/cutoff effect, or final-arbitration
replacement rather than being hidden by a green transition-parity result.

### Startup and packaging

The first uncached driver compilation of the new PTX took about 102 seconds in
this environment. A subsequent native handshake took about 353 ms. Run the
native verification gate as installation/build warmup before entering a live
game; a driver or PTX update can invalidate that cache. Do not confuse this cold
native compilation with the separate cold packaged-WASM smoke requirement.

The built extension is `dist/`; the companion is
`engine/target/release/colonist-assistant-gpu`. Building does not reload Chrome,
replace an installed Windows/WSL launcher, or install native-host registration.
Keep extension and companion artifacts from the same candidate together.

## Claims still requiring separate evidence

This contract and its parity gates do not prove live Balanced-Dice model
adequacy, autonomous target-game readiness, or competitive superiority.
Independent review and live model evaluation remain separate from the user's
decision to proceed with implementation before those reviews return.
