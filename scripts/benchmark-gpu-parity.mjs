#!/usr/bin/env node

import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import process from "node:process";

const ROOT = resolve(import.meta.dirname, "..");
const GPU_BENCHMARK = resolve(ROOT, "scripts", "benchmark-gpu-zoom.py");

function readOptions(argv) {
  const options = {
    inputs: [],
    outputPrefix: resolve(ROOT, "benchmark-results", "gpu-parity"),
    hidden: 32,
    epochs: 12,
    policyEpochs: 6,
    batchSize: 4096,
    policyBatchGroups: 256,
    seed: 20260728,
    inferenceRepeats: 100,
    tolerance: 1e-5,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const name = argv[index];
    const value = argv[index + 1];
    switch (name) {
      case "--input":
        options.inputs.push(resolve(value));
        index += 1;
        break;
      case "--output-prefix":
        options.outputPrefix = resolve(value);
        index += 1;
        break;
      case "--hidden":
        options.hidden = Number(value);
        index += 1;
        break;
      case "--epochs":
        options.epochs = Number(value);
        index += 1;
        break;
      case "--policy-epochs":
        options.policyEpochs = Number(value);
        index += 1;
        break;
      case "--batch-size":
        options.batchSize = Number(value);
        index += 1;
        break;
      case "--policy-batch-groups":
        options.policyBatchGroups = Number(value);
        index += 1;
        break;
      case "--seed":
        options.seed = Number(value);
        index += 1;
        break;
      case "--inference-repeats":
        options.inferenceRepeats = Number(value);
        index += 1;
        break;
      case "--tolerance":
        options.tolerance = Number(value);
        index += 1;
        break;
      case "--help":
      case "-h":
        console.log(`Usage: npm run benchmark:gpu:parity -- --input CORPUS [--input CORPUS ...] [options]

Runs benchmark-gpu-zoom.py with identical data/configuration on CPU and CUDA,
then fails if quality metrics differ by more than the requested tolerance.
Throughput is reported separately and is not part of numerical parity.

  --input PATH                Expert JSONL corpus; repeatable
  --output-prefix PATH        Output prefix (default: benchmark-results/gpu-parity)
  --hidden N                  Hidden width (default: 32)
  --epochs N                  Value epochs (default: 12)
  --policy-epochs N           Policy epochs (default: 6)
  --batch-size N              Value batch size (default: 4096)
  --policy-batch-groups N     Policy groups per batch (default: 256)
  --seed N                    Shared deterministic seed (default: 20260728)
  --inference-repeats N       Throughput repetitions (default: 100)
  --tolerance X               Max absolute quality-metric delta (default: 1e-5)
`);
        process.exit(0);
      default:
        throw new Error(`Unknown option: ${name}`);
    }
  }
  if (
    options.inputs.length === 0 ||
    !Number.isInteger(options.hidden) ||
    options.hidden < 1 ||
    !Number.isInteger(options.epochs) ||
    options.epochs < 1 ||
    !Number.isInteger(options.policyEpochs) ||
    options.policyEpochs < 1 ||
    !Number.isInteger(options.batchSize) ||
    options.batchSize < 1 ||
    !Number.isInteger(options.policyBatchGroups) ||
    options.policyBatchGroups < 1 ||
    !Number.isSafeInteger(options.seed) ||
    options.seed < 0 ||
    !Number.isInteger(options.inferenceRepeats) ||
    options.inferenceRepeats < 1 ||
    !Number.isFinite(options.tolerance) ||
    options.tolerance < 0
  ) {
    throw new Error("GPU parity options are outside the supported range.");
  }
  return options;
}

function run(command, args) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, { cwd: ROOT, stdio: "inherit" });
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
const cpuOutput = `${options.outputPrefix}-cpu.json`;
const cudaOutput = `${options.outputPrefix}-cuda.json`;

const commonArgs = [
  "run",
  "--with",
  "torch",
  "--with",
  "numpy",
  "python",
  GPU_BENCHMARK,
  ...options.inputs,
  "--hidden",
  String(options.hidden),
  "--epochs",
  String(options.epochs),
  "--policy-epochs",
  String(options.policyEpochs),
  "--batch-size",
  String(options.batchSize),
  "--policy-batch-groups",
  String(options.policyBatchGroups),
  "--seed",
  String(options.seed),
  "--inference-repeats",
  String(options.inferenceRepeats),
];

await run("uv", [...commonArgs, "--device", "cpu", "--output", cpuOutput]);
await run("uv", [...commonArgs, "--device", "cuda", "--output", cudaOutput]);

const cpu = JSON.parse(await readFile(cpuOutput, "utf8"));
const cuda = JSON.parse(await readFile(cudaOutput, "utf8"));
const qualityMetrics = [
  "valueBrier",
  "valueLogLoss",
  "valueTop1Accuracy",
  "uniformValueBrier",
  "uniformValueLogLoss",
  "temperature",
  "policyCrossEntropy",
  "uniformPolicyCrossEntropy",
];
const deltas = Object.fromEntries(
  qualityMetrics.map((metric) => [
    metric,
    Math.abs(Number(cpu.metrics[metric]) - Number(cuda.metrics[metric])),
  ]),
);
const maxDelta = Math.max(...Object.values(deltas));
const ratio = (gpuValue, cpuValue) =>
  Number(cpuValue) > 0 ? Number(gpuValue) / Number(cpuValue) : null;
const speedups = {
  valueTrainingExamplesPerSecond: ratio(
    cuda.metrics.throughput.valueTrainingExamplesPerSecond,
    cpu.metrics.throughput.valueTrainingExamplesPerSecond,
  ),
  policyTrainingActionsPerSecond: ratio(
    cuda.metrics.throughput.policyTrainingActionsPerSecond,
    cpu.metrics.throughput.policyTrainingActionsPerSecond,
  ),
  valuePositionsPerSecond: ratio(
    cuda.metrics.throughput.valuePositionsPerSecond,
    cpu.metrics.throughput.valuePositionsPerSecond,
  ),
  policyActionsPerSecond: ratio(
    cuda.metrics.throughput.policyActionsPerSecond,
    cpu.metrics.throughput.policyActionsPerSecond,
  ),
};

const report = {
  kind: "colonist-gpu-cpu-parity",
  inputs: options.inputs,
  cpuOutput,
  cudaOutput,
  tolerance: options.tolerance,
  maxQualityMetricDelta: maxDelta,
  qualityMetricDeltas: deltas,
  speedups,
  parity: maxDelta <= options.tolerance,
};
console.log(JSON.stringify(report, null, 2));
if (!report.parity) process.exitCode = 1;
