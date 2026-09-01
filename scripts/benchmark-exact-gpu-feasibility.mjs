#!/usr/bin/env node

import { existsSync } from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import process from "node:process";

const ROOT = resolve(import.meta.dirname, "..");

function readOptions(argv) {
  const options = {
    states: 512,
    repeats: 32,
    seed: 108100001,
    playerTradesEnabled: false,
    output: resolve(ROOT, "benchmark-results", "exact-gpu-feasibility.json"),
  };
  for (let index = 0; index < argv.length; index += 1) {
    const name = argv[index];
    const value = argv[index + 1];
    switch (name) {
      case "--states":
        options.states = Number(value);
        index += 1;
        break;
      case "--repeats":
        options.repeats = Number(value);
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
      case "--player-trades":
        options.playerTradesEnabled = true;
        break;
      case "--no-player-trades":
        options.playerTradesEnabled = false;
        break;
      case "--help":
      case "-h":
        console.log(`Usage: npm run benchmark:gpu:exact-feasibility -- [options]

Measures the corrected Rust evaluator and the non-evaluator node primitive for
3-player and 4-player states. This does not claim GPU acceleration by itself;
it measures the exact evaluator share and the Amdahl ceiling that an exact GPU
port would have to exploit.

  --states N           Representative states per format (default: 512)
  --repeats N          Timing repeats (default: 32)
  --seed N             Deterministic state-generation seed (default: 108100001)
  --output PATH        JSON report path
  --player-trades      Generate states with player trading enabled
  --no-player-trades   Disable player trading (default)
`);
        process.exit(0);
      default:
        throw new Error(`Unknown option: ${name}`);
    }
  }
  if (
    !Number.isInteger(options.states) ||
    options.states < 1 ||
    !Number.isInteger(options.repeats) ||
    options.repeats < 1 ||
    !Number.isSafeInteger(options.seed) ||
    options.seed < 0
  ) {
    throw new Error("Exact GPU feasibility options are outside the supported range.");
  }
  return options;
}

function cargoExecutable() {
  if (process.env.CARGO) return process.env.CARGO;
  const userCargo = resolve(homedir(), ".cargo", "bin", "cargo");
  return existsSync(userCargo) ? userCargo : "cargo";
}

function runFormat(players, options) {
  const result = spawnSync(
    cargoExecutable(),
    [
      "run",
      "--quiet",
      "--release",
      "-p",
      "colonist-catan-arena",
      "--bin",
      "evaluator-bench",
      "--",
      "--players",
      String(players),
      "--states",
      String(options.states),
      "--repeats",
      String(options.repeats),
      "--seed",
      String(options.seed),
      options.playerTradesEnabled ? "--player-trades" : "--no-player-trades",
    ],
    {
      cwd: resolve(ROOT, "engine"),
      encoding: "utf8",
      maxBuffer: 16 * 1024 * 1024,
    },
  );
  if (result.status !== 0) {
    process.stderr.write(result.stderr ?? "");
    throw new Error(`evaluator-bench ${players}P exited with ${result.status}`);
  }
  return JSON.parse(result.stdout);
}

function measurement(report, name) {
  const value = report.measurements.find((item) => item.name === name);
  if (!value) throw new Error(`Missing ${name} measurement for ${report.players}P`);
  return value;
}

function requiredAcceleratorSpeedup(fraction, targetSpeedup) {
  const denominator = 1 / targetSpeedup - (1 - fraction);
  if (denominator <= 0) return null;
  return fraction / denominator;
}

function summarize(report) {
  const evaluate = measurement(report, "evaluate");
  const base = measurement(report, "node-without-evaluate");
  const full = measurement(report, "node-with-evaluate");
  const fraction = report.evaluatorShareOfNodePrimitive;
  const routeArrivalShare =
    report.evaluatorBreakdown.routeMapShare +
    report.evaluatorBreakdown.arrivalScoreShare;
  const routeArrivalNodeFraction = fraction * routeArrivalShare;
  const routeArrivalOnlyCeiling = 1 / Math.max(1e-12, 1 - routeArrivalNodeFraction);
  return {
    players: report.players,
    sampledStates: report.sampledStates,
    evaluateMicrosecondsPerState: evaluate.microsecondsPerState,
    evaluateStatesPerSecond: evaluate.statesPerSecond,
    nonEvaluatorNodeMicroseconds: base.microsecondsPerState,
    fullNodeMicroseconds: full.microsecondsPerState,
    evaluatorShare: fraction,
    evaluatorBreakdown: report.evaluatorBreakdown,
    routeArrivalEvaluatorShare: routeArrivalShare,
    infiniteRouteArrivalOnlyNodeSpeedupCeiling: routeArrivalOnlyCeiling,
    routeArrivalOnlyCanReach2x: routeArrivalOnlyCeiling >= 2,
    infiniteEvaluatorSpeedupCeiling: report.infiniteEvaluatorSpeedupCeiling,
    evaluatorSpeedupNeededFor2xNode: requiredAcceleratorSpeedup(fraction, 2),
    evaluatorSpeedupNeededFor4xNode: requiredAcceleratorSpeedup(fraction, 4),
    exactGpuTwoXFeasibleInPrinciple: report.infiniteEvaluatorSpeedupCeiling >= 2,
  };
}

const options = readOptions(process.argv.slice(2));
const threePlayer = runFormat(3, options);
const fourPlayer = runFormat(4, options);
const report = {
  schemaVersion: 1,
  kind: "colonist-exact-gpu-feasibility",
  baseEngineRevision: "post-task12-integrated",
  seed: options.seed,
  statesPerFormat: options.states,
  repeats: options.repeats,
  playerTradesEnabled: options.playerTradesEnabled,
  formats: [summarize(threePlayer), summarize(fourPlayer)],
  raw: {
    threePlayer,
    fourPlayer,
  },
  interpretation: {
    exactPath:
      "An exact GPU port must preserve the handcrafted Rust evaluator. The current Torch value/policy benchmark is not a drop-in exact evaluator.",
    twoXGate:
      "Proceed with an exact GPU evaluator prototype only if it can exceed the reported evaluator speedup required for a 2x node primitive and preserve CPU decisions within the defined floating-point tolerance.",
    moderateSubsetGate:
      "If route maps plus arrival/race preparation cannot reach 2x even at infinite speed, a route-only CUDA port is rejected; Option 1 then requires a broader exact evaluator backend rather than a moderate subset offload.",
  },
};
await mkdir(dirname(options.output), { recursive: true });
await writeFile(options.output, `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify(report, null, 2));
