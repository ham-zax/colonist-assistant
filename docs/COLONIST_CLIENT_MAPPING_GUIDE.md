# Mapping the shipped Colonist client

This guide explains how to reproduce the compatibility/rules audit from the archived public client assets under `docs/colonist-evidence/`. The goal is not to trust a previous agent's prose; it is to make each rule claim independently traceable to a hashed artifact, runtime observation, official rule, or local engine path.

## 1. Never edit the raw snapshot

The evidence files under `docs/colonist-evidence/v320/raw/` are byte-for-byte captures. Verify them first:

```sh
scripts/colonist-evidence/verify-v320.sh
```

Create disposable readable copies with:

```sh
scripts/colonist-evidence/prettify-v320.sh
```

This writes to `docs/colonist-evidence/v320/generated/pretty/` using Prettier 3.8.1. Formatting changes line numbers, so durable claims should record the raw artifact name plus semantic/module anchors, not only a pretty-file line number.

## 2. Understand the Webpack shape

The shipped JavaScript is minified Webpack output. A useful mental model is:

```text
chunk -> numeric module id -> imports/exports -> state/schema/controller -> caller
```

A formatted module commonly looks conceptually like:

```js
12345: (module, exports, require) => {
  // module body
}
```

Calls such as `s(36872)` or similar are module imports. Short identifiers are minifier-generated and are not stable names. Numeric module IDs are useful inside one captured build but may change in a later Colonist release.

When a module exports a useful enum or class, trace both directions:

1. **definition** — what does the symbol mean?
2. **consumer/caller** — what behavior uses it?

A string or enum alone is not behavioral proof.

## 3. Start from semantic anchors

Search the generated readable copies with `rg`. Examples for the v320 snapshot:

```sh
E=docs/colonist-evidence/v320/generated/pretty

rg -n "diceSetting|DiceSetting|ClickedDice|diceThrown|dice1|dice2" "$E"
rg -n "DEFAULT_BANK_TRADE_RATIO|bankTradeRatiosState|getBankTradeError|getTradeCreationDataError" "$E"
rg -n "whereCanPlayerPlaceBeginningRoads|areAdjacentEdgesEmpty|InitialPlacement|StartingResources" "$E"
rg -n "TileBlockedByRobber|ResourceDistribution|distributedCount|requiredCount" "$E"
rg -n "Classic4P|Classic4PRandom|Classic56P|Classic78P" "$E"
rg -n "tileHexStates|tileCornerStates|tileEdgeStates|portEdgeStates" "$E"
rg -n "activeEmbargosAgainst|only1HumanPlayer|embargo" "$E"
```

The English localization file is also useful as a vocabulary index, but localization text is supporting evidence, not an implementation path.

## 4. High-value v320 anchors

These anchors were useful in the September 2026 audit. Re-search them rather than assuming old line numbers.

| Topic | Artifact / anchor | What it can establish |
|---|---|---|
| Dice enum | `shared...js`, modules reported as `36872` / `47683`, `diceSetting` / enum strings | Random/Balanced enum representation and serialization vocabulary |
| Room dice state | `shared...js`, `RoomInfoMessage`, `diceType`, `gameSetting.diceSetting` | incoming room setting is mapped into the game-setting model |
| Dice action/result | `shared...js`, `ClickedDice`; dice schema with `diceThrown`, `dice1`, `dice2` | client sends roll intent and consumes resulting dice state |
| Game manager | module `82391` / `UIMGameManager` | active-game manager/store lifecycle; absent from preserved lobby runtime |
| Trade default | `DEFAULT_BANK_TRADE_RATIO` | default 4:1 maritime ratio |
| Trade state | `bankTradeRatiosState` | one effective ratio is carried per resource |
| Bank trade validation | `getTradeCreationDataError`, `getBankTradeError`, `isBankTrade` | bank path bypasses domestic-trade/embargo validation, while retaining bank-specific legality checks |
| Embargo | `tradeState.embargoState`, `activeEmbargosAgainst` | directed player-trade restrictions |
| Opening road validation | `whereCanPlayerPlaceBeginningRoads`, `areAdjacentEdgesEmpty` | client legal-placement model for beginning roads |
| Starting distribution | `StartingResources` | protocol distinguishes starting-resource distribution; by itself it does not prove the server's adjacency calculation |
| Production/shortage | `ResourceDistribution`, `TileBlockedByRobber`, shortage event fields | protocol/log representation of production outcomes |
| Board state | `tileHexStates`, `tileCornerStates`, `tileEdgeStates`, `portEdgeStates` | instantiated board schema consumed by the client |
| Map families | `Classic4P`, `Classic4PRandom`, `Classic56P`, `Classic78P` | Colonist distinguishes normal classic, random-number, and extended base boards |

## 5. Protocol schema is not server algorithm proof

A Zod/schema definition proves what the client accepts. It does not necessarily prove how the server calculated the payload.

For example:

```text
StartingResources event exists
```

does not alone prove:

```text
server computes one card per adjacent producing hex
```

For base-game starting resources, the latter is established by published base-game rules and our deterministic board state, while the client schema shows how the result is represented.

Similarly, shortage events carrying `distributedCount` prove that partial distributions can be represented, but the base-game branch condition is better anchored in the official CATAN rule.

## 6. Absence searches require a bounded surface

Before writing “the client does not implement X,” inventory the inspected execution surfaces:

- initially loaded first-party bundles;
- gameplay bundle(s);
- relevant lazy chunks;
- site service worker;
- `Worker` construction sites;
- WebAssembly construction/loading sites;
- dynamically requested first-party resources visible through browser DevTools.

For the v320 snapshot, no gameplay WASM or gameplay worker containing dice generation was found. The preserved service worker is caching/routing support. The defensible conclusion is therefore:

> The exact Balanced Dice generator was not located in the inspected client execution surface; the client carries the selected dice mode, sends a roll intent, and receives roll results.

Do not turn that negative finding into a claim about the server's private implementation language or exact code path.

## 7. Runtime evidence must actually be runtime evidence

`docs/colonist-evidence/v320/runtime/dice-settings.json` deliberately records that `rootStoreState.gameSettings.diceSetting` was **not accessible in the passive lobby state** because no active game manager/store was instantiated.

Static client dataflow strongly supports the active-game property, and our bridge already reads neighboring `rootStoreState.gameSettings` fields. But future work that wants the strongest runtime proof should passively capture an already-active game and save the exact evaluated value/object under a new evidence snapshot.

Never write `missing => Random`. An unavailable path is an observation failure/unknown state, not proof of enum value `0`.

## 8. Balanced Dice: separate mechanism from live constants

Colonist's first-party Balanced Dice article describes a stateful dice-deck plus recent-roll weighting mechanism. The article prose and its linked updated code have differed on tuning constants. Therefore:

- **safe architectural fact:** Balanced Dice is stateful/non-i.i.d.; a second fixed pip table is not an exact model;
- **unsafe live-contract assumption:** hard-code a published reshuffle threshold or probability-reduction constant as if it were proven production-server v320 behavior.

Use the actual `diceSetting` state to distinguish mode. Treat an exact stochastic model as a separate evidence/research problem.

## 9. Compare against the engine at ownership boundaries

For a rule claim, check the smallest local owner:

- setup, production, robber, bank and trading rules: `engine/crates/catan-core/src/state.rs`;
- board generator/topology: `engine/crates/catan-core/src/board.rs`;
- heuristic production and opening economics: `engine/crates/catan-search/src/eval.rs` and `opening.rs`;
- browser observability: `src/page/bridge.ts` and `src/core/placement.ts`;
- WASM contract: `engine/crates/catan-wasm/src/lib.rs` and worker adapter;
- GPU simulator/parity: `engine/crates/catan-search/src/cuda_sim.rs` and `src/cuda/sim.cu`.

Do not say “engine exact” without specifying whether you mean the deterministic rules transition, the search simulator, or the heuristic evaluator.

## 10. Claim template for future audits

For every material finding, record:

```text
Claim:
Scope: base 4P / base 5-6P / base 7-8P / other
Evidence class: shipped client | live runtime | live protocol | official rule | local engine | inference
Artifact + stable anchor:
Local owner:
Directly proves:
Does not prove:
Status: exact | approximate | mismatch | unknown
Follow-up:
```

## 11. Updating the corpus for a new Colonist release

Do not overwrite `v320`.

1. Create `docs/colonist-evidence/v<new-version>/`.
2. Re-enumerate loaded first-party assets in the live page.
3. Save exact raw bytes and hashes.
4. Save runtime identity fields.
5. Re-run the semantic anchor searches above.
6. Diff conclusions, not just file hashes.
7. Update `COLONIST_BASE_GAME_RULE_FIDELITY.md` only where new evidence changes a rule/observability contract.

The purpose of versioned evidence is to make “Colonist changed” a testable proposition rather than a guess.
