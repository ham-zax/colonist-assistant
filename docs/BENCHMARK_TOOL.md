# Benchmark tool

The project has three deliberately separate benchmark layers. Keeping them
separate prevents simulator strength, browser reliability, and unrelated
upstream bot results from being blended into one misleading win rate.

## 1. Native Colonist Assistant arena

This is the reproducible policy benchmark. It runs the same clean-room rules
and search crates used by the packaged WASM engine, with complete simulator
state. Every board block rotates the candidate through every seat and reuses
the board/chance seed. The report includes blocked-bootstrap confidence
intervals, cutoffs, throughput, and invariant failures.

```bash
npm run benchmark:local -- \
  --players 2,3,4 \
  --baselines random,weighted,alphabeta,uct,puct \
  --games 1000 \
  --threads 16 \
  --output benchmark-results/local-main
```

`--games` is rounded up to a multiple of the player count so that seat rotation
is complete. JSON and Markdown reports are written at the output path.

## 2. Live Colonist.io Base-map autopilot

This is the end-to-end test: current Colonist DOM, public-state extraction,
background WASM, decision translation, and actual autopilot clicks. Each game
uses an isolated browser profile. The harness selects `Play vs. Bots`, verifies
the exact map text `Base`, enables Deep MaxN/autopilot through extension
storage, and parses the final Colonist score table.

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
- Never hide cutoffs or failed browser sessions in the denominator.
