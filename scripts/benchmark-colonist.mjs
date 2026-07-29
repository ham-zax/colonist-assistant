#!/usr/bin/env node

import { spawn } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import process from "node:process";
import WebSocket from "ws";

const ROOT = resolve(import.meta.dirname, "..");
const DIST = resolve(ROOT, "dist");
const SETTINGS_KEY = "colonistAssistantSettings";
const DIFFICULTIES = new Set(["Easy", "Medium", "Hard"]);

const delay = (milliseconds) =>
  new Promise((resolvePromise) => setTimeout(resolvePromise, milliseconds));

function readOptions(argv) {
  const options = {
    difficulties: ["Easy", "Medium", "Hard"],
    games: 1,
    jobs: 1,
    staggerSeconds: 12,
    timeoutMinutes: 15,
    headed: true,
    chromium:
      process.env.CHROMIUM_PATH ?? "/run/current-system/sw/bin/chromium",
    output: resolve(ROOT, "benchmark-results", "colonist-latest"),
    keepProfiles: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const name = argv[index];
    const value = argv[index + 1];
    switch (name) {
      case "--difficulties":
        options.difficulties = value
          .split(",")
          .map((item) => item.trim())
          .filter(Boolean);
        index += 1;
        break;
      case "--games":
        options.games = Number(value);
        index += 1;
        break;
      case "--jobs":
        options.jobs = Number(value);
        index += 1;
        break;
      case "--timeout-minutes":
        options.timeoutMinutes = Number(value);
        index += 1;
        break;
      case "--stagger-seconds":
        options.staggerSeconds = Number(value);
        index += 1;
        break;
      case "--chromium":
        options.chromium = resolve(value);
        index += 1;
        break;
      case "--output":
        options.output = resolve(value);
        index += 1;
        break;
      case "--headed":
        options.headed = true;
        break;
      case "--headless":
        options.headed = false;
        break;
      case "--keep-profiles":
        options.keepProfiles = true;
        break;
      case "--help":
      case "-h":
        console.log(`Usage: npm run benchmark:colonist -- [options]

  --difficulties Easy,Medium,Hard  Colonist bot levels
  --games N                       Games per difficulty
  --jobs N                        Parallel Chromium profiles (default 1, max 4)
  --stagger-seconds N             Delay between parallel profile launches
  --timeout-minutes N             Per-game timeout
  --chromium PATH                 Chromium/Chrome executable
  --output PATH                   Output path without extension
  --headless                      Experimental; fresh profiles may be rejected
  --headed                        Show browser windows (default)
  --keep-profiles                 Preserve temporary profiles for debugging

Every run selects and verifies the normal four-player Base map. Weekly maps are
never accepted. Startup failures and timeouts are reported separately and are
not counted as losses.
`);
        process.exit(0);
      default:
        throw new Error(`Unknown option: ${name}`);
    }
  }
  if (
    options.difficulties.some((item) => !DIFFICULTIES.has(item)) ||
    !Number.isInteger(options.games) ||
    options.games < 1 ||
    !Number.isInteger(options.jobs) ||
    options.jobs < 1 ||
    options.jobs > 4 ||
    !Number.isFinite(options.staggerSeconds) ||
    options.staggerSeconds < 0 ||
    !Number.isFinite(options.timeoutMinutes) ||
    options.timeoutMinutes <= 0
  ) {
    throw new Error("Invalid difficulty, game count, job count, or timeout.");
  }
  return options;
}

class CdpClient {
  constructor(url) {
    this.nextId = 1;
    this.pending = new Map();
    this.events = [];
    this.socket = new WebSocket(url);
    this.ready = new Promise((resolvePromise, reject) => {
      this.socket.once("open", resolvePromise);
      this.socket.once("error", reject);
    });
    this.socket.on("message", (payload) => {
      const message = JSON.parse(payload.toString());
      if (message.id) {
        const pending = this.pending.get(message.id);
        if (!pending) return;
        this.pending.delete(message.id);
        if (message.error) pending.reject(new Error(message.error.message));
        else pending.resolve(message.result);
        return;
      }
      this.events.push(message);
      if (this.events.length > 500) this.events.shift();
    });
  }

  async command(method, params = {}) {
    await this.ready;
    const id = this.nextId++;
    return new Promise((resolvePromise, reject) => {
      this.pending.set(id, { resolve: resolvePromise, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  close() {
    this.socket.close();
  }
}

async function evaluate(client, expression) {
  const result = await client.command("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
    userGesture: true,
  });
  if (result.exceptionDetails) {
    throw new Error(
      result.exceptionDetails.exception?.description ??
        result.exceptionDetails.text,
    );
  }
  return result.result?.value;
}

async function trustedClick(client, finderExpression) {
  const point = await evaluate(
    client,
    `(() => {
      const element = ${finderExpression};
      if (!element) return undefined;
      element.scrollIntoView({ block: "center", inline: "center" });
      const rect = element.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) return undefined;
      return {
        x: rect.left + rect.width / 2,
        y: rect.top + rect.height / 2
      };
    })()`,
  );
  if (!point) return false;
  await client.command("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: point.x,
    y: point.y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await client.command("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: point.x,
    y: point.y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  return true;
}

async function removeTemporaryProfile(profile) {
  for (let attempt = 0; attempt < 5; attempt += 1) {
    try {
      await rm(profile, { recursive: true, force: true, maxRetries: 3 });
      return;
    } catch (error) {
      if (attempt === 4) {
        console.warn(
          `[Colonist] Could not fully remove temporary profile ${profile}: ${
            error instanceof Error ? error.message : String(error)
          }`,
        );
        return;
      }
      await delay(150 * (attempt + 1));
    }
  }
}

async function freePort() {
  const server = createServer();
  await new Promise((resolvePromise, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolvePromise);
  });
  const address = server.address();
  const port = typeof address === "object" && address ? address.port : 0;
  await new Promise((resolvePromise) => server.close(resolvePromise));
  return port;
}

async function waitForTargets(port, timeoutMs = 20_000) {
  const started = Date.now();
  let lastError;
  while (Date.now() - started < timeoutMs) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`);
      if (response.ok) return await response.json();
    } catch (error) {
      lastError = error;
    }
    await delay(200);
  }
  throw new Error(`Chromium CDP did not start: ${lastError ?? "timeout"}`);
}

async function waitUntil(probe, timeoutMs, intervalMs = 500) {
  const started = Date.now();
  let latest;
  while (Date.now() - started < timeoutMs) {
    latest = await probe();
    if (latest) return latest;
    await delay(intervalMs);
  }
  return undefined;
}

async function stopProcess(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill("SIGTERM");
  await Promise.race([
    new Promise((resolvePromise) => child.once("exit", resolvePromise)),
    delay(3_000),
  ]);
  if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
}

function scrapeExpression() {
  return `(() => {
    const text = document.body?.innerText ?? "";
    const rankMatch = text.match(/You are\\s+(1st|2nd|3rd|4th)!/i);
    let rank = rankMatch
      ? ({ "1st": 1, "2nd": 2, "3rd": 3, "4th": 4 })[rankMatch[1].toLowerCase()]
      : undefined;
    const endgameHeading = [...document.querySelectorAll("[class*=heading]")]
      .map((node) => node.textContent?.trim())
      .find((value) => /Victory|Defeat|Game Over|Well Played/i.test(value ?? ""));
    const overviewScores = [...document.querySelectorAll("[class^=row-]")]
      .map((row) => {
        const name = row.querySelector("[class^=name-]")?.textContent?.trim();
        const rawScore = row.querySelector("[class^=victoryPoint-]")?.textContent?.trim();
        const score = Number(rawScore);
        return name && Number.isFinite(score) ? { name, score } : undefined;
      })
      .filter(Boolean);
    const panelScores = [...document.querySelectorAll("[class*=playerInformation]")]
      .map((panel) => {
        const name = panel.querySelector("[class*=usernameLarge]")?.textContent?.trim();
        const rawScore = panel.querySelector("[class*=victoryPoints-]")?.textContent?.trim();
        const scoreParts = rawScore?.match(/\\d+/g);
        const score = Number(scoreParts?.at(-1));
        return name && Number.isFinite(score) ? { name, score } : undefined;
      })
      .filter(Boolean);
    const finalScores = [...new Map(
      (overviewScores.length ? overviewScores : panelScores)
        .map((player) => [player.name, player])
    ).values()];
    const myName =
      document.querySelector("[name=web-header-visitor-name]")?.textContent?.trim() ||
      document.querySelector("[name=web-header-username]")?.textContent?.trim();
    if (!rank && endgameHeading && myName) {
      const own = finalScores.find((player) => player.name === myName);
      if (own) rank = 1 + finalScores.filter((player) => player.score > own.score).length;
    }
    if (!rank && /Victory/i.test(endgameHeading ?? "")) rank = 1;
    const benchmark = window.__caBenchmark ?? { clicks: [] };
    const assistantRoot = document.querySelector("#colonist-assistant-root");
    const assistantText =
      assistantRoot?.shadowRoot?.textContent?.replace(/\\s+/g, " ").trim() ?? "";
    return {
      url: location.href,
      title: document.title,
      readyState: document.readyState,
      gameCanvas: Boolean(document.querySelector("#game-canvas")),
      assistant: Boolean(document.querySelector("#colonist-assistant-root")),
      assistantRuntime:
        /LOCAL FALLBACK/i.test(assistantText)
          ? "fallback"
          : /BACKGROUND WASM|WASM SEARCHING/i.test(assistantText)
            ? "wasm"
            : /CONNECTING/i.test(assistantText)
              ? "connecting"
              : "unknown",
      assistantText: assistantText.slice(0, 900),
      actionGuide: Boolean(document.querySelector("#colonist-assistant-action-guide")),
      rank,
      endgameHeading,
      finalScores,
      myName,
      gameTime: text.match(/Time:\\s*([^\\n-]+)\\s*-\\s*Turns:\\s*(\\d+)/i)?.slice(1),
      winnerText: text.match(/[^\\n]{0,80}won the game[^\\n]{0,80}/i)?.[0],
      clicks: benchmark.clicks?.length ?? 0,
      lastClick: benchmark.clicks?.at(-1),
      textTail: text.slice(-3000),
    };
  })()`;
}

async function saveScreenshot(client, path) {
  const screenshot = await client.command("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  await writeFile(path, Buffer.from(screenshot.data, "base64"));
}

async function runGame(task, options) {
  const started = Date.now();
  const port = await freePort();
  const profile = await mkdtemp(
    resolve(tmpdir(), "colonist-assistant-benchmark-"),
  );
  const stderr = [];
  const args = [
    "--no-sandbox",
    "--disable-dev-shm-usage",
    "--no-first-run",
    "--no-default-browser-check",
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${profile}`,
    `--disable-extensions-except=${DIST}`,
    `--load-extension=${DIST}`,
    "--window-size=1280,900",
    "--disable-features=Translate,OptimizationHints",
    "about:blank",
  ];
  if (!options.headed) args.unshift("--headless=new");
  const child = spawn(options.chromium, args, {
    stdio: ["ignore", "ignore", "pipe"],
  });
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk) => {
    stderr.push(...chunk.split("\n").filter(Boolean));
    if (stderr.length > 200) stderr.splice(0, stderr.length - 200);
  });

  let page;
  let worker;
  let phase = "launch";
  let lastState;
  let artifact;
  try {
    let targets = await waitForTargets(port);
    const pageTarget = targets.find((target) => target.type === "page");
    const workerTarget = await waitUntil(async () => {
      targets = await fetch(`http://127.0.0.1:${port}/json/list`).then(
        (response) => response.json(),
      );
      return targets.find(
        (target) =>
          target.type === "service_worker" &&
          target.url.startsWith("chrome-extension://") &&
          target.url.endsWith("/background.js"),
      );
    }, 20_000);
    if (!pageTarget?.webSocketDebuggerUrl || !workerTarget?.webSocketDebuggerUrl) {
      throw new Error("The unpacked extension did not expose its page and worker.");
    }
    page = new CdpClient(pageTarget.webSocketDebuggerUrl);
    await Promise.all([
      page.command("Page.enable"),
      page.command("Runtime.enable"),
    ]);

    phase = "configure-extension";
    const extensionId = new URL(workerTarget.url).hostname;
    await page.command("Page.navigate", {
      url: `chrome-extension://${extensionId}/popup.html`,
    });
    const popupReady = await waitUntil(
      () => evaluate(page, `document.readyState === "complete"`),
      10_000,
    );
    if (!popupReady) throw new Error("Extension settings page did not load.");
    const settings = await evaluate(
      page,
      `(async () => {
        const key = ${JSON.stringify(SETTINGS_KEY)};
        const stored = await chrome.storage.sync.get(key);
        const settings = {
          ...(stored[key] ?? {}),
          enabled: true,
          engine: "deep-search",
          highlightNextAction: true,
          autonomousPrivateGames: true
        };
        await chrome.storage.sync.set({ [key]: settings });
        return settings;
      })()`,
    );
    if (!settings?.autonomousPrivateGames || settings.engine !== "deep-search") {
      throw new Error("Autopilot settings did not persist.");
    }

    await page.command("Page.addScriptToEvaluateOnNewDocument", {
      source: `(() => {
        window.__caBenchmark = { clicks: [] };
        addEventListener("click", (event) => {
          const target = event.target;
          const label =
            target?.getAttribute?.("aria-label") ||
            target?.innerText ||
            target?.getAttribute?.("alt") ||
            target?.tagName ||
            "unknown";
          window.__caBenchmark.clicks.push({
            at: Date.now(),
            label: String(label).trim().slice(0, 120),
            trusted: event.isTrusted
          });
          if (window.__caBenchmark.clicks.length > 500) {
            window.__caBenchmark.clicks.shift();
          }
        }, true);
      })();`,
    });

    phase = "load-colonist";
    await page.command("Page.navigate", { url: "https://colonist.io/" });
    const homeReady = await waitUntil(
      async () =>
        evaluate(
          page,
          `document.readyState === "complete" &&
           Boolean(
             document.querySelector("#landingpage_cta_playonline") ||
             [...document.querySelectorAll("button, a, [role='button']")]
               .find((node) => node.textContent?.trim() === "Play Online")
           )`,
        ),
      60_000,
    );
    if (!homeReady) throw new Error("Colonist home page did not become ready.");

    const botLobbyReady = await waitUntil(
      async () => {
        const visible = await evaluate(
          page,
          `(() => {
            const card =
              [...document.querySelectorAll(".mm-mode-card")]
                .find((node) => node.querySelector(".mm-mode-card-title")?.textContent?.trim() === "Play vs. Bots") ||
              [...document.querySelectorAll("button, div, h1, h2, h3, span")]
                .find((node) => node.textContent?.trim() === "Play vs. Bots");
            if (!card) return false;
            const rect = card.getBoundingClientRect();
            return rect.width > 0 && rect.height > 0;
          })()`,
        );
        if (visible) return true;
        await trustedClick(
          page,
          `document.querySelector("#landingpage_cta_playonline") ||
           [...document.querySelectorAll("button, a, [role='button']")]
             .find((node) => node.textContent?.trim() === "Play Online")`,
        );
        return false;
      },
      60_000,
      1_000,
    );
    if (!botLobbyReady) throw new Error("Colonist bot lobby did not open.");

    phase = "select-base-game";
    const selectedBotsCard = await trustedClick(
      page,
      `[...document.querySelectorAll(".mm-mode-card")]
         .find((node) => node.querySelector(".mm-mode-card-title")?.textContent?.trim() === "Play vs. Bots") ||
       [...document.querySelectorAll("button, div, h1, h2, h3, span")]
         .find((node) => node.textContent?.trim() === "Play vs. Bots")`,
    );
    if (!selectedBotsCard) throw new Error("Play vs. Bots card was not clickable.");
    await delay(500);
    const selectedDifficulty = await trustedClick(
      page,
      `[...document.querySelectorAll("button")]
        .find((node) => node.textContent?.trim() === ${JSON.stringify(task.difficulty)})`,
    );
    if (!selectedDifficulty) {
      throw new Error(`${task.difficulty} difficulty was not clickable.`);
    }
    await delay(500);
    const baseReady = await waitUntil(
      async () => {
        const selected = await evaluate(
          page,
          `[...document.querySelectorAll(".item-scroll-cell.selected .item-scroll-cell-text")]
            .some((node) => node.textContent?.trim() === "Base")`,
        );
        if (selected) return true;
        await trustedClick(
          page,
          `[...document.querySelectorAll(".item-scroll-cell")]
            .find((node) => node.querySelector(".item-scroll-cell-text")?.textContent?.trim() === "Base")`,
        );
        return false;
      },
      10_000,
      500,
    );
    if (!baseReady) throw new Error("Exact Base map was not selectable.");
    const selection = await evaluate(
      page,
      `(() => {
        const difficulty = ${JSON.stringify(task.difficulty)};
        const selectedCard = [...document.querySelectorAll(".mm-mode-card")]
          .find((node) => node.querySelector(".mm-mode-card-title")?.textContent?.trim() === "Play vs. Bots");
        const selectedDifficulty = [...document.querySelectorAll("button")]
          .find((node) => node.textContent?.trim() === difficulty);
        const selectedMap = [...document.querySelectorAll(".item-scroll-cell.selected .item-scroll-cell-text")]
          .map((node) => node.textContent?.trim())
          .find((name) => name === "Base");
        return {
          card: selectedCard?.classList.contains("selected") ?? false,
          difficulty: selectedDifficulty?.getAttribute("aria-pressed") === "true",
          map: selectedMap
        };
      })()`,
    );
    if (
      selection?.error ||
      !selection?.card ||
      !selection?.difficulty ||
      selection?.map !== "Base"
    ) {
      throw new Error(`Exact Base-map selection failed: ${JSON.stringify(selection)}`);
    }

    phase = "start-game";
    const clickedStart = await trustedClick(
      page,
      `document.querySelector("#mm-mode-card-button")`,
    );
    if (!clickedStart) throw new Error("Start Game button was missing.");

    let guestDismissed = false;
    const startWaitBegan = Date.now();
    let lastStartAttempt = startWaitBegan;
    const gameStarted = await waitUntil(async () => {
      const hasGuest = await evaluate(
        page,
        `Boolean([...document.querySelectorAll("button, div")]
          .find((node) => node.textContent?.trim() === "Continue as Guest"))`,
      );
      if (hasGuest) {
        guestDismissed =
          (await trustedClick(
            page,
            `[...document.querySelectorAll("button, div")]
              .find((node) => node.textContent?.trim() === "Continue as Guest")`,
          )) || guestDismissed;
        await delay(500);
      }
      const state = await evaluate(page, scrapeExpression());
      lastState = state;
      if (
        !state.gameCanvas &&
        Date.now() - startWaitBegan > 5_000 &&
        Date.now() - lastStartAttempt > 5_000
      ) {
        await trustedClick(
          page,
          `document.querySelector("#mm-mode-card-button")`,
        );
        lastStartAttempt = Date.now();
      }
      return state.gameCanvas ? state : undefined;
    }, 90_000, 750);
    if (!gameStarted) throw new Error("The Base game did not start.");

    phase = "verify-extension";
    const extensionReady = await waitUntil(async () => {
      const state = await evaluate(page, scrapeExpression());
      lastState = state;
      return state.assistant ? state : undefined;
    }, 30_000);
    if (!extensionReady) {
      throw new Error("Colonist Assistant did not attach to the live board.");
    }
    const wasmReady = await waitUntil(async () => {
      const state = await evaluate(page, scrapeExpression());
      lastState = state;
      if (state.assistantRuntime === "fallback") {
        throw new Error(
          `Packaged WASM fell back before play: ${state.assistantText}`,
        );
      }
      return state.assistantRuntime === "wasm" ? state : undefined;
    }, 30_000);
    if (!wasmReady) {
      throw new Error(
        `Packaged WASM did not become authoritative: ${lastState?.assistantText ?? "no runtime text"}`,
      );
    }

    phase = "play-game";
    await evaluate(
      page,
      `(() => {
        if (window.__caBenchmark) window.__caBenchmark.clicks = [];
        return true;
      })()`,
    );
    const deadline = Date.now() + options.timeoutMinutes * 60_000;
    while (Date.now() < deadline) {
      lastState = await evaluate(page, scrapeExpression());
      if (lastState.rank) {
        return {
          status: "completed",
          difficulty: task.difficulty,
          game: task.game,
          rank: lastState.rank,
          won: lastState.rank === 1,
          durationMs: Date.now() - started,
          assistantAttached: lastState.assistant,
          assistantRuntime: lastState.assistantRuntime,
          automatedClicks: lastState.clicks,
          lastClick: lastState.lastClick,
          winnerText: lastState.winnerText,
          finalScores: lastState.finalScores,
          gameTime: lastState.gameTime,
        };
      }
      await delay(1_000);
    }
    throw new Error(`Game exceeded ${options.timeoutMinutes} minutes.`);
  } catch (error) {
    phase = `failed:${phase}`;
    if (page) {
      try {
        await mkdir(`${options.output}-artifacts`, { recursive: true });
        artifact = resolve(
          `${options.output}-artifacts`,
          `${task.difficulty.toLowerCase()}-${task.game}.png`,
        );
        await saveScreenshot(page, artifact);
      } catch {
        artifact = undefined;
      }
    }
    return {
      status: "failed",
      difficulty: task.difficulty,
      game: task.game,
      phase,
      error: error instanceof Error ? error.message : String(error),
      durationMs: Date.now() - started,
      lastState,
      artifact,
      chromiumStderrTail: stderr.slice(-30),
    };
  } finally {
    page?.close();
    worker?.close();
    await stopProcess(child);
    if (
      !options.keepProfiles &&
      profile.startsWith(resolve(tmpdir(), "colonist-assistant-benchmark-"))
    ) {
      await removeTemporaryProfile(profile);
    }
  }
}

function percent(value) {
  return `${(value * 100).toFixed(1)}%`;
}

function markdown(report) {
  const rows = report.byDifficulty
    .map(
      (item) =>
        `| ${item.difficulty} | ${item.completed} | ${item.wins} | ${item.completed ? percent(item.winRate) : "n/a"} | ${item.failures} |`,
    )
    .join("\n");
  return `# Colonist.io Base-map autopilot benchmark

Generated: ${report.generatedAt}

Each completed game ran in an isolated Chromium profile against three
Colonist.io bots. The harness verified the exact \`Base\` map before starting,
enabled Belief PUCT and autopilot through extension storage, and waited for
Colonist's final rank. Infrastructure failures and timeouts are excluded from
the win-rate denominator and listed in the JSON report.

| Difficulty | Completed | Wins | Win rate | Harness failures |
|---|---:|---:|---:|---:|
${rows}

Completed games: ${report.completedGames}/${report.requestedGames}. Overall
win rate: ${report.completedGames ? percent(report.wins / report.completedGames) : "n/a"}.
`;
}

async function workerLoop(queue, results, options) {
  while (queue.length) {
    const task = queue.shift();
    console.error(
      `[Colonist] ${task.difficulty} game ${task.game}: launching isolated browser…`,
    );
    const result = await runGame(task, options);
    results.push(result);
    console.error(
      result.status === "completed"
        ? `  completed: rank ${result.rank}, ${result.won ? "WIN" : "loss"}, ${Math.round(result.durationMs / 1000)}s`
        : `  failed at ${result.phase}: ${result.error}`,
    );
  }
}

const options = readOptions(process.argv.slice(2));
const manifest = JSON.parse(
  await readFile(resolve(DIST, "manifest.json"), "utf8"),
);
if (manifest.name !== "Colonist Assistant") {
  throw new Error(
    "dist/ is missing or stale. Run npm run build before the live benchmark.",
  );
}
const tasks = options.difficulties.flatMap((difficulty) =>
  Array.from({ length: options.games }, (_, index) => ({
    difficulty,
    game: index + 1,
  })),
);
const queue = [...tasks];
const results = [];
await Promise.all(
  Array.from(
    { length: Math.min(options.jobs, tasks.length) },
    (_, workerIndex) =>
      (async () => {
        if (workerIndex > 0 && options.staggerSeconds > 0) {
          await delay(workerIndex * options.staggerSeconds * 1_000);
        }
        await workerLoop(queue, results, options);
      })(),
  ),
);
results.sort(
  (left, right) =>
    options.difficulties.indexOf(left.difficulty) -
      options.difficulties.indexOf(right.difficulty) ||
    left.game - right.game,
);
const byDifficulty = options.difficulties.map((difficulty) => {
  const matching = results.filter((result) => result.difficulty === difficulty);
  const completed = matching.filter((result) => result.status === "completed");
  const wins = completed.filter((result) => result.won).length;
  return {
    difficulty,
    requested: matching.length,
    completed: completed.length,
    wins,
    losses: completed.length - wins,
    failures: matching.length - completed.length,
    winRate: completed.length ? wins / completed.length : null,
  };
});
const completed = results.filter((result) => result.status === "completed");
const report = {
  schemaVersion: 1,
  kind: "colonist-live-base-autopilot",
  generatedAt: new Date().toISOString(),
  extensionVersion: manifest.version,
  configuration: {
    difficulties: options.difficulties,
    gamesPerDifficulty: options.games,
    jobs: options.jobs,
    staggerSeconds: options.staggerSeconds,
    timeoutMinutes: options.timeoutMinutes,
    headless: !options.headed,
    exactMap: "Base",
    players: 4,
  },
  requestedGames: tasks.length,
  completedGames: completed.length,
  failures: results.length - completed.length,
  wins: completed.filter((result) => result.won).length,
  byDifficulty,
  results,
};
await mkdir(dirname(options.output), { recursive: true });
await Promise.all([
  writeFile(`${options.output}.json`, `${JSON.stringify(report, null, 2)}\n`),
  writeFile(`${options.output}.md`, markdown(report)),
]);
console.log(
  JSON.stringify(
    {
      json: `${options.output}.json`,
      markdown: `${options.output}.md`,
      completedGames: report.completedGames,
      wins: report.wins,
      failures: report.failures,
    },
    null,
    2,
  ),
);
