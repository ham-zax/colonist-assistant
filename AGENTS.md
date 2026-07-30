# Colonist Assistant contributor guide

## Mission and scope

Colonist Assistant is an unofficial Manifest V3 browser extension for
consensual, friendly Colonist games. It observes information already available
to the player, maintains honest hidden-card beliefs, recommends legal actions,
and can execute those actions through a state-validated UI harness.

Do not add WebSocket interception, cookie or token access, remote code,
analytics, account automation, or claims that hidden server-only information is
known. Preserve the local-only privacy model and the extension’s
`https://colonist.io/*` host scope.

## Architecture

- `src/page/bridge.ts`: reads Colonist’s public page state and publishes a
  validated board snapshot.
- `src/content/`: session orchestration, overlay UI, action highlighting, and
  the state-validated click executor.
- `src/core/`: parser, belief tracker, heuristics, trades, placement, and
  shared domain types.
- `src/worker/`: converts live observations into Rust/WASM requests and maps
  search results back to extension actions.
- `engine/crates/catan-core`: deterministic rules and legal action generation.
- `engine/crates/catan-search`: exact solvers, MaxN, paranoid AlphaBeta,
  PUCT/UCT, evaluation, planning, and learned model weights.
- `engine/crates/catan-wasm`: browser boundary.
- `engine/crates/catan-arena`: reproducible 2–4 player benchmarking.
- `static/`: extension manifest, popup, icons, fonts, and privacy copy.
- `tests/`: TypeScript/jsdom regression tests.

## Source of truth

- Deep MaxN is the validated default.
- AlphaBeta is a defensive simulator peer and selectable comparison.
- Belief PUCT is experimental until held-out results justify promotion.
- User-facing win percentages are stabilized model estimates, not calibrated
  guarantees.
- Mandatory actions and legal-state validation take priority over strategic
  search. Never let a stale search result click through a changed board.

Update code, tests, README, and benchmark documentation together when these
truths change. Do not describe tuning runs as held-out results or simulator
results as live Colonist win rates.

## Development commands

```bash
npm ci
npm run check
npm test
npm run verify:rust
npm run build
npm run verify
```

The release build is `dist/`. Reload the unpacked extension and refresh every
open Colonist tab after rebuilding.

For targeted native benchmarks:

```bash
cd engine
cargo run --release -p colonist-catan-arena -- \
  --players 4 --blocks 250 --threads 4 --validate --quiet \
  --candidate maxn --baseline alphabeta --seed 9100001
```

Use matched board/chance seeds, rotate every seat, report cutoffs, and retain
confidence intervals. Keep interactive live decisions under one second on the
reference machine; long training and arena runs must not share the live worker.

## Implementation rules

- Use TypeScript strict types and Rust’s existing domain types; avoid `any`
  outside narrow test fixtures or serialization boundaries.
- Put reusable decision logic in `src/core` or `catan-search`, not in overlay
  rendering or click orchestration.
- Treat Colonist DOM selectors as unstable. Prefer validated public bridge
  state, semantic controls, bounded retries, and explicit recovery.
- Every multi-click workflow must assert its state signature and legal target
  set before continuing. On mismatch, cancel and replan.
- Keep trade workflows idempotent. Remember rejected/failed bundles, stop
  identical-offer loops, and close stale panels before unrelated actions.
- Preserve honest uncertainty in card tracking and UI copy.
- Add a focused regression test for every reported live failure.
- Use existing resource and piece artwork/token helpers rather than emoji for
  game pieces and cards. Small engine-status symbols such as `★` and `🧪` are
  acceptable.

## UI conventions

The in-game overlay and popup share the compact dark navy, warm yellow, and
Archivo Narrow visual language. Keep the primary next action unmistakable,
avoid nested card-like containers, keep engine labels short, and test long
player names plus 3–4 player layouts. A cosmetic recommendation must never
contradict the highlighted or autonomous action.

## Repository hygiene

Do not commit `dist/`, `release/`, `node_modules/`, `engine/target/`,
`benchmark-results/`, training data, browser profiles, logs, or secrets.
Generated WASM bindings under `src/generated/wasm/` are source inputs for the
packaged build and should remain versioned. Preserve third-party notices for
fonts and other bundled assets.

## Cursor Cloud specific instructions

There is no `npm run dev` server. The runnable artifact is the unpacked
extension in `dist/` after `npm run build` (or `npm run verify`).

### Toolchain (one-time per VM image)

- Node.js 22+ (preinstalled on the reference image).
- Rust **1.90+** (`rustup update stable`). The workspace `rust-version` is
  1.90; older stable toolchains fail `verify:rust`.
- `rustup target add wasm32-unknown-unknown`
- `wasm-bindgen-cli` (`cargo install wasm-bindgen-cli --locked`) when
  `wasm-bindgen` is not already on `PATH`.

### Build and test

Use the standard commands from this file: `npm ci`, `npm run check`, `npm
test`, `npm run verify:rust`, `npm run build`, `npm run verify`. The release
extension lands in `dist/`.

Quick native smoke (no browser): after a release build, `engine/target/release/colonist-arena`
can run a 2-player block in under a second, e.g. `--candidate weighted --baseline
random --players 2 --blocks 1 --threads 1 --validate --quiet`.

### Manual Chrome testing in the cloud VM

Load the unpacked build with Chrome/Chromium:

```bash
google-chrome-stable --no-sandbox --disable-dev-shm-usage --no-first-run \
  --use-gl=angle --use-angle=swiftshader-webgl \
  --user-data-dir=/tmp/chrome-colonist-dev \
  --disable-extensions-except="$PWD/dist" \
  --load-extension="$PWD/dist" \
  https://colonist.io/
```

Without the WebGL flags, colonist.io shows a blocking “WebGL Inactive” modal.
After each rebuild, reload the extension on `chrome://extensions` and refresh
open colonist.io tabs.

Automated live play: `CHROMIUM_PATH=/usr/bin/google-chrome-stable npm run
benchmark:colonist -- --difficulties Easy --games 1 --jobs 1`. On Chrome 148,
CDP may list the MV3 worker as `background.html` instead of `background.js`, so
the harness can fail at launch even when the extension is loaded; unit tests and
`npm run verify` remain the reliable gate in this environment.
