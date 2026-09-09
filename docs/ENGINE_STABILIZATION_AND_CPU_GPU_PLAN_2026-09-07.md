# Engine stabilization and CPU/GPU decision contract

**User requirement clarified — 2026-09-08:** acceptance is governed by soundness and robustness, not a minimum speedup. The former 2× threshold is superseded. Preserve same-policy parity, evidence validity, cancellation/recovery, stale-result rejection, safe trade execution, and packaged recommendation-to-confirmed-transition verification. Report performance as diagnostic evidence; sub-2× speedup is not a failure or a reason by itself to block a backend. This clarification does not itself enable production routing or establish playing strength.

Date: 2026-09-07. Original investigated checkout: `main` at `dcf1a2c86ee09cc310038df7cf5f9749d84ab9da`. The implementation candidate described below was developed from `d77935fda984e3006b8a8ad9d65eb58a6d162eaf` and is recorded with dirty-working-tree artifact provenance until its final isolated commit is created.

Status: **v14 implementation candidate; acceptance incomplete and production GPU promotion blocked.** The repository implementation covers Steps 1–7. Most saved focused correctness artifacts describe v13 and do not by themselves certify the later v14 orchestration, while a clean revision-matched v14 3P/4P exact arena smoke now passes deterministic CPU/CUDA parity and reports informational speedups (1.455× for 3P; 1.375× for 4P). Packaged browser execution evidence remains missing. Exact CUDA is exposed as `deep-maxn-cuda-exact-fixed-work-v1` under Native Messaging protocol 7 / state schema 3; CPU/WASM remains the production `deep-search` authority. `gpu-root-rollout` remains a separately named experimental algorithm. M2 remains opt-in; M3/M4/M5 were not started as part of stabilization.

## Decision to make

For continuation assignments and review, use [Engine stabilization: bounded subagent tasks](ENGINE_STABILIZATION_SUBAGENT_TASKS_2026-09-08.md). It records the committed baseline, remaining acceptance work, file ownership, dependencies, verification, and stopping rules. The implementation appendix below records completed candidate work; do not redispatch Steps 1–7 as unimplemented features.

Adopt **one production decision policy, with interchangeable verified computation backends**. Use repaired observation-safe weighted-belief MaxN as the initial reference, consistent with the repository's stated default. Keep `gpu-root-rollout` as an explicitly named experimental algorithm until it earns a separate promotion decision.

This does not assert that MaxN currently plays well. It establishes one reproducible behavior to repair and measure. At the investigated checkout, the product's single `deep-search` setting selected different algorithms depending on companion availability, while the strength benchmarks and replay tools did not consistently reproduce that product behavior. The stabilization candidate removes that algorithm switch; remaining GPU work is backend promotion, not a second default strategist.

Reuse the existing `cuda-exact` implementation when investigating acceleration. Do not begin by building another strategy engine, rewriting the rules engine, or porting every component to CUDA. GPU availability must eventually change where the reference computation runs, not silently select a different player.

## 1. Baseline route at the investigated checkout

| Situation | Current route and authority |
| --- | --- |
| Settings loaded or saved | `normalizeDecisionEngine()` always returns `deep-search`; this is stronger than a default preference. |
| Initial placement | WASM/CPU; dedicated opening handling. |
| Eligible own-turn decision, compatible companion available | Native `gpu-root-rollout`, despite the request entering as `deep-search`/`maxn`. |
| Pending incoming trade | Can also qualify for native GPU, including when it is not our turn. |
| Ordinary opponent-turn pondering | WASM/CPU. |
| Companion unavailable or transport failure | WASM/CPU MaxN, preserving the requested stochastic model. Semantic errors do not authorize the same fallback. |
| Explicit M2 strategy policy | CPU/WASM route; native rollout client/host reject it. Ordinary live requests do not send this policy. |

Sources: [settings](../src/content/settings.ts), [background routing and native profile](../src/background/index.ts), [live message construction](../src/content/decision-worker.ts), [request adaptation](../src/worker/deep-search.ts), [native request handler](../engine/crates/catan-wasm/src/native_gpu.rs).

The current ordinary CPU request specifies depth 5, root cap 10, 8,000 nodes per depth wave, up to 24 interactive belief particles, a 2,000 ms decision budget, and a conditional 2,500 ms evidence-escalation reserve. Native routing imposes floors of 12 roots, 384 total rollouts, 96 horizon units, and 4,000 ms. Native sampling races candidates and can stop at a deadline: `384 / 12 = 32` is not a guarantee of 32 samples for every root.

The five M1 strategy families are diagnostics. M2 can affect candidate admission only when explicitly requested. There is no production meta-controller that chooses among those five strategies as independent engines.

The search winner is also not necessarily the executed click. Rust applies mandatory/tactical/exact-family/safety authority; TypeScript maps the action to the live prompt; the overlay handles mandatory actions, trades and workflow continuations, waits for strategic results, and checks state/target validity. Investigation must retain the chain from search winner to final authority to mapped action to executed action. See [WASM arbitration](../engine/crates/catan-wasm/src/lib.rs) and [overlay decision and execution orchestration](../src/content/overlay.ts).

These are facts about the original investigated checkout, not the stabilized candidate. In the candidate, ordinary browser `deep-search` remains CPU/WASM MaxN; companion availability cannot substitute `gpu-root-rollout`. The companion advertises rollout and exact-MaxN capabilities separately, and an exact request is eligible for product use only after an explicit promotion gate. The extension and companion actually loaded in the user's browser were not inspected; stale or mismatched installed artifacts remain an untested possibility, not a diagnosed cause.

## 2. Which conversation claims survive verification

| Claim | Finding |
| --- | --- |
| CPU and GPU are already the same strategist at different speeds | False at the investigated checkout: production could switch between MaxN and rollout. In the stabilization candidate, exact CUDA is a same-policy backend candidate while rollout remains a distinct algorithm. |
| Existing parity allows different strategic choices | Historical rollout parity does; exact-backend parity does not. The current contract separates `deep-maxn-v14` / `deep-maxn-cuda-exact-fixed-work-v1` from `gpu-root-rollout`. |
| The resident GPU win rate measures the extension | False. Its information, roots, continuation, opening, comparator and execution path differ. |
| Weak opponents explain most of the GPU/CPU gap | Plausible, but not isolated. The inspected evidence cannot assign a dominant causal share among the confounders. |
| One perfect-information CPU block falsifies hidden information as the main explanation | Unsupported. It changes information in a different algorithm on a tiny cohort; it is not a GPU belief-versus-realized-state ablation. |
| A depth-3 EndTurn root cannot reach our next decision in 4p | Confirmed by the depth increment/cutoff code. |
| Mean depth around 1.93 proves horizon starvation caused the weak 4p result | Too strong. The statistic is not a per-root coverage guarantee, and no isolated causal experiment was verified. |
| M1 automatically identifies evidence-backed valuation/horizon failures | False as a causal claim. The implementation assigns labels from retention, winner and budget/depth status. |
| Existing replay tools already provide faithful live/budget comparisons | False in their current form; concrete contract drift is described below. |
| M2 is promoted in the mainstream extension | False. It remains opt-in and its documented pilot supplies no strength evidence. |
| `23f7dff` already changed `main` production budgets | False. It exists on `budget/4p-production` in a separate worktree and is not integrated into this HEAD. |

The [M0/M1 screen document](MILESTONE_0_1_EMPIRICAL_SCREEN_2026-09-06.md) records 9/36 versus 17/36 wins. It explicitly describes the current binary as a dirty `c21e4ae` working tree containing M1, not a reproducibly pinned clean `06b14f8` build. Those results were not rerun here. The [M2 design](ADAPTIVE_STRATEGY_LAYER_DESIGN_2026-09-06.md) records 3,463 decisions, three admission decisions, four admitted challengers and zero challenger search wins. This is documented pilot evidence, not independently reproduced evidence.

The older committed [GPU JSON artifact](benchmarks/gpu-strategy-2026-09-02/gpu-agent-p3-canonical-102-12x32x96.json) really does contain 101/102 three-player wins and identifies its baseline as `gpu-weighted`. That confirms the reported kind of result exists; it does not validate browser strength. The later specific 4p 15/16 and 16/16 runs, CPU 2k/12k/48k tables, and perfect-information block in the conversation were not independently verified from matching raw artifacts in the inspected locations. Do not treat either their numerical accuracy or their causal interpretation as established by this investigation.

## 3. Why the resident GPU benchmark is not production-faithful

The [benchmark](../engine/crates/catan-arena/src/bin/gpu-sim-agent-benchmark.rs) creates authoritative `GameState` instances and passes them directly to [resident searched simulation](../engine/crates/catan-search/src/cuda_sim.rs). Candidate roots are evaluated from that realized complete state. It does not reconstruct the player's posterior before choosing each root. An observation-safe rollout policy alone does not remove this problem: root values can still depend on the actual hidden world.

Production [native search](../engine/crates/catan-wasm/src/native_gpu.rs) instead constructs belief particles, checks that they share the actor's observation, and distributes samples over eligible particles by posterior weight. These are materially different information contracts.

There are additional direct code differences:

| Dimension | Resident searched benchmark | Production native search |
| --- | --- | --- |
| Root construction | CUDA sampled root proposals | Host observation-safe ranking, protected/promoted roots and exclusions |
| Information | One realized authoritative state | Weighted belief particles |
| Continuation horizon | `run_rollout_steps_kernel`: 96 micro-actions | `run_root_rollout_turns_kernel`: `ceil(96 / 4) = 24` completed turns from the common pre-action base |
| Future controlled player | Generic stochastic rollout policy; controlled-player argument is `0` | Controlled-player identity passed to deliberate continuation |
| Opponents | `gpu-weighted`, the resident policy family used in rollout simulation | Whatever opponents are in the actual game, approximated by the search model |
| Opening | Candidate search runs whenever the candidate is the decision actor, including setup | Dedicated CPU opening route |
| Sampling | Configured rollouts per sampled root | Total rollout budget, posterior allocation, candidate racing, deadline and possible horizon escalation |
| Final choice | Benchmark kernel selection | Native aggregation and authority/safety arbitration, followed by live mapping/execution |

Kernel evidence: [sim.cu](../engine/crates/catan-search/src/cuda/sim.cu), particularly `run_rollout_steps_kernel`, `run_root_rollout_turns_kernel`, and `run_until_candidate_kernel`; host dispatch and the horizon conversion are in [cuda_sim.rs](../engine/crates/catan-search/src/cuda_sim.rs).

Therefore the headline benchmark does not even exercise the new GPU controlled-player continuation in the same way as production. Describing both configurations as “12 × 32 × 96” hides different meanings. Its high win rate can be a valid result for its own artificial matchup while being unsuitable for choosing live production budgets.

Do not infer that the GPU hardware is incorrect, that hidden-information access explains everything, or that the resident simulator necessarily has a rules bug. Those are separate hypotheses requiring separate checks.

## 4. Concrete defects and misleading diagnostics to repair first

### S1 — Replay effort overrides do not set the effective search effort

[replay-engine.ts](../scripts/replay-engine.ts) spreads `searchConfiguration` or reference configurations over `buildDeepSearchRequest()`. Those configurations change top-level `depth`, `branchCap`, `maxNodes`, and `timeBudgetMs`, but leave the builder's nested `effort` intact. [Rust `Request::resolved_effort()`](../engine/crates/catan-wasm/src/lib.rs) prefers that nested object.

Thus the advertised medium/max reference depths and node budgets do not become the effective MaxN settings. Repair the runner to set the authoritative effort structure and assert the response's effective effort equals the intended experiment. Report actual settings, not labels copied from the input configuration.

### S2 — Replay discards decision context and does not measure historical-action regret

The runner rebuilds requests with empty search constraints, player trades enabled, and no stochastic argument. It does not replay recorded Mref evidence, the original strategy policy, or native routing. Its `live` result is a fresh WASM run. `rootRegretAgainst()` compares that run's chosen action to another engine result, not the recorded `deepChosenAction` or executed `finalAction`.

Preserve the original request or reconstruct it losslessly from versioned evidence. Reject unsupported legacy traces for fidelity claims; they can remain labeled approximate replays. Distinguish historical action, reproduced action and reference action. Reference-engine score disagreement is a diagnostic proxy, not true optimal regret or proof that the reference action wins.

### S3 — M1 failure labels are hypotheses, not explanations

[strategy.rs](../engine/crates/catan-search/src/strategy.rs) classifies a nonwinning proposal as coverage when omitted, horizon when the deadline/depth criterion fails, otherwise valuation. A perfectly reasonable rejected proposal receives one of those labels too. No correct-action oracle or controlled intervention is consulted.

Record neutral observations such as `omitted`, `searched_not_selected`, and `budget_limited`. Keep causal attribution separate and initially unknown. A coverage miss becomes a demonstrated strategic failure only with evidence that the omitted alternative matters. A deadline alone does not establish that more horizon would change the correct decision.

### S4 — Search depth does not describe all evaluated branches

In [depth.rs](../engine/crates/catan-search/src/depth.rs), `deepest_depth` is a maximum reached by one searcher; `wave_depth` takes the maximum across searchers. The arena averages the resulting reported depth. Completing a wave means its root/particle cells were processed, not that every descendant was expanded to that depth.

Add per-root evidence: nodes used, completed wave, distribution of cutoff depths, and weighted mass reaching the controlled player's next decision. Keep terminal branches separate. Use this to test horizon starvation instead of inferring it from mean maximum depth.

### S5 — The proposed 10-second profile conflicts with the client deadline

`23f7dff` raises ordinary 4p CPU search to 10,000 ms while retaining 2,500 ms of possible evidence escalation. `belief_search` constructs the hard deadline as their sum. [DecisionWorkerClient](../src/content/decision-worker.ts) fails a decision after 12,000 ms, including request/service overhead. An escalating search can therefore still be working when the client cancels it.

Do not merge this commit unchanged. Use a shared end-to-end deadline with explicit allowance for preparation, transport, finalization and escalation. This is an independently supported reason to revisit the budget change, beyond the invalid GPU strength justification. A nominal 768-rollout total also does not promise 64 samples for each root under racing and deadline constraints.

### S6 — Previously reviewed milestone correctness issues remain prerequisites

* M0: controlled-player `truncate(1)` consumes a quota-reordered list, not necessarily the maximum-prior action. Fix selection before quota truncation; cover pre-roll Knight and later-family high-prior actions. See [depth.rs](../engine/crates/catan-search/src/depth.rs) and [policy.rs](../engine/crates/catan-search/src/policy.rs).
* M1/M2: award ownership alone does not justify a contested award-race proposal. Require concrete award impact/competition or downgrade its evidence tier. See [strategy.rs](../engine/crates/catan-search/src/strategy.rs).
* M1: an optimistic upper bound above the target does not prove reachability; sample extrema do not cover every compatible world. Rename or weaken the assertions in [reachability.rs](../engine/crates/catan-search/src/reachability.rs).
* Mref tactical repair: retain the positive-weight chance filtering and add a focused impossible-outcome regression before relying on it as a permanent fix.

These are bounded repairs, not justification for an engine rewrite.

## 5. Settle CPU/GPU parity with three distinct contracts

**Game-model parity:** same rules, legal transitions, chance/posterior updates, terminal conditions and evaluation inputs on matched states. Preserve and rerun the existing mechanical gates. Mref remains a named public-reference hypothesis; CPU/GPU agreement does not establish that it matches the current live server.

**Algorithm/backend parity:** same canonical observation and particles, legal proposal domain, retained roots, continuation policy, evaluator, effort schedule, backup/comparator and final arbitration. With fixed work and deterministic seeds, require matching actions and ordering outside declared numerical ties, and value agreement within a justified frozen tolerance. Equal top actions alone can hide different root sets or values. Near ties must be reported, not silently discarded.

**Product strength:** paired complete games and realistic decision/execution cases for the actual product route. Mechanical or backend parity proves consistency, not good play.

Wall-clock comparisons belong in a separate lane. The same algorithm on faster hardware can search more deeply and legitimately change its preferred move. That is not backend failure. First prove fixed-work parity; then compare quality and latency at the same time allowance.

The existing [CPU/GPU Mref contract](CPU_GPU_MREF_CONTRACT.md) deliberately covers the first contract while separating the two algorithms. The older [exact acceleration document](GPU_SEARCH_ACCELERATION_NOTE_2026-09-01.md) already describes the stronger second contract. The repository therefore needs explicit reconciliation of these product meanings, not another ambiguous use of “parity.”

## 6. Implementation sequence and acceptance gates

Work sequentially in reviewable changes. The files below identify responsibility boundaries, not requests to run parallel agents.

| Stage | Concrete changes and owners | Exit evidence |
| --- | --- | --- |
| 0. Repair measurement and known defects | `replay-engine.ts`, shared effort/request boundaries, `depth.rs`, `policy.rs`, `strategy.rs`, `reachability.rs`, focused regressions | Replay effective effort matches requested effort; context survives reconstruction; no causal labels without evidence; known milestone failures have focused reproductions and fixes. |
| 1. Establish one production authority | Settings/background routing, native capability/identity handling, response provenance, README and contracts | Standard `deep-search` uses repaired belief MaxN regardless of companion availability. GPU rollout is explicit experimental tooling. Mandatory and safety authority, cancellation and stale-result rejection remain intact. |
| 2. Verify and integrate exact acceleration | Existing `cuda-exact` path, native host adapter and parity binaries | Current-reference fixed-work parity across supported settings, then an end-to-end latency/throughput result. CPU stays authoritative if exact acceleration is unsupported or not beneficial. |
| 3. Establish product evidence | Existing arena, replay tooling, native regression harness and packaged execution checks | A reproducible report identifies actual routes/artifacts and separates decision consistency, game strength and execution reliability. |
| 4. Resume adaptive strategy work | M2 admission first; then independently evaluated M3/M4/M5 work | Each mechanism has targeted cases and a held-out ablation. No promotion from generic scenario plausibility or a handful of favorable games. |

Stage 1 is now applied in the candidate. Repaired MaxN is the reference because it is the declared baseline and offers a controlled starting point, not because this work proved it is stronger than native rollout. Production rollout substitution was removed; the browser route stays on CPU/WASM until an exact-MaxN backend is explicitly promoted.

`cuda-exact` now exposes weighted-belief MaxN through the native companion. [catan-wasm/Cargo.toml](../engine/crates/catan-wasm/Cargo.toml) enables both `cuda-sim` and `cuda-exact` under `native-gpu`; protocol 7 reports them as distinct algorithms. `analyze-exact` invokes the same MaxN decision contract and shared final arbitration, while `analyze` remains the rollout experiment. An old rollout-only companion cannot satisfy the exact capability check.

The candidate fixed two correctness gaps that the older exact smoke did not cover: CUDA exact now follows the same global iterative fixed-work depth schedule as CPU MaxN, and the resident exact evaluator prepares the **actual request board topology** rather than assuming canonical generated vertex/edge indices. Historical v13 evaluator, search, host-request and frozen-takeover evidence is stored under `docs/benchmarks/engine-stabilization-2026-09-07/`; it does not certify v14. The same directory now also retains a clean `ff7b9b7` v14 matched exact arena smoke: deterministic CPU/CUDA game parity passed, but end-to-end elapsed speedup was only 1.455× for 3P and 1.375× for 4P. Exact CUDA remains unpromoted pending the remaining robustness and packaged-execution checks; these timing ratios are not a blocker. Correctness parity alone does not establish complete release acceptance or a strength claim.

Version the repaired policy/reference and record source SHA plus WASM/companion artifact identities. The current parity document pins an older reference, while M0 changed continuation semantics. Historical parity results cannot certify the modified policy automatically.

## 7. Minimum trustworthy evaluation system

Extend existing tools; do not invent a fourth game simulator.

1. **Frozen decision corpus.** Include 2/3/4 players; fair and usable Mref evidence; 10/15 VP targets; opening, pre-roll Knight, constrained expansion, development scarcity, awards, trades and known bad live decisions. Store the canonical request, source/artifact identities, effective effort, stochastic/posterior identity, proposal/retained-root signatures, search winner, final authority, mapped action and execution result. Keep truth state available only to the simulator/validator, never the candidate decision input.
2. **Backend parity lane.** Run CPU and exact CUDA on identical canonical particles, roots and fixed-work schedules. Audit intermediate values and decisions. Exercise cancellation and fallback. Compare production rollout separately as an algorithm experiment, with no requirement that it imitate MaxN.
3. **Complete-game strength lane.** Use `catan-arena` rules and matched board/chance blocks with all seats rotated, but verify the player adapter actually invokes the intended production decision contract. A native arena alone does not cover TypeScript mapping or browser execution. Use frozen searched opponents as well as Weighted; retain Weighted as a smoke baseline. Keep tuning seeds separate from held-out seeds and report uncertainty over matched blocks, not independent-game intervals for correlated seat rotations.
4. **Packaged execution lane.** In consensual test games, verify observed state → request → final recommendation → click → confirmed transition. Include stale state, disconnected companion, unavailable dice evidence, trade retry and long decision cases. A good simulator move that is never executed correctly is a product failure.

Freeze the opponent lineup, rules, seeds, root policy, belief budget, stopping rules and primary metrics before a campaign. Preserve raw per-game records, per-decision diagnostics where needed, commands and build identities in the existing ignored artifact locations; put concise reproducible conclusions in docs. Size the held-out campaign using block-level variability and the smallest useful effect, not a target headline win rate.

To test the horizon hypothesis, begin with one repaired policy and keep beliefs, roots, opponents, seeds and all other settings fixed. Compare depth 3 versus depth 5 at the same node budget; then compare node budgets at depth 5. Report future-self reach mass, per-root work, decisions changed, VP/rank, wins, timeouts and latency. The quoted depth-3/2k versus depth-5/12k contrast changes two factors and cannot isolate either.

Test M2 with a paired baseline/admission comparison at the same work budget. An omitted candidate is not automatically better; include targeted fixtures where the opportunity is demonstrable. Do not call a searched-but-rejected candidate a valuation defect without additional evidence.

For a particular bad recommendation, classify by intervention: validate the observation and legality; trace final authority; add the missing candidate under a controlled comparison if needed; deepen while keeping roots fixed; vary continuation or belief assumptions separately. Leave unresolved cases unknown. A stronger reference search can provide evidence, but its agreement with itself is not ground truth.

## 8. Definition of stabilized

Stabilization means the product's behavior and failures are reproducible, not that it always wins:

* Hardware availability cannot silently switch the production algorithm.
* Requests, effective budgets and artifact versions are identifiable and consistent across tooling and live execution.
* Exact CPU/GPU comparisons pass on the current reference; unsupported cases use the same-policy CPU path.
* Impossible chance outcomes, stale actions, mapping errors, cancellation and trade workflows have focused regression coverage.
* Selected, overridden and executed actions can be distinguished in traces.
* Failure diagnostics state observed facts and uncertainty accurately.
* Strength and latency claims come from the appropriate held-out/product evidence, with failures and cutoffs included.

Do not merge the 10-second profile unchanged, promote M2, tune new evaluator bonuses, or announce another GPU win-rate improvement before the relevant preceding gates are met. M4's transition-aware economics remains a reasonable hypothesis to test; M5's contingent future-self optimization is a separate algorithm project. Neither should absorb parser, replay, routing or execution bugs.

## Verification and limits of this investigation

Repository source, current branches/worktrees, the referenced documents, the budget commit diff, and the committed three-player GPU artifact were inspected. Codebase Memory Tier 2 coverage metadata matched the inspected paths, with no recorded gaps; material claims were checked against source. This is task-directed evidence, not an exhaustive audit of every file or proof of completeness.

The historical v13 implementation session reported focused Rust/TypeScript regressions, packaged WASM/native builds, CUDA evaluator/search parity, the production-shaped adapter/native exact parity gate, and a frozen arena takeover smoke. Those results are not acceptance of the later v14 orchestration or product-strength evidence. Current repair verification, the clean revision-matched v14 performance-gate failure, and the packaged browser execution limitation are recorded separately in the acceptance report. Browser-installed artifact versions and the reported bad live game were not available for direct validation.

The raw reproducible artifacts and current gate disposition are summarized in `ENGINE_STABILIZATION_ACCEPTANCE_2026-09-07.md` and `benchmarks/engine-stabilization-2026-09-07/`.

## Appendix — Executed implementation plan

This appendix is retained as the implementation checklist. Steps 1–7 are implemented in the candidate subject to the explicit acceptance limitations above; Step 8 remains intentionally deferred until stabilization gates justify resuming strategy work. Use the existing domain types, request adapter, engine, arena and regression harnesses; introduce shared helpers only where they replace duplicated behavior.

### Step 1 — Repair replay and make effective requests verifiable

**Files:** `scripts/replay-engine.ts`, `src/worker/deep-search.ts`, the existing decision-trace serialization boundary, and focused replay/adapter tests. Change Rust request parsing only if needed to establish one unambiguous compatibility rule.

1. Make the nested `effort` structure authoritative in replay configurations. Set `effort.cpu.maxDepth`, `rootCap`, `nodesPerDepthWave`, tactical effort and decision time explicitly for each reference run. Keep legacy fields consistent while they remain supported; do not let two independently maintained sets of knobs describe different experiments.
2. After every run, compare returned `effectiveEffort` with the normalized requested settings. Reject a mislabeled run instead of publishing its metrics. Include effective effort in the output report.
3. Extend the existing trace format to retain the canonical serialized engine request at the adapter boundary, plus source/artifact identities. If reconstructing instead, preserve all decision-affecting inputs: player order, rules, beliefs, stochastic evidence, trade setting, root exclusions, strategy policy and seed. Record which representation is authoritative.
4. Distinguish exact request replays from approximate legacy reconstructions. A missing Balanced/Mref history must not silently become fair dice in a report labeled production-faithful. Legacy traces with insufficient evidence receive an explicit limitation or are excluded from that lane.
5. Report three separate actions: historical executed action, reproduced production action, and reference-search action. Compare the actual historical action when reporting historical disagreement. When an action is absent from reference evaluation, report unavailable evidence; do not invent a zero regret value.

**Acceptance:** A reference-medium run demonstrably receives its intended effort; changing that effort changes the returned contract. A complete Mref trace preserves its stochastic identity and posterior inputs. Disabled trades and root exclusions survive replay. An incomplete trace cannot pass as a faithful replay. Reference-score differences are labeled as proxies, not optimal regret.

### Step 2 — Fix controlled-player continuation and freeze the repaired reference

**Files:** `engine/crates/catan-search/src/depth.rs`, `policy.rs`, focused Rust tests, and the current CPU reference/parity documentation.

1. Separate selecting the controlled player's continuation from building the opponent's bounded action mixture.
2. For the controlled player, select the highest-scoring legal observation-safe policy action before diversity quotas, root ordering, or top-three truncation discard candidates. Use a canonical deterministic tie-break. Preserve existing immediate-win handling and legality checks.
3. Use the same selection rule in CPU recursion and the CUDA-exact deferred search path. Leave the opponent mixture unchanged in this commit so the intervention is isolated.
4. Test a pre-roll Knight that outranks Roll, a later action family that outranks an available settlement, and indistinguishable hidden worlds that must produce the same continuation. Include an immediate winning continuation and a deterministic tie.
5. Assign a new policy/reference identity and record its source revision. Rebuild generated WASM through the normal build workflow when the source change is accepted; do not hand-edit generated bindings or reuse an older binary as evidence.

**Acceptance:** Selection follows the score rather than family position, depends only on the acting player's observation, and matches between CPU and exact-backend host search. This becomes the correctness baseline for later strength comparisons; no strength improvement is presumed.

### Step 3 — Repair strategy evidence without promoting M2

**Files:** `engine/crates/catan-search/src/strategy.rs`, `reachability.rs`, `depth.rs`, and their WASM/TypeScript diagnostic mappings.

1. Require concrete evidence before assigning award-race priority: for example, a transition that acquires/protects an award or a supported competitive race assessment. Mere nonownership is insufficient. If existing helpers cannot establish the evidence, downgrade or omit the race proposal rather than inventing a new confidence score.
2. Replace reachability assertions with statements justified by the computation: an optimistic ceiling and sampled-world bounds. Preserve a sound necessary-source deduction when the optimistic non-development ceiling is below the target. Do not infer attainability from a ceiling above the target.
3. Separate proposal status from causal failure attribution. Record admission, search entry, result rank and budget status directly; use unknown attribution until a scenario/reference/intervention supports a cause. Version or migrate serialized diagnostics consistently across Rust, WASM and TypeScript.
4. Add per-root search work and cutoff evidence. Track whether each sampled continuation reaches the controlled player's next decision, with weight and early-terminal cases handled explicitly. Keep this diagnostic work bounded and verify it does not change the selected action under fixed work.
5. Retain M2's opt-in identity, fixed root cap and protected roots. Verify that an ordinary noncompetitive road/Knight gains no contested priority, while a demonstrated race can be admitted.
6. Add the missing tactical regression for the Mref zero-weight chance fix: impossible outcomes are not applied, and surviving chance mass is weighted correctly.

**Acceptance:** Diagnostics cannot call an ordinary rejected proposal a proven valuation error. Sampled bounds are honestly named. Genuine and nongenuine race fixtures separate correctly, and baseline requests remain free of M2 admission changes.

### Step 4 — Fix deadline ownership before changing production budgets

**Files:** `src/content/decision-worker.ts`, `src/background/index.ts`, `src/worker/deep-search.ts`, Rust effort/deadline handling and focused cancellation tests.

1. Keep `23f7dff` unmerged while reconciling the complete deadline path.
2. Define one end-to-end decision allowance and explicitly budget preparation, search, evidence escalation, finalization and transport. CPU escalation consumes a reserved portion of that allowance; it must not extend execution beyond the client safety deadline.
3. Pass remaining allowances across process boundaries using durations or the existing local clock abstraction. Do not compare unrelated JavaScript/Rust monotonic clock origins.
4. Make native profile floors respect the overall limit. Report requested work, completed work, elapsed time, cutoff and cancellation separately.
5. Exercise an escalating CPU decision, delayed native response, transport failure and board change while work is running. The client must reject stale results, and fallback must not restart a full independent allowance after the original deadline is exhausted.

**Acceptance (clarified by Hamza on 2026-09-08):** Engine search time is a cooperative target. Preserve completed results through required final arbitration even when root preparation or search overruns that target, and report the cutoff honestly. The separate client safety limit, cancellation and stale-result rejection remain authoritative; fallback cannot restart a full allowance. Do not claim an exact engine elapsed-time ceiling by construction. Select any later 4p budget from measured end-to-end latency and quality, not campaign throughput or nominal samples per root.

### Step 5 — Give the production extension one decision policy

**Files:** `src/background/index.ts`, `src/background/native-gpu.ts`, `src/content/settings.ts`, request/response protocol types, and user-facing runtime descriptions.

1. Route ordinary `deep-search` to the repaired MaxN reference. Until an exact native backend passes Step 6, use CPU/WASM for that policy even when the existing rollout companion is available.
2. Keep `gpu-root-rollout` available only through explicit experimental tooling. Do not create another default UI engine selector or a second strategy orchestrator merely to preserve automatic rollout routing.
3. Separate algorithm identity from computation backend in the existing diagnostics: the request selects the policy; capabilities determine whether a backend can execute that exact policy and stochastic/strategy contract.
4. Reject mismatched algorithm or unsupported semantic responses. An unavailable exact backend may fall back to CPU execution of the same policy; it may not substitute rollout search or alter Mref evidence.
5. Retain opening, mandatory, exact-family, safety, trade and stale-state authority. Ensure runtime labels and final-action provenance describe what actually ran, including overrides.
6. Update README and both parity documents together so the product contract no longer implies that different algorithms are interchangeable accelerators.

**Acceptance:** Connecting or disconnecting the existing rollout companion cannot change the production algorithm. Fixed-input/fixed-work requests preserve the same decision policy, and tests cover both available and unavailable companion cases. This is an intentional routing change, not a claim that CPU is inherently stronger.

### Step 6 — Reuse exact CUDA behind the same orchestration

**Files:** existing `cuda-exact` search/evaluator implementation, `engine/crates/catan-wasm` request and response orchestration, native companion capability handling, and existing exact parity binaries.

1. Run the existing exact evaluator and search parity gates against the repaired reference. Resolve divergence before integrating the backend into live routing.
2. Expose the existing exact MaxN implementation through the native host. Reuse request parsing, posterior construction, mandatory/tactical handling, root admission and final arbitration. Factor only the duplicated shared stages needed for this; do not copy the native rollout pipeline and rename its algorithm.
3. Make backend selection occur after the canonical decision inputs and policy are fixed. Preserve root exclusions, M2 identity when explicitly requested, chance semantics, tie-breaking, node accounting and deadline/cancellation behavior.
4. Extend the capability handshake and artifact identity to distinguish exact MaxN support from rollout support. An old rollout-only companion must never satisfy the new capability check.
5. Compare identical particles and fixed-work schedules across CPU and exact CUDA: legal domain, retained roots, backed-up values, chosen action and final authority. Cover 2/3/4 players, supported victory targets, fair/Mref evidence, trades and mandatory phases. Unsupported cases stay on CPU with an explicit reason.
6. Measure complete request latency and resource usage after correctness passes. Preserve the documented performance gate unless it is explicitly revised with evidence. If acceleration fails the gate, leave it experimental and keep the stable CPU route; optimization is a separate task.

**Acceptance:** A GPU-backed production request performs the same reference decision computation. Different algorithms are never relabeled as parity. Fixed-work numerical ties use declared tolerances; any substantive disagreement blocks promotion.

### Step 7 — Establish a release gate that measures the product

**Files:** existing arena adapters and reports, replay tooling, native strategic regression harness, packaged extension tests, and benchmark documentation.

1. Save a frozen diagnostic corpus covering the concrete failures above and representative real decisions. Keep its tuning role separate from held-out gameplay evidence.
2. Add the intended production request/decision route to the existing arena integration where it is missing. Keep authoritative simulator truth outside the candidate's observation and posterior. Verify route and artifact identities in each result.
3. Run separate comparisons for reference-versus-candidate strength and CPU-versus-exact-backend consistency. Use frozen searched opponents, rotate seats, match board/chance blocks and retain cutoffs/errors. Weighted remains a smoke opponent, not the sole promotion test.
4. Freeze the budget experiment before running it: depth-cap change first with node budget fixed, then node-budget change at fixed depth. Only after that compare M2 on/off with all other settings fixed. Record root-specific evidence and future-self reach, not just average reported depth.
5. Verify the packaged recommendation-to-click path in consensual test games. Match extension and companion artifacts, reload them, and validate that the executed action and resulting state correspond to recorded final authority.
6. Publish one acceptance report with source/build identities, effective settings, raw artifact locations, matched-block uncertainty, latency distribution, timeouts, execution failures and unresolved cases. Do not translate simulator win rates into live win-rate guarantees.

**Acceptance:** A reviewer can reproduce what ran, distinguish strategy defects from execution defects, and determine whether the candidate meets the preregistered quality and latency criteria. Poor or inconclusive results remain visible and block unsupported promotion claims.

### Step 8 — Resume the strategy roadmap from demonstrated failures

After the production route and measurement gates are stable, evaluate the repaired M2 admission layer first. Proceed to M3 only when omitted-candidate reconsideration is demonstrated to matter; to M4 when transition-aware economic evaluation fixes specific comparisons; and to M5 as a separately reviewed contingent-search algorithm. Keep each change independently switchable in offline experiments and compare it against the frozen reference before promotion.

For the user's original development-card example, construct legal fixtures with an exact own hand, a consistent public development history, posterior remaining-deck composition, a declared victory target and competing point routes. Verify the probability and point-source premise before requiring a development purchase. For a road race, construct the actual blocking/settlement opportunity and opponent response window. These scenarios must establish why one action is preferable; tests should not simply encode “always buy development” or “always defend a road.”

### Execution checklist and stopping rules

Before source changes, apply the repository's implementation-scope policy and preserve other worktrees and staged edits. Run focused regressions for each causal fix, then the required TypeScript/Rust/build checks for the combined candidate (`npm run check`, relevant tests, `npm run verify:rust`, and the packaged build/release verification). CUDA changes additionally require the relevant current-reference evaluator, transition and search gates. Record actual results; missing CUDA hardware or an unavailable live-game fixture is a remaining validation gap, not a pass.

Stop promoting the candidate if same-policy parity fails, hidden truth enters candidate decisions, an effective-request mismatch occurs, or deadline/stale-action guarantees regress. Roll back only the affected candidate behavior to the frozen reference; do not weaken the gate or change the experiment after seeing its results. Finishing this plan means a reproducible, consistent and tested extension with defensible strength evidence—not a promised win percentage.
