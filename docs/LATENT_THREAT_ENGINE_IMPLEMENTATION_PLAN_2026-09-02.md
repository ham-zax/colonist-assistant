# Latent Threat Engine Implementation Plan

**Status:** Review draft — revised after external architecture/correctness review

**Date:** 2026-09-02

**Repository baseline inspected:** `d49a410`

**Companion design note:** `docs/latent-threat-strategy.md`

**Goal:** Convert the strategic threat-modeling design into a production-safe implementation that improves opponent anticipation, road/disruption coverage, hidden-development-card defense, malicious-trade handling, and GPU verification without replacing search with a fixed defensive heuristic.

**Architecture:** Preserve the existing weighted-belief + MaxN/native-GPU architecture. Add a bounded root-level strategic-impact layer that computes exact or posterior-weighted tactical consequences, uses those consequences primarily to keep important actions inside the search width, and lets CPU/GPU search remain authoritative for the final choice. Strengthen CUDA rollout policies so simulated opponents can actually exploit the tactical opportunities the root layer is trying to defend against.

**Tech Stack:** Rust (`colonist-catan-core`, `colonist-catan-search`, `colonist-catan-wasm`, `colonist-catan-arena`), CUDA (`catan-search/src/cuda/sim.cu`), TypeScript tracker/deep-search adapter, existing GPU-resident arena and takeover/replay infrastructure.

## Global Constraints

- Do not implement a fixed OWS, expansion, Longest Road, port, or defensive strategy mode.
- Do not add a generic reward for "web-shaped" roads. Reward only demonstrated route resilience, expansion preservation, award protection, or denial value.
- Do not assume the worst hidden development card exists. Aggregate over the weighted posterior.
- Do not give simulated opponents private information unavailable from their information set.
- Do not pass the root player's private-hand-conditioned posterior into simulated opponent policy logic.
- Reuse `GameState::apply()`, core legality, award updates, victory checks, bank rules, and development-card timing rules instead of duplicating rules in a threat evaluator.
- Keep expensive topology/progress-card analysis out of the generic leaf `evaluate()` hot path unless later evidence proves a leaf feature is both cheap and valuable.
- Compare threat state **before versus after a candidate action**. Unavoidable danger must not penalize every root equally.
- Use hard safety overrides only for proven or near-forced outcomes. Ordinary uncertain risk remains a search input, not a categorical veto.
- Preserve the existing `HARD_VETO_POSTERIOR = 0.99` philosophy for domestic-trade hard vetoes unless new evidence justifies changing it.
- Preserve full distinct production belief particles; do not reintroduce lossy particle compression into the live path.
- Keep CPU MaxN and native GPU root-admission semantics aligned. A defensive action must not be visible to one backend and silently pruned by the other.
- Treat existing `gpu-sim-agent-benchmark` results as strategy/search screening, not calibrated browser win rate.
- Do not tune against the existing 102-game P3 canonical cohort. Preserve it as a regression reference because its 101/102 result has a severe ceiling effect.
- Verification must separate tactical correctness, root coverage, rollout-policy competence, matched takeover evidence, whole-game strength, and real browser behavior.
- Keep player-count, `victory_target`, discard limit, friendly robber, bank visibility, and player-trading rules configurable; do not assume one standard table contract.
- No compatibility shim or legacy parallel strategy path should be added merely for A/B comparison.
- Do not fold general search-wave redesign into this mission. In particular, this plan does not authorize partial-wave blending, global same-turn action canonicalization, or a new recursive transposition table. Preserve the current posterior-wide complete-wave fallback and existing sibling-budget/permutation-stability semantics unless a separate search mission demonstrates a defect.

## Current production baseline facts

These are baseline constraints for implementation review, not new work items:

- The learned policy/value machinery exists, but the checked-in model currently has `VALUE_MODEL_PROMOTED = false` and `POLICY_MODEL_PROMOTED = false`; do not describe learned inference as active production authority.
- The current live worker budgets are approximately 2.0 s for ordinary WASM decisions, 1.5 s for trade decisions, and 2.5 s for opening decisions. The native GPU strength profile raises eligible decisions to at least 4.0 s. Treat these as current configuration facts, not permanent latency targets.
- The TypeScript tracker/worker boundary is the live lossy approximation point: interactive joint sampling is capped at 24 particles. Once supplied to Rust, production belief search preserves every exact-distinct state after lossless identical-state coalescing; lossy strategic coresets remain experimental.
- `order_scored_with_state_quotas()` already retains at most two material domestic-trade representatives. Reducing domestic-trade root count to two is therefore not a Latent Threat implementation task.
- Current recursive search already distributes node budgets across ranked siblings with carryover and canonicalizes equal-prior siblings. The recovered Turn-54 controls also cover same-turn BuyDevelopment/MaritimeTrade state equivalence and sibling-permutation stability. Do not reopen those mechanisms as part of Wave 1 without new causal evidence.

## Source-to-implementation boundary

`docs/latent-threat-strategy.md` remains the strategic/rules/research specification. This document is the implementation and evidence contract.

The strategy note deliberately describes hypotheses and desired behavior. This plan decides where those concepts belong in the current codebase:

```text
tracker/public evidence
        |
        v
weighted resource + development belief
        |
        v
ordinary observation-safe root generation
        |
        v
root strategic-impact prepass
  - latent tactical threat delta
  - road resilience/disruption delta
  - closeout/response-window delta
        |
        v
root admission / exact-family collapse / safety
        |
        +--------------------+
        |                    |
        v                    v
   CPU MaxN             native GPU root rollout
                             |
                             v
                    stronger CUDA opponent policy
        |                    |
        +---------+----------+
                  v
             final action
                  |
                  v
      provenance + explanation data
```

The strategic layer should answer **what must be considered**. Search should answer **what should be chosen**.

## Resolved review corrections

The following points were challenged during external review and checked against the current repository. Treat these as implementation constraints unless the underlying core rules change:

- **Progress-card timing comes from `GameState`, not a prose matrix.** The current core exposes playable Knight, Road Building, Year of Plenty, and Monopoly actions in the phases allowed by `legal_actions()` and validates them again in `apply()`. Do not impose an external assumption that only Knight is legal before rolling.
- **Same-turn purchase tracking uses counts, not a `turn_bought` timestamp.** `development[]`, `bought_development[]`, and `played_development_this_turn` are the authoritative current contracts.
- **Longest Road ties preserve the current holder when that holder remains among the qualifying leaders.** If the old holder no longer leads, a unique qualifying leader takes the award; otherwise it can become unowned. `resilience.rs` must reuse this core behavior rather than simplify it to "all ties are unowned."
- **Domestic-trade creator/actor is not equivalent to turn owner.** Counteroffers can change `trade.creator`, while `current_player` remains the player whose turn is in progress. Dirty Monopoly analysis must follow the real protocol to the resulting Main phase and inspect that actual turn owner.
- **The explicit-root CUDA benchmark isolates rollout evaluation, not the whole native-GPU engine.** Production native GPU additionally performs belief allocation, exact-family arbitration, shared root admission, trade safety, racing, deadlines, and final safety/exact replacement.
- **Performance numbers are measurements, not assumptions.** Do not bake guessed microsecond costs, register counts, or fixed beam widths into the design; profile the implementation before narrowing candidate coverage.

# 1. Design Decisions

## D1 - Add root-level strategic impact, not a second evaluator

Do not turn `strategic_utility()` into a sum of road-cut, Monopoly, hostility, and blocking coefficients. Those quantities are expensive, posterior-dependent, and most useful near the root where branch width can omit a decisive action.

Introduce a bounded root-level result that can be consumed by both CPU belief search and native GPU root preparation.

Proposed responsibility split:

- `threats.rs`: concrete opponent tactical continuations and posterior threat mass;
- new `resilience.rs`: public-board road topology, cuts, bypass/recovery, award and expansion consequences;
- new `root_impact.rs`: compute belief-wide before/after tactical and topology consequences and classify strategically important roots;
- `policy.rs`: cheap ordinary priors and state quotas remain the first-pass broad candidate generator;
- existing `shared.rs`: own only the shared observation-safe admission seam that merges ordinary ranked roots with strategic promotions, deduplicates them, preserves mandatory/promoted coverage and `EndTurn`, and applies the width cap;
- `depth.rs` and `native_gpu.rs`: consume the same admitted-root result instead of maintaining separate promotion semantics.

The names `resilience.rs` and `root_impact.rs` remain proposed ownership boundaries. `shared.rs` is intentionally reused rather than introducing another broad root-generation abstraction, because it already owns observation-safe shared root scaffolding. Do not move exact-family solving, trade-safety evaluation, deadline policy, GPU racing, final safety replacement, or other backend orchestration into `shared.rs`; those remain with their current owners.

## D2 - Root promotion is coverage, not automatic value

A root can be promoted because ordinary priors understate its strategic consequence. Promotion guarantees search evidence; it does not give the action a synthetic win bonus.

Examples:

- a low-production settlement that cuts a rival's Longest Road;
- a road that protects the only expansion route from a likely Road Building jump;
- a Knight line that prevents a verified Largest Army win;
- rejecting a trade that creates a nearly certain Dirty Monopoly win.

After admission, MaxN/GPU remains free to reject the promoted action because its opportunity cost is too high.

## D3 - Exact mechanical consequences beat proxy graph scores

Road resilience should be measured by hypothetical legal/public-board mutations followed by the existing engine's exact Longest Road and expansion calculations.

For a candidate cut vertex or edge, measure consequences such as:

- Longest Road length change;
- award holder change;
- VP swing;
- best expansion value change;
- route distance change;
- whether a bypass exists;
- minimum additional road recovery cost.

Do not infer resilience only from graph degree or arbitrary branch count. Do not reimplement Longest Road ownership/tie rules in `resilience.rs`: apply the hypothetical public-board consequence through rules-consistent state mutation, then reuse `GameState::longest_road_length()` and the core award-update semantics. In particular, the current holder retains the award when still tied for the best qualifying length; otherwise a unique qualifying leader takes it, and unresolved ties can leave it unowned.

## D4 - Latent threat analysis uses a bounded tactical grammar

`threats.rs` should remain a tactical proof/diagnostic layer, not become a general recursive search engine.

The bounded grammar covers only high-impact next-turn transforms:

```text
no progress card
  -> direct builds / awards / win

Monopoly(resource)
  -> transfer
  -> direct builds / awards / development purchase / win

YearOfPlenty(a, b)
  -> bank-constrained resource injection
  -> direct builds / development purchase / win

RoadBuilding(edge1, edge2)
  -> exact road topology transition
  -> settlement / award / win

Knight(hex, victim)
  -> robber transition
  -> probabilistic steal where required
  -> Largest Army / immediate conversion / win

BuyDevelopment
  -> immediate VP-draw win chance when relevant
```

Treat `GameState::legal_actions()` and `GameState::apply()` as the timing/legality authority for progress cards. Do not maintain a second hand-written phase matrix in `threats.rs`: the current rules engine permits playable action cards in the phases it exposes, tracks same-turn purchases through `development[]` versus `bought_development[]`, and enforces the one-development-card-per-turn restriction through `played_development_this_turn`. Threat analysis should enumerate the relevant legal actions and apply them, then bound only the continuation grammar after that transition.

Keep dependency direction simple: `threats.rs` may reuse core transitions and lightweight search helpers, but it should not depend on `exact.rs` merely to enumerate opponent progress-card parameters. `exact.rs` remains the owner of authoritative parameter selection for our own compact exact action families.

## D5 - Differentiate proof from expected risk

Threat outputs require two categories:

1. **proof/safety fields** used for mandatory escapes and near-forced hard vetoes;
2. **expected strategic fields** used for root coverage, ranking diagnostics, and GPU evaluation context.

A 20% Monopoly posterior is not an immediate-loss proof. A 99.5% posterior in which accepting a trade enables a same-turn winning Monopoly line can qualify for the existing hard-veto policy.

## D6 - Keep hidden state uncertainty separate from opponent-objective uncertainty

Hidden cards/resources are uncertainty about **state**. Hostility/coalition behavior is uncertainty about **opponent policy**.

The first implementation waves address hidden state and public-board disruption only. MaxN-versus-Paranoid interpolation is introduced later as an offline stress diagnostic. Pair-specific hostility enters production only after recorded behavior shows predictive value.

## D7 - CUDA needs competent punishers, not independent strategic authority

The live native GPU path already receives Rust-generated roots. Root-level strategic coverage should therefore be implemented once in Rust and reused by CPU/native-GPU search.

CUDA must eventually improve its rollout policy so opponents can execute:

- disruptive settlement cuts;
- road choke/award moves;
- coherent two-road Road Building sequences;
- YOP build completion;
- Knight Largest Army or unblock/deny lines;
- Monopoly choices based on likely yield **and conversion value**.

Do not front-load all of those changes into Wave 1. The road/disruption slice first measures the road corpus. If that evidence shows CUDA is failing to propose or punish the relevant settlement/road tactic, make only the narrow road/settlement rollout-policy repair needed for that demonstrated failure. Road Building/YOP/Knight/Monopoly rollout competence remains Wave 2 work.

Otherwise thousands of rollouts can systematically underestimate the exact dangers the root layer is designed to expose.

## D8 - The current searched-agent benchmark cannot be the first proof

`gpu-sim-agent-benchmark` samples candidate roots from the CUDA rollout policy. A feature whose value is "this previously omitted root now enters search" can therefore be invisible to that benchmark.

Add an **explicit-root tactical GPU benchmark** where the scenario defines all candidate root actions and `CudaSim::search_root_actions()` evaluates them directly. Treat this as the authoritative CUDA rollout-evaluation oracle for supplied roots, not as a byte-for-byte reproduction of the complete native-GPU decision pipeline: native GPU also performs posterior allocation, exact-family collapse, root admission, trade safety, adaptive racing, deadline handling, and final arbitration. Use the explicit-root benchmark to isolate rollout quality, then use native/takeover evidence to verify the complete pipeline.

## D9 - Measure causal intermediate outcomes

Whole-game win rate is necessary but not sufficient. Emit tactical metrics that show whether the intended mechanism changed:

- root admission rate for known blockers;
- successful/attempted road cuts;
- Longest Road award transfers;
- opponent best-expansion portfolio value removed by a block/protection move;
- Monopoly cards transferred;
- progress-card build conversion;
- Dirty Monopoly trade exploitation;
- one-turn closeout rate;
- terminal dead-end-road count/rate, defined as an owned terminal edge whose terminal vertex has no friendly building, cannot accept a settlement under the current public occupancy/distance rule, and whose removal does not reduce that player's current Longest Road length; this is a structural diagnostic, not a claim that the road was historically useless;
- baseline/candidate root-choice concordance on matched replay/takeover positions;
- decision latency/deadline/truncation, including candidate-seat versus opponent-seat truncation where applicable.

## D10 - Use matched blocks as the statistical unit

When comparing baseline and candidate, use identical board/chance blocks and rotate the candidate through every seat. Aggregate and bootstrap by matched block, not by individual snapshots from the same game.

## D11 - Preserve a holdout seed family

Use three evidence partitions:

- development/screening seeds: safe for iteration;
- validation seeds: used after a wave is feature-complete;
- final holdout seeds: not inspected while tuning the wave.

The existing September 2 canonical P3 cohort remains a historical regression reference and is not one of these tuning partitions.

## D12 - Learning comes after structural competence

Do not train a model to imitate current blind spots. First prove root coverage and opponent continuation competence. Only then add demonstrated useful structural features to `features.rs` and generate new expert labels from the stronger high-budget search.

# 2. File Ownership Map

## Files proposed for creation

### `engine/crates/catan-search/src/resilience.rs`

Own public-board route resilience and disruption consequences.

Responsibilities:

- critical cut vertex/edge detection;
- exact Longest Road consequence after a hypothetical opponent settlement/road;
- award transfer consequence;
- expansion loss/denial consequence;
- bypass/recovery estimate;
- compact per-state `RoadResilience` / `DisruptionConsequence` output.

Must not:

- inspect private opponent resource identities when called from observation-safe policy code;
- perform belief aggregation;
- choose final actions.

### `engine/crates/catan-search/src/root_impact.rs`

Own posterior aggregation and root strategic-impact diagnostics.

Responsibilities:

- compute baseline threat/resilience once per belief root;
- apply a root candidate in each legal weighted world;
- compute after-state threat/resilience;
- aggregate delta fields;
- classify roots for promotion before branch truncation;
- expose bounded diagnostics shared by CPU/native GPU.

Must not:

- implement rules transitions independently;
- run recursive MaxN;
- hard-code a final defensive score that bypasses search.

### `engine/crates/catan-arena/src/bin/gpu-latent-threat-benchmark.rs`

Own explicit-root GPU tactical/counterfactual benchmarking.

Responsibilities:

- load deterministic scenario fixtures;
- provide explicit candidate root actions;
- run `CudaSim::search_root_actions()` with configurable rollout count/horizon;
- emit per-action terminal outcome, VP margin, variance, and selected action;
- compare against expected tactical behavior and negative controls;
- preserve scenario/seed identifiers for reproducibility.

### `tests/fixtures/latent-threat-tactical-corpus.json`

Own deterministic tactical scenario definitions used by the new benchmark and replay tooling.

Each scenario should contain:

- stable scenario ID and family;
- players/rules configuration;
- serialized or reproducibly constructed state parameters;
- actor/root seat;
- explicit candidate actions;
- hidden-state/posterior variants where applicable;
- expected mechanical consequence labels;
- negative-control relationship where applicable.

## Files expected to be modified

### `engine/crates/catan-search/src/lib.rs`

- register/export the new focused modules and public diagnostics needed by `catan-wasm`/arena;
- do not broaden unrelated public API surface.

### `engine/crates/catan-search/src/threats.rs`

- extend immediate/latent tactical analysis to progress-card-aware bounded continuations;
- add posterior threat snapshot aggregation;
- retain existing verified immediate-win semantics as the strict safety subset.

### `engine/crates/catan-search/src/policy.rs`

- add cheap public disruption terms to settlement/road priors;
- preserve existing family normalization and relevance quotas;
- avoid expensive belief-wide analysis here.

### `engine/crates/catan-search/src/shared.rs`

- extend the existing observation-safe root scaffolding so ordinary ranked roots and `root_impact.rs` promotions are merged/deduplicated through one shared admission contract;
- preserve `EndTurn` retention and the existing observation identity assumptions;
- own only merge/deduplication, protected coverage, and width admission;
- keep exact-family solving, trade-safety evaluation, deadline policy, GPU racing, and final safety arbitration with their existing owners;
- keep policy scoring in `policy.rs` and strategic consequence calculation in `root_impact.rs` rather than turning `shared.rs` into a second evaluator or orchestration layer.

### `engine/crates/catan-search/src/depth.rs`

- consume shared root-impact promotions before ordinary width truncation;
- preserve exact-family collapse, trade safety, complete one-ply posterior floor, and iterative-wave semantics;
- include root-impact diagnostics in provenance.

### `engine/crates/catan-search/src/trade_safety.rs`

- replace build-only tactical reachability with bounded progress-card-aware reachability for trade consequences;
- compare newly enabled before/after threat sets;
- preserve `HARD_VETO_POSTERIOR` for hard vetoes.

### `engine/crates/catan-search/src/planner.rs`

- expose same-turn/short-close completion data needed for closeout/response-window diagnostics;
- do not add an artificial "hide visible VP" objective.

### `engine/crates/catan-search/src/eval.rs`

Initial wave:

- avoid broad evaluator changes;
- reuse existing expansion/LR/army/production helpers from new root-level code.

Later, only if validated:

- add a cheap resource-concentration/Monopoly-exposure feature if benchmark evidence shows it improves leaf ordering without duplicating the root tactical layer.

### `engine/crates/catan-search/src/cuda/sim.cu`

- improve public-observation-safe disruptive settlement/road policies;
- make Road Building pair selection coherent;
- make YOP/Monopoly/Knight choices conversion-aware;
- preserve all information-set restrictions.

### `engine/crates/catan-search/src/cuda_sim.rs`

- expose benchmark-only action-proposal frequency diagnostics if needed for the G1 coverage gate;
- preserve existing device-resident production path;
- keep proposal diagnostics out of live response payloads unless later needed.

### `engine/crates/catan-wasm/src/native_gpu.rs`

- consume the same admitted-root contract from `catan-search::shared` used by CPU belief search, with `root_impact.rs` supplying the strategic promotions;
- preserve exact-family arbitration, posterior-weighted root availability, racing, and safety overrides;
- expose compact root-impact provenance.

### `engine/crates/catan-wasm/src/lib.rs`

- serialize any new compact root-impact/threat diagnostics required by the extension/replay tools;
- preserve the distinction between decision authority and explanatory diagnostics.

### `src/worker/deep-search.ts`

Initial waves:

- map new root/threat provenance into `DeepSearchResult` diagnostics;
- do not alter particle construction merely for strategic tuning.

Later public-belief wave:

- pass public-event-derived opponent resource marginals only if a separate observation-safe contract is added.

### `src/core/tracker.ts` and `src/core/types.ts`

Not part of the first tactical implementation wave.

Potential later responsibility:

- pair-specific hostility evidence;
- public resource marginal state that is independent of the root player's exact private hand.

### `engine/crates/catan-arena/src/main.rs`

- extend existing game/trajectory metrics with tactical mechanism counters when the corresponding production behavior is implemented;
- reuse current takeover/trajectory infrastructure rather than creating a second simulator.

### `engine/crates/catan-arena/src/bin/gpu-sim-agent-benchmark.rs`

- add compact matched-block output sufficient for paired block bootstrap;
- keep existing benchmark semantics explicit;
- optionally add a live-root-generation mode only after it can call the same root-preparation contract as native GPU without duplicating logic.

### `docs/benchmarks/`

- freeze benchmark outputs only after a wave is accepted;
- separate screening, validation, and holdout results.

# 3. Proposed Internal Data Contracts

These names are proposed for review. The behavior is the important contract.

## 3.1 Tactical threat snapshot

```rust
#[derive(Clone, Copy, Debug, Default)]
pub struct LatentThreatSnapshot {
    // Strict safety/proof mass.
    pub immediate_win_mass: f32,

    // Expected tactical conversion mass.
    pub monopoly_conversion_mass: f32,
    pub road_building_conversion_mass: f32,
    pub yop_conversion_mass: f32,
    pub knight_conversion_mass: f32,
    pub development_purchase_win_mass: f32,

    // Expected root-player damage / opponent gain.
    pub monopoly_loss_ev: f32,
    pub award_swing_mass: f32,
    pub expansion_denial_mass: f32,
}
```

Rules:

- every field is normalized over positive posterior mass;
- no field is called a calibrated win probability unless it actually represents a direct event probability;
- `immediate_win_mass` remains the strict field allowed to drive forced-loss safety;
- expected-value fields can influence coverage/diagnostics but cannot independently trigger a hard override.

## 3.2 Road resilience result

```rust
#[derive(Clone, Debug, Default)]
pub struct RoadResilience {
    pub critical_vertices: Vec<CriticalVertex>,
    pub critical_edges: Vec<CriticalEdge>,
    pub maximum_longest_road_loss: u8,
    pub maximum_award_vp_swing: i8,
    pub maximum_expansion_value_loss: f32,
    pub minimum_bypass_roads: Option<u8>,
}
```

A `CriticalVertex`/`CriticalEdge` must record the exact consequence it represents. Do not store only a scalar "criticality score."

## 3.3 Root impact result

Do not duplicate the full baseline road/threat structures into every candidate record. Cache the baseline once per observation-safe belief root and keep per-action consequence deltas compact.

```rust
#[derive(Clone, Copy, Debug, Default)]
pub struct RoadImpactDelta {
    pub longest_road_loss_prevented: i8,
    pub longest_road_loss_inflicted: i8,
    pub award_vp_swing: i8,
    pub expansion_portfolio_delta: f32,
    pub minimum_bypass_roads_delta: i8,
}

#[derive(Clone, Debug)]
pub struct RootStrategicImpact {
    pub action: Action,
    pub threat_after: LatentThreatSnapshot,
    pub road_delta: RoadImpactDelta,
    pub threat_reduction: f32,
    pub closeout_gain: f32,
    pub promotion: Option<RootPromotionReason>,
}

#[derive(Clone, Debug)]
pub struct RootImpactReport {
    pub baseline_threat: LatentThreatSnapshot,
    pub baseline_road: RoadResilience,
    pub actions: Vec<RootStrategicImpact>,
}
```

`threat_reduction`, `road_delta`, and `closeout_gain` are bounded diagnostics used for ordering/promotion. They must be derived from named measurable consequences and must never be added as a second scalar bonus on top of the search value. This is the explicit double-counting invariant: promotion changes **coverage**, not backed-up MaxN/GPU utility.

## 3.4 Shared root promotion/admission API

`root_impact.rs` computes the report; existing `shared.rs` owns the merge/admission seam. Proposed boundaries:

```rust
pub fn posterior_root_impacts(
    particles: &[BeliefParticle],
    actor: u8,
    actions: &[Action],
) -> RootImpactReport;

// Exact signature should follow existing ranked-root types rather than inventing
// a parallel candidate type. Behavior shown here is the contract.
pub fn admit_shared_roots(
    ranked: &[(Action, f32)],
    impacts: &RootImpactReport,
    cap: usize,
) -> Vec<(Action, f32)>;
```

The implementation may internally split cheap spatial analysis from expensive retained-trade analysis. CPU and native GPU must consume the same admission behavior from `shared.rs`; neither backend should independently reinterpret `RootPromotionReason`. The shared contract ends at admitted roots: exact-family resolution, trade safety, deadlines, backend-specific racing, and final arbitration stay outside this API.

Possible promotion reasons:

```rust
pub enum RootPromotionReason {
    VerifiedImmediateEscape,
    RoadAwardProtection,
    CriticalExpansionProtection,
    OpponentRouteCut,
    LatentThreatReduction,
    CloseoutCompression,
}
```

Do not create one promotion reason per strategy phrase in the research document.

# 4. Implementation Tasks

## Task 1: Implement public-board road resilience consequences

**Files:**
- Create: `engine/crates/catan-search/src/resilience.rs`
- Modify: `engine/crates/catan-search/src/lib.rs`
- Modify, only for the smallest non-strategic rules seam needed by the probe: `engine/crates/catan-core/src/state.rs`
- Consume existing helpers from: `engine/crates/catan-search/src/eval.rs`

**Interfaces:**
- Consumes: board topology, buildings, roads, award state, `longest_road_length()`, expansion helpers.
- Produces: compact exact `RoadResilience`/cut consequence data for one public state and player.

**Steps:**
- [ ] Enumerate publicly open/distance-rule-valid intersection cuts that can interrupt or deny the target player's route; keep structural vulnerability separate from the later posterior question of whether an opponent can afford/reach the site.
- [ ] For each candidate cut, evaluate a hypothetical public topology occupancy and recompute Longest Road/award/expansion consequences using core rules semantics.
- [ ] If `catan-search` cannot obtain the post-mutation award result without duplicating private `update_longest_road()` logic or fabricating resource transfers, expose the smallest non-strategic recomputation/probe helper from `catan-core`; do not move choke/resilience strategy into core.
- [ ] Never encode the award tie rule independently in `resilience.rs`; the probe result must come from the same core semantics used by normal gameplay.
- [ ] Distinguish route-length loss, award loss, expansion loss, and recoverability.
- [ ] Detect whether an alternate existing path preserves connectivity; do not reward cycles/forks that do not preserve a meaningful route or award.
- [ ] Bound output to the most consequential cuts needed by root search; avoid retaining every zero-impact graph point.
- [ ] Keep this module independent of resource-belief aggregation.

**Acceptance criteria:**
- A fragile single-intersection chain reports material loss when the cut is occupied.
- A structurally equivalent route with a real bypass reports lower damage/recovery cost.
- A visually branched road with no useful bypass receives no synthetic resilience benefit.
- The calculation uses the engine's actual Longest Road and expansion semantics rather than a replacement graph rule.

## Task 2: Add cheap disruptive spatial priors

**Files:**
- Modify: `engine/crates/catan-search/src/policy.rs`
- Modify only if the Wave 1 road corpus demonstrates rollout blindness: `engine/crates/catan-search/src/cuda/sim.cu`

**Interfaces:**
- Consumes: public board only for simulated actors.
- Produces: better ordinary action proposal/ranking for settlements and roads that have exceptional disruption value.

**Steps:**
- [ ] Add a cheap settlement disruption term for cutting rival roads, transferring/protecting Longest Road, or denying a critical expansion vertex.
- [ ] Add a cheap road disruption term for seizing a choke, changing award equity, or denying a critical endpoint.
- [ ] Keep these terms cheaper than the full `resilience.rs` root analysis; they are proposal heuristics, not proof.
- [ ] Keep CPU cheap spatial priors in the initial implementation. Add the equivalent CUDA settlement/road proposal terms in Wave 1 only if the road corpus shows that current rollout proposals or punishers miss the demonstrated tactic; otherwise defer the CUDA mutation to the evidence-triggered follow-up before Wave 2.
- [ ] Preserve existing family normalization so many spatial parameters do not acquire unfair aggregate mass merely by count.

**Acceptance criteria:**
- Known high-impact cut settlements/roads no longer sit below generic low-impact expansion alternatives solely because their production endpoint is weak.
- Normal high-production settlements/expansion roads remain competitive when the supposed block has little actual consequence.
- If Wave 1 changes CUDA settlement/road proposal scoring, the change is justified by a failing road-corpus proposal/punishment case and does not consult exact hidden opponent resource identities.

## Task 3: Build the shared root strategic-impact prepass

**Files:**
- Create: `engine/crates/catan-search/src/root_impact.rs`
- Modify: `engine/crates/catan-search/src/shared.rs`
- Modify: `engine/crates/catan-search/src/lib.rs`
- Modify: `engine/crates/catan-search/src/depth.rs`
- Modify: `engine/crates/catan-wasm/src/native_gpu.rs`

**Interfaces:**
- Consumes: weighted `BeliefParticle`s, legal/ranked root actions, tactical threat snapshot, cached public-road resilience.
- Produces: one `RootImpactReport`; `shared.rs` converts its promotion classifications plus ordinary ranked roots into the admitted root set.

**Steps:**
- [ ] Compute the public-board road baseline once per public/observation root and reuse it across particles; do not repeat identical topology work merely because hidden hands differ.
- [ ] Compute posterior-dependent tactical baseline data once per belief root.
- [ ] For each spatial root candidate, compute its public topology consequence once when public legality/topology is identical across the belief; aggregate only the hidden-state consequences that genuinely vary by particle.
- [ ] Promote only roots with material named consequences: verified escape, road-award protection, critical expansion protection, disruptive cut, latent-threat reduction, or closeout compression.
- [ ] In `shared.rs`, merge promoted roots with ordinary ranked roots before the width cap, deduplicating actions and preserving `EndTurn` retention.
- [ ] Leave exact development-family collapse in its existing later arbitration stage unless implementation evidence shows it must move; root admission must not create a second family solver.
- [ ] Make CPU belief search and native GPU consume the same `shared.rs` admission contract.
- [ ] Add compact provenance that records why a root was promoted and the named before/after deltas without serializing full baseline structures per root.

**Acceptance criteria:**
- A low-prior verified blocker can enter both CPU and native-GPU root sets before truncation.
- The same public/belief root produces the same promotion reason across CPU/native GPU.
- A high-threat state with no action that changes the threat does not promote arbitrary defensive-looking moves.
- Promotion alone never bypasses normal final search comparison.

## Task 4: Extend opponent tactical threat grammar

**Files:**
- Modify: `engine/crates/catan-search/src/threats.rs`
- Modify: `engine/crates/catan-search/src/lib.rs`

**Interfaces:**
- Consumes: one exact belief particle at a time, then weighted posterior aggregation.
- Produces: strict immediate-win proof plus expected latent threat fields.

**Steps:**
- [ ] Preserve the existing direct build/award threat path as the no-progress-card branch.
- [ ] Construct the opponent-turn probe state using the same phase/turn semantics that the threat question actually asks, then enumerate progress-card actions from `GameState::legal_actions()` rather than reproducing timing rules in `threats.rs`.
- [ ] For legal Monopoly actions, use `GameState::apply()` to transfer resources before probing direct conversions.
- [ ] For legal YOP actions, rely on `legal_actions()`/`apply()` for bank availability before probing direct conversions.
- [ ] For legal Road Building actions, rely on `legal_actions()`/`apply()` for road-piece limits, connectivity, and award updates.
- [ ] For legal Knight actions, follow the resulting robber/steal chance tail when stolen identity matters; do not credit a strict immediate-win proof unless every relevant chance outcome wins or the win is independent of the steal.
- [ ] Add relevant `BuyDevelopment` chance when an immediate VP draw can end the game under the engine's current deck.
- [ ] Let core transition rules enforce one-development-card-per-turn and bought-this-turn restrictions; threat code should not duplicate those predicates.
- [ ] Aggregate per-family posterior mass and expected consequence without relabeling expected threat mass as certain loss.

**Acceptance criteria:**
- A ready Road Building card can create a two-road Longest Road/settlement win threat that the old one-paid-road proof misses.
- A ready YOP can create an immediate build/win only when the bank can supply the pair.
- A ready Monopoly can create a build/win when the target resource transfer actually supplies it.
- A Knight can prove Largest Army transfer when legal without assuming a perfect random steal.
- A 9-VP player who can buy a development card exposes the correct immediate VP-draw chance from the remaining deck.
- No progress-card continuation bypasses `GameState` legality: same-turn purchases, prior action-card use, bank shortages, phase timing, road inventory, robber destinations, and victims are accepted or rejected by the rules engine rather than a parallel threat-only rules table.

## Task 5: Extend domestic trade safety to progress-card exploitation

**Files:**
- Modify: `engine/crates/catan-search/src/trade_safety.rs`
- Consume: new bounded threat grammar from `threats.rs`

**Interfaces:**
- Consumes: actual domestic-trade protocol states/actions plus the resulting post-resolution turn owner.
- Produces: newly enabled tactical threat set and existing hard-veto classification.

**Steps:**
- [ ] Replace `reachable_build_threats()` with a bounded `reachable_tactical_threats()` that can include ready progress-card transforms before direct builds.
- [ ] Model the real trade protocol transition rather than synthesizing `Main` for whichever participant gained resources. `OfferTrade`, `RespondTrade`, `CounterTrade`, and `ConfirmTrade` can change `trade.creator`/actor while `current_player` remains the actual turn owner.
- [ ] For each candidate trade action, resolve only the bounded response/confirmation tail needed to obtain the exchange state, then inspect the actual `current_player` and `Phase::Main` reached by `GameState`.
- [ ] Compare before-trade versus after-trade threat sets so a pre-existing opponent capability does not make every trade unsafe.
- [ ] Detect a same-turn Dirty Monopoly only when the player who can legally act after the exchange is the player holding/possibly holding Monopoly and the exchange increased the resource that Monopoly can reclaim enough to enable a material conversion.
- [ ] If the trade beneficiary is not the post-resolution turn owner, do not label the exchange a same-turn Dirty Monopoly; any next-turn risk belongs in ordinary latent threat/search unless another legal continuation proves it sooner.
- [ ] Include Road Building, YOP, and Knight only where the real post-trade turn state makes the card legal and the exchange changes the reachable tactical consequence.
- [ ] Preserve the existing near-certain posterior threshold for hard vetoes.
- [ ] Expose ordinary sub-threshold malicious-trade risk to root-impact diagnostics rather than converting it into a categorical rejection.

**Acceptance criteria:**
- A near-certain winning Dirty Monopoly continuation by the actual post-trade turn owner is hard-vetoed.
- Accepting an offer from the current player can expose same-turn Monopoly when the real confirmation path returns control to that current player.
- A counteroffer whose `trade.creator` differs from `current_player` does not incorrectly grant the counteroffer creator a fictional immediate Main phase after confirmation.
- A low-probability Monopoly possibility alone does not hard-veto an otherwise useful trade.
- A trade is not penalized merely because the opponent could already execute the same tactical line before the exchange.

## Task 6: Improve progress-card continuation policy on CUDA

**Files:**
- Modify: `engine/crates/catan-search/src/cuda/sim.cu`

**Interfaces:**
- Consumes: observation-safe public state plus the acting simulated player's own exact hidden hand in the sampled world.
- Produces: stronger rollout action proposals.

**Steps:**
- [ ] Change Road Building selection from two mostly independent `road_policy_score()` draws to a joint pair score that values coherent settlement reach, Longest Road transfer, choke capture, and bypass creation.
- [ ] Change YOP pair scoring from generic resource weights to resulting-hand build completion/closeout value, while respecting public bank visibility/availability.
- [ ] Change Monopoly resource scoring from likely public yield alone to likely yield multiplied by immediate conversion value in the acting player's own hand.
- [ ] For Monopoly/victim/resource inference, never read exact third-party resource arrays merely because the CUDA lane contains a determinized world. The acting simulated player may use its own exact hand plus public totals/public production or a later observation-safe public marginal model.
- [ ] Extend Knight/robber score with explicit Largest Army transfer/win and self-unblocking value.
- [ ] Preserve public-observation safety for opponent hand estimates and victim targeting. For any proposal scorer, two determinized worlds with the same acting player's `observation_hash()` and the same RNG input must not choose differently solely because a third party's hidden resource/dev identity differs.

**Acceptance criteria:**
- In curated Road Building positions, the damaging two-edge sequence is proposed materially more often than unrelated high-production endpoints.
- YOP selects a pair that completes a city/settlement/win over generically valuable resources when the completion is legal.
- Monopoly prefers a smaller but immediately convertible haul over a larger strategically inert haul when rollouts show the former is stronger.
- Knight proposals prioritize a legal winning Largest Army transfer over routine robber pressure.
- An observation-safety control with identical actor observation but different third-party hidden identities produces identical immediate proposal scoring/selection under the same RNG input.

## Task 7: Add closeout/response-window diagnostics

**Files:**
- Modify: `engine/crates/catan-search/src/planner.rs`
- Modify: `engine/crates/catan-search/src/root_impact.rs`

**Interfaces:**
- Consumes: existing current-turn plan completion mass and legal turn transitions.
- Produces: bounded `closeout_gain`/response-window data.

**Steps:**
- [ ] Distinguish plans that complete a decisive build/award/win within the current turn from plans that require passing control.
- [ ] Record the number of opponent decision windows before the next reachable decisive action for the small root set where the planner already has evidence.
- [ ] Use reduced response windows as a root diagnostic/promotion tie-break only when the underlying strategic endpoint is materially strong.
- [ ] Do not add a bonus for low visible VP, sandbagging, or "looking harmless."

**Acceptance criteria:**
- A true same-turn closeout is distinguishable from an otherwise similar plan that exposes one full opponent round.
- The engine does not prefer a weaker action solely because it preserves hidden/public ambiguity.

## Task 8: Add explicit-root GPU tactical benchmark infrastructure

**Files:**
- Create: `engine/crates/catan-arena/src/bin/gpu-latent-threat-benchmark.rs`
- Create: `tests/fixtures/latent-threat-tactical-corpus.json`
- Modify: `engine/crates/catan-search/src/cuda_sim.rs` only if proposal-frequency diagnostics require a benchmark-only API.

**Interfaces:**
- Consumes: deterministic scenario corpus and explicit root actions.
- Produces: machine-readable per-scenario/per-root CUDA rollout outcome plus separate proposal-coverage evidence.

**Steps:**
- [ ] Define a stable scenario schema with exact state/rules, candidate roots, hidden posterior variants, and negative-control links.
- [ ] Run explicit candidate roots through the same `CudaSim::search_root_actions()` primitive used by native GPU so root-generation bias cannot hide a rollout-evaluation improvement.
- [ ] Keep the claim narrow: this benchmark is an oracle for CUDA continuation/evaluation of supplied roots, not for native GPU's posterior allocation, exact-family collapse, shared root admission, trade safety, adaptive racing, deadline behavior, or final arbitration.
- [ ] Add a proposal-frequency mode that repeatedly invokes the CUDA rollout proposal policy without applying the action, if no existing API exposes this evidence.
- [ ] Emit JSON containing scenario ID, search budget, device identity, action proposal frequencies, root samples/errors, terminal rate, net terminal outcome, mean VP margin, variance, and selected root.
- [ ] Fail a scenario only on its declared mechanical/ordering contract; do not require an exact stochastic win rate.

**Acceptance criteria:**
- The benchmark can distinguish "the damaging continuation was rarely proposed" from "the supplied root was evaluated poorly even when forced into CUDA rollouts."
- Baseline and candidate can be compared with identical scenario and rollout seeds.
- A separate native/takeover gate verifies that production root preparation and racing preserve the tactical improvement.
- Negative controls prevent a heuristic that blindly blocks/defends from passing.

### Required initial corpus families

**Road topology**

- fragile chain with one settlement cut;
- same nominal road length with a genuine bypass/cycle;
- apparent choke with an equivalent cheap detour;
- low-production opponent settlement that transfers Longest Road;
- defensive road that preserves a critical settlement lane;
- vanity branch that adds road count but no route/award/expansion value.

**Road Building**

- two-road jump to a contested settlement;
- two-road Longest Road transfer/win;
- two-road bypass that protects an existing route;
- negative case where only one road piece remains and that single free road cannot reach a settlement, transfer Longest Road, or create a useful bypass.

**Monopoly**

- same hand size, concentrated versus diversified root hand;
- Dirty Monopoly accept versus reject;
- 2:1-port stockpile target;
- large likely haul with no useful conversion versus smaller haul that completes a city/win;
- high Monopoly posterior where the target resource has at most one card of plausible transferable mass and no material conversion, as a negative control against blanket Monopoly defense.

**Year of Plenty**

- exactly two resources short of city;
- winning settlement enabled by YOP;
- requested pair unavailable from bank as negative control.

**Knight**

- opponent on two played Knights with a ready third;
- Knight clears robber from a critical own producer before roll;
- Knight blocks the root player's critical producer and steals;
- negative case where another player already has a sufficiently large Knight lead that the candidate Knight cannot transfer Largest Army or produce another decisive conversion.

**Development purchase**

- 9 VP + dev affordability with varying remaining VP-card fraction;
- no remaining VP cards as negative control.

**Trade denial/economic independence**

- same board with domestic trading available versus root-seat embargo/disable;
- viable port conversion versus expensive port detour;
- port stockpile benefit versus Monopoly concentration cost.

**Closeout**

- same endpoint value with same-turn completion versus one-opponent-round exposure;
- weaker same-turn action versus stronger two-turn action as a negative control against excessive closeout bias.

## Task 9: Extend arena mechanism metrics and paired block output

**Files:**
- Modify: `engine/crates/catan-arena/src/main.rs`
- Modify: `engine/crates/catan-arena/src/bin/gpu-sim-agent-benchmark.rs`

**Interfaces:**
- Consumes: executed game transitions and search diagnostics.
- Produces: mechanism counters plus compact matched-block records.

**Steps:**
- [ ] Add event counters only for mechanics implemented in prior tasks: road cut, award transfer, expansion-portfolio denial/protection, progress-card conversion, Monopoly transfer, Dirty Monopoly sequence, one-turn closeout.
- [ ] Define expansion denial as the decrease in the opponent's best `expansion_option_value().portfolio_value` (or the closest existing portfolio quantity), not merely denial of the highest-pip settlement.
- [ ] Define a terminal dead-end road as an owned edge with a terminal owned-road endpoint where (a) no friendly settlement/city occupies that endpoint, (b) the public occupancy/distance rule prevents a settlement there, and (c) removing that edge leaves the player's `longest_road_length()` unchanged. Emit it only as a structural end-state diagnostic; do not infer that the road had no earlier blocking/tempo value.
- [ ] Preserve existing road/build/dev/trade/discard/robber/search metrics.
- [ ] Add per-matched-block summaries to the GPU searched-agent benchmark: block ID/seed, candidate seat outcomes, candidate wins, mean candidate VP, mean best-opponent VP, mean margin, total truncations, candidate-seat truncations, and opponent-seat truncations.
- [ ] Keep aggregate summary output for backwards readability.

**Acceptance criteria:**
- A strength result can be traced to matched blocks rather than independent game rows.
- Reports can show whether tactical mechanism rates changed alongside win rate.
- Existing benchmark semantics remain explicitly labelled `sampled-root-actions + fixed-step gpu-weighted continuations` until a live-root mode actually exists.

## Task 10: Run the tactical GPU verification ladder

**Files:**
- No production-code mutation required by this task beyond prior completed benchmark tooling.
- Freeze accepted reports under a new dated `docs/benchmarks/latent-threat-*` directory only after review.

**Required benchmark command shape from repository root:**

```bash
cargo run --release --manifest-path engine/Cargo.toml \
  -p colonist-catan-arena --features cuda-sim \
  --bin gpu-latent-threat-benchmark -- \
  --corpus tests/fixtures/latent-threat-tactical-corpus.json \
  --rollouts-per-action 512 \
  --rollout-steps 96 \
  --seed 2026092101
```

**Evidence gates:**

### G0 - Mechanical scenario consequence

For every fixture, confirm the engine transition actually creates the declared cut/award/build/win consequence. This protects the benchmark from testing an incorrectly authored scenario.

### G1 - Proposal coverage

Measure CUDA proposal frequency for consequential opponent actions. A scenario is not ready for rollout-strength interpretation if the important continuation is effectively never proposed.

Target evidence is directional and scenario-specific; do not impose one universal proposal percentage. Compare baseline versus candidate with common seeds.

### G2 - Explicit-root GPU regret

Provide all declared roots explicitly and compare GPU outcome estimates. Track:

- selected action;
- regret against the best explicit root by net terminal outcome, then VP margin;
- confidence interval/variance from rollout samples;
- negative-control behavior.

### G3 - Posterior sensitivity

For hidden-card scenarios, sweep controlled posterior mass while holding the public state fixed. Required qualitative behavior:

- action changes occur when threat probability and consequence justify them;
- no discontinuous blanket defense at tiny nonzero posterior mass;
- near-certain forced cases trigger the strict safety path.

Suggested posterior grid:

```text
0%, 5%, 15%, 30%, 50%, 75%, 95%, 99.5%, 100%
```

Not every scenario must switch actions; negative controls should remain stable.

## Task 11: Run matched takeover/fork evidence

**Files:**
- Reuse existing snapshot/takeover infrastructure in `engine/crates/catan-arena/src/main.rs` and frozen replay tooling.
- Extend serialization only if new root-impact diagnostics are required for causal explanation.

**Interfaces:**
- Consumes: identical frozen mid-game state + restored RNG/chance streams.
- Produces: paired baseline/candidate continuation outcomes.

**Steps:**
- [ ] Capture/freeze takeover positions where baseline and candidate disagree or where a targeted tactical scenario occurs naturally.
- [ ] Replay baseline and candidate from the identical state/chance stream.
- [ ] Record baseline and candidate selected root, root-choice concordance, rescue/regression, final rank, VP, award changes, tactical mechanism events, and paired root regret.
- [ ] Report a paired confidence interval for VP-margin/root-regret deltas at the source-game/block level when the corpus is large enough; do not treat many snapshots from one game as independent evidence.

**Acceptance criteria:**
- Local tactical improvements produce net paired rescues without a disproportionate regression set.
- For regressions, provenance identifies whether the cause was root promotion, rollout policy, value comparison, or safety arbitration.

## Task 12: Run fresh whole-game P3/P4 matched campaigns

**Files:**
- Use existing `engine/crates/catan-arena/src/bin/gpu-sim-agent-benchmark.rs` after Task 9.

**Screening partition:**

Use fresh development seeds only. A practical screen is at least 32 matched blocks per player count with seat rotation and zero/tightly bounded truncation.

**Validation partition:**

For a wave that passes tactical screening:

- P3: 100 matched blocks = 300 candidate-seat games;
- P4: 100 matched blocks = 400 candidate-seat games;
- run the current promotion gate with ordinary player trades enabled;
- use the same board/game/search seed family for baseline and candidate.

For the current product scope, root/candidate-seat domestic-trade disablement is a deferred robustness stress rather than a promotion gate. Preserve existing embargo evidence and tooling for later work under `docs/DEFERRED_CANDIDATE_SEAT_EMBARGO_STRESS_2026-09-03.md`; do not tune current production strategy to that artificial mode.

The current gating whole-game validation therefore contains 700 seat-rotated P3/P4 games with ordinary trades enabled. A future reopened embargo track would add its own separately predeclared evidence rather than extending the current promotion sample adaptively.

**Primary outcomes:**

- paired candidate win delta;
- paired mean VP-margin delta;
- mean rank if available;
- block-bootstrap 95% interval;
- truncation/deadline rate;
- candidate decision count and search effort.

**Mechanism outcomes:**

- successful road-cut frequency;
- root road-cut exposure;
- Longest Road transfer rate;
- best expansion-portfolio value denied/protected;
- Monopoly cards transferred and conversion rate;
- Dirty Monopoly exploitation rate;
- YOP/RB/Knight conversion rate;
- one-turn closeout rate;
- explicitly defined terminal dead-end-road rate;
- port/maritime usage under trade denial when the deferred embargo stress track is active;
- discard exposure.

**Promotion rule:**

A wave is not accepted solely because aggregate win rate is higher. It must:

1. pass its targeted tactical/negative-control corpus;
2. show lower or equal explicit-root tactical regret;
3. avoid a material matched whole-game strength regression;
4. avoid unacceptable latency/truncation growth;
5. preserve information-set safety.

Do not require every secondary mechanism metric to move in a preselected direction. Use them to explain the causal path and detect overfitting.

## Task 13: Add hostility stress as an offline diagnostic

**Files:**
- Prefer a focused addition in `engine/crates/catan-search/src/depth.rs` or a small helper module if the implementation cannot remain local.
- Modify arena benchmark tooling to expose the stress parameter.
- Do not modify tracker/runtime hostility state in this task.

**Interfaces:**
- Consumes: normal MaxN value and bounded anti-root objective mixture.
- Produces: root brittleness under controlled hostile-policy assumptions.

**Proposed stress utility for simulated opponent `i`:**

```text
U_i(a | h) = (1 - h) * V_i(a) + h * (1 - V_root(a))
```

with `h` used only in explicit offline experiments initially.

**Suggested stress grid:**

```text
h = 0.00, 0.25, 0.50, 0.75, 1.00
```

`h = 0` should recover ordinary MaxN-like selfish play; `h = 1` approximates fully anti-root stress.

**Steps:**
- [ ] Evaluate serious root alternatives under the stress grid.
- [ ] Record which actions are unusually brittle to targeted play.
- [ ] Compare stress predictions against recorded robber/block/embargo behavior before considering online estimation.

**Acceptance criteria:**
- Ordinary production behavior remains MaxN at `h = 0`.
- The stress tool identifies fragile choices without replacing production search with Paranoid by default.
- No pair-specific hostility posterior is shipped from this task.

## Task 14: Add observation-safe public resource marginals only if GPU evidence requires them

**Files:**
- Potentially modify: `src/core/types.ts`
- Potentially modify: `src/core/tracker.ts`
- Potentially modify: `src/worker/deep-search.ts`
- Potentially modify packed native/CUDA state contract if consumed on device.

**Dependency:**

Do not begin this task merely because richer opponent beliefs are theoretically desirable. Begin only if Monopoly/robber/trade rollout benchmarks show that public-production + hand-total estimates materially limit strength after the earlier policy fixes.

**Required contract:**

Maintain a public-event-derived expectation per player/resource that does not condition on the root player's exact private hand.

Examples of public evidence allowed:

- observed production gains;
- public maritime/domestic trade transfers;
- public build spends;
- public discards where identities are known;
- public Monopoly/YOP transfers where resource identity is known;
- public hand totals.

Do not expose root-private deductions that another simulated actor could not know.

**Acceptance criteria:**
- Simulated opponent policy gains richer public card inference without information leakage.
- Given two reconstructed histories with identical public events and identical public hand totals but different root-private resource identities, every non-root player's public resource marginals are bitwise identical.
- The old public-production fallback remains valid when event history is incomplete.

## Task 15: Extend learned policy/value features only after structural gates pass

**Files:**
- Modify: `engine/crates/catan-search/src/features.rs`
- Modify expert-data generation only as required to emit the new features/labels.
- Reuse: `scripts/train-strategic-model.py`, `scripts/run-expert-iteration.mjs`, GPU zoom tooling.

**Interfaces:**
- Consumes: proven structural quantities from accepted tactical waves.
- Produces: compact model inputs that may learn when the structural effects matter.

**Candidate learned features after evidence:**

- maximum/public road cut damage;
- route recovery/bypass cost;
- posterior latent threat mass by family;
- Monopoly concentration exposure;
- closeout response-window count;
- trade-access/economic-independence state;
- hostility stress brittleness only if later behavior evidence supports it.

**Steps:**
- [ ] Generate expert labels from the stronger high-budget production/reference search, not the pre-fix policy.
- [ ] Use grouped held-out board/chance seed splits.
- [ ] Reject learned variants that improve training fit but worsen held-out action regret or matched arena strength.
- [ ] Keep the learned head optional/unpromoted until the existing promotion contract is satisfied.

**Acceptance criteria:**
- Learned features compress already-proven strategic behavior rather than inventing a second unverified strategy layer.
- Held-out policy/value metrics and matched game strength both support promotion.

## Task 16: Expose causal decision explanations

**Files:**
- Modify: `engine/crates/catan-search/src/depth.rs` provenance types
- Modify: `engine/crates/catan-wasm/src/native_gpu.rs` provenance output
- Modify: `engine/crates/catan-wasm/src/lib.rs`
- Modify: `src/worker/deep-search.ts`
- Modify user-facing reasoning formatter only where it consumes existing diagnostics.

**Interfaces:**
- Consumes: root promotion reason, before/after threat fields, exact-family/safety authority, search values.
- Produces: factual explanation ingredients.

**Steps:**
- [ ] Emit bounded numeric/root identifiers, not full `GameState` blobs.
- [ ] Distinguish "root was promoted for coverage" from "root won search" and from "safety override replaced search."
- [ ] Surface exact measured reasons such as award protection, expansion preservation, Monopoly-risk reduction, or same-turn closeout.
- [ ] Keep natural-language generation downstream; the engine provides evidence, not prose narratives.

**Acceptance criteria:**
- A reviewer can reconstruct why an action entered search and why it ultimately won/lost without inferring from generic strategy text.
- Explanations do not claim a tactical threat was certain when only posterior risk was measured.

# 5. Dependency and Delivery Waves

## Wave 1 - Road/disruption foundation

Tasks:

- Task 1: road resilience;
- Task 2: CPU disruptive spatial priors, with CUDA road/settlement proposal changes only if the road corpus demonstrates rollout blindness;
- Task 3: shared root impact for spatial promotions;
- Task 8: initial road-focused GPU tactical benchmark.

Evidence-triggered Wave 1 CUDA rule:

```text
road resilience + CPU root coverage
        ->
road explicit-root/proposal corpus
        ->
if CUDA misses the demonstrated road/settlement punishment:
    make the narrow road/settlement rollout-policy repair
else:
    do not change CUDA policy yet
```

Road Building, YOP, Monopoly, and Knight CUDA policy work remains in Wave 2.

Why first:

- public-board consequences are deterministic and easy to inspect;
- they exercise the full pipeline from proposal -> root admission -> GPU rollout -> benchmark;
- they establish the reusable evidence architecture for hidden-card work.

Wave 1 must be independently reviewable and promotable.

## Wave 2 - Hidden progress-card tactical closure

Tasks:

- Task 4: Monopoly/RB/YOP/Knight/dev-buy threat grammar;
- Task 6: CUDA progress-card policy competence;
- expand Task 8 corpus with hidden-card scenarios.

Do not combine Wave 2 with hostility modeling.

## Wave 3 - Malicious trade and closeout

Tasks:

- Task 5: progress-card-aware trade safety;
- Task 7: closeout/response-window diagnostics;
- Task 16: causal provenance for new decisions.

Dirty Monopoly is the primary positive control for this wave.

## Wave 4 - Strength evidence

Tasks:

- Task 9: mechanism metrics/paired block output;
- Task 10: tactical GPU gates;
- Task 11: takeover evidence;
- Task 12: fresh whole-game P3/P4 campaigns.

No new strategic heuristic should be tuned during final holdout evaluation.

## Wave 5 - Hostility research

Task:

- Task 13 only.

Production hostility estimation is explicitly not authorized by this plan without held-out behavior evidence.

## Wave 6 - Better public opponent beliefs if required

Task:

- Task 14 only if benchmark evidence identifies the current public-resource inference as a material remaining bottleneck.

## Wave 7 - Learning/compression

Task:

- Task 15 after accepted structural waves have generated better expert labels.

# 6. GPU Verification Design

## 6.1 Why explicit-root GPU evaluation is mandatory

There are two independent failure modes:

```text
A. root omission
   correct action never enters search

B. continuation weakness
   correct action enters search, but simulated opponents fail to punish/convert
```

The existing searched-agent GPU benchmark mixes both because candidate roots are sampled from the same rollout policy used in continuation.

The new tactical benchmark must report both:

```text
proposal frequency
        +
explicit-root outcome
```

so an improvement can be localized.

## 6.2 Common-random-number comparisons

For each tactical scenario:

- baseline and candidate use identical base state;
- identical explicit roots;
- identical rollout seed;
- identical rollout count/horizon;
- identical hidden posterior weights.

Only the engine behavior under review should differ.

## 6.3 Rollout budgets

Default tactical screen:

- 256 rollouts/action;
- 64-96 rollout steps.

Promotion-quality tactical run:

- 512-1,024 rollouts/action for ambiguous roots;
- 96 rollout steps unless a scenario demonstrates a shorter sufficient horizon;
- extend horizon only when the tactical consequence occurs beyond the current horizon and the extra depth improves stability rather than merely adding noise.

Do not assume "more rollout steps is always stronger"; the existing P3 budget sweep already showed non-monotonic strength.

## 6.4 Tactical regret

For scenario `s`, define the explicit-root reference using the benchmark's existing interpretable ordering:

1. net terminal outcome;
2. mean VP margin;
3. shorter mean game length as tie-break.

Report:

```text
selected_root
best_explicit_root
terminal_outcome_gap
vp_margin_gap
proposal_frequency(selected_root)
proposal_frequency(best_root)
```

Do not collapse all of this into one opaque strategic score.

# 7. Whole-Game Evidence and Statistical Protocol

## 7.1 Evidence partitions

Use reproducible but separate seed families.

Proposed bases:

```text
development/screening:
  board  = 2026092101
  game   = 2026092102
  search = 2026092103

validation:
  board  = 2026102101
  game   = 2026102102
  search = 2026102103

final holdout:
  board  = 2026112101
  game   = 2026112102
  search = 2026112103
```

The exact integers can be changed once during plan review. After implementation starts, freeze them in the benchmark report and do not rotate seeds in response to unfavorable results.

## 7.2 Baseline definition

The baseline for each wave is the repository revision immediately before that wave's production behavior changes, with the same benchmark harness/reporting version where possible.

If benchmark tooling itself changes during the wave:

1. run the old engine through the new measurement tooling;
2. run the candidate engine through the same tooling;
3. compare only those two new-format reports.

Do not compare old aggregate JSON against a new richer benchmark as if harness semantics were identical.

## 7.3 Block bootstrap

Bootstrap matched board/chance blocks, preserving all seat rotations inside each sampled block.

Report 95% intervals for:

- paired win-share delta;
- paired VP-margin delta;
- selected mechanism-rate deltas when denominators are sufficient.

Avoid treating each root decision or trajectory sample as an independent observation.

## 7.4 Truncation and deadline contract

A strength ranking is invalid if one variant benefits from materially different truncation/deadline failure behavior.

Required report fields:

- total games;
- terminal games;
- truncated games;
- truncation rate;
- candidate decisions;
- root actions evaluated;
- mean turns/actions;
- live decision deadline share where applicable.

If truncation exceeds the benchmark's accepted threshold, raise max-turn/max-action limits and rerun the matched pair rather than interpreting the partial ranking.

# 8. Promotion Gates by Wave

## Wave 1 road/disruption gate

Must demonstrate:

- known cut/block actions enter CPU and native-GPU roots;
- the road corpus establishes whether CUDA proposal/punishment is already competent; if a Wave 1 CUDA repair is triggered, the demonstrated consequential action becomes materially more reachable without weakening observation safety;
- explicit-root road-defense regret does not worsen;
- negative-control detours/vanity branches are not overvalued;
- no material P3/P4 whole-game regression in screening.

## Wave 2 progress-card gate

Must demonstrate:

- Monopoly/RB/YOP/Knight threat continuations are legal/timing-correct;
- CUDA can execute relevant conversions;
- posterior sensitivity is gradual/contextual except strict forced-loss thresholds;
- dev-purchase immediate VP chance is represented correctly;
- no information leakage.

## Wave 3 trade/closeout gate

Must demonstrate:

- Dirty Monopoly near-forced cases are rejected;
- low-probability Monopoly cases remain available to ordinary search;
- closeout diagnostics distinguish response windows without artificial stealth bonuses;
- real/takeover disagreements have causal provenance.

## Wave 4 release-strength gate

Must demonstrate:

- all targeted tactical families pass positive and negative controls;
- matched takeover has non-negative net rescue evidence or regressions are understood and accepted;
- fresh P3/P4 validation does not materially regress strength;
- latency/truncation remains within the approved live budget;
- final holdout was not used for tuning.

# 9. Information-Set Safety Contract

Every implementation/review should classify a datum into one of three categories.

## Exact private-to-actor

A simulated actor may know:

- its own exact resource hand in the sampled world;
- its own exact development-card identities in the sampled world;
- its own bought-this-turn development identities;
- legal actions derived from those exact holdings.

## Public to all actors

A simulated actor may know:

- board, roads, buildings, ports;
- public victory points/awards;
- hand totals;
- development-card totals and public played-card counts;
- public trade events/embargoes;
- public production/spend history if represented in an observation-safe public model;
- public bank composition only when the table/UI makes it public under the engine contract.

## Root-private inference - forbidden to simulated opponents

A simulated opponent must not receive:

- the root player's exact private hand;
- a posterior conditioned on the root player's exact private hand unless transformed into an independently public inference contract;
- hidden development identities belonging to third parties;
- exact hidden bank composition when bank state is not public.

A reviewer should reject any CUDA or Rust policy shortcut that crosses this boundary even if benchmarks improve.

# 10. Performance Boundaries

## Root strategic-impact budget

The root prepass should be bounded by action class:

- begin by analyzing every legal/relevant settlement and road root on the single public topology so destructive spatial omissions cannot be hidden by a heuristic prefilter;
- measure the actual cost before introducing a lossy first-stage spatial filter; if profiling shows the full spatial pass is material, filter only after preserving a mechanically guaranteed set of award-changing/cut/critical-expansion candidates;
- cache baseline public topology by the applicable public/observation identity and reuse it across hidden particles;
- cache bank/deck-derived data across particles only when the information contract proves those values are identical; hidden bank/deck composition must not be assumed globally cacheable;
- analyze exact development-card families through existing family collapse rather than every parameterization independently;
- do not run expensive soft malicious-trade analysis over every generated offer bundle; first use existing materiality/ranking to select a bounded trade subset, while strict incoming/confirm safety remains exhaustive where required.

## Leaf path

Do not add the full `LatentThreatSnapshot` or `RoadResilience` computation to every `evaluate()` invocation in the initial waves.

## CUDA path

Keep device-resident rollout flow intact. Prefer local integer/public-state scores and pair-wise selection changes over host/device round trips. For Road Building, do not hard-code an assumed register/occupancy budget in the plan: profile the straightforward bounded pair scorer first, then use a small top-k/beam first-road scheme only if device evidence shows exhaustive legal-pair scoring materially harms occupancy or rollout throughput.

## Live latency

The user is willing to spend roughly 2-4 seconds on strong decisions. That budget does not justify uncontrolled root analysis. The implementation should still exploit reuse/caching so added time buys search evidence rather than duplicate recomputation.

# 11. Failure Modes Reviewers Should Look For

1. **Evaluator creep:** threat terms quietly added to every leaf because root promotion was harder to implement.
2. **Double counting:** the same road award/expansion benefit appears in ordinary eval, root impact, and a new additive search bonus.
3. **Promotion becoming choice:** a promoted root receives enough synthetic score that rollout evidence cannot overturn it.
4. **Information leakage:** simulated opponents use the exact sampled root hand or root-conditioned tracker posterior.
5. **Threat grammar becoming search:** `threats.rs` recursively explores broad game trees instead of bounded transforms.
6. **Dirty Monopoly false positives:** any possible Monopoly card causes trade rejection.
7. **Road "web" proxy:** branches/cycles receive bonus without exact resilience benefit.
8. **Benchmark ceiling:** relying on the existing 101/102 P3 cohort as proof of improvement.
9. **Proposal/evaluation conflation:** a GPU benchmark declares a root weak when the real failure is that it was never proposed.
10. **Seed overfitting:** rerunning new seed families until one supports the desired conclusion.
11. **Hostility overreach:** Paranoid-like play ships before real behavior evidence.
12. **Testing the scenario author:** a fixture's declared cut/win is mechanically false but benchmark logic assumes it.
13. **Trade bundle explosion:** soft latent-threat analysis runs across every generated offer and consumes the live budget before search.
14. **CPU/GPU drift:** CPU root promotion is implemented but native GPU retains old candidate generation.
15. **Explanation overclaim:** UI says "opponent has Monopoly" when the engine only measured posterior mass.

# 12. Explicit Non-Goals

This plan does not authorize:

- fixed strategy personalities as final decision rules;
- full nested belief particles for every simulated actor;
- social-chat persuasion or coalition messaging;
- intentional public-score sandbagging;
- automatic hostility estimation from a player simply being ahead;
- full Paranoid production search;
- replacing MaxN with a risk-averse objective such as CVaR without separate product intent;
- broad opening evaluator rewrite;
- replacing the existing weighted tracker posterior;
- changing game rules to simplify tactical analysis;
- reintroducing lossy strategic particle coresets in production;
- shipping a learned policy/value model before structural evidence is accepted;
- partial-wave value blending or acceptance of a deeper wave that is incomplete across retained roots/positive-weight particles;
- global same-turn action canonicalization such as "always BuyDevelopment before trade" merely because two fixed-outcome action permutations can reach the same final state;
- a new recursive search transposition table or broad same-turn DAG rewrite;
- reopening existing domestic-trade candidate caps or sibling-budget allocation as part of the Latent Threat mission without new evidence of a remaining defect.

# 13. Review Questions for the Next Agent

The reviewing agent should explicitly answer these questions rather than only proofreading the plan.

## Architecture

1. Is `resilience.rs` the correct owner for exact public-board cut/recovery consequences, or does an existing module already own this responsibility more cleanly?
2. Is the revised ownership split correct: `root_impact.rs` computes consequences while existing `shared.rs` owns the final observation-safe merge/admission seam consumed by CPU/native GPU?
3. Does the implementation preserve one-way ownership by keeping opponent threat enumeration independent of `exact.rs`, while `exact.rs` remains the authoritative solver for our own compact progress-card parameter families?
4. Does any proposed field duplicate an existing evaluator/planner quantity strongly enough to cause double counting?

## Correctness

5. Are the proposed Monopoly, YOP, Road Building, Knight, and dev-purchase continuations legal under every phase/timing rule represented by `GameState`?
6. Does the Knight proof handle random steals conservatively enough?
7. Does the road-cut model correctly handle opponent settlements interrupting Longest Road, own settlements, loops, ties, and award transfer?
8. Does trade safety follow the actual response/counter/confirm transition until it knows the resulting `current_player`, rather than assuming `trade.creator` or the resource beneficiary receives the immediate Main phase?

## Information sets

9. Does any CPU or CUDA proposal accidentally inspect exact third-party hidden resources or development identities?
10. If public resource marginals are later added, is there a clean way to prove they are independent of root-private information?

## Performance

11. Which root-impact calculations are proven identical under `public_hash()`/`observation_hash()` and can therefore be cached once, and which bank/deck/hidden-state quantities must remain particle-specific?
12. What is the measured cost of analyzing all legal spatial roots once on the public topology under the live 2-4 second budget, and is any prefilter actually necessary after caching?
13. What does device profiling show for joint Road Building pair scoring, and is a top-k/beam scheme needed to preserve occupancy/rollout throughput?

## Evidence

14. Does the explicit-root benchmark correctly limit its claim to CUDA rollout evaluation of supplied roots, with takeover/native evidence covering the additional production root-preparation/racing/arbitration semantics?
15. Do matched reports include the block-level fields needed for paired intervals, plus candidate/opponent truncation splits and root-choice concordance where a matched-position comparison exists?
16. Are expansion denial and terminal dead-end-road counters defined mechanically enough to attribute without subjective post-hoc labels?
17. What negative control should be added before accepting each tactical family?

## Scope

18. Which tasks are unnecessary for the first road/disruption slice and should be deferred rather than bundled?
19. Is hostility stress sufficiently separated from the demonstrated tactical omissions?
20. Can all new root promotion behavior remain behind the existing `shared.rs` observation-safe root seam without refactoring unrelated search preparation?

# 14. Recommended First Execution Slice

Do not execute the entire plan as one implementation wave.

The first coding mission should be limited to:

1. `resilience.rs` exact public-board road cut/award/expansion consequences;
2. cheap CPU disruptive settlement/road proposal terms;
3. shared CPU/native-GPU spatial root promotion based on those consequences;
4. road-only subset of `gpu-latent-threat-benchmark` and its positive/negative controls;
5. only if that road corpus demonstrates CUDA rollout blindness, the smallest public-information-safe settlement/road proposal repair needed to make the demonstrated punishment reachable in rollouts.

Stop after that slice and review its tactical GPU evidence before adding hidden progress-card logic. Do not add Road Building/YOP/Monopoly/Knight rollout changes preemptively.

This first slice proves the core architecture:

```text
mechanical consequence
      ->
CPU proposal/root coverage
      ->
shared root admission
      ->
CPU/native-GPU candidate parity
      ->
road explicit-root + proposal evidence
      ->
[only if demonstrated necessary]
narrow CUDA road/settlement rollout repair
```

If this pipeline is not clean, adding Monopoly/YOP/Road Building/Knight complexity will make diagnosis harder rather than improving the engine.

# 15. Definition of Done for the Full Design

The design is fully implemented only when all of the following are true:

- road topology distinguishes fragile and genuinely resilient networks;
- disruptive low-production settlements/roads can enter both CPU and native-GPU search;
- latent progress-card threats are represented with legal posterior-aware continuations;
- Dirty Monopoly and other trade-created tactical threats are compared before/after the exchange;
- hard vetoes remain reserved for near-certain severe outcomes;
- CUDA rollout opponents competently execute the damaging tactics the engine is expected to anticipate;
- explicit-root tactical GPU benchmarks pass positive and negative controls;
- posterior sensitivity behaves contextually rather than as worst-case paranoia;
- matched takeover evidence shows the tactical changes improve or preserve outcomes from identical states/chance streams;
- fresh matched P3/P4 campaigns show no material whole-game strength regression and provide mechanism evidence for any claimed improvement;
- CPU/native-GPU root semantics remain aligned;
- simulated opponents remain information-set safe;
- decision provenance exposes actual causal strategic evidence rather than post-hoc generic explanations;
- hostility modeling remains diagnostic until independently justified;
- learned policy/value work begins only after the structural behavior is proven.

The intended final engine behavior is:

> **Posterior-aware adversarial anticipation over public topology, hidden development-card uncertainty, resource-belief uncertainty, and bounded opponent-policy uncertainty, used to improve tactical coverage and root search while preserving search as the final decision authority.**
