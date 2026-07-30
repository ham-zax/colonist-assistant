# Benchmark tool

The project has three deliberately separate benchmark layers. Keeping them
separate prevents simulator strength, browser reliability, and unrelated
upstream bot results from being blended into one misleading win rate.

## 1. Native Colonist Assistant arena

This is the deterministic native policy benchmark for a fixed source build and
configuration. It runs the same clean-room rules and search crates used by the
packaged WASM engine. Every board block rotates the candidate through every
seat and reuses the board/chance seed.

By default, MaxN, experimental belief PUCT, AlphaBeta, and UCT receive the same
synthetic observer-consistent weighted belief particles.
`--perfect-information` is a separate oracle mode; never combine its results
with weighted-belief runs. `--validate` checks every transition and aborts on
the first invariant failure. The final report includes terminal games, cutoffs,
throughput, behavioral metrics, and a matched-block bootstrap interval.

The packaged live authority is branded Strategist and uses weighted-belief Deep
MaxN. In the arena, `maxn` (also accepted as `deep`) is the closest comparison
to that search core. `puct` selects the experimental belief-PUCT policy, with
`strategist` retained only as a compatibility alias. That compatibility label
does not make arena PUCT the live engine, and its results must remain
explicitly experimental.

```bash
npm run benchmark:local -- \
  --candidate maxn \
  --players 3,4 \
  --baselines weighted,alphabeta,uct,puct \
  --games 4 \
  --threads 4 \
  --output benchmark-results/local-main
```

`--games` is rounded up to a multiple of the player count so that seat rotation
is complete. JSON and Markdown reports are written at the output path.
Each matchup also writes a JSONL checkpoint under `<output>.checkpoints/`.
The file is created at the start of that invocation, then receives a cumulative
snapshot that is flushed after every completed game. Interrupting the runner
therefore preserves that invocation's latest standings, but starting it again
does not resume or append to the previous file.

Within arena reports, `puct` is the canonical serialization name for the
experimental PUCT comparator. The native arena accepts `strategist` as its
compatibility alias and `deep` as an alias for MaxN. The matrix runner rejects
aliases that would create a meaningless self-match. Do not describe this arena
naming convention as production authority.

For a heterogeneous four-engine field, invoke the arena directly:

```bash
mkdir -p benchmark-results
cd engine
cargo run --release -p colonist-catan-arena -- \
  --players 4 --blocks 1 --threads 4 --validate --json \
  --lineup maxn,weighted,puct,alphabeta \
  --iterations 112 --rollout-actions 72 --belief-particles 32 \
  --seed 9200001 \
  --checkpoint-output ../benchmark-results/four-engine.jsonl
```

The checkpoint is JSON Lines. Every line contains the cumulative completed and
terminal game counts, scheduled game count, fully matched block count, cutoffs,
information mode, per-engine wins, win share, seat samples, average rank,
average victory points, and the latest raw game result. A cutoff's raw
`lastGame.winner` is a non-terminal evaluator adjudication; it is not included
in the arena's win rate.

Read a checkpoint during a run or after an interruption with:

```bash
npm run benchmark:checkpoint -- benchmark-results/four-engine.jsonl
```

The reader ignores an incomplete trailing line and prints the latest valid
snapshot as a standings table. Partial matchup results can have incomplete seat
rotations; use `completedMatchedBlocks` and per-engine `seatSamples` to judge
balance. Add `--split-lineup` to report duplicate engines such as
`weighted #1` and `weighted #2` as distinct lineup participants.

Combining checkpoint paths is safe only after manually confirming identical
player count, canonical lineup, candidate/baseline, search budgets, information
mode, validation mode, and non-overlapping seeds. The current reader validates
only the lineup. Its combined and `--split-lineup` win rates also do not
correctly censor cutoff adjudications, so use those views only for cutoff-free
runs and retain the raw checkpoints.

## 2. Live Colonist.io Base-map autopilot

This is the end-to-end test: current Colonist DOM, public-state extraction,
background WASM, decision translation, and actual autopilot clicks. Each game
uses an isolated browser profile. The harness selects `Play vs. Bots`, verifies
the exact map text `Base`, enables the single Strategist UI authority
(`deep-search`) and autopilot through extension storage, and parses the final
Colonist score table. The internal live WASM request is `maxn`; the
`deep-search` extension identifier is a UI/service identifier, not the arena's
legacy `deep` alias.

```bash
npm run build
npm run benchmark:colonist -- \
  --difficulties Easy,Medium,Hard \
  --games 3 \
  --jobs 2 \
  --timeout-minutes 15 \
  --output benchmark-results/colonist-live
```

Headed Chromium is the default. `--headless` is available, but Colonist's
invisible Cloudflare/guest-session gate can silently reject Start Game in a
fresh headless profile. A rejected start, timeout, or DOM mismatch is reported
as a harness failure and is never counted as a loss. `--jobs` is capped at four
to avoid opening an unreasonable number of live sessions. Parallel launches
are staggered by 12 seconds by default; adjust with `--stagger-seconds`.

For release screenshots, run exactly one headed game and provide a temporary
capture directory:

```bash
npm run benchmark:colonist -- \
  --difficulties Easy --games 1 --jobs 1 \
  --store-screenshots /tmp/colonist-assistant-store-screenshots \
  --output /tmp/colonist-assistant-store-smoke
```

The capture mode fixes the page surface at 1280 by 800 and briefly pauses
Autopilot to record the packaged settings, a meaningful live recommendation,
table-card beliefs, and an actual discard state before any discard click. It
then resumes the game and still requires a normal terminal result. Treat the
result as an integration and release-artifact check, not a strength estimate.
Visually inspect every PNG before replacing `assets/screenshots/`.

Use this only with Colonist bot games or explicitly consensual private games.
It is not a ranked/tournament load-testing tool.

## 3. Pinned upstream Catanatron reference

This runner executes an unmodified Catanatron checkout. It is useful for
checking published algorithm families and simulator behavior.

```bash
python3 -m venv /tmp/catanatron-bench-venv
/tmp/catanatron-bench-venv/bin/pip install /path/to/catanatron
/tmp/catanatron-bench-venv/bin/python scripts/benchmark-catanatron.py \
  --source /path/to/catanatron \
  --players 2,3,4 \
  --games 100 \
  --jobs 8 \
  --candidate AB:2 \
  --baselines R,W,F \
  --output benchmark-results/catanatron-reference.json
```

These are upstream Catanatron results, not Colonist Assistant results.
Catanatron and the extension do not share state/action serialization, trade
semantics, or hidden-information models. Direct cross-engine play would
require a complete, tested rules adapter. The report embeds the upstream Git
revision and carries a warning so reference results cannot accidentally be
relabelled as extension results.

Monte Catano has the same integration boundary: its UCI-like protocol accepts
Monte Catano states/actions, not Colonist Assistant states/actions. Its own
SPRT/self-play tools can be run as upstream reference evidence, but a claimed
head-to-head result would be invalid until a bidirectional legality-tested
adapter exists.

## Reading results

- Compare win share to `1 / players`, not to 50% in multiplayer.
- Treat intervals that overlap fair share as inconclusive.
- Do not treat random or weighted opponents as skilled-human proxies.
- Live results include DOM/inference/autopilot failures; native results do not.
- External-reference results compare upstream bots only.
- Native win share uses terminal games; always report scheduled games, terminal
  games, and cutoffs together.
- Live win share uses completed games; always report harness failures
  separately.
- One matched block is an engineering smoke, not a useful confidence interval
  or strength estimate.
- Arena `puct`/`strategist` results are experimental belief-PUCT results;
  `maxn`/`deep` results exercise the live search family. Native results are
  still not packaged end-to-end Strategist results.
- The packaged cold-adapter regression must remain below one second. The normal
  request uses depth 4, branch cap 16, at most 4,000 strategic nodes, and a
  cooperative 350 ms strategic-search deadline. Mandatory and exact tactical
  work is bounded separately, so this deadline is not a promise that every
  complete request takes 350 ms.
