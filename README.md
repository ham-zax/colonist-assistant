# Colonist Assistant

Colonist Assistant is an unofficial Chrome extension for friendly Colonist
games where all players agree to its use. It reads game data shown to the
player, tracks known cards, keeps honest ranges for unknown cards, and marks
one legal next step in the Colonist page.

The decision engine runs locally as Rust and WebAssembly in the browser.
Strategist ★ is the default user-facing decision engine. The settings panel
also exposes the historical comparison algorithms: defensive AlphaBeta and
experimental belief PUCT for WASM search, plus the Hybrid, Race ETA, and
Vector rollouts display policies. MaxN remains the strongest validated default.

This project is not affiliated with, endorsed by, or sponsored by Colonist or
CATAN Studio. Use it only in games where every participant has agreed to
assistant use, and review the platform rules before playing.

## Install

Install from the
[Chrome Web Store](https://chromewebstore.google.com/detail/colonist-assistant/giljoabonkhaolnpnbfpahenndegloin).

## What it does

- Tracks exact public gains, spends, trades, and known transfers.
- Maintains possible opponent hands after unknown steals and discards.
- Reconciles those beliefs with exact own cards, public hand totals, and
  visible bank counts.
- Uses a bounded snake-order search for both opening settlements and their
  roads, with a production floor that rejects catastrophically weak local
  opening clicks, then compares normal settlements, connected road routes, city
  upgrades, compound robber/victim targets, discards, trades, and
  development-card timing.
- Applies Colonist's Friendly Robber restriction to recommendations and every
  click validation, including the default rule in 1v1 games.
- Shows live win estimates by player.
- Highlights the next Colonist control or board location.
- Can carry out the next step when the user turns on autopilot. Autopilot is
  off by default. A settings delay of 1, 3, or 5 seconds paces each automatic
  click.
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
settings screen shows the installed build number and Strategist runtime.

For the Chrome Web Store field copy, test steps, and release package command,
see [docs/CHROME_WEB_STORE.md](docs/CHROME_WEB_STORE.md).

`Background WASM` means the packaged Rust Strategist is authoritative. A
normal live request uses depth 4, a branch cap of 16, at most 4,000 strategic
nodes, and a cooperative 350 ms strategic-search deadline. The generated-WASM
cold-adapter regression must return a legal action in less than one second.
The `WASM · 1s+` label and twelve-second client cutoff are failure-containment
limits, not acceptable target latency. At the client cutoff the request is
reported as an engine error and autopilot remains paused. A service-worker or
WASM failure is also shown as `WASM error`; no JavaScript action policy
substitutes for Strategist.

## Decision engine

**Deep MaxN / Strategist** is the default live engine. Complete local enumeration handles
mandatory and parameterized action families. Setup uses the dedicated
belief-aggregated snake-order draft solver; normal play uses bounded,
vector-valued weighted-belief Deep MaxN with structured action ordering. Each
simulated player advances its own race value rather than being treated as part
of a single hostile coalition.

The extension ships with a live-search budget of depth 3, a 16,000-node limit,
48 belief particles, and a 1-second hard thinking-time cap for Deep MaxN.
AlphaBeta defaults to the Catanatron-inspired depth 2 profile with 12,000
nodes, 32 belief particles, pruning, and the same cap. Open **How it thinks**
to customize depth, branch cap, node budget, belief particles, PUCT
iterations/rollouts, and maximum thinking time independently for each deep
engine; the panel also shows an estimate for the selected budget.

The bundled learned policy and value heads are both unpromoted and disabled
because their grouped validation evidence did not pass the production gates.
Structured action priors and the strategic evaluator remain authoritative.
The selected AlphaBeta and belief PUCT modes use the same observation-safe
WASM boundary and weighted beliefs; Hybrid, Race ETA, and Vector rollouts are
display-only comparison policies and do not replace the state-validated click
harness. The public build-time estimate remains display-only. Current strength
is still being measured; this README does not claim that any mode is stronger
than humans or every baseline, and model estimates are not calibrated
guarantees.

### Win percentages

The displayed percentages are normalized relative race estimates and always
sum to 100% across the players in the current game. Deep search contributes
the Rust evaluator's normalized strategic-value vector; the display also
blends public ETA/evidence and smooths abrupt changes between materially
similar board states. These are not calibrated win probabilities.

The Rust evaluator is inspired by the same broad features as Catanatron's
reference value function—public points, production, expansion, hand safety,
development cards, and awards—but it is not a verbatim port of Catanatron's
Python weights or hidden-state assumptions. Benchmark win shares should not be
used as calibration data for the live percentage display without a separate
held-out calibration run.

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
  --players 4 --blocks 1 --threads 4 --validate --quiet \
  --candidate maxn --baseline weighted \
  --iterations 75 --belief-particles 32 \
  --seed 9200001
```

In the native arena, `maxn` (also accepted as `deep`) is the comparison closest
to the packaged live core. `puct` selects experimental belief PUCT; the old
`strategist` token remains only as a compatibility alias for `puct`. Neither
PUCT name identifies the current live authority.

See [docs/BENCHMARKS.md](docs/BENCHMARKS.md) for results and limitations.
For the matrix runner, live Colonist Base-map harness, and pinned Catanatron
reference runner, see [docs/BENCHMARK_TOOL.md](docs/BENCHMARK_TOOL.md).
The current Strategist architecture and model gates are documented in
[docs/STRATEGIC_ENGINE_V3.md](docs/STRATEGIC_ENGINE_V3.md).

## Scope

The simulator targets standard base-game Catan for 2–4 players. Other Colonist
variants are outside the validated strategy and benchmark scope; they do not
inherit standard-base strength claims. Benchmarks are bot-vs-bot simulator
results, not a claim of a particular win rate against humans.

## License and privacy

Code is available under the [MIT License](LICENSE). See
[PRIVACY.md](PRIVACY.md), [SECURITY.md](SECURITY.md), and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
