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
  "strategist",
  "deep",
  "puct",
]);
const canonicalEngine = (engine) =>
  engine === "strategist"
    ? "puct"
    : engine === "deep"
      ? "maxn"
      : engine;

function parseCsv(value) {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function readOptions(argv) {
  const options = {
    candidate: "maxn",
    baselines: ["random", "weighted", "alphabeta", "uct", "puct"],
    players: [2, 3, 4],
    games: 200,
    iterations: 80,
    rolloutActions: 140,
    maxTurns: 600,
    maxnDepth: 4,
    maxnBranch: 8,
    maxnNodes: 4_000,
    maxnTimeMs: 350,
    beliefParticles: 24,
    strategicParticles: 12,
    threads: Math.min(cpus().length, 4),
    seed: 91_000_001,
    validate: true,
    build: true,
    challengeOutputDirectory: null,
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
      case "--maxn-depth":
        options.maxnDepth = Number(value);
        index += 1;
        break;
      case "--maxn-branch":
        options.maxnBranch = Number(value);
        index += 1;
        break;
      case "--maxn-nodes":
        options.maxnNodes = Number(value);
        index += 1;
        break;
      case "--maxn-time-ms":
        options.maxnTimeMs = Number(value);
        index += 1;
        break;
      case "--belief-particles":
        options.beliefParticles = Number(value);
        index += 1;
        break;
      case "--strategic-particles":
        options.strategicParticles = Number(value);
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
      case "--challenge-output-directory":
        options.challengeOutputDirectory = resolve(value);
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

  --candidate ENGINE       Engine under test (default: maxn)
  --baselines A,B,C        Opponent engines
  --players 2,3,4          Player counts
  --games N                Minimum games per matchup; rounded for seat rotation
  --threads N              Native arena worker threads
  --iterations N           MCTS iteration budget
  --rollout-actions N      MCTS rollout action budget
  --max-turns N            Cutoff (reported, never hidden)
  --maxn-depth N           MaxN depth (default: 4)
  --maxn-branch N          MaxN branch cap (default: 8)
  --maxn-nodes N           MaxN node budget (default: 4000)
  --maxn-time-ms N         MaxN deadline in ms (default: 350)
  --belief-particles N     Arena posterior particles (default: 24)
  --strategic-particles N  Rust strategic particle cap (default: 12)
  --seed N                 First deterministic seed
  --output PATH            Output path without extension
                           Checkpoints go to <output>.checkpoints/
  --challenge-output-directory PATH
                           Persist eligible PreRoll takeover snapshots by matchup
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
  options.candidate = canonicalEngine(options.candidate);
  options.baselines = [
    ...new Set(options.baselines.map(canonicalEngine)),
  ];
  if (options.baselines.includes(options.candidate)) {
    throw new Error(
      "Candidate and baseline resolve to the same engine; self-matches are not strength evidence.",
    );
  }
  if (
    options.players.some((players) => ![2, 3, 4].includes(players)) ||
    !Number.isInteger(options.games) ||
    options.games < 1 ||
    !Number.isInteger(options.threads) ||
    options.threads < 1 ||
    !Number.isInteger(options.maxnDepth) ||
    options.maxnDepth < 1 ||
    !Number.isInteger(options.maxnBranch) ||
    options.maxnBranch < 1 ||
    !Number.isInteger(options.maxnNodes) ||
    options.maxnNodes < 1 ||
    !Number.isInteger(options.maxnTimeMs) ||
    options.maxnTimeMs < 0 ||
    !Number.isInteger(options.beliefParticles) ||
    options.beliefParticles < 1 ||
    !Number.isInteger(options.strategicParticles) ||
    options.strategicParticles < 1
  ) {
    throw new Error("Benchmark numeric options are outside the supported range.");
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
        `| ${result.players} | ${result.baseline} | ${result.games.toLocaleString()} | ${(result.terminalGames ?? result.games - result.cutoffs).toLocaleString()} | ${result.candidateWins.toLocaleString()} | ${percent(result.winShare)} | ${percent(result.blockedCi95.lower)}–${percent(result.blockedCi95.upper)} | ${percent(result.fairShareDelta)} | ${result.cutoffs} |`,
    )
    .join("\n");
  return `# Colonist Assistant native arena benchmark

Generated: ${report.generatedAt}

Build: \`${report.buildIdentity.buildGitSha}${report.buildIdentity.buildDirty ? "+dirty" : ""}\`

Engine: \`${report.buildIdentity.engineRevision}\`

Production profile: depth ${report.liveProductionProfile.maxnDepth}, branch ${report.liveProductionProfile.maxnBranch}, ${report.liveProductionProfile.maxnNodes.toLocaleString()} nodes, ${report.liveProductionProfile.maxnTimeMs} ms, ${report.liveProductionProfile.beliefParticles} belief / ${report.liveProductionProfile.strategicParticles} strategic particles, weighted-belief information.

Candidate: \`${report.configuration.candidate}\`. Each board block rotates the
candidate through every seat while preserving the board and chance seed.
Confidence intervals are block bootstraps. Strategic engines receive the same
deterministic weighted belief particles unless an arena run is explicitly
labelled perfect-information, so these results do not grant one candidate
hidden-card access that another candidate lacks.

| Players | Opponents | Games | Terminal | Wins | Win share | 95% CI | Above fair share | Cutoffs |
|---:|---|---:|---:|---:|---:|---:|---:|---:|
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
const challengeOutputs = [];
const checkpointDirectory = `${options.output}.checkpoints`;
await mkdir(checkpointDirectory, { recursive: true });
if (options.challengeOutputDirectory) {
  await mkdir(options.challengeOutputDirectory, { recursive: true });
}
const matchupCount = options.players.length * options.baselines.length;
let matchupIndex = 0;
for (const players of options.players) {
  for (const [baselineIndex, baseline] of options.baselines.entries()) {
    matchupIndex += 1;
    const blocks = Math.ceil(options.games / players);
    const seed =
      options.seed + players * 1_000_000 + baselineIndex * 100_000;
    const checkpointOutput = resolve(
      checkpointDirectory,
      `${players}p-${options.candidate}-vs-${baseline}.jsonl`,
    );
    console.error(
      `[${matchupIndex}/${matchupCount}] ${options.candidate} vs ${baseline}, ${players} players, ${blocks * players} games…`,
    );
    console.error(`  checkpoint: ${checkpointOutput}`);
    const challengeOutput = options.challengeOutputDirectory
      ? resolve(
          options.challengeOutputDirectory,
          `${players}p-${options.candidate}-vs-${baseline}.jsonl`,
        )
      : null;
    if (challengeOutput) {
      challengeOutputs.push(challengeOutput);
      console.error(`  challenges: ${challengeOutput}`);
    }
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
      "--maxn-depth",
      String(options.maxnDepth),
      "--maxn-branch",
      String(options.maxnBranch),
      "--maxn-nodes",
      String(options.maxnNodes),
      "--maxn-time-ms",
      String(options.maxnTimeMs),
      "--belief-particles",
      String(options.beliefParticles),
      "--strategic-particles",
      String(options.strategicParticles),
      "--threads",
      String(options.threads),
      "--checkpoint-output",
      checkpointOutput,
      "--json",
    ];
    if (challengeOutput) {
      args.push("--challenge-output", challengeOutput);
    }
    if (options.validate) args.push("--validate");
    const output = await run(ARENA, args, {
      cwd: ENGINE_DIR,
      capture: true,
    });
    const result = JSON.parse(output.trim());
    results.push(result);
    console.error(
      `  ${result.candidateWins}/${result.terminalGames ?? result.games - result.cutoffs} terminal = ${percent(result.winShare)} (${percent(result.blockedCi95.lower)}–${percent(result.blockedCi95.upper)}), ${result.cutoffs} cutoffs`,
    );
  }
}

const sourceIdentities = [
  ...new Map(
    results.map((result) => [
      `${result.buildGitSha}:${result.buildDirty ?? "unknown"}:${result.engineRevision}`,
      {
        buildGitSha: result.buildGitSha,
        buildDirty: result.buildDirty ?? null,
        engineRevision: result.engineRevision,
      },
    ]),
  ).values(),
];
if (sourceIdentities.length !== 1) {
  throw new Error("Arena matchups were produced by inconsistent build identities.");
}
const report = {
  schemaVersion: 2,
  kind: "colonist-assistant-native-arena-matrix",
  generatedAt: new Date().toISOString(),
  buildIdentity: sourceIdentities[0],
  liveProductionProfile: {
    algorithm: "weighted-belief Deep MaxN",
    maxnDepth: options.maxnDepth,
    maxnBranch: options.maxnBranch,
    maxnNodes: options.maxnNodes,
    maxnTimeMs: options.maxnTimeMs,
    beliefParticles: options.beliefParticles,
    strategicParticles: options.strategicParticles,
    trackerWorldLimit: 4_096,
    adapterSourceWorldLimit: 96,
    interactiveParticleLimit: 24,
    tacticalDepth: 14,
    tacticalNodes: 900,
    informationMode: "weighted-belief",
  },
  configuration: options,
  checkpointDirectory,
  challengeOutputs,
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
      checkpoints: checkpointDirectory,
      challenges: challengeOutputs,
      totalGames: report.totalGames,
    },
    null,
    2,
  ),
);
