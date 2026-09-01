# Runtime recommendation findings — 2026-09-01

Scope: persisted decision traces from the older Colonist Assistant build currently loaded in Windows Edge, compared with current repository HEAD `aebe9ed` (`Fix belief search integrity`).

The old loaded extension reports only package version `0.9.1`, so its exact source revision cannot be recovered from the running UI. The build-identification change described below fixes that for future builds.

## Current status

### Deep Search latency and deadline exhaustion — not fully fixed

Old-build traces include decisions around 400–1100 ms, a recorded `deepTimedOut: true` at about 422.6 ms, and an older persisted decision around 9.9 seconds.

Current production requests still give ordinary Deep MaxN a 350 ms search budget in `src/worker/deep-search.ts`. Setup uses 1.2 seconds, and setup pondering can use 2.5 seconds. `deadlineReached` therefore remains a valid outcome when the bounded depth search spends its budget.

The current implementation is safer than the old runtime in two ways:

- `DecisionWorkerClient` discards stale responses whose state key or generation no longer matches the desired position.
- While a Deep Search request is pending, the overlay does not start a competing strategic placement/build/roll/end-turn fallback for that same state.

However, the 350 ms deadline covers the bounded depth search, not the complete WASM analysis. In `engine/crates/catan-wasm/src/lib.rs`, tactical and exact solvers run after the timed depth search and do not share that wall-clock deadline. The content-side service has a separate 12-second hard safety limit.

Status: **partially mitigated, not resolved as a total-latency guarantee**. A deadline-limited result can still be returned, and the full analysis can exceed the depth-search budget.

### Belief support and posterior integrity — state validity improved, probability integrity not fixed

The old trace history showed belief particle counts falling roughly `24 -> 11 -> 7 -> 6 -> 2`.

Commit `aebe9ed` does improve physical-state integrity:

- contradictory public hand totals remove incompatible belief support;
- visible-bank contradictions remove incompatible support;
- hidden determinizations must satisfy public resource conservation;
- every WASM particle is validated before search.

However, the end-to-end audit reproduced three remaining failures that prevent treating the resulting posterior as correct:

- `resampleDegenerateWorlds()` can replace duplicated high-mass samples with omitted low-mass worlds and reset selected samples to uniform weight, materially changing posterior probability mass (F1);
- incoming active-trade `give/receive` vectors are local-user-relative at the bridge but interpreted creator-relative by tracker reconciliation, and the same visible offer can be conditioned more than once (F2);
- `stateFromPublicBoard()` seeds opponents with empty hands, so a normal midgame attach/reload can collapse all fallback worlds to zero when public hand sizes are reconciled (F12).

A low particle count by itself is still not automatically a defect. The defect is when the support or weights no longer represent the public evidence correctly.

Status: **physical conservation/validation is stronger at `aebe9ed`, but hidden-resource belief integrity is not fixed end-to-end**. See `docs/END_TO_END_RECOMMENDATION_REVIEW_2026-09-01.md` F1, F2, and F12.

### Deep result replaced by `placement-heuristic` — no reproduced action substitution; source telemetry can still be wrong

An old persisted record appeared to contain a completed Deep Search result with `deepTimedOut: false` while the final action source was encoded near `placement-heuristic`. The LevelDB value was only partially recoverable, so that old record is suspicious evidence rather than a proven exact mismatch.

The current overlay has explicit arbitration guards:

- a pending Deep Search blocks competing strategic fallbacks;
- an authoritative Deep Search spatial target must map to the current legal placement set;
- if an authoritative deep target cannot be mapped, the overlay returns no click and waits for a new state-locked search instead of selecting the top heuristic coordinate;
- the normal board-action path intends to tag a mapped deep target as `deep` or `tactical`, although F13 proves that equivalent final actions can still be mislabeled by other rendering/helper paths.

Status: **no current deep-to-placement-heuristic action substitution was reproduced for board actions**. Later replay work did reproduce incorrect `finalActionSource` labels for actions whose payload still matched the deep recommendation (F13), so source tags alone cannot be used as proof of arbitration. The separate accepted-trade fast-confirm path remains a real authority violation (F4).

### Tracker degraded-evidence warnings — expected and still present

The tracker still intentionally emits warnings such as:

- `Some earlier game history was unavailable; lower bounds were repaired.`
- `A trade referenced cards from before tracking began.`
- `Your exact hand repaired an incomplete section of the public game log.`

These warnings mean the extension did not observe enough earlier history to reconstruct every hidden resource transition exactly. Missing historical observations cannot be recovered after the fact.

Current code improves the safety of this degraded mode: exact own-hand evidence may repair the user's hand, but contradictory hand-size or visible-bank evidence now removes belief support, and invalid particles are rejected at the WASM boundary rather than searched as if they were valid.

Status: **not a removable warning condition; safety under degraded evidence is improved, but a session started after game history was already missed remains less trustworthy than one tracked from the beginning**.

## Build identity added

`scripts/build.mjs` now stamps the generated extension manifest with `version_name` in this form:

```text
<package version> · <branch>@<source revision>[+dirty] · <UTC build time>
```

Example from the first instrumented local build:

```text
0.9.1 · main@aebe9ede07ae+dirty · 2026-09-01T04:52:56.929Z
```

The extension popup shows the package/source portion, with the full value available as its title. The in-game Settings panel shows the installed build identity and build time. The existing runtime status independently reports the WASM engine revision, for example `deep-maxn-v9 ready`.

This distinguishes the browser-loaded JavaScript bundle from the actual WASM engine running behind it. A build marked `+dirty` identifies the exact build invocation but is not attributable to a clean Git snapshot. For source-level reproducibility, commit the intended changes and rebuild so the displayed identity has no `+dirty` marker.
