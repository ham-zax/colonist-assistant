#!/usr/bin/env node

import { spawn } from "node:child_process";
import { access, mkdir, writeFile } from "node:fs/promises";
import { cpus } from "node:os";
import { dirname, resolve } from "node:path";
import process from "node:process";

const ROOT = resolve(import.meta.dirname, "..");
const ENGINE_DIR = resolve(ROOT, "engine");
const ARENA = resolve(ENGINE_DIR, "target/release/colonist-arena");
const KNOWN_ENGINES = new Set([
  "random",
  "weighted",
  "maxn",
  "alphabeta",
  "uct",
  "puct",
]);

function parseCsv(value) {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function readOptions(argv) {
  const options = {
    candidate: "puct",
    baselines: ["random", "weighted", "maxn", "alphabeta", "uct"],
    players: [2, 3, 4],
    games: 200,
    iterations: 80,
    rolloutActions: 140,
    maxTurns: 600,
    threads: Math.min(cpus().length, 4),
    seed: 91_000_001,
    validate: true,
    build: true,
    output: resolve(ROOT, "benchmark-results", "local-latest"),
  };
  for (let index = 0; index < argv.length; index += 1) {
    const name = argv[index];
    const value = argv[index + 1];
    switch (name) {
      case "--candidate":
        options.candidate = value;
        index += 1;
        break;
      case "--baselines":
        options.baselines = parseCsv(value);
        index += 1;
        break;
      case "--players":
        options.players = parseCsv(value).map(Number);
        index += 1;
        break;
      case "--games":
        options.games = Number(value);
        index += 1;
        break;
      case "--iterations":
        options.iterations = Number(value);
        index += 1;
        break;
      case "--rollout-actions":
        options.rolloutActions = Number(value);
        index += 1;
        break;
      case "--max-turns":
        options.maxTurns = Number(value);
        index += 1;
        break;
      case "--threads":
        options.threads = Number(value);
        index += 1;
        break;
      case "--seed":
        options.seed = Number(value);
        index += 1;
        break;
      case "--output":
        options.output = resolve(value);
        index += 1;
        break;
      case "--no-build":
        options.build = false;
        break;
      case "--no-validate":
        options.validate = false;
        break;
      case "--validate":
        options.validate = true;
        break;
      case "--quick":
        options.games = 24;
        options.baselines = ["weighted", "alphabeta"];
        break;
      case "--help":
      case "-h":
        console.log(`Usage: npm run benchmark:local -- [options]

  --candidate ENGINE       Engine under test (default: puct)
  --baselines A,B,C        Opponent engines
  --players 2,3,4          Player counts
  --games N                Minimum games per matchup; rounded for seat rotation
  --threads N              Native arena worker threads
  --iterations N           MCTS iteration budget
  --rollout-actions N      MCTS rollout action budget
  --max-turns N            Cutoff (reported, never hidden)
  --seed N                 First deterministic seed
  --output PATH            Output path without extension
  --no-validate            Skip per-transition invariant validation
  --no-build               Reuse the current release binary
  --quick                  24 games versus weighted and alpha-beta
`);
        process.exit(0);
      default:
        throw new Error(`Unknown option: ${name}`);
    }
  }
  if (!KNOWN_ENGINES.has(options.candidate)) {
    throw new Error(`Unknown candidate engine: ${options.candidate}`);
  }
  for (const baseline of options.baselines) {
    if (!KNOWN_ENGINES.has(baseline)) {
      throw new Error(`Unknown baseline engine: ${baseline}`);
    }
  }
  if (
    options.players.some((players) => ![2, 3, 4].includes(players)) ||
    !Number.isInteger(options.games) ||
    options.games < 1 ||
    !Number.isInteger(options.threads) ||
    options.threads < 1
  ) {
    throw new Error("Players, games, or threads are outside the supported range.");
  }
  return options;
}

function run(command, args, { cwd, capture = false } = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, {
      cwd,
      stdio: capture ? ["ignore", "pipe", "inherit"] : "inherit",
    });
    let stdout = "";
    if (capture) {
      child.stdout.setEncoding("utf8");
      child.stdout.on("data", (chunk) => {
        stdout += chunk;
      });
    }
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (code === 0) resolvePromise(stdout);
      else {
        reject(
          new Error(
            `${command} exited with ${signal ? `signal ${signal}` : `code ${code}`}`,
          ),
        );
      }
    });
  });
}

async function cargoCommand() {
  const preferred = resolve(
    process.env.CARGO_HOME ?? resolve(process.env.HOME ?? "", ".cargo"),
    "bin",
    "cargo",
  );
  try {
    await access(preferred);
    return preferred;
  } catch {
    return "cargo";
  }
}

function percent(value, digits = 1) {
  return `${(value * 100).toFixed(digits)}%`;
}

function markdown(report) {
  const rows = report.results
    .map(
      (result) =>
        `| ${result.players} | ${result.baseline} | ${result.games.toLocaleString()} | ${result.candidateWins.toLocaleString()} | ${percent(result.winShare)} | ${percent(result.blockedCi95.lower)}–${percent(result.blockedCi95.upper)} | ${percent(result.fairShareDelta)} | ${result.cutoffs} |`,
    )
    .join("\n");
  return `# Colonist Assistant native arena benchmark

Generated: ${report.generatedAt}

Candidate: \`${report.configuration.candidate}\`. Each board block rotates the
candidate through every seat while preserving the board and chance seed.
Confidence intervals are block bootstraps. Arena games expose complete
simulator state, so these results measure policy/search strength rather than
live DOM extraction or hidden-card inference.

| Players | Opponents | Games | Wins | Win share | 95% CI | Above fair share | Cutoffs |
|---:|---|---:|---:|---:|---:|---:|---:|
${rows}

Total games: ${report.totalGames.toLocaleString()}.
`;
}

const options = readOptions(process.argv.slice(2));
if (options.build) {
  console.error("Building the optimized native arena…");
  await run(
    await cargoCommand(),
    ["build", "--release", "-p", "colonist-catan-arena"],
    { cwd: ENGINE_DIR },
  );
}

const results = [];
const matchupCount = options.players.length * options.baselines.length;
let matchupIndex = 0;
for (const players of options.players) {
  for (const [baselineIndex, baseline] of options.baselines.entries()) {
    matchupIndex += 1;
    const blocks = Math.ceil(options.games / players);
    const seed =
      options.seed + players * 1_000_000 + baselineIndex * 100_000;
    console.error(
      `[${matchupIndex}/${matchupCount}] ${options.candidate} vs ${baseline}, ${players} players, ${blocks * players} games…`,
    );
    const args = [
      "--players",
      String(players),
      "--blocks",
      String(blocks),
      "--seed",
      String(seed),
      "--candidate",
      options.candidate,
      "--baseline",
      baseline,
      "--iterations",
      String(options.iterations),
      "--rollout-actions",
      String(options.rolloutActions),
      "--max-turns",
      String(options.maxTurns),
      "--threads",
      String(options.threads),
      "--json",
    ];
    if (options.validate) args.push("--validate");
    const output = await run(ARENA, args, {
      cwd: ENGINE_DIR,
      capture: true,
    });
    const result = JSON.parse(output.trim());
    results.push(result);
    console.error(
      `  ${result.candidateWins}/${result.games} = ${percent(result.winShare)} (${percent(result.blockedCi95.lower)}–${percent(result.blockedCi95.upper)})`,
    );
  }
}

const report = {
  schemaVersion: 1,
  kind: "colonist-assistant-native-arena-matrix",
  generatedAt: new Date().toISOString(),
  configuration: options,
  totalGames: results.reduce((sum, result) => sum + result.games, 0),
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
      totalGames: report.totalGames,
    },
    null,
    2,
  ),
);
