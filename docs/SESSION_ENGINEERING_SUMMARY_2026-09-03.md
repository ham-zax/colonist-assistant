# Engineering session summary — 2026-09-03

This document records the accepted Catan-engine work completed during the 2026-09-03 session and the state from which further learning experiments should continue.

## Scope

The repository implements a Catan assistant. All opponent, blocking, robber, search, native-runtime, and GPU terminology below refers to Catan gameplay and software that evaluates Catan positions.

## Integrated strategic/search work

### Wave 4 — causal strength and takeover evidence

Wave 4 was accepted for the ordinary-domestic-trading product scope. The accepted line includes shared root-admission/tactical fixes, matched takeover evidence, repeated-stream analysis support, and whole-game GPU validation. Candidate-seat trade disablement remains a deferred robustness stress and is not a production optimization target.

The final Wave 4 evidence did not demonstrate a generalized Road49 regression. The previously severe Road49 continuation remained an unresolved isolated case rather than a promotion blocker.

### Wave 5 — offline opponent-pressure sensitivity

Wave 5 added an offline MaxN sensitivity diagnostic using the controlled opponent objective

`U_i(a | h) = (1 - h) * V_i(a) + h * (1 - V_root(a))`

for `h in {0, .25, .5, .75, 1}`. The root player remains on ordinary MaxN utility and `h=0` delegates to ordinary bounded MaxN. The diagnostic has no production callers.

The reviewed tactical corpus showed value sensitivity but no selected-root reversal through `h=1`. This does not justify live opponent-pressure inference, pair-specific behavior models, or a Paranoid default. No genuine recorded human-behavior corpus currently supports such a model.

### Wave 6 — deliberately skipped

Task 14 richer public-resource inference remains skipped. Accepted evidence has not shown public-resource estimation to be a material remaining gameplay-strength bottleneck.

## Wave 7 — learning/compression infrastructure

Main commit `eebb968f5383219ce3ecbbf641b6841b5c886d0c` integrates the Wave 7 learning-screening infrastructure as one squash commit. The squash intentionally excludes intermediate JSONL training corpora from main history.

### Strategic feature schema

The strategic feature schema is now version 2. Action features increase from 48 to 52 by appending four public-board road-impact quantities from the existing authoritative `compute_spatial_root_impacts()` owner:

1. longest-road loss prevented;
2. longest-road loss inflicted;
3. road-award VP swing;
4. expansion-portfolio delta.

The learning layer consumes these structural values; it does not reimplement road strategy. State feature width remains 247.

The bundled checkpoint remains schema 1, while runtime features are schema 2. `VALUE_MODEL_PROMOTED` and `POLICY_MODEL_PROMOTED` both remain `false`, so the learned heads cannot affect live Catan decisions.

### Teacher contracts

Schema-2 PUCT teacher records carry visit-share policy targets and normalized strategic root-value vectors. They may support both policy and value training.

Native-GPU teacher records are policy-only evidence. Their policy target is one-hot on the native engine's final chosen action. Native adaptive-racing sample counts are diagnostics, not preference visits, and rollout VP vectors are not treated as strategic value, action-regret, or teacher-top-1 labels.

Mixed PUCT/native-GPU corpora are rejected because their policy semantics differ.

### Evidence gates repaired during review

The Wave 7 evidence path now enforces all of the following:

- held-out value calibration uses a temperature fitted only on training groups;
- the exact historical Task 15 baseline is unchanged state features plus the first 48 action features;
- an omitted or non-48 baseline cannot report a passed feature ablation;
- schema-2 records must contain exactly 52 action features across every policy group;
- the CUDA screen defaults to `--device cuda`; CPU fallback requires an explicit request;
- native-GPU policy records must contain a genuine one-hot final decision;
- stale native adaptive-racing visit-share labels are rejected;
- generated training corpora are not tracked in Git;
- invalidated historical fold files with stale machine-readable `accepted=true` fields are not retained in the landed tree.

## Corrected CUDA feature screen

The corrected native-GPU policy-only five-fold screen used 77 previously generated schema-2 teacher records across 24 independent `boardSeed:chanceSeed` groups, with the exact 48-feature baseline and the RTX 3070 Ti.

The 52-feature candidate did **not** pass the matched feature-ablation screen:

- folds passing: 2 / 5;
- pooled candidate policy cross-entropy: `2.4273587447`;
- pooled 48-feature baseline policy cross-entropy: `2.4270200583`;
- candidate minus baseline cross-entropy: `+0.0003386864` (worse);
- candidate final-choice top-1 agreement: `13.846%`;
- baseline final-choice top-1 agreement: `16.923%`;
- top-1 delta: `-3.077` percentage points.

Therefore the four-feature tail is **not promotion-qualified**. The result is retained as a negative learning result rather than tuned against the same held-out groups.

## Existing PUCT development corpus

A pre-existing temporary schema-2 PUCT corpus is available at:

`/tmp/wave7-task15-snapshot-screen/expert-action.jsonl`

It contains 179 Main-phase samples across 34 independent board/chance groups, with 247 state features and 52 action features. It was already used for Wave 7 development, so it is suitable for further model development/training but is **not** a fresh final promotion holdout.

Its prior matched 52-vs-48 development screen also rejected the four-feature tail. It did, however, demonstrate valid PUCT value-target semantics and enough groups to exercise the full policy/value training pipeline.

## Compute policy

CPU gameplay/corpus campaigns are disabled for ongoing work. Do not run CPU `colonist-arena` gameplay campaigns, CPU expert-generation campaigns, or CPU PUCT/MaxN strength campaigns.

Small compile, syntax, serialization, and feature-contract checks are allowed when required. Gameplay/model-strength work should use existing CUDA/native-GPU paths. If an evidence requirement cannot be satisfied without a CPU gameplay campaign, report the blocker instead of silently falling back to CPU.

## Current continuation plan

1. Build the current unpacked Chrome extension into `dist/` for live play. The build performs one required release WASM compile; it is a build step, not a gameplay benchmark.
2. Keep learned heads unpromoted in the live extension.
3. Run development-only policy/value training on the existing PUCT corpus using CUDA, recording checkpoints and metrics outside Git.
4. Use a fresh independent GPU-generated state corpus before making any promotion claim or tuning decision based on final evidence.
5. Do not begin Task 14 or Task 16 as a side effect of the training work.

## Extension build and background training launch

After the session record was created, the packaged WASM artifact was synchronized with the current strategic schema and committed as `16b841fc8b3e3ee452a3e5e7a4d55f2f8e07f073`. The unpacked Chrome extension was then rebuilt cleanly into `dist/` as:

`0.9.1 · main@16b841fc8b3e · 2026-09-03T13:55:37.885Z`

The live extension still has both learned-head promotion flags disabled.

A development-only CUDA training corpus was assembled from existing schema-2 PUCT teacher artifacts without generating new gameplay. It contains 731 deduplicated samples across 40 `boardSeed:chanceSeed` groups, state width 247, action width 52, and has SHA-256:

`56ea77ceb271f8fd2e57c36da394a3530b870bbd67aaa349f3b7469247259b5a`

The corpus and training outputs live under `/tmp/wave7-gpu-training/` and are not Git artifacts. They combine prior development data, so results from this sweep remain model-development evidence, not a fresh promotion holdout.

The background sweep uses `scripts/benchmark-gpu-zoom.py --device cuda`, the exact 48-feature baseline, a 50% grouped validation split (20 of 40 groups), and several bounded hidden-width/epoch configurations. CPU math libraries are capped to one thread. The launcher waits for low observed GPU utilization before starting each configuration so an already-running native GPU companion has opportunities to serve live play between training runs.
