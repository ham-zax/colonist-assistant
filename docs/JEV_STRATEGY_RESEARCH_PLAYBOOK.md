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
- the same pre-root chance history;
- the same stochastic model;
- the same player count and rules;
- one forced root action per branch;
- otherwise identical continuation policy;
- matched post-root random streams partitioned by causal event family when branches can consume different random events.

A single shared RNG seed is not sufficient when one branch consumes an event that the other does not. For example, BuyDevelopment consumes a development-card draw while EndTurn does not. If rolls, development draws, and steals all advance one shared generator, that extra draw shifts every later event and the branches stop being meaningful counterfactuals. Use independent common-random-number streams for logically separate event families, keyed from the same continuation seed.

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

Development-card purchases should likewise expose an information-set-safe next-draw belief derived from public card totals, publicly played cards, the actor's own exact cards, and public deck size. Do not leak exact opponent-held cards or exact hidden deck composition, but do not force Jev to judge an "unknown card" without the probability distribution the player can legitimately infer.

This distinction matters. An earlier D17 experiment withheld exact road-intent evidence and Jev preferred a locally attractive road. Once the mechanical facts were inspected, the Jev road was clearly weaker and the matched continuation confirmed it. Withholding authoritative mechanics can create false positives just as leaking engine scores can create confirmation bias.

## Direct judgment versus decomposed dimensions

Do not assume that decomposing a decision into many Jev dimensions is automatically better than asking one direct typed question.

Use three competing shapes:

1. **direct judgment** — one bounded question such as "Which of these two completed openings gives the actor the stronger route to the victory target?";
2. **decomposed dimensions** — several narrower scores such as self-sufficiency, settlement cadence, port realization, liquidity resilience, or route optionality;
3. **deterministic/code features** — exact quantities such as production, build ETAs, conversion ratios, route length, survival, or self-funding cost.

Measure all three against labeled counterfactual outcomes before deciding which shape belongs in the research loop.

A direct question is preferable when it is already strong and stable. Decomposition is worth the extra cost only when it adds independent signal on held-out data. The supplied Jev study is a useful warning: dimensions improved a difficult Japanese NLI task but degraded an already-easy task and produced dramatically worse false positives on hard benign examples. The lesson for strategy research is not "use dimensions"; it is "test whether dimensions help this question family."

When decomposition is warranted, current main-game dimensions include:

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

Decomposition is especially useful for diagnosis. Instead of "Jev likes vertex 24," we can see whether the disagreement is about resource coverage, expansion flexibility, or some other concept. Do not promote the arithmetic mean of those dimensions into production authority without validating that the dimensions and combination predict outcomes.

## Feature semantics before weights

A badly defined feature cannot be repaired reliably by fitting a better coefficient.

Before tuning a weight, threshold, or linear combination, ask whether the feature measures the causal concept we actually care about.

Examples from Colonist Assistant:

- a "port value" feature is wrong if it only rewards owning a 2:1 port instead of measuring how that port changes complete-build affordability and conversion;
- an "expansion value" feature is wrong if it credits a future repair settlement that the current economy cannot self-fund;
- a "rival value" feature is wrong if it penalizes a strong opponent who would have been strong regardless of our move; denial must measure the opportunity our move actually removed or worsened;
- a "resource scarcity" feature is wrong if it considers only the nearest currently affordable build and therefore treats an irreplaceable imported ore as expendable.

Use this sequence:

```text
define causal concept
  -> compute/ask feature
  -> construct hard positive and hard negative cases
  -> validate feature against matched outcomes
  -> only then fit weight/threshold
```

If hard negatives expose the wrong semantics, redesign the feature. Do not respond by repeatedly changing its weight.

## False-positive control

Jev is tunable. Tune the research process for precision, not for maximum disagreement.

### 1. Supply better authoritative evidence

If Jev makes a bad judgment because a deterministic fact was omitted, add that fact to the candidate description rather than rewriting the prompt to favor the engine.

Useful additions are facts that are:

- exact or mechanically derived;
- independent of the engine's selected answer;
- stable under candidate ordering;
- directly relevant to the questioned dimension.

### 2. Treat Jev confidence as a feature to calibrate, not as correctness probability

The atomic score pass records Jev confidence per question, and the current harness can compute confidence-weighted aggregates. That is useful as an experimental signal, not proof that confidence weighting improves decisions.

Calibrate confidence separately for each question family against held-out labels. The supplied Jev study found examples where answers reported confidence >= 0.9 while accuracy on those cases was much lower. Do not assume that 0.9 confidence means 90% correctness.

Compare at least:

- raw direct score;
- direct score plus its confidence as a screening variable;
- unweighted dimension combination;
- confidence-weighted dimension combination;
- locally fitted combination on training data only.

Keep the simplest variant that improves the held-out error profile.

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

### 6. Keep direct and decomposed judgments independent

Do not automatically feed a good direct Jev judgment into the dimensional model as one more feature. That can destroy the direct judge's useful error profile.

Treat direct judgment and decomposed dimensions as independent comparators unless a specific stacking experiment on held-out data shows that combining them improves the deployment-relevant metric.

For strategy research the useful question is often whether their errors are orthogonal:

- cases solved only by the direct judge;
- cases solved only by the dimensions;
- cases both miss;
- cases where deterministic mechanics dominate both.

That error matrix is more informative than one aggregate accuracy number.

### 7. Separate screening from proof

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

### hill6758: feature semantics and multi-player objective failure

The live four-player opening chose a second settlement with production approximately:

```text
lumber 2
brick 5
wool 0
grain 8
ore 5
```

The opening deliberately accepted zero wool because future expansion credit assumed that a later settlement could repair the hole. That repair was not free: a settlement itself requires wool, so the no-player-trade economy first had to import the missing resource.

The live setup road had also already created a corridor toward a legal 9-grain / 10-wool settlement on a 2:1 brick port. The opening evaluator saw that site but priced the port mostly as a small local ratio bonus instead of asking how 2:1 brick conversion changes complete-build economics.

Matched continuation on one controlled future stream produced:

- historical zero-wool root: win on turn 96;
- all-five-resource root: win on turn 92;
- 9/10 wool + 2:1 brick-port root: win on turn 84.

One stream is evidence, not a universal ranking. The more fundamental result came from the evaluator decomposition: after a self-funding correction, the actor's own opening value already preferred the all-five root, but the final four-player score flipped back because it subtracted a fixed fraction of the strongest rival's value.

That exposed a modeling error. Opponent seats were already choosing strong actions for themselves during the snake draft. Subtracting generic strongest-rival strength again at the leaf double-counted adversarial pressure and confused **opponent strength** with **causal denial**.

The correct distinction is:

```text
generic strong opponent
  != value we denied

causal denial
  = opponent opportunity without our root
    - opponent opportunity after our root
```

Lesson: when a feature changes the winner for the wrong semantic reason, revisit the building block before fitting its weight. In multiplayer search, model opponents choosing for themselves and value only interaction effects that our action actually caused.

### D31: an apparent Jev win exposed research flaws

The engine preferred BuyDevelopment with search value around 0.072 versus roughly 0.033 for EndTurn.

The player had just spent four grain to import their only ore, while having zero native ore and zero lumber production. Buying the development card consumed the imported ore immediately. Jev preferred EndTurn.

An early forced-action replay appeared to validate Jev because the EndTurn branch eventually won while the BuyDevelopment control lost. That result was not admissible evidence: both branches consumed one shared future random stream, so BuyDevelopment's extra development-card draw shifted later rolls and steals.

The corrected counterfactual harness uses separate common-random-number streams for rolls, development draws, and steals. Four independent matched continuation streams then produced the opposite conclusion from the early replay: BuyDevelopment won for the actor in three of four streams, while EndTurn won for the actor in only one. D31 is therefore retained as a false-positive / seed-sensitive Jev case, and no production dev-card nerf is admitted from it.

A depth sweep also showed that BuyDevelopment already leads at depth 1, which rules out future-self continuation policy as the primary cause of the search preference. Immediate evaluator decomposition showed that four of five development-card outcomes are locally worse than EndTurn, while the Victory Point outcome contributes a large positive jump. That observation remains useful diagnostic evidence, but the stronger common-random-number result demonstrates why an intuitively suspicious local valuation is not enough to justify a strategy change.

The Jev prompt had another information defect: it exposed the purchase cost but not the actor-safe development draw distribution. The research harness now supplies that distribution without leaking hidden card identities.

Lesson: validate the experimental design before interpreting a counterfactual winner. When a Jev disagreement survives only under poorly matched randomness or incomplete mechanical evidence, improve the research harness rather than changing production.

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
- swap candidate order;
- vary wording without changing semantics;
- flag unstable rankings.

A useful critic should not require perfect numerical repeatability, but a preference that flips frequently without new evidence should not drive production changes.

## Calibration and evaluation protocol

Calibrate per question family. Do not create one global "Jev threshold."

Examples of separate families:

- opening portfolio comparison;
- setup-road target quality;
- prospective-port realization;
- save-versus-spend;
- development-card timing;
- contested expansion / denial;
- tactical closeout.

For each family:

1. **Define labels first.** Use matched counterfactual outcomes, exact mechanics, or another independent authority. Do not use the current engine choice as ground truth.
2. **Define the split before fitting.** Group by board, game seed, scenario family, or other structure that prevents near-duplicate positions from leaking across train and test.
3. **Print a cheap baseline.** For Catan this can be an existing deterministic heuristic, exact feature rule, or current engine score. If the cheap baseline already meets the target, Jev decomposition may add no value.
4. **Measure the direct judge.** A strong direct typed question is the default comparator.
5. **Measure decomposed dimensions.** Fit combinations only on the training split and evaluate once on held-out groups.
6. **Inspect error overlap.** Count direct-only wins, dimensions-only wins, shared misses, and mechanically dominated errors.
7. **Calibrate threshold/margin on training data only.** Report held-out precision, false-positive rate, coverage, and abstention/escalation rate.
8. **Test invariance.** Swap option order, paraphrase the question, change batching, and repeat calls.
9. **Test ranking validity before sorting many candidates.** Pairwise consistency and score ordinality must be demonstrated; a score that works for thresholding is not automatically a valid global sort key.
10. **Keep a locked holdout.** Do not tune prompts, dimensions, or thresholds repeatedly against the same final evaluation set.

Use metrics that match the research risk. Overall agreement is rarely enough. For a critic that triggers expensive simulations or production investigations, false-positive rate and useful-signal precision can matter more than raw accuracy.

### Hard-positive and hard-negative corpus

Every question family should include cases designed to expose semantic shortcuts.

Opening hard negatives should include:

- a 2:1 port with too little matching production to exploit it;
- five-resource coverage with disastrously weak total throughput;
- a high-value repair target that cannot be self-funded;
- apparent denial against an opponent who was not meaningfully constrained by our root;
- high raw pips concentrated on fragile/correlated numbers;
- a nearby expansion site that looks good locally but destroys a stronger route portfolio.

Road hard negatives should include mechanically dominated routes such as D17.

Save/spend hard negatives should include cases where preserving a scarce resource looks prudent but an immediate forced conversion is objectively decisive.

Do not add a hard case merely to make Jev fail. Each case should represent a real confusion the production system or critic could plausibly make.

## Research artifact discipline

Keep local research artifacts under ignored paths such as `benchmark-results/jev-lab/`.

Each experiment should preserve:

- board seed;
- pre-root chance history and post-root continuation seed(s);
- game index;
- decision index;
- state hash;
- actor;
- phase;
- rules/player-trade mode;
- question family and question version;
- split/group identifier;
- engine-selected action;
- direct Jev answer/score/confidence;
- decomposed dimension scores/confidences when used;
- deterministic feature vector;
- sanitized candidate facts;
- option order / batching metadata;
- screening result;
- forced counterfactual action;
- terminal or cutoff result;
- counterfactual label and label provenance.

Store raw critic outputs separately from fitted weights and thresholds. Raw extraction is the paid/networked step; fitting, threshold sweeps, ablations, and error analysis should be repeatable locally without another Jev call.

Never store credentials.

Prefer JSONL for appendable game/decision traces and JSON for compact one-position probes.

## How to tune Jev without teaching it the answer

Good tuning improves information quality.

Good changes:

- add missing exact mechanics;
- make a question more local;
- separate two concepts previously mixed in one dimension;
- remove or redesign a dimension that fails hard negatives;
- clarify what evidence is authoritative;
- forbid invented hidden information;
- distinguish guaranteed actions from proposals;
- compare direct versus decomposed judgment instead of assuming either wins;
- calibrate confidence/thresholds per question family;
- add deterministic dominance screening;
- validate thresholds against grouped held-out true and false positives;
- test order, wording, batching, and repeatability invariance.

Bad changes:

- tell Jev which action the engine selected;
- include engine utility in a blind strategic pass;
- add examples that say "the correct answer is X" for the current position;
- change prompts until Jev agrees with a preferred production choice;
- average many dimensions and call the result truth without held-out validation;
- use one global confidence threshold for unrelated question families;
- treat Jev confidence as calibrated game probability;
- use a Jev score as a global ranking key before testing ordinality/pairwise consistency;
- ask Jev to recompute exact arithmetic;
- tune only against successful Jev disagreements and forget false positives;
- repeatedly tune on the final holdout.

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

### Step 3: establish the direct baseline before decomposing

Start with one well-bounded direct typed judgment. Measure it before deciding that decomposition is necessary.

If the direct judge is already strong, stable under option order/wording, and has an acceptable false-positive profile, keep it as the research critic. Do not add twelve dimensions merely because decomposition is possible.

When the direct judge is weak or its errors suggest missing structure, add dimensions that map to real system concepts.

A useful dimension should either:

- correspond to an existing production concept; or
- expose a missing concept that could plausibly be implemented deterministically.

If a dimension cannot be translated into a testable production hypothesis, it is probably too vague. If it fires on hard negatives for the wrong reason, redesign the dimension before fitting its weight.

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
2. **Measure the direct judge before decomposing.** More questions are not automatically more accurate.
3. **Validate feature meaning before feature weight.** A wrong semantic dimension cannot be rescued reliably by coefficient tuning.
4. **Use common random numbers by causal event family.** Matched stochastic streams dramatically improve counterfactual signal, but branches that consume different event types must not shift one shared generator. Partition streams by logically independent sources such as rolls, hidden draws, steals, arrivals, or service outcomes.
5. **Force only the root action.** Let normal policy resume afterward unless the experiment is explicitly about a multi-action forced plan.
6. **Compare intermediate and terminal state.** Either alone can mislead.
7. **Keep false positives.** They are training data for the research process.
8. **Prefer missing concepts over new weights.** A weight change is useful only after the concept being weighted is correct.
9. **Check arbitration before evaluation.** The engine may already know the right answer and discard it later.
10. **Do not confuse confidence with probability.** Calibrate confidence per question family before using it as a gate.
11. **Do not assume score ordinality.** A score suitable for thresholding may still be a poor global ranking key.
12. **Design the split before the dataset grows.** Group by board/scenario family to prevent near-duplicate leakage.
13. **Do not overfit one seed.** Strong deterministic tactical facts can be proven with one state; strategic long-horizon claims usually need repeated matched continuations.
14. **Keep the critic removable.** Production should remain correct and usable if Jev disappears tomorrow.

## Recommended admission standard

A production strategy change should satisfy all of the following:

1. a reproducible engine/Jev disagreement exists;
2. exact mechanical evidence does not invalidate the Jev preference;
3. the question family has a known direct/decomposed error profile, or the individual case is supported by stronger independent evidence;
4. independent analysis can state the strategic reason without referring to Jev authority;
5. the relevant feature semantics survive hard-positive and hard-negative cases;
6. matched counterfactual evidence supports the alternative with causally matched randomness;
7. the responsible production owner is identified;
8. the repair is deterministic and local to that owner;
9. the original case improves;
10. relevant counterexamples, false positives, and held-out groups do not regress;
11. both player-trade modes are checked when applicable;
12. the live engine has no runtime dependency on Jev.

That standard keeps Jev useful without allowing it to become an unverified oracle or turning a calibration artifact into production truth.
