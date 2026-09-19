# Agent B — Mission B1: Jev Calibration and Opening Feature Validation

## Role

Role: investigation + research tooling  
Review independence: separate from A1 implementation  
Wave: 1  
Effort: ~20%

## Repository

Repository: /home/hamza/repo/colonist-assistant  
Workspace: /home/hamza/repo/colonist-jev-calibration  
Can start: now  
Depends on: none for investigation; final calibration waits for A1 feature semantics

## Read first

- /home/hamza/repo/colonist-jev-calibration/AGENTS.md
- /home/hamza/repo/colonist-jev-calibration/docs/JEV_STRATEGY_RESEARCH_PLAYBOOK.md
- /home/hamza/repo/colonist-jev-calibration/docs/agent-work/opening-strategy-rebuild/README.md

Load: mcp-harness-router, persistent-agent-loop; systematic-debugging for concrete failures.

## Objective

Own approved items 6–7 and prepare item 8 evidence:

6. reevaluate opening features/components against matched terminal simulation;
7. fit/calibrate only after feature semantics are correct.

Build a reusable calibration workflow inspired by the supplied Jev methodology:

- direct Jev judgment remains an independent comparator;
- decomposed dimensions are cached features, not truth;
- grouped/held-out splits are designed before fitting;
- hard-negative / false-positive corpus;
- question-family-specific thresholds;
- wording/order invariance;
- pairwise/ranking validity checks;
- local cheap baselines where meaningful;
- matched counterfactual outcomes are the authority.

## Current evidence

- D17 was a Jev false positive when exact road-intent facts were omitted.
- D31 shows Jev can find a save-vs-spend issue, but admission still requires simulation.
- The supplied article shows decomposition helps only where a direct judge is weak and can make false positives much worse if dimensions encode the wrong concept.
- Jev confidence is not automatically calibrated to correctness.
- Full candidate ranking must not assume score ordinality before validation.
- hill6758/task9783 are known positive cases; D17 is a known Jev false positive.

## Ownership

You own:

- research-only scripts/docs/ignored benchmark artifacts needed to label and evaluate opening positions;
- schema for direct-judge result, dimension features, confidence, screening outcome, matched counterfactual label, and split group;
- calibration/threshold methodology and diagnostics.

You do not own:

- production opening objective/evaluator semantics (A1);
- runtime Jev integration;
- TypeSafe credentials.

## Success conditions

- Reproducible dataset/eval path comparing direct Jev vs decomposed dimensions vs deterministic engine features against matched outcome labels.
- Grouped/held-out splits avoid board/template leakage.
- Hard-negative cases include attractive-but-bad ports, five-resource low-throughput starts, unreachable/self-unfundable repair targets, non-causal denial, and mechanically dominated roads.
- Direct Jev judgment stays separate from dimensional fitting unless a deliberate experiment demonstrates benefit.
- Identify which question families benefit from decomposition versus one direct judge.
- Establish per-question-family threshold calibration and invariance/ranking checks.
- Cache raw Jev outputs so later fitting/threshold sweeps are local.
- No secrets in source/artifacts.
- No production behavior changes.

## Testing / validation authority

Research/eval scripts and focused tests authorized. Reuse existing `benchmark-results/jev-lab` inputs and generate ignored outputs. If `TYPESAFE_API_KEY` is unavailable locally, complete the offline framework and mark live Jev calls blocked rather than embedding a key.

## Execution lifetime

persistent-agent-loop

## Out of scope

- A1 production evaluator changes.
- D31 production fix.
- final integration.

## Finish report

Return:

1. status;
2. Agent B, Mission B1;
3. branch/worktree and commits/artifacts;
4. dataset schema and split strategy;
5. direct-vs-dimensions methodology;
6. false-positive/hard-negative corpus;
7. threshold/invariance/ranking checks;
8. validation actually performed;
9. blockers such as missing local API credential;
10. exact handoff A1/final validation needs.
