# Browser threading benchmark

This disposable MV3 extension benchmarks the current generated portable and
threaded modules through dedicated workers in a real offscreen document. It has
no host permissions. Requests come from an external JSON fixture file; keep them
and the generated extension outside the repository. Run one heavy build or
benchmark at a time.

Prepare after the normal WASM build:

```bash
node scripts/prepare-threading-browser.mjs /absolute/path/requests.json /tmp/colonist-threading-benchmark
```

The script prints the extension directory, ID, and benchmark URL. Fixture shape
is `[{"id":"fixture#s0","request":{...}}]`, matching the native benchmark. It
does not launch a browser or alter browser configuration.

Use the installed `browser` skill to load this unpacked extension in an isolated
profile. For Linux Clearcote, define a temporary named profile with this extension
in `~/.config/mcp-dev-bridge/browser-fast.json`, preserve the original bytes, and
use the same target/backend/profile on every call. For example, substituting the
printed URL and your isolated profile name:

```bash
wh-browser fast observe '{"browser_target":"linux","browser_backend":"clearcote","browser_profile":"colonist-threading-benchmark","scope":"compact"}'
wh-browser devtools new_page '{"browser_target":"linux","browser_backend":"clearcote","browser_profile":"colonist-threading-benchmark","url":"chrome-extension://PRINTED_ID/benchmark.html"}'
```

Use the page ID returned for your new page in these calls. Extension targets can
be omitted from `list_pages`, so retain the ID returned by `new_page`.

Start fixed work (20 fixtures if the input contains 20), depth 2 with original
node allowances, no clock budget or evidence escalation:

```bash
wh-browser devtools evaluate_script '{"browser_target":"linux","browser_backend":"clearcote","browser_profile":"colonist-threading-benchmark","pageId":2,"function":"async () => await startBenchmark({mode: \"fixed\", threads: [0,1,2,4,8], reps: 3})"}'
```

Zero means the portable module; positive counts initialize an actual Rayon pool.
Every configuration warms up with a depth-2 fixed request before recording runs.
Read progress with `() => ({progress:benchmark.progress,error:benchmark.error})`.
Wait until `progress` is `done`, then save raw output:

```bash
wh-browser devtools evaluate_script '{"browser_target":"linux","browser_backend":"clearcote","browser_profile":"colonist-threading-benchmark","pageId":2,"function":"() => benchmark","filePath":"/tmp/colonist-browser-fixed.json"}'
node scripts/prepare-threading-browser.mjs --extract /tmp/colonist-browser-fixed.json /tmp/colonist-browser-results
node scripts/compare-threading.mjs /tmp/colonist-browser-results/fixed-threads-0.jsonl /tmp/colonist-browser-results/fixed-threads-1.jsonl /tmp/colonist-browser-results/fixed-threads-2.jsonl /tmp/colonist-browser-results/fixed-threads-4.jsonl /tmp/colonist-browser-results/fixed-threads-8.jsonl
```

Extracted files use the native harness schema: request rows contain `rep`, `id`,
`ms`, `sig`, and `stages`; each repetition adds a `rep`/`sumMs` row. The comparer
prints `compared`, `mismatch`, `extra`, `duplicateIds`, and `minMs` per candidate.
Compare browser portable against browser threaded, and native sequential against
native parallel. Cross-runtime JSON float formatting can differ even when the
underlying f32 agrees.

After saving fixed results, start timed replay. `live-current` uses the current
four-player 10,000 ms / 48,000 nodes-per-wave configuration; other effort settings
stay as supplied. Without `ids`, it selects four-player fixtures ending in `#s0`.
For two representative fixtures:

```bash
wh-browser devtools evaluate_script '{"browser_target":"linux","browser_backend":"clearcote","browser_profile":"colonist-threading-benchmark","pageId":2,"function":"async () => await startBenchmark({mode: \"live-current\", threads: [0,1,2,4,8], reps: 1, ids: [\"recovered-turn-54#s0\",\"hidden-dev-seed-stability#s0\"]})"}'
```

Save and extract as above. `live-original` retains the supplied fixture budgets
and includes all fixtures unless `ids` selects a subset. Timed outcomes need not
match because different completed depths change the decision. Report completed
depth, nodes, stages, elapsed time, and the exact budget; use fixed work for parity.

Each run records offscreen isolation and engine-worker isolation, actual spawned
Rayon worker count, and whether exported WASM memory uses SharedArrayBuffer.
To verify the actual offscreen context, evaluate:

```js
async () => await chrome.runtime.getContexts({contextTypes: ["OFFSCREEN_DOCUMENT"]})
```

After benchmarking, close the offscreen document (`chrome.offscreen.closeDocument()`),
close only task-owned browser pages, restore the browser configuration byte-for-byte,
and stop task-owned browser daemons/profiles through the browser skill.
