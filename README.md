# Colonist Assistant

Colonist Assistant is an unofficial Chrome extension for friendly Colonist
games where all players agree to its use. It reads game data shown to the
player, tracks known cards, keeps honest ranges for unknown cards, and marks
one legal next step in the Colonist page.

The main game engine runs as Rust and WebAssembly in the browser. Deep MaxN is
the default. AlphaBeta is a more guarded peer. Belief PUCT is still an
experiment. Older JavaScript and race models remain as test choices.

This project is not affiliated with, endorsed by, or sponsored by Colonist or
CATAN Studio. Use it only in games where every participant has agreed to
assistant use, and review the platform rules before playing.

## What it does

- Tracks exact public gains, spends, trades, and known transfers.
- Maintains possible opponent hands after unknown steals and discards.
- Reconciles those beliefs with exact own cards, public hand totals, and
  visible bank counts.
- Scores opening and normal settlements, connected road routes, city upgrades,
  robber targets, discards, player trades, maritime trades, and development
  card timing.
- Shows live win estimates by player.
- Highlights the next Colonist control or board location.
- Can carry out the next step when the user turns on autopilot. Autopilot is
  off by default.
- Resets its game state when Colonist publishes a new game identity.

Before each automatic step, the extension checks that the board still matches
the state used to pick the move. It stops and plans again when the state has
changed.

## What it reads

On `colonist.io`, the extension may read player display names, the public game
log, public game actions, the board, public card totals, legal move targets,
your own shown resource cards, and bank counts when the room shows them.

It does not read game chat, cookies, account tokens, network messages,
opponents’ hidden cards, or hidden development cards. It does not use a
server, ads, tracking, or usage reports. Game work stays in the browser.
Chrome may sync settings through the user’s Google account when Chrome Sync is
on.

Read the full
[privacy policy](https://rgo.pt/privacy/colonist-assistant).

## Build

Requirements:

- Node.js 22+
- Rust 1.90+
- the `wasm32-unknown-unknown` Rust target
- `wasm-bindgen-cli`

```bash
npm ci
npm run verify
```

The unpacked extension is written to `dist/`. Load that directory from
`chrome://extensions` with Developer mode enabled. After loading or updating
the unpacked build, refresh every already-open Colonist tab; Chrome does not
replace a running content script until its page reloads. The live panel’s
settings screen shows the installed build number and active decision engine.

For the Chrome Web Store field copy, test steps, and release package command,
see [docs/CHROME_WEB_STORE.md](docs/CHROME_WEB_STORE.md).

`Background WASM` means the packaged Rust engine is authoritative. Selected
engines are never replaced by a local JavaScript policy. A decision that is
still running after five seconds is labelled `WASM · 5s+` and continues in the
same background request. A genuine service-worker or WASM failure is shown as
`WASM error`; autopilot waits instead of executing a different algorithm.

## Decision engines

- **Deep MaxN** — production default. Deterministic multiplayer search
  aggregated over weighted hidden-card beliefs; currently the strongest
  validated live policy.
- **AlphaBeta** — a clean-room paranoid multiplayer AlphaBeta search. It is a
  close simulator peer to MaxN but assumes the opponents collectively minimize
  your result, so it generally plays more defensively.
- **Belief PUCT** — experimental. Runs vector-valued shared
  information-set search over weighted hidden-card particles, with exact
  mandatory solving, whole-turn plans, progressive widening, and tree reuse.
- **Hybrid** — the previous deterministic/rollout blend.
- **Vector rollouts** — the previous JavaScript multiplayer simulation model.
- **Race ETA** — fastest deterministic fallback.

This project does not copy or embed Catanatron, JSettlers, Monte Catano, or
HexMachina code. The rules engine and search implementation are clean-room and
MIT licensed.

## Benchmarking

The native arena supports 2–4 players, seat rotation, matched board/chance
seeds, parallel execution, invariant validation after every transition, turn
cutoff reporting, and board-blocked bootstrap confidence intervals.

```bash
cd engine
cargo run --release -p colonist-catan-arena -- \
  --players 4 --blocks 250 --threads 16 --validate --quiet \
  --candidate maxn --baseline alphabeta --seed 9100001
```

See [docs/BENCHMARKS.md](docs/BENCHMARKS.md) for results and limitations.
For the matrix runner, live Colonist Base-map harness, and pinned Catanatron
reference runner, see [docs/BENCHMARK_TOOL.md](docs/BENCHMARK_TOOL.md).
The current strategic architecture and promotion gates are documented in
[docs/STRATEGIC_ENGINE_V3.md](docs/STRATEGIC_ENGINE_V3.md).

## Scope

The simulator targets standard base-game Catan for 2–4 players. The live
extension degrades to its board heuristics when Colonist exposes a variant the
rules core does not support. Benchmarks are bot-vs-bot simulator results, not a
claim of a particular win rate against humans.

## License and privacy

Code is available under the [MIT License](LICENSE). See
[PRIVACY.md](PRIVACY.md), [SECURITY.md](SECURITY.md), and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
