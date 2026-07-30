# Chrome Web Store release

This file holds the text for the Chrome Web Store dashboard.

## Build the file to upload

Run:

```bash
npm ci
npm run verify
release_version=$(node -p "require('./package.json').version")
(
  cd dist
  zip -r "../colonist-assistant-${release_version}.zip" .
)
```

The ZIP must have `manifest.json` at its root. `npm run verify` includes the
packaged cold-WASM adapter regression: the single Strategist authority must
return a legal weighted-belief Deep MaxN result in less than one second before
packaging. Do not substitute an experimental arena policy or publish a build
that misses this gate.

## Store listing

### Name

```text
Colonist Assistant
```

The name comes from `manifest.json`.

### Short description

```text
Local card tracking and board-aware advice for friendly Colonist games.
```

The short description comes from `manifest.json`.

### Full description

```text
Colonist Assistant gives local, board-aware help for friendly Colonist games
where all players agree to its use.

On colonist.io, it reads game data shown to you. This may include player
display names, the public game log, public game actions, the board, public card
totals, legal move targets, your own shown resource cards, and bank counts when
the room shows them.

It uses that data to:

• track known cards and honest ranges for unknown cards
• suggest legal moves, builds, trades, robber targets, and discards
• show model-based win estimates
• mark the next Colonist control or board place
• carry out moves in a verified private or all-bot game when you turn on
  autopilot

Autopilot is off by default and remains unavailable unless the extension can
verify a private game or an all-bot match. Before each step, it checks that the
board still matches the state used to pick the move. If the state has changed,
it stops and plans again.

Game work runs in your browser. The extension has no server, ads, tracking, or
usage reports. It does not send game data to the developer.

The extension does not read game chat, cookies, account tokens, network
messages, or hidden card data that Colonist has not shown to you.

This extension is not linked to, backed by, or made by Colonist or CATAN
Studio. Use it only when every player has agreed to its use. Check the game
site rules before you play.
```

### Category

Choose `Tools` if the dashboard offers it. If not, choose the closest tool or
game aid group.

### Language

Choose `English`.

### Homepage

```text
https://github.com/rodrgds/colonist-assistant
```

### Support URL

```text
https://github.com/rodrgds/colonist-assistant/issues
```

### Privacy policy URL

```text
https://rgo.pt/privacy/colonist-assistant
```

## Privacy

### Single purpose

```text
Give local, board-aware advice and optional state-checked play help for
friendly Colonist games where all players agree to its use.
```

### Storage reason

```text
The storage permission saves settings, the overlay place, current game events,
possible card states, a short game summary, and bounded current-game decision
diagnostics used to replay advice or automation failures. Game data stays in
the local Chrome profile and is replaced when a different game starts. Chrome
may sync settings when Chrome Sync is on.
```

### Host access reason

```text
Access to https://colonist.io/* lets the extension read game data shown to the
player, add its help panel, mark legal controls, and carry out a step only when
the user turns on autopilot in a verified private or all-bot game.
```

### Remote code

Choose:

```text
No, I am not using remote code.
```

The JavaScript and WebAssembly game engine ship in the ZIP.

### Data types

Tick:

- Personally identifiable information
- Website content

Player display names can count as names or user names. The board, game log,
shown cards, and game state count as website content.

Do not tick browser history. The extension does not keep a list of sites or
pages the user has visited.

### Data use checks

Confirm each check that says:

- the extension does not sell user data;
- the extension does not use user data for ads or credit checks;
- the extension uses data only for its stated task; and
- people do not read the user data.

These claims must keep matching the code and privacy policy.

## Test instructions

Paste:

```text
1. Install the extension.
2. Open https://colonist.io/ and start an all-bot base game, or join a private
   base game where all players agreed to use the extension.
3. If the Colonist tab was open before install, refresh it.
4. The Colonist Assistant panel appears during the game.
5. Use the extension popup to turn hints on or off, confirm the single
   Strategist engine status, or turn on autopilot. Autopilot is off by default;
   it is available only after the extension verifies a private or all-bot
   game. There is no engine selector.
6. The panel reads shown game state and gives a legal next step.
7. Use Reset session in the popup to clear the current game state.

No extension account or test login is needed. The extension has no server and
does not run remote code.
```

## Distribution

Set:

- visibility to `Public`;
- all regions, unless there is a clear reason to limit them; and
- in-app purchases to `No`.

Choose delayed publish when you send the first build for review. Once Google
accepts it, install and test the store build, then publish it.

## Images

Add:

- the 128 by 128 icon from `assets/icons/icon-128.png`;
- the 1280 by 800 images from `assets/screenshots/`;
- the 440 by 280 small tile from
  `assets/promo-tiles/colonist-assistant-small-440x280.png`; and
- the 1400 by 560 marquee tile from
  `assets/promo-tiles/colonist-assistant-marquee-1400x560.png`.

Use real screens. Hide private player names. Do not claim that the extension
can see hidden cards or promise a win rate.
