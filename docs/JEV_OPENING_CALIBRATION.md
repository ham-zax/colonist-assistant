# Jev opening calibration protocol

This protocol turns Jev opening judgments into reproducible research evidence. It does not change the production evaluator and does not make Jev an authority. Matched terminal counterfactuals remain the label source.

The governing methodology is [JEV_STRATEGY_RESEARCH_PLAYBOOK.md](JEV_STRATEGY_RESEARCH_PLAYBOOK.md). The machine-readable case contract is [jev-opening-calibration-schema-v1.json](benchmarks/jev-opening-calibration-schema-v1.json), and the required positive/false-positive/hard-negative inventory is [jev-opening-hard-negative-corpus-v1.json](benchmarks/jev-opening-hard-negative-corpus-v1.json).

## Experimental unit

One JSONL record is one opening candidate set at one reconstructed decision. It contains:

- a question family: `first_settlement`, `second_settlement`, or `setup_road`;
- a predeclared `splitGroup` and `partition`;
- a feature-semantics version and `draft` or `validated` status;
- sanitized source identity and rule mode;
- candidate actions, corrected deterministic features, and an optional cheap deterministic score;
- raw-cache references for repeated direct and decomposed Jev runs;
- screening outcome;
- pairwise labels from matched terminal simulation.

The direct comparator is exactly one bounded choice question over the candidate set. The decomposed comparator scores family-specific dimensions. Direct output is not inserted into the dimension vector. A later stacking experiment would require its own frozen analysis plan and held-out benefit.

## Split contract

Freeze the split manifest before collecting Jev outputs or fitting anything.

Use one `splitGroup` for all observations that share exploitable structure, including:

- the same board or recorded opening template;
- seat rotations of that board;
- player-trades ON/OFF variants;
- chance-seed repetitions and forced-root branches;
- paraphrases, candidate-order permutations, and repeated Jev calls.

Assign each whole group to either `calibration` or `holdout`. The calibration partition is the only partition used to select dimension weights, confidence use, or family thresholds. Do not move a group after seeing an outcome or Jev answer. Do not repeatedly inspect the locked holdout while changing prompts or dimensions.

The analyzer rejects a group that appears in both partitions. It also refuses threshold selection unless every case says its feature semantics are `validated`. A1 must provide that version after causal port, self-funding, and denial semantics are stable.

## Label contract

Each conclusive pairwise label must come from matched terminal simulation:

- identical reconstructed pre-root state and chance history;
- one forced root action per branch;
- normal continuation policy after the root;
- matched post-root random streams partitioned by causal event family;
- terminal result, not only an intermediate evaluator or development count;
- paired sample count, mean delta, and 95% interval retained.

Use `inconclusive` when the paired evidence does not establish a direction. The analyzer excludes ties and inconclusive comparisons from directional accuracy instead of manufacturing a winner.

## Jev collection and raw cache

The existing lab now has a one-question direct pass and a content-addressed raw cache:

```bash
node scripts/jev-strategy-lab.mjs \
  --input benchmark-results/jev-lab/opening-decisions.jsonl \
  --output benchmark-results/jev-lab/direct.jsonl \
  --pass direct \
  --stage setup \
  --cache-dir benchmark-results/jev-lab/raw-cache

node scripts/jev-strategy-lab.mjs \
  --input benchmark-results/jev-lab/opening-decisions.jsonl \
  --output benchmark-results/jev-lab/dimensions-run-1.jsonl \
  --pass pass3 \
  --stage setup \
  --cache-dir benchmark-results/jev-lab/raw-cache
```

The cache key is SHA-256 over canonical JSON containing model, state, and questions. A hit replays without `TYPESAFE_API_KEY`; a miss requires the environment variable. Cache files contain the sanitized request and raw response, never the credential. `benchmark-results/` is ignored and remains local.

Collect at least two candidate orders and two meaning-preserving wording variants for each method. Candidate facts must be identical across those perturbations. Cache keys, order, wording version, returned probabilities/scores, and confidence are copied into the normalized case record.

The lab exposes `--candidate-order canonical|reverse` and `--wording-variant standard|alternate`. Run the Cartesian product for `direct` and `pass3`, writing each run to a separate ignored JSONL file. The alternate wording changes only the framing, not the evidence or strategic concept.

Create a frozen manifest before those calls. It has `schemaVersion: 1`, `kind: "jev-opening-calibration-manifest"`, and a `cases` array. Each case uses the calibration schema fields except `recordType`, `schemaVersion`, and `judgments`. Its `source` must include the exact lab identity fields (`game`, `boardSeed`, `decisionIndex`, `stateHash`, `phase`, and `actor`) so runs join without heuristic matching. Then assemble the cached evaluations:

```bash
node scripts/assemble-jev-opening-calibration.mjs \
  --manifest benchmark-results/jev-lab/opening-manifest-v1.json \
  --direct benchmark-results/jev-lab/direct-standard-canonical.jsonl \
  --direct benchmark-results/jev-lab/direct-standard-reverse.jsonl \
  --direct benchmark-results/jev-lab/direct-alternate-canonical.jsonl \
  --direct benchmark-results/jev-lab/direct-alternate-reverse.jsonl \
  --dimensions benchmark-results/jev-lab/dimensions-standard-canonical.jsonl \
  --dimensions benchmark-results/jev-lab/dimensions-standard-reverse.jsonl \
  --dimensions benchmark-results/jev-lab/dimensions-alternate-canonical.jsonl \
  --dimensions benchmark-results/jev-lab/dimensions-alternate-reverse.jsonl \
  --output benchmark-results/jev-lab/opening-calibration-v1.jsonl
```

The assembler joins only on the full recorded source identity, maps Jev option ids back to manifest candidate ids by exact action, and refuses missing cache keys, incomplete answers, unknown actions, or cases missing either comparator.

## Evaluation

Run locally after normalizing cases:

```bash
node scripts/analyze-jev-opening-calibration.mjs \
  --input benchmark-results/jev-lab/opening-calibration-v1.jsonl \
  --output benchmark-results/jev-lab/opening-calibration-report-v1.json
```

For each question family, the report compares:

1. the direct Jev choice probabilities;
2. the unweighted mean of decomposed dimensions;
3. a separately named confidence-weighted dimensional diagnostic;
4. the deterministic cheap baseline when supplied.

Primary dimensional ranking is unweighted. Jev confidence is retained but is not treated as correctness probability. The confidence-weighted result stays visibly separate until held-out evidence shows benefit.

Reported confidence is binned into low, medium, and high ranges and compared with held-out directional accuracy. This is a reliability diagnostic, not probability calibration; sparse bins stay visible as sparse rather than being pooled across unrelated question families.

Thresholds are selected independently per question family on `calibration`, prioritizing directional accuracy and false-positive control while retaining at least 50% calibration coverage. The selected threshold is then evaluated once on `holdout`. The report includes pairwise accuracy, coverage/abstention, incumbent false-positive rate, and direct-versus-dimensions discordant errors with a paired exact test. A decomposition benefit is reported only when dimensions win significantly more discordant held-out pairs; otherwise the result is `not_demonstrated`.

Holdout metrics are also stratified by corpus tag and mechanical-screening status. This keeps D17-style domination errors and other hard negatives visible instead of allowing easy positives to hide them in an overall average.

## Invariance and ranking admission

The analyzer checks top-choice consistency across candidate-order permutations and wording variants. It also buckets held-out comparisons by absolute score margin and checks whether directional accuracy is non-decreasing as the margin grows.

No Jev score may be used as a general candidate ranker until its question family has all of:

- at least 20 conclusive held-out comparisons;
- at least 0.65 pairwise accuracy;
- at least 0.80 non-abstaining coverage;
- at least 0.80 order consistency;
- at least 0.80 wording consistency;
- monotone margin-versus-accuracy behavior.

These are minimum research gates, not claims of calibration or production admission. If a family fails, use Jev only as a case generator and inspect its false positives.

## Required corpus

The locked corpus must retain:

- `hill6758` and `task9783` as positive strategic disagreements;
- D17 as a known Jev false positive with the omitted mechanical road facts restored;
- attractive-but-bad port cases;
- five-resource but low-throughput openings;
- unreachable and self-unfundable repair targets;
- non-causal denial;
- mechanically dominated setup roads.

Every hard-negative template needs an instantiated state and matched terminal label before entering metric totals. The manifest intentionally distinguishes `generation_required` from labeled evidence so an expected failure is never silently turned into ground truth.

## A1 and final-validation handoff

A1 must provide:

- a stable feature-semantics version for causal denial, complete-build port conversion, and self-funding expansion;
- deterministic feature extraction for every candidate without changing Jev prompts based on the engine winner;
- reconstructed hill6758/task9783 candidate sets and any additional hard-negative generators.

Final validation must then freeze group assignments, populate both trade modes, generate matched terminal comparisons, run direct and dimension variants from the raw cache, choose thresholds on calibration only, and open the holdout once. No feature or threshold should be promoted from the current offline framework alone.
