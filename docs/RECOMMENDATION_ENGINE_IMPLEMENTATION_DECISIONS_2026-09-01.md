# Recommendation Engine Implementation Decision Log

Date: 2026-09-01
Repository: `/home/hamza/repo/colonist-assistant`
Mission baseline: `862e3110069524fbcda654b4f702ac4726774c0f` (`deep-maxn-v9`)

This file records implementation decisions, trade-offs, compromises, and interpretations made during execution that are not already explicit in the correctness or before/after evidence plans. It is not a replacement for the plans or the historical audit. Entries are append-only in spirit: if a later result changes a decision, add the replacement and mark the earlier entry superseded rather than rewriting the history.

## D1 - Version the native benchmark report after freezing additional provenance

**Status:** active

`scripts/benchmark-local.mjs` now treats the production-search profile and build identity as first-class benchmark output, not only command-line inputs or per-match fields. The matrix report schema is therefore bumped from `1` to `2`.

Reason: E0 requires the frozen artifact itself to identify the exact source/build and live search configuration. Keeping those facts only in invocation notes would make later before/after comparison easier to misattribute.

Trade-off: this is a measurement-artifact schema change. Consumers that hard-code schema version `1` may need to accept version `2`; no production recommendation semantics change.

## D2 - Add missing `buildDirty` and `maxnTimeMs` to arena JSON output

**Status:** active

The arena checkpoint already records `build_dirty` and `maxn_time_ms`, but the final `--json` summary did not expose both values. The benchmark wrapper needs those fields to prove that every matchup in one frozen matrix came from the same clean/dirty source identity and the intended 350 ms MaxN deadline.

Decision: expose `buildDirty` and `maxnTimeMs` in the arena JSON summary rather than reconstructing them in JavaScript or relying on filenames.

Trade-off: this expands measurement output only. It does not change engine behavior.

## D3 - Persist both the passed benchmark configuration and an explicit live-profile snapshot

**Status:** active

The benchmark matrix will retain the normal parsed `configuration` object and also write a compact `liveProductionProfile` containing the cross-layer limits relevant to the v9 baseline: depth 4, branch 8, 4,000 nodes, 350 ms, 24 arena belief particles, 12 Rust strategic particles, tracker `MAX_WORLDS = 4096`, TypeScript source-world cap `96`, interactive WASM cap `24`, and tactical depth/nodes `14/900`.

Reason: not all production limits are arena CLI parameters. The separate snapshot prevents a later reader from assuming the arena command alone captured the entire live boundary.

Trade-off: some fields duplicate source constants. They are intentionally frozen provenance, not a new configuration authority.

## D4 - Use a minimal stable `SplitMix64` state API for persistent counterfactual replay

**Status:** active

E1 requires exact restoration of the chance RNG and every policy RNG. `SplitMix64` kept its state private and had no restore seam.

Decision: add `SplitMix64::state()` and `SplitMix64::from_state(u64)` in `catan-core` instead of duplicating the RNG algorithm or serializing private memory from `catan-arena`.

Reason: the RNG type is the actual owner of its continuation state, and the evidence plan explicitly permits a minimal stable snapshot/restore API when needed for persistent replay.

Trade-off: this slightly expands the internal core API, but it avoids an arena-only representation that could silently diverge from the RNG implementation.

## D5 - Extend the existing arena CLI with JSONL challenge/takeover artifacts

**Status:** active

Rather than create a second simulator or a separate replay binary, the existing `colonist-arena` CLI is being extended with challenge capture and takeover replay inputs/outputs. The chosen artifact shape is newline-delimited JSON, matching the arena's existing checkpoint/trajectory streaming style.

Planned flags introduced during this implementation are:

- `--challenge-output <path>`
- `--takeover-input <path>`
- `--takeover-output <path>`
- `--takeover-engine control|random|weighted|maxn|alphabeta|uct|puct`

Reason: JSONL supports incremental durable capture, one-record replay, and partial inspection without inventing another simulator or storage format.

Trade-off: the arena CLI becomes broader. The alternative was a second program with duplicate game-continuation logic, which would weaken the requirement that takeover evidence use the existing native arena.

## D6 - Treat a tie at the minimum public VP as "last by public VP"

**Status:** active

The challenge-selection rule says the target qualifies when it is "last by public VP OR >=2 VP behind leader" but does not specify whether tied-last seats count.

Decision: a seat whose public VP equals the minimum public VP in that state counts as last, including ties. The separate `>=2 VP behind leader` condition remains unchanged.

Reason: excluding tied-last seats would add an unstated strict-last requirement and would make corpus membership depend on arbitrary tie ordering.

Trade-off: the corpus can contain positions where multiple players share last place. Seat balancing and the one-state-per-target-seat-per-source-game rule still apply.

## D7 - Freeze the challenge corpus from the completed weighted matchup streams

**Status:** active

The selection contract fixes player count, challenge criteria, uniqueness, and seat balance but does not require challenge states to be mixed across baseline opponent policies.

Decision: freeze the 50 three-player states from the completed 3-player MaxN-versus-weighted challenge stream and the 100 four-player states from the completed 4-player MaxN-versus-weighted stream once those streams independently supplied enough qualifying positions. The resulting seat balance is `17/17/16` for three players and `25/25/25/25` for four players.

Reason: this freezes one deterministic source per player count before any corrected-engine result exists and avoids making corpus membership depend on the later AlphaBeta completion.

Trade-off: the takeover corpus measures positions reached against weighted opponents rather than a mixture of weighted and AlphaBeta opponents. The whole-game matrix still measures both baselines separately.

## D8 - Retain frozen targets whose source policy is already MaxN

**Status:** active

After freezing the corpus, inspection showed that 41 of 150 target seats were already controlled by MaxN in their source game: 17 of 50 three-player states and 24 of 100 four-player states.

Decision: keep those snapshots. The stated freeze criteria do not exclude a target based on its source engine, and changing the corpus after inspection would violate the immutable-corpus rule.

Trade-off: those 41 records are degenerate `control == v9` forks and do not measure takeover benefit. Reports must identify them and must not count them as independent evidence that a v9 takeover improved the source policy.

## D9 - Preserve the interrupted baseline rather than backfilling lost aggregate metrics with a rerun

**Status:** active

The original four-matchup `benchmark-local.mjs` process completed the first three matchups, then its 4-player MaxN-versus-AlphaBeta child was terminated by `SIGTERM` after 58 of 200 games. The wrapper therefore never wrote its top-level JSON/Markdown matrix report.

Decision: preserve the original completed checkpoint/challenge streams for the first three matchups. Do not rerun those legs and merge later aggregate metrics into the original baseline. The missing 4-player AlphaBeta leg was rerun from the same pre-fix source SHA, engine revision, seed, and production profile and is labeled as the replacement pre-fix run, not as a continuation of the interrupted 58-game prefix.

Reason: the checkpoints are the original evidence. Re-running completed legs only to recover metrics that were held in the terminated wrapper process would create a second wall-clock search sample and falsely present it as the first run.

Trade-off: detailed per-engine search/trade/build metrics that existed only in the lost wrapper result are unavailable for the first three original legs. Their original win/rank/VP/cutoff outcomes and blocked intervals remain recoverable from checkpoints and terminal output. The replacement 4-player AlphaBeta run has its full final JSON metrics.

## D10 - Parallelize only independent remaining takeover records

**Status:** active

Once the whole-game matrix stopped consuming twelve cores, the serial control/v9 takeover runs were stopped after flushing 113 control outcomes and 62 v9 outcomes.

Decision: partition only the remaining immutable corpus line ranges and run them as independent single-core arena processes, then concatenate results back in original corpus order. No completed record is rerun or replaced.

Reason: each takeover record restores its own complete `GameState`, chance RNG state, policy RNG states, and source seed. There is no cross-record state, so record-level parallelism changes elapsed time but not the continuation contract.

Trade-off: execution is more operationally complex than one serial process. The final assembly must verify exactly 150 unique snapshot IDs per arm and exact ID/order agreement with the frozen corpus before the evidence is accepted.

## D11 - Treat same-policy takeover divergence as a runtime-noise calibration

**Status:** active

The frozen corpus contains 41 target seats whose source policy is already MaxN. For those snapshots, the control arm and the v9 arm use the same target engine, same restored game state, same chance RNG state, same policy RNG states, and the same production search limits. Despite that, 21 of 41 final game states diverged and 5 of 41 target win/loss statuses disagreed.

Decision: preserve these outcomes and use the 41 degenerate pairs as a direct calibration of production wall-clock replay noise. Do not remove them from the immutable corpus, and do not interpret raw control-versus-v9 win flips as a noise-free causal estimate of policy strength.

Reason: the 350 ms production search deadline is wall-clock sensitive. The degenerate pairs show that identical semantics can expand different node counts and cross different search deadlines under separate executions.

Trade-off: takeover evidence remains valid as production-runtime evidence, but its causal strength claims must be calibrated against the same-policy disagreement rate. Correctness closure continues to rely on deterministic/focused reproductions and production-style node limits, not on takeover win flips alone.

## Review rule

At the end of the mission, review every active entry and classify it as:

- retained design decision;
- superseded decision, with the replacement entry identified;
- temporary measurement-only compromise;
- unresolved boundary requiring a follow-up decision.
