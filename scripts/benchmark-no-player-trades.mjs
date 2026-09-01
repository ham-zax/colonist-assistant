#!/usr/bin/env node

import { spawn } from "node:child_process";
import { mkdir } from "node:fs/promises";
import { cpus } from "node:os";
import { basename, dirname, resolve } from "node:path";
import process from "node:process";

const ROOT = resolve(import.meta.dirname, "..");
const ENGINE_DIR = resolve(ROOT, "engine");
const ARENA = resolve(ENGINE_DIR, "target/release/colonist-arena");
const LOCAL_BENCHMARK = resolve(ROOT, "scripts", "benchmark-local.mjs");
const GPU_BENCHMARK = resolve(ROOT, "scripts", "benchmark-gpu-zoom.py");

function readOptions(argv) {
  const options = {
    label: "latest",
    games: 200,
    threads: Math.min(cpus().length, 4),
    seed: 97_100_001,
    gpuBlocks: 4,
    gpuExpertStride: 4,
    gpuHidden: 512,
    gpuEpochs: 8,
    gpuPolicyEpochs: 4,
    build: true,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const name = argv[index];
    const value = argv[index + 1];
    switch (name) {
      case "--label":
        options.label = value;
        index += 1;
        break;
      case "--games":
        options.games = Number(value);
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
      case "--gpu-blocks":
        options.gpuBlocks = Number(value);
        index += 1;
        break;
      case "--gpu-expert-stride":
        options.gpuExpertStride = Number(value);
        index += 1;
        break;
      case "--gpu-hidden":
        options.gpuHidden = Number(value);
        index += 1;
        break;
      case "--gpu-epochs":
        options.gpuEpochs = Number(value);
        index += 1;
        break;
      case "--gpu-policy-epochs":
        options.gpuPolicyEpochs = Number(value);
        index += 1;
        break;
      case "--no-build":
        options.build = false;
        break;
      case "--quick":
        options.games = 24;
        options.gpuBlocks = 2;
        options.gpuExpertStride = 8;
        options.gpuEpochs = 4;
        options.gpuPolicyEpochs = 2;
        break;
      case "--help":
      case "-h":
        console.log(`Usage: npm run benchmark:no-player-trades -- [options]

Runs the canonical 3-player and 4-player no-player-trades strength matrix,
then generates a fixed no-player-trades expert corpus and evaluates it on CUDA.
Player-to-player offers, accepts, counters, and confirmations are disabled;
bank and port maritime trades remain enabled.

  --label NAME              Output label (default: latest)
  --games N                 Games per native matchup before seat rounding (default: 200)
  --threads N               Native arena worker threads
  --seed N                  Deterministic base seed (default: 97100001)
  --gpu-blocks N            Expert-corpus blocks per player count (default: 4; minimum: 2)
  --gpu-expert-stride N     Record every Nth decision for GPU data (default: 4)
  --gpu-hidden N            GPU model hidden width (default: 512)
  --gpu-epochs N            GPU value epochs (default: 8)
  --gpu-policy-epochs N     GPU policy epochs (default: 4)
  --no-build                Reuse the current release arena binary
  --quick                   24 games per native matchup, 2 GPU corpus blocks, fewer epochs
`);
        process.exit(0);
      default:
        throw new Error(`Unknown option: ${name}`);
    }
  }
  if (
    !options.label ||
    !Number.isInteger(options.games) ||
    options.games < 1 ||
    !Number.isInteger(options.threads) ||
    options.threads < 1 ||
    !Number.isSafeInteger(options.seed) ||
    options.seed < 0 ||
    !Number.isInteger(options.gpuBlocks) ||
    options.gpuBlocks < 2 ||
    !Number.isInteger(options.gpuExpertStride) ||
    options.gpuExpertStride < 1 ||
    !Number.isInteger(options.gpuHidden) ||
    options.gpuHidden < 1 ||
    !Number.isInteger(options.gpuEpochs) ||
    options.gpuEpochs < 1 ||
    !Number.isInteger(options.gpuPolicyEpochs) ||
    options.gpuPolicyEpochs < 1
  ) {
    throw new Error("Benchmark options are outside the supported range.");
  }
  return options;
}

function run(command, args, { cwd } = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, { cwd, stdio: "inherit" });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (code === 0) {
        resolvePromise();
        return;
      }
      reject(
        new Error(
          `${command} exited with ${signal ? `signal ${signal}` : `code ${code}`}`,
        ),
      );
    });
  });
}

const options = readOptions(process.argv.slice(2));
const safeLabel = options.label.replace(/[^A-Za-z0-9._-]+/g, "-");
const nativeOutput = resolve(
  ROOT,
  "benchmark-results",
  `no-player-trades-${safeLabel}`,
);
const corpusDirectory = resolve(ROOT, "training-data", "no-player-trades");
await mkdir(dirname(nativeOutput), { recursive: true });
await mkdir(corpusDirectory, { recursive: true });

const nativeArgs = [
  LOCAL_BENCHMARK,
  "--players",
  "3,4",
  "--baselines",
  "weighted,alphabeta",
  "--games",
  String(options.games),
  "--threads",
  String(options.threads),
  "--seed",
  String(options.seed),
  "--output",
  nativeOutput,
  "--no-player-trades",
];
if (!options.build) nativeArgs.push("--no-build");

console.error("Running canonical no-player-trades native strength matrix...");
await run(process.execPath, nativeArgs, { cwd: ROOT });

const expertInputs = [];
const gpuOutputs = {};
for (const players of [3, 4]) {
  const expertOutput = resolve(
    corpusDirectory,
    `no-player-trades-${safeLabel}-${players}p.jsonl`,
  );
  const gpuOutput = resolve(
    ROOT,
    "benchmark-results",
    `no-player-trades-${safeLabel}-${players}p-gpu.json`,
  );
  expertInputs.push(expertOutput);
  const corpusSeed = options.seed + 8_000_000 + players * 1_000_000;
  console.error(
    `Generating ${players}-player no-player-trades expert corpus for CUDA zoom...`,
  );
  await run(
    ARENA,
    [
      "--players",
      String(players),
      "--blocks",
      String(options.gpuBlocks),
      "--seed",
      String(corpusSeed),
      "--candidate",
      "maxn",
      "--baseline",
      "weighted",
      "--iterations",
      "80",
      "--rollout-actions",
      "140",
      "--max-turns",
      "600",
      "--maxn-depth",
      "4",
      "--maxn-branch",
      "8",
      "--maxn-nodes",
      "4000",
      "--maxn-time-ms",
      "350",
      "--belief-particles",
      "24",
      "--strategic-particles",
      "12",
      "--threads",
      String(options.threads),
      "--expert-output",
      expertOutput,
      "--expert-stride",
      String(options.gpuExpertStride),
      "--expert-iterations",
      "80",
      "--expert-rollout-actions",
      "140",
      "--no-player-trades",
      "--validate",
      "--quiet",
    ],
    { cwd: ENGINE_DIR },
  );

  console.error(
    `Running CUDA strategic-model zoom on the ${players}-player no-player-trades corpus...`,
  );
  await run(
    "uv",
    [
      "run",
      "--with",
      "torch",
      "--with",
      "numpy",
      "python",
      GPU_BENCHMARK,
      expertOutput,
      "--device",
      "cuda",
      "--hidden",
      String(options.gpuHidden),
      "--epochs",
      String(options.gpuEpochs),
      "--policy-epochs",
      String(options.gpuPolicyEpochs),
      "--batch-size",
      "4096",
      "--policy-batch-groups",
      "512",
      "--seed",
      String(options.seed),
      "--output",
      gpuOutput,
    ],
    { cwd: ROOT },
  );
  gpuOutputs[`${players}p`] = gpuOutput;
}

console.log(
  JSON.stringify(
    {
      label: safeLabel,
      playerTradesEnabled: false,
      maritimeTradesEnabled: true,
      nativeJson: `${nativeOutput}.json`,
      nativeMarkdown: `${nativeOutput}.md`,
      expertInputs: expertInputs.map((path) => basename(path)),
      gpuJson: gpuOutputs,
    },
    null,
    2,
  ),
);
