# Engineering session summary — 2026-09-04

This document records the accepted Colonist rules/runtime findings and integrated repairs completed during the 2026-09-04 session. It is a closure index, not a replacement for the authoritative rule-fidelity ledger in `docs/COLONIST_BASE_GAME_RULE_FIDELITY.md`.

## Scope

The session focused on ordinary 2–4 player base-game fidelity, synthetic Classic4P generation, dice-mode observability, opening economics, bank-shortage production, and live action-execution reliability.

## Opening evaluation and maritime economics

The opening solver now distinguishes completed setup endpoints from partial node-limit values instead of allowing incomplete setup paths to remain authoritative when complete roots exist.

Opening valuation also accounts for deterministic second-settlement starting resources and legal maritime conversion at the best available 4:1 / generic 3:1 / matching specific 2:1 rate without speculative domestic trades or double-counting shared surplus.

Strategic placement remains resource/bottleneck aware rather than reducing opening quality to raw pips or five-resource coverage. A weak-number resource can still be valuable when it closes a concrete road/settlement bottleneck, while concentrated strong production can be valuable when ports make it convertible.

Integrated commits include `d1b36c2` (`Fix opening setup valuation and maritime economics`) and the corresponding packaged-WASM refresh `61aa999`.

## Bank-shortage production fidelity

Base-game production now follows the established bank-shortage rule:

- if the bank has enough of a resource, all owed production is paid normally;
- if supply is short and exactly one distinct player is owed that resource, that player receives the remaining bank supply up to demand;
- if supply is short and multiple distinct players are owed that resource, none of those players receives that resource.

CPU and CUDA behavior were repaired together and parity-covered. Integrated source commit: `b320215`; packaged-WASM synchronization: `760a951`.

## Dice-mode observability and E3 boundary

The canonical observed dice-mode contract remains:

- `unknown`
- `random`
- `balanced`
- `unsupported`

E3 closed the previous active-game observation gap in live Colonist game `game4519`:

- `gameController.gameSettings.diceSetting = 1`;
- the active game store independently reported `gameSettings.diceSetting = 1`;
- shipped-client enum evidence maps `1` to `Balanced`.

The normal Roll authority path was established as:

```text
User presses Roll
    -> gameSendEvent.sendClickedDice()
    -> socketGameSend.clickedDice()
    -> send(ClickedDice, true)
    -> GameAction sent to the Colonist server
```

The ordinary Roll request does not carry a chosen dice pair, result, client RNG value, probability vector, or RNG seed. The realized `dice1` / `dice2` values subsequently arrive through server game-state updates. `ConfirmDicePair` is a separate explicit dice-selection mechanic and is not the ordinary Roll path.

The investigated client exposes the current dice pair and historical individual dice pairs, but no sufficient hidden balancing state or authoritative transition function was established. Therefore the exact state-dependent conditional next-roll law for Colonist Balanced Dice is **not identifiable from current client observability**.

This does **not** make Balanced-mode information strategically useless. The existing ordinary bell-curve pip hierarchy remains the defensible long-run placement/production prior:

```text
6 / 8  -> 5 pips
5 / 9  -> 4 pips
4 / 10 -> 3 pips
3 / 11 -> 2 pips
2 / 12 -> 1 pip
```

The assistant already uses that hierarchy across placement/opening/evaluation surfaces. In Balanced games it may be treated as an approximate long-run production guide, but not as proof of an exact hidden next-roll probability table at a particular game state. No E3 evidence justifies changing chance sampling, state hashes, RNG state, CUDA state, PTX, or inventing a second static Balanced probability table.

The detailed evidence boundary is maintained in `docs/COLONIST_BASE_GAME_RULE_FIDELITY.md`.

## Synthetic Classic4P generator and provenance

The historical unrestricted synthetic generator is now an explicit compatibility contract:

`legacy-randomized-v1`

A separate repository-defined legal/intentional ordinary 4P synthetic generator is now explicit as:

`classic4p-v1`

Classic V1 preserves the normal base topology/resource/number/port multisets and rejects number-token layouts containing adjacent red 6/8 tokens. It is deterministic by seed but does **not** claim exact Colonist server seed-to-board equivalence.

Ordinary 4P self-play/game-strength/training generation routes to Classic V1. Historical/parity/tactical/stress fixtures retain Legacy V1 where preserving the former seeded identity is the correct contract. Explicitly captured live Colonist boards remain explicit and are not regenerated from either synthetic contract.

Seed-bearing persisted data now records generator provenance where required. Historical missing `boardGenerator` fields retain Legacy interpretation. Snapshot schema-v2 binds generator identity to state identity so changing the generator label cannot be silently accepted even if two generator contracts happen to produce the same structural board for one seed.

Independent R1 found one remaining provenance defect in persisted arena/WebGPU benchmark output. The repair added explicit generator identity to both producers. R2 subsequently passed and discharged that blocker: 4P arena output reports `classic4p-v1`, 2P/3P report Legacy semantics, and WebGPU feasibility reports `legacy-randomized-v1`.

Integrated commits:

- `5c106bc` — `Split Classic4P synthetic generator contracts`
- `d70049a` — `Finalize Classic4P fidelity integration`

## Control-dispatch commit lifecycle

A live execution failure showed that the post-click commit observer incorrectly treated harmless recommendation/signature refresh as proof that a dispatched control had failed. This produced premature failures such as:

`Colonist state changed without the expected control commit`

The repaired lifecycle deliberately separates two authorities:

1. **Before dispatch:** validation remains strict. The recommendation signature/legal target must still be current before the click is allowed.
2. **After dispatch:** the already-dispatched control owns an in-flight commit observation. Harmless guide/search/recommendation churn inside the same live game must not cancel that observer. A real game-scope invalidation still terminates it.

`src/content/action-guide.ts` implements this through a separate post-dispatch `validateControlContinuation` contract. `src/content/overlay.ts` supplies the same-game authority through `stillInGuideGame`.

The reported decline-trade failure is now permanent regression coverage: the decline is clicked, guide signature refreshes while the bridge update is delayed, and the observer remains alive until rejection appears. The same `awaitControlCommit()` boundary is shared by Roll and End Turn, so the repair addresses the same premature-failure mechanism there without relaxing their pre-click legality checks.

Integrated commit: `315fbf1` (`Keep control commit watcher across guide refresh`).

The regression and integrated type/build checks passed. A real post-reload Colonist game is still the final confirmation that the previous premature-failure cycles no longer appear in live recordings.

## Live-board and browser boundaries

Synthetic board-generation changes do not change live recommendation ingestion. Live recommendations continue to consume the instantiated Colonist board observed from the client rather than regenerating it from a repository seed.

Repository/build completion is also distinct from browser deployment. After the final build, the unpacked extension still needs to be reloaded in the actual Edge profile used for Colonist and open Colonist tabs refreshed before a live smoke can establish runtime deployment.

## Deferred / intentionally not implemented

- No exact Colonist Balanced-Dice stochastic model is claimed or implemented without authoritative server algorithm/state evidence.
- No static alternate Balanced pip table is introduced; the existing bell-curve pip hierarchy remains the strategic long-run prior.
- Exact Colonist server seed-to-board reproduction is not claimed for `classic4p-v1`.
- First-class base-board topology extraction and complete removal of the historical `standard()` compatibility alias remain optional cleanup, not closure blockers.
- 5–8 player support remains a separate future program and is not part of this ordinary 2–4 player closure.

## Closure state

At the time this summary was authored, the reviewed F2 generator/provenance work, E2/E3 dice-mode findings, bank-shortage repair, opening economics repair, and control-commit lifecycle repair were all integrated on `main`. The remaining closure work is final integrated verification/build synchronization plus live extension reload/smoke; those are deployment/verification steps rather than additional game-model implementation missions.
