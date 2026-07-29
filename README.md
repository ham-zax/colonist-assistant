# Colonist Assistant

Colonist Assistant is an unofficial, local-only Chrome extension for friendly,
fully informed Colonist games. It tracks public resource evidence, reads the
user’s own visible hand, models hidden opponent hands as legal belief
particles, and keeps one recommended next action highlighted in the Colonist
interface.

The default decision engine is a clean-room Rust/WASM implementation of
belief-aware multiplayer MaxN. It combines weighted hidden-card worlds, exact
mandatory-action solvers, complete-turn plans, learned value/trade models,
and a state-validated live executor. A defensive paranoid AlphaBeta peer,
experimental PUCT, JavaScript multiplayer rollouts, a deterministic race
model, and a blended legacy model remain selectable comparisons.

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
- Provides an opt-in autopilot for the current match. It is off by default.
- Resets its game state when Colonist publishes a new game identity.

The extension does not intercept WebSockets, read cookies, inspect account
tokens, reveal server-only information, use analytics, or make runtime network
requests. It cannot know hidden cards Colonist has not shown; uncertainty is
represented explicitly.

## Build

Requirements:

- Node.js 22+
- Rust 1.90+
- the `wasm32-unknown-unknown` Rust target
- `wasm-bindgen-cli`

```bash
npm install
npm run verify
```

The unpacked extension is written to `dist/`. Load that directory from
`chrome://extensions` with Developer mode enabled. After loading or updating
the unpacked build, refresh every already-open Colonist tab; Chrome does not
replace a running content script until its page reloads. The live panel’s
settings screen shows the installed build number and active decision engine.

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
