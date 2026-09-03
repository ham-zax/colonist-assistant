# v320 client anchor index

This file is a fast navigation index into the hashed v320 snapshot. Generate readable copies first with `scripts/colonist-evidence/prettify-v320.sh`, then use the search anchors below.

Statuses are deliberately narrower than the original D1/D2 prose.

| ID | Topic | Search anchor | Evidence disposition |
|---|---|---|---|
| DICE-01 | Dice enum | `diceSetting`, `Random`, `Balanced`, `StressTestSequence`, `PredefinedDiceSequenceDeleted` | **Shipped-client proven:** enum representation exists; D2 reported modules 36872/47683 |
| DICE-02 | Game setting property | `gameSetting`, `gameSettings`, `diceSetting` | **Shipped-client proven:** dice mode is represented in game/room setting state |
| DICE-03 | Room protocol mapping | `RoomInfoMessage`, `diceType`, `gameSetting` | **Shipped-client proven:** incoming room `diceType` maps into client dice setting |
| DICE-04 | Roll request | `ClickedDice` | **Shipped-client proven:** client sends a roll intent |
| DICE-05 | Roll result | `diceThrown`, `dice1`, `dice2` | **Shipped-client proven:** result state/schema carries the two die values |
| DICE-06 | Exact Balanced generator | search RNG/dice/deck/worker/WASM surfaces | **Not located in inspected client surface.** Do not claim private server implementation details from absence alone |
| DICE-07 | Active-game runtime path | `rootStoreState.gameSettings.diceSetting` | **Static contract supported; archived runtime not observed.** `runtime/dice-settings.json` records lobby unavailability |
| OPEN-01 | Initial placement states | `InitialPlacement`, `PlaceSettlement`, `RoadPlacement` | **Shipped-client representation proven;** server setup scheduler not live-captured |
| OPEN-02 | Beginning road locations | `whereCanPlayerPlaceBeginningRoads`, `areAdjacentEdgesEmpty` | **Shipped-client legal-placement evidence;** aligns with one opening road from each settlement |
| OPEN-03 | Starting resource event | `StartingResources` | **Protocol representation proven.** Exact base calculation comes from published rule + deterministic board, not this enum alone |
| PROD-01 | Production representation | `ResourceDistribution` | **Shipped-client protocol/schema evidence** |
| PROD-02 | Robber blocking | `TileBlockedByRobber`, `No resources produced` | **Shipped-client/log semantics;** published base rule supplies behavioral contract |
| PROD-03 | Shortage events | `distributedCount`, `requiredCount`, shortage/log event IDs | **Shipped-client representation supports zero/partial outcomes;** official CATAN rule establishes branch conditions |
| TRADE-01 | Default maritime ratio | `DEFAULT_BANK_TRADE_RATIO` | **Shipped-client proven:** 4 |
| TRADE-02 | Effective ratio state | `bankTradeRatiosState` | **Shipped-client proven:** one effective ratio per resource |
| TRADE-03 | Bank path | `isBankTrade`, `getTradeCreationDataError`, `getBankTradeError` | **Shipped-client proven:** bank validation path is separated from domestic player-trade validation |
| TRADE-04 | Domestic embargo | `embargoState`, `activeEmbargosAgainst` | **Shipped-client proven:** directed embargo concept exists |
| TRADE-05 | Player count validator | `only1HumanPlayer`, `playerValidators.length` | **Narrow conclusion:** validator blocks a one-player-state trade case; do not reinterpret localization wording as proof of human/bot counting semantics |
| ROOM-01 | Room settings | `RoomInfoMessage`, `maxPlayers`, `friendlyRobber`, `hiddenBankCards`, `diceType` | **Positive settings inventory:** no current base-room global domestic-trade toggle in this mapping |
| BOARD-01 | Instantiated board | `tileHexStates`, `tileCornerStates`, `tileEdgeStates`, `portEdgeStates` | **Shipped-client proven:** client consumes an instantiated board |
| BOARD-02 | Normal/random base maps | `Classic4P`, `Classic4PRandom` | **Shipped-client proven:** distinct map identities |
| BOARD-03 | Extended base maps | `Classic56P`, `Classic78P` | **Shipped-client proven:** 5–6 and 7–8 base-mode/map identities exist |
| WORKER-01 | Service worker | archived `raw/service-worker.js` | **Inspected:** caching/routing support, not game-rule dice generation |
| WORKER-02 | Other workers/WASM | search `new Worker`, `WebAssembly` across snapshot | **No gameplay dice generator located** in inspected execution surface; telemetry/compression helpers do not establish game-rule ownership |

## Common searches

```sh
E=docs/colonist-evidence/v320/generated/pretty
rg -n "diceSetting|ClickedDice|diceThrown|dice1|dice2" "$E"
rg -n "whereCanPlayerPlaceBeginningRoads|areAdjacentEdgesEmpty|StartingResources" "$E"
rg -n "DEFAULT_BANK_TRADE_RATIO|bankTradeRatiosState|getBankTradeError|getTradeCreationDataError" "$E"
rg -n "distributedCount|requiredCount|TileBlockedByRobber|ResourceDistribution" "$E"
rg -n "Classic4P|Classic4PRandom|Classic56P|Classic78P" "$E"
rg -n "RoomInfoMessage|maxPlayers|friendlyRobber|hiddenBankCards|diceType" "$E"
rg -n "new Worker|WebAssembly" "$E"
```

When an anchor is found, read enough surrounding module/caller context to establish semantics. A matching token is a starting point, not a conclusion.
