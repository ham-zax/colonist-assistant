# Jev Strategy Research Playbook

## Purpose

Jev is an offline strategic critic for Colonist Assistant. It is not part of the production decision path.

Production decisions remain deterministic Rust/WASM or the native Rust GPU companion. Jev is used to find positions where the current engine may be strategically wrong, explain the missing concept in bounded terms, and generate hypotheses that must be validated before any behavior is distilled into production code.

The core loop is:

```text
exact game facts
  -> blind local Jev judgment
  -> mechanical sanity screen
  -> independent strategic review
  -> matched counterfactual simulation
  -> repeated evidence
  -> smallest deterministic engine repair
  -> regression and held-out replay
```

The important design choice is that Jev never becomes the authority simply because it disagrees with the engine.

## Current implementation

The research lane is intentionally isolated from the live extension.

Relevant files:

- `engine/crates/catan-arena/src/bin/jev-strategy-lab.rs` — reproducible Catan simulation and decision capture.
- `scripts/jev-strategy-lab.mjs` — builds blind candidate descriptions, sends bounded questions to Jev, aggregates answers, and screens weak disagreements.
- `engine/crates/catan-arena/src/bin/opening-evidence-probe.rs` — decomposes opening value for exact recorded positions.
- `benchmark-results/jev-lab/` — ignored local research outputs, counterfactuals, and intermediate analysis.

The TypeSafe credential is supplied only through the local `TYPESAFE_API_KEY` environment variable. Do not put the key in source, commands that will be committed, logs, checkpoints, benchmark artifacts, or documentation.

## Division of labor

The most reliable results come from giving each system the work it is good at.

### Rust/code owns exact facts

Code should compute anything that has a definite mechanical answer:

- legal actions;
- Catan costs;
- production pips;
- resource vectors;
- maritime trade ratios;
- current hand and exact resource deltas;
- build affordability;
- board topology;
- road connectivity;
- settlement distance legality;
- longest-road length;
- expansion targets;
- roads remaining;
- expected-roll estimates;
- race survival estimates;
- belief-derived opponent affordability;
- planner completion mass;
- opponent response windows;
- exact score/search diagnostics.

Do not ask Jev to redo arithmetic that the engine can calculate exactly.

### Jev owns bounded strategic judgment

Jev is useful for questions such as:

- Is this resource portfolio structurally brittle?
- Does this road preserve useful expansion options?
- Is an immediate spend worth giving up strategic liquidity?
- Does this action overcommit to one plan?
- Is a development-card purchase consuming a rare bottleneck resource too early?
- Does this line compound tempo over several builds?
- Does the action remain strong after one plausible opponent response?

These should be local questions about supplied facts, not requests to solve an entire game from scratch.

### Counterfactual simulation owns admission

A strategic preference is not accepted because Jev sounds persuasive.

When Jev identifies a material disagreement, run the engine from the same reconstructed state with:

- the same board seed;
- the same chance seed;
- the same stochastic model;
- the same player count and rules;
- one forced root action per branch;
- otherwise identical continuation policy.

Compare terminal outcomes when practical. At intermediate horizons also compare actor VP, production, settlements, cities, roads, development cards, expansion options, and evaluator state.

One seed can reject an obviously bad hypothesis. A production change should normally require repeated or especially strong causal evidence.

## Blind evaluation

Jev should not see the engine answer during the main research pass.

For blind passes:

- sort candidates neutrally;
- omit engine search values;
- omit the selected action;
- omit engine ranking;
- omit policy promotion labels;
- omit prose that describes one candidate as preferred.

Jev may see exact mechanical evidence even when that evidence was produced by engine code. Mechanical facts are not the same thing as engine utility.

For example, road candidates should expose facts such as:

- target vertex;
- roads remaining;
- expected rolls;
- target value;
- survival probability;
- frontier gain;
- introduced fragility;
- road-cut posterior.

This distinction matters. An earlier D17 experiment withheld exact road-intent evidence and Jev preferred a locally attractive road. Once the mechanical facts were inspected, the Jev road was clearly weaker and the matched continuation confirmed it. Withholding authoritative mechanics can create false positives just as leaking engine scores can create confirmation bias.

## Atomic questions

Avoid one giant question such as "What is the best move?"

Score each candidate on independent dimensions, then aggregate.

Current main-game dimensions include:

- immediate strategic value;
- resource efficiency;
- future optionality;
- opponent pressure;
- block resilience;
- city/development balance;
- tempo compounding;
- long-horizon value.

Opening questions use different dimensions because opening decisions have a different job. A first settlement is an anchor that will be complemented later; a second settlement completes the initial portfolio; a setup road is mainly a route/frontier decision.

Examples for a second settlement:

- completed production;
- build-cost coverage;
- starting hand;
- next-settlement cadence;
- city/development transition;
- closed-economy self-sufficiency;
- number resilience;
- long-horizon portfolio.

This decomposition makes disagreements diagnosable. Instead of "Jev likes vertex 24," we can see whether the disagreement is about resource coverage, expansion flexibility, or some other concept.

## False-positive control

Jev is tunable. Tune the research process for precision, not for maximum disagreement.

### 1. Supply better authoritative evidence

If Jev makes a bad judgment because a deterministic fact was omitted, add that fact to the candidate description rather than rewriting the prompt to favor the engine.

Useful additions are facts that are:

- exact or mechanically derived;
- independent of the engine's selected answer;
- stable under candidate ordering;
- directly relevant to the questioned dimension.

### 2. Confidence-weight the score aggregation

The atomic score pass records Jev confidence per dimension.

The harness now computes both ordinary mean score and a confidence-weighted score. Candidate ranking for atomic passes uses the confidence-weighted score first.

Low-confidence judgments therefore contribute less than high-confidence ones without being discarded entirely.

### 3. Require a meaningful margin

A tiny Jev lead is not a useful research signal.

The harness screens disagreements whose confidence-weighted normalized score margin is below a minimum threshold. The current threshold is deliberately conservative and should be tuned against known true/false disagreement cases rather than increased merely to reduce volume.

### 4. Reject mechanical domination

A strategic model should not override exact mechanics.

For road candidates, a Jev preference is screened when the engine alternative is no worse on all of the following and materially better on at least one:

- target value;
- survival probability;
- roads remaining;
- expected rolls.

This filter was motivated by D17, where Jev preferred a road with another road still required and roughly 0.665 survival while the engine road led directly to a stronger target at roughly 0.995 survival.

The same pattern can be extended carefully to other action families when a real partial order exists. Do not invent dominance rules for subjective dimensions.

### 5. Keep an explicit false-positive corpus

Do not delete failed Jev hypotheses from memory.

Maintain examples of:

- Jev right, engine wrong;
- Jev wrong, engine right;
- ambiguous / seed-sensitive;
- mechanically invalid Jev reasoning;
- good short-horizon result that fails to convert into terminal value.

Use this corpus to tune evidence, prompts, thresholds, and questions.

### 6. Separate screening from proof

A screened-in Jev disagreement is only a counterfactual candidate.

The production admission bar remains higher:

```text
Jev signal
  != engine defect

Jev signal + mechanical sanity
  != engine defect

Jev signal + matched continuation
  = evidence

repeated causal evidence + identified owner
  = candidate production change
```

## Examples and lessons

### task9783 opening: validated strategic valuation failure

The historical engine preferred a 21-pip second-settlement portfolio approximately equivalent to:

```text
lumber 5
brick 11
wool 0
grain 5
ore 0
```

Jev preferred another 21-pip portfolio approximately equivalent to:

```text
lumber 5
brick 4
wool 5
grain 4
ore 3
```

The second portfolio covers all five resources without sacrificing total raw production.

Independent analysis agreed with Jev. The engine's opening utility was allowing speculative future repair/expansion credit to overwhelm current economic completeness.

The distilled fix reused the engine's existing closed-economy/self-sufficiency concepts and reduced speculative expansion credit when current conversion efficiency is poor. The recorded opening corpus remained green in both player-trade modes.

Lesson: use Jev to reveal the missing strategic concept, then repair the existing deterministic owner. Do not add a "Jev bonus."

### task9783 turn 25: planner authority failure

The player held exactly three lumber and three brick. Three connected roads were legal and immediately produced Longest Road for +2 VP with no current holder.

The planner already knew the line was fully completed, decisive, and had zero opponent response windows. Final shallow MaxN still preferred End Turn.

Jev agreed with building the roads, but Jev was not needed to prove the mechanics. The real defect was authority composition: exact same-turn plan evidence was discovered and then discarded.

The repair therefore lives in search/planner arbitration, not in a new strategic weight.

Lesson: before changing evaluation, check whether the engine already knows the correct fact and loses it at a later decision boundary.

### D17: Jev false positive

Jev preferred a different road.

Exact road-intent evidence showed that the Jev road needed more construction and had much lower survival. Matched continuation also favored the engine line.

No engine repair was admitted.

Lesson: a false positive is useful. It identifies missing research evidence or a weak question and improves the critic.

### D23: strong intermediate progress, weak terminal conversion

Jev preferred a 4:1 grain-to-lumber maritime trade for a player with no lumber production.

At the matched intermediate horizon, the Jev line produced more development: more roads, more settlements, and an extra public VP. At terminal outcome it still failed to improve the actor's result.

Lesson: "better development" is not automatically "better win race." Counterfactual evaluation must include terminal conversion and opponent acceleration, not only the actor's local economy.

### D31: preserve liquidity before buying development

The engine preferred BuyDevelopment with search value around 0.072 versus roughly 0.033 for EndTurn.

The player had just spent four grain to import their only ore, while having zero native ore and zero lumber production. Buying the development card consumed the imported ore immediately.

The matched EndTurn branch eventually won while the BuyDevelopment control lost.

The current investigation is therefore not "development cards are overvalued" in general. The narrower hypothesis is that the engine underprices the opportunity cost of spending a rare, expensive, unproducible resource immediately after importing it.

Lesson: formulate the smallest causal hypothesis that explains the validated failure. Do not generalize from one winning branch to a global rule.

## Player trades ON and OFF

Treat both rule modes as first-class.

When player trades are disabled:

- self-sufficiency matters more;
- missing direct production matters more;
- maritime repair quality matters more;
- domestic-trade rescue must never be assumed.

When player trades are enabled:

- do not assign a speculative fixed domestic trade rate;
- let the actual domestic-trade search/planner model determine available exchanges;
- reduce, but do not eliminate, the value of self-sufficiency.

A strategic repair should be tested against both modes when its logic applies to both.

## Candidate coverage

The current Jev harness often shortlists candidates using engine output before blind evaluation. This is useful for cost control but can hide a good action that the engine ranked outside the shortlist.

For high-value opening research, prefer evaluating all legal candidates in batches. Keep candidate order neutral within each batch and reconcile across batches without leaking engine rank.

For large main-game action spaces, use a two-stage process:

1. deterministic mechanics eliminate illegal, dominated, duplicate, or obviously immaterial candidates;
2. Jev evaluates the remaining strategically distinct candidates.

Do not use engine utility as the only gate for what Jev is allowed to see.

## Repeatability

Jev is probabilistic enough that repeatability matters.

For a candidate set:

- rerun the same blind question set;
- compare the top action;
- compare dimension-level scores;
- inspect confidence;
- flag unstable rankings.

A useful critic should not require perfect numerical repeatability, but a preference that flips frequently without new evidence should not drive production changes.

## Research artifact discipline

Keep local research artifacts under ignored paths such as `benchmark-results/jev-lab/`.

Each experiment should preserve:

- board seed;
- chance seed;
- game index;
- decision index;
- state hash;
- actor;
- phase;
- rules/player-trade mode;
- engine-selected action;
- Jev-selected action;
- sanitized candidate facts;
- Jev answers/confidence;
- screening result;
- forced counterfactual action;
- terminal or cutoff result.

Never store credentials.

Prefer JSONL for appendable game/decision traces and JSON for compact one-position probes.

## How to tune Jev without teaching it the answer

Good tuning improves information quality.

Good changes:

- add missing exact mechanics;
- make a question more local;
- separate two concepts previously mixed in one dimension;
- clarify what evidence is authoritative;
- forbid invented hidden information;
- distinguish guaranteed actions from proposals;
- confidence-weight outputs;
- add deterministic dominance screening;
- validate thresholds against known true and false positives.

Bad changes:

- tell Jev which action the engine selected;
- include engine utility in a blind strategic pass;
- add examples that say "the correct answer is X" for the current position;
- change prompts until Jev agrees with a preferred production choice;
- treat Jev confidence as calibrated game probability;
- ask Jev to recompute exact arithmetic;
- tune only against successful Jev disagreements and forget false positives.

The goal is not agreement. The goal is a critic whose disagreements have high information value.

## Reusable workflow for other repositories

The same pattern works outside games when a deterministic system has a fuzzy quality objective.

### Step 1: split facts from judgment

Identify what code can establish exactly and what still requires qualitative judgment.

Examples:

- compiler optimization: exact IR/cost counters vs maintainability/performance trade-off;
- scheduler: exact queue/latency facts vs policy fairness;
- recommender: exact exposure/conversion history vs long-horizon portfolio quality;
- query planner: exact cardinality/cost estimates vs robustness under uncertainty;
- agent workflow: exact tool traces vs whether the plan is brittle or wasteful.

### Step 2: expose local candidates

Give the critic a bounded candidate set with enough context to judge each option independently.

Keep exact facts and candidate descriptions stable. Hide the incumbent system's winner and score.

### Step 3: ask orthogonal questions

Replace "Which is best?" with dimensions that map to real system concepts.

A useful dimension should either:

- correspond to an existing production concept; or
- expose a missing concept that could plausibly be implemented deterministically.

If a dimension cannot be translated into a testable production hypothesis, it is probably too vague.

### Step 4: screen cheaply

Before expensive experiments:

- reject mechanically invalid candidates;
- reject dominated candidates where a real partial order exists;
- reject tiny-margin / low-confidence disagreements;
- collapse duplicate candidates.

### Step 5: validate causally

Use the closest available equivalent of matched continuation:

- A/B replay with fixed inputs;
- simulation with common random numbers;
- trace replay;
- shadow execution;
- offline evaluation on frozen data;
- differential benchmarking.

Change one root choice while holding the rest of the environment fixed.

### Step 6: find the production owner

When the critic is right, ask why the production system missed it.

Typical owners:

- missing feature/evaluation term;
- incorrect weight;
- search horizon;
- candidate pruning;
- authority arbitration;
- stale state;
- incorrect uncertainty model;
- wrong objective;
- an exact fact computed but discarded downstream.

Fix the owner, not the observed example.

### Step 7: verify generalization

Run:

- the original failure;
- nearby counterexamples;
- known false positives;
- existing regression corpus;
- relevant rule/config modes;
- held-out or fresh generated positions.

A change that fixes the example but breaks the false-positive corpus is not a successful distillation.

## Practical heuristics

1. **Ask code first.** If a number can be computed, compute it.
2. **Use common random numbers.** Matched stochastic streams dramatically improve counterfactual signal.
3. **Force only the root action.** Let normal policy resume afterward unless the experiment is explicitly about a multi-action forced plan.
4. **Compare intermediate and terminal state.** Either alone can mislead.
5. **Keep false positives.** They are training data for the research process.
6. **Prefer missing concepts over new weights.** A weight change is useful only after the concept being weighted is correct.
7. **Check arbitration before evaluation.** The engine may already know the right answer and discard it later.
8. **Do not confuse confidence with probability.** Jev confidence describes its answer, not the chance an action wins.
9. **Do not overfit one seed.** Strong deterministic tactical facts can be proven with one state; strategic long-horizon claims usually need repeated matched continuations.
10. **Keep the critic removable.** Production should remain correct and usable if Jev disappears tomorrow.

## Recommended admission standard

A production strategy change should satisfy all of the following:

1. a reproducible engine/Jev disagreement exists;
2. exact mechanical evidence does not invalidate the Jev preference;
3. independent analysis can state the strategic reason without referring to Jev authority;
4. matched counterfactual evidence supports the alternative;
5. the responsible production owner is identified;
6. the repair is deterministic and local to that owner;
7. the original case improves;
8. relevant counterexamples and false-positive cases do not regress;
9. both player-trade modes are checked when applicable;
10. the live engine has no runtime dependency on Jev.

That standard keeps Jev useful without allowing it to become an unverified oracle.
