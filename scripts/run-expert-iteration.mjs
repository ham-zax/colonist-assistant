#!/usr/bin/env node

import { spawn } from "node:child_process";
import { appendFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { cpus } from "node:os";
import { resolve } from "node:path";
import process from "node:process";

const ROOT = resolve(import.meta.dirname, "..");
const ENGINE = resolve(ROOT, "engine");
const ARENA = resolve(ENGINE, "target/release/colonist-arena");

const options = {
  generations: 1,
  blocks: 4,
  iterations: 8,
  rolloutActions: 60,
  expertIterations: 48,
  expertRolloutActions: 140,
  maxTurns: 600,
  threads: Math.min(4, cpus().length),
  seed: 20260728,
  output: resolve(ROOT, "training-data"),
};

for (let index = 2; index < process.argv.length; index += 1) {
  const argument = process.argv[index];
  const value = process.argv[index + 1];
  switch (argument) {
    case "--generations":
      options.generations = Number(value);
      index += 1;
      break;
    case "--blocks":
      options.blocks = Number(value);
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
    case "--expert-iterations":
      options.expertIterations = Number(value);
      index += 1;
      break;
    case "--expert-rollout-actions":
      options.expertRolloutActions = Number(value);
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
    default:
      throw new Error(`Unknown option: ${argument}`);
  }
}

function run(command, args, cwd) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, {
      cwd,
      stdio: ["ignore", "pipe", "inherit"],
    });
    let output = "";
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      output += chunk;
    });
    child.once("error", reject);
    child.once("exit", (code) => {
      if (code === 0) resolvePromise(output);
      else reject(new Error(`${command} exited with code ${code}`));
    });
  });
}

const lineups = [
  ["puct", "maxn"],
  ["puct", "alphabeta"],
  ["puct", "weighted", "maxn"],
  ["puct", "weighted", "weighted", "random"],
  ["puct", "puct", "weighted", "weighted"],
  ["puct", "uct", "weighted", "random"],
];

await mkdir(options.output, { recursive: true });
const cumulativeStrategic = [];
const cumulativeTrades = [];
for (let generation = 0; generation < options.generations; generation += 1) {
  console.error(`Expert Iteration generation ${generation + 1}/${options.generations}`);
  await run(
    "nix-shell",
    [
      "-p",
      "cargo",
      "rustc",
      "--run",
      "cargo build --release -p colonist-catan-arena",
    ],
    ENGINE,
  );
  const combined = resolve(options.output, `generation-${generation + 1}.jsonl`);
  const combinedTrades = resolve(
    options.output,
    `generation-${generation + 1}-trades.jsonl`,
  );
  await rm(combined, { force: true });
  await rm(combinedTrades, { force: true });
  const arenaReports = [];
  for (const [lineupIndex, lineup] of lineups.entries()) {
    const shard = resolve(
      options.output,
      `generation-${generation + 1}-lineup-${lineupIndex + 1}.jsonl`,
    );
    const tradeShard = resolve(
      options.output,
      `generation-${generation + 1}-lineup-${lineupIndex + 1}-trades.jsonl`,
    );
    const output = await run(
      ARENA,
      [
        "--players",
        String(lineup.length),
        "--blocks",
        String(options.blocks),
        "--lineup",
        lineup.join(","),
        "--candidate",
        "puct",
        "--iterations",
        String(options.iterations),
        "--rollout-actions",
        String(options.rolloutActions),
        "--max-turns",
        String(options.maxTurns),
        "--threads",
        String(options.threads),
        "--seed",
        String(options.seed + generation * 1_000_000 + lineupIndex * 100_000),
        "--expert-output",
        shard,
        "--trade-output",
        tradeShard,
        "--expert-stride",
        "5",
        "--expert-iterations",
        String(options.expertIterations),
        "--expert-rollout-actions",
        String(options.expertRolloutActions),
        "--validate",
        "--json",
      ],
      ENGINE,
    );
    arenaReports.push(JSON.parse(output));
    await appendFile(combined, await readFile(shard));
    await appendFile(combinedTrades, await readFile(tradeShard));
  }
  const metrics = resolve(
    ROOT,
    "benchmark-results",
    `strategic-model-generation-${generation + 1}.json`,
  );
  cumulativeStrategic.push(combined);
  const training = await run(
    "python3",
    [
      resolve(ROOT, "scripts/train-strategic-model.py"),
      ...cumulativeStrategic,
      "--metrics",
      metrics,
      "--seed",
      String(options.seed + generation),
      "--baseline-action-features",
      "48",
    ],
    ROOT,
  );
  await writeFile(
    resolve(options.output, `generation-${generation + 1}-arena.json`),
    `${JSON.stringify(arenaReports, null, 2)}\n`,
  );
  console.error(training.trim());
  cumulativeTrades.push(combinedTrades);
  const tradeTraining = await run(
    "python3",
    [
      resolve(ROOT, "scripts/train-trade-model.py"),
      ...cumulativeTrades,
      "--seed",
      String(options.seed + generation),
    ],
    ROOT,
  );
  await writeFile(
    resolve(
      ROOT,
      "benchmark-results",
      `trade-model-generation-${generation + 1}.json`,
    ),
    `${tradeTraining.trim()}\n`,
  );
  console.error(tradeTraining.trim());
}
