#!/usr/bin/env node

import { spawn } from "node:child_process";
import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import { cpus } from "node:os";
import { delimiter, dirname, resolve } from "node:path";
import process from "node:process";

const ROOT = resolve(import.meta.dirname, "..");
const ENGINE_DIR = resolve(ROOT, "engine");
const ARENA = resolve(ENGINE_DIR, "target/release/colonist-arena");
const CANONICAL_BLOCKS = 250;
const PLAYERS = [3, 4];
const CONFIG = Object.freeze({
  iterations: 80,
  rolloutActions: 140,
  maxTurns: 600,
  maxnDepth: 4,
  maxnBranch: 8,
  maxnNodes: 4_000,
  maxnTimeMs: 0,
  openingNodes: 12_000,
  openingTimeMs: 0,
  tradeResponseNodes: 2_000,
  tradeResponseTimeMs: 0,
  beliefParticles: 24,
  strategicParticles: 24,
});

const TORCH_PROBE = String.raw`
import ctypes.util
import importlib.util
import json
from pathlib import Path

import torch


def module_paths(name):
    try:
        spec = importlib.util.find_spec(name)
    except (ImportError, ModuleNotFoundError, ValueError):
        return []
    paths = []
    if spec and spec.origin and spec.origin not in ("built-in", "frozen"):
        paths.append(Path(spec.origin).resolve().parent)
    if spec and spec.submodule_search_locations:
        paths.extend(Path(path).resolve() for path in spec.submodule_search_locations)
    return paths


torch_root = Path(torch.__file__).resolve().parent
roots = {torch_root, torch_root / "lib"}
for module in ("nvidia", "nvidia.cuda_nvrtc", "nvidia.cuda_runtime"):
    roots.update(module_paths(module))

libraries = []
for root in roots:
    for pattern in ("libnvrtc.so*", "libcudart.so*", "libnvrtc.dylib*"):
        try:
            libraries.extend(path.resolve() for path in root.rglob(pattern) if path.is_file())
        except (OSError, RuntimeError):
            pass
libraries = sorted({str(path) for path in libraries})

try:
    available = bool(torch.cuda.is_available())
    count = int(torch.cuda.device_count()) if available else 0
    device_name = torch.cuda.get_device_name(0) if count else None
    memory_free, memory_total = torch.cuda.mem_get_info(0) if count else (None, None)
except Exception as error:
    available = False
    count = 0
    device_name = None
    memory_free = None
    memory_total = None
    runtime_error = str(error)
else:
    runtime_error = None

print(json.dumps({
    "torchVersion": getattr(torch, "__version__", None),
    "torchRoot": str(torch_root),
    "cudaVersion": getattr(getattr(torch, "version", None), "cuda", None),
    "cudaAvailable": available,
    "deviceCount": count,
    "deviceName": device_name,
    "memoryFreeBytes": int(memory_free) if memory_free is not None else None,
    "memoryTotalBytes": int(memory_total) if memory_total is not None else None,
    "runtimeError": runtime_error,
    "libraryDirs": sorted({str(Path(path).parent) for path in libraries}),
    "nvrtcLibraries": [path for path in libraries if "nvrtc" in path.lower()],
    "cudaRuntimeLibraries": [path for path in libraries if "cudart" in path.lower()],
    "findLibrary": {
        "nvrtc": ctypes.util.find_library("nvrtc"),
        "cudart": ctypes.util.find_library("cudart"),
        "cuda": ctypes.util.find_library("cuda"),
    },
}))
`;

const number = (value) => {
  const result = typeof value === "number" ? value : Number(value);
  return Number.isFinite(result) ? result : null;
};

function required(args, index, flag) {
  const value = args[index + 1];
  if (value === undefined || value.startsWith("--")) {
    throw new Error(`${flag} requires a value`);
  }
  return value;
}

function readOptions(args) {
  const options = {
    profile: "canonical",
    players: [...PLAYERS],
    blocks: CANONICAL_BLOCKS,
    blocksExplicit: false,
    threads: Math.min(cpus().length, 4),
    seed: 9_100_001,
    output: resolve(ROOT, "benchmark-results", "exact-gpu-arena.json"),
    build: true,
    validate: true,
  };
  for (let index = 0; index < args.length; index += 1) {
    switch (args[index]) {
      case "--canonical":
        options.profile = "canonical";
        if (!options.blocksExplicit) options.blocks = CANONICAL_BLOCKS;
        break;
      case "--smoke":
        options.profile = "smoke";
        if (!options.blocksExplicit) options.blocks = 1;
        break;
      case "--players":
        options.players = required(args, index, "--players").split(",").map(Number);
        index += 1;
        break;
      case "--blocks":
        options.blocks = Number(required(args, index, "--blocks"));
        options.blocksExplicit = true;
        index += 1;
        break;
      case "--threads":
        options.threads = Number(required(args, index, "--threads"));
        index += 1;
        break;
      case "--seed":
        options.seed = Number(required(args, index, "--seed"));
        index += 1;
        break;
      case "--output":
        options.output = resolve(required(args, index, "--output"));
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
      case "--help":
      case "-h":
        console.log(`Usage: node scripts/benchmark-exact-gpu.mjs [options]

Runs matched exact handcrafted CPU/CUDA arena lanes for 3P and 4P all-MaxN
games with player-to-player offers, accepts, counters, and confirmations
disabled. Maritime bank/port trades remain enabled. CPU evaluate(state) is the
oracle; no neural evaluator is used.

  --canonical             250 blocks per selected lane (default)
  --smoke                 1 block per selected lane
  --players 3,4           Player counts (default: 3,4)
  --blocks N              Override the profile block count
  --threads N             Shared native worker count (default: 4)
  --seed N                Shared board/chance seed (default: 9100001)
  --output PATH           JSON report path
  --no-build              Reuse target/release/colonist-arena
  --no-validate            Skip arena transition validation
`);
        process.exit(0);
        break;
      default:
        throw new Error(`Unknown option: ${args[index]}`);
    }
  }
  if (
    options.players.length === 0 ||
    options.players.some((players) => ![3, 4].includes(players)) ||
    new Set(options.players).size !== options.players.length ||
    !Number.isInteger(options.blocks) ||
    options.blocks < 1 ||
    !Number.isInteger(options.threads) ||
    options.threads < 1 ||
    !Number.isSafeInteger(options.seed) ||
    options.seed < 0
  ) {
    throw new Error("Use distinct player counts from 3,4, positive blocks/threads, and a safe seed.");
  }
  return options;
}

function run(command, args, { cwd = ROOT, env = process.env, capture = false, label = command } = {}) {
  return new Promise((resolvePromise, reject) => {
    const started = process.hrtime.bigint();
    const child = spawn(command, args, {
      cwd,
      env,
      stdio: capture ? ["ignore", "pipe", "pipe"] : ["ignore", "inherit", "inherit"],
    });
    let stdout = "";
    let stderr = "";
    if (capture) {
      child.stdout.setEncoding("utf8");
      child.stderr.setEncoding("utf8");
      child.stdout.on("data", (chunk) => {
        stdout += chunk;
      });
      child.stderr.on("data", (chunk) => {
        stderr += chunk;
        process.stderr.write(chunk);
      });
    }
    child.once("error", (error) => reject(new Error(`${label} could not start: ${error.message}`)));
    child.once("exit", (code, signal) => {
      const elapsedMs = Number(process.hrtime.bigint() - started) / 1_000_000;
      if (code !== 0) {
        reject(new Error(`${label} exited with ${signal ? `signal ${signal}` : `code ${code}`}\n${stderr}`));
        return;
      }
      resolvePromise({ stdout, stderr, elapsedMs });
    });
  });
}

function parseJson(text, label) {
  const candidates = [text.trim(), ...text.trim().split(/\r?\n/).reverse()];
  for (const candidate of candidates) {
    if (!candidate) continue;
    try {
      return JSON.parse(candidate);
    } catch {
      // The arena is expected to emit one JSON object; tolerate harmless line noise.
    }
  }
  throw new Error(`${label} emitted no parseable JSON`);
}

async function discoverCuda() {
  const result = await run(
    process.env.UV ?? "uv",
    ["run", "--with", "torch", "--no-project", "python", "-c", TORCH_PROBE],
    { capture: true, label: "Torch CUDA discovery" },
  );
  const discovery = parseJson(result.stdout, "Torch CUDA discovery");
  if (!discovery.cudaAvailable || discovery.deviceCount < 1) {
    throw new Error(
      `Torch reports no usable CUDA device (available=${String(discovery.cudaAvailable)}, devices=${String(discovery.deviceCount)}, error=${String(discovery.runtimeError ?? "none")}).`,
    );
  }
  if (discovery.nvrtcLibraries.length === 0 && !discovery.findLibrary?.nvrtc) {
    throw new Error("Torch CUDA is available but NVRTC was not discoverable.");
  }
  return discovery;
}

function cudaEnvironment(discovery) {
  const variable = process.platform === "darwin" ? "DYLD_LIBRARY_PATH" : "LD_LIBRARY_PATH";
  const existing = process.env[variable]?.split(delimiter).filter(Boolean) ?? [];
  const directories = [...new Set([...(discovery.libraryDirs ?? []), ...existing])];
  return { ...process.env, [variable]: directories.join(delimiter) };
}

function arenaArgs(players, backend, checkpoint, options) {
  const args = [
    "--players", String(players),
    "--blocks", String(options.blocks),
    "--seed", String(options.seed),
    "--candidate", "maxn",
    "--baseline", "maxn",
    "--lineup", Array.from({ length: players }, () => "maxn").join(","),
    "--evaluator", backend,
    "--iterations", String(CONFIG.iterations),
    "--rollout-actions", String(CONFIG.rolloutActions),
    "--max-turns", String(CONFIG.maxTurns),
    "--maxn-depth", String(CONFIG.maxnDepth),
    "--maxn-branch", String(CONFIG.maxnBranch),
    "--maxn-nodes", String(CONFIG.maxnNodes),
    "--maxn-time-ms", String(CONFIG.maxnTimeMs),
    "--opening-nodes", String(CONFIG.openingNodes),
    "--opening-time-ms", String(CONFIG.openingTimeMs),
    "--trade-response-nodes", String(CONFIG.tradeResponseNodes),
    "--trade-response-time-ms", String(CONFIG.tradeResponseTimeMs),
    "--belief-particles", String(CONFIG.beliefParticles),
    "--strategic-particles", String(CONFIG.strategicParticles),
    "--threads", String(options.threads),
    "--checkpoint-output", checkpoint,
    "--no-player-trades",
    "--json",
    "--quiet",
  ];
  if (options.validate) args.push("--validate");
  return args;
}

const same = (actual, expected) => String(actual) === String(expected);

function validateRun(result, checkpoint, players, backend, options) {
  const reportedBackends = [result.backend, result.evaluatorBackend, result.evaluator]
    .filter((value) => typeof value === "string");
  if (reportedBackends.length === 0) {
    throw new Error(`${players}P ${backend} omitted its backend; refusing a silent fallback.`);
  }
  for (const reported of reportedBackends) {
    const matches = backend === "cuda"
      ? /cuda|gpu/i.test(reported)
      : /cpu/i.test(reported) && !/cuda/i.test(reported);
    if (!matches) throw new Error(`${players}P ${backend} reported ${reported}.`);
  }
  if (result.fallback === true || result.usedFallback === true || result.neural === true) {
    throw new Error(`${players}P ${backend} reported fallback/neural evaluation.`);
  }

  const expected = {
    players,
    blocks: options.blocks,
    seed: options.seed,
    threads: options.threads,
    iterations: CONFIG.iterations,
    rolloutActions: CONFIG.rolloutActions,
    maxTurns: CONFIG.maxTurns,
    beliefParticles: CONFIG.beliefParticles,
    strategicParticleLimit: CONFIG.strategicParticles,
    maxnDepth: CONFIG.maxnDepth,
    maxnBranch: CONFIG.maxnBranch,
    maxnNodes: CONFIG.maxnNodes,
    maxnTimeMs: 0,
    openingNodes: CONFIG.openingNodes,
    openingTimeMs: 0,
    tradeResponseNodes: CONFIG.tradeResponseNodes,
    tradeResponseTimeMs: 0,
    playerTradesEnabled: false,
    maritimeTradesEnabled: true,
    perfectInformationSearch: false,
    candidate: "maxn",
    baseline: "maxn",
    informationMode: "weighted-belief",
    validate: options.validate,
    evaluatorBackend: backend === "cuda" ? "cuda-exact" : "cpu-exact",
  };
  const lineup = Array.from({ length: players }, () => "maxn");
  for (const [key, value] of Object.entries(expected)) {
    const reported = [result[key], checkpoint.final[key]].filter((item) => item !== undefined);
    if (reported.length === 0) {
      throw new Error(`${players}P ${backend} omitted required configuration field ${key}.`);
    }
    for (const actual of reported) {
      if (!same(actual, value)) {
        throw new Error(`${players}P ${backend} ${key} mismatch: expected ${String(value)}, got ${String(actual)}`);
      }
    }
  }
  for (const source of [result, checkpoint.final]) {
    if (source.lineup !== undefined && JSON.stringify(source.lineup) !== JSON.stringify(lineup)) {
      throw new Error(`${players}P ${backend} lineup is not all-MaxN.`);
    }
    if (source.maritimeTradesEnabled === false) {
      throw new Error(`${players}P ${backend} disabled maritime trades.`);
    }
  }
  for (const metrics of [result.candidateMetrics, ...Object.values(result.engineMetrics ?? {}), result.tradeMetrics]) {
    if (!metrics || typeof metrics !== "object") continue;
    for (const key of ["meanDomesticOffers", "tradeAcceptanceRate", "meanCounters", "offers", "accepts", "counters", "confirmations"]) {
      const value = number(metrics[key]);
      if (value !== null && value > 0) {
        throw new Error(`${players}P ${backend} reported forbidden player-trade metric ${key}=${value}.`);
      }
    }
  }
}

function checkpointData(path) {
  return readFile(path, "utf8").then((contents) => {
    let final;
    let lines = 0;
    const games = {};
    for (const line of contents.split(/\r?\n/)) {
      if (!line.trim()) continue;
      let value;
      try {
        value = JSON.parse(line);
      } catch {
        throw new Error(`Checkpoint ${path} contains invalid JSONL.`);
      }
      if (!value || typeof value !== "object" || Array.isArray(value)) {
        throw new Error(`Checkpoint ${path} contains a non-object record.`);
      }
      final = value;
      const game = value.lastGame;
      if (game && game.block !== undefined && game.rotation !== undefined) {
        games[`${game.block}:${game.rotation}`] = game;
      }
      lines += 1;
    }
    if (!final) throw new Error(`Checkpoint ${path} is empty.`);
    if (final.complete === false) throw new Error(`Checkpoint ${path} is incomplete.`);
    return { path, final, lines, games };
  });
}

function stable(value, omitLastGame = false) {
  const volatile = /elapsed|latency|backend|device|gpu|batch|throughput|speedup|nodes|depth|particles|decisions|rootactions/i;
  if (Array.isArray(value)) return value.map((item) => stable(item, omitLastGame));
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.keys(value)
      .sort()
      .filter((key) => !volatile.test(key) && !(omitLastGame && key === "lastGame"))
      .map((key) => [key, stable(value[key], omitLastGame)]),
  );
}

function compareCheckpoints(players, cpu, cuda) {
  const cpuGames = Object.entries(cpu.games).sort(([left], [right]) => left.localeCompare(right));
  const cudaGames = Object.entries(cuda.games).sort(([left], [right]) => left.localeCompare(right));
  const cpuProjection = {
    final: stable(cpu.final, cpuGames.length > 0),
    games: cpuGames.map(([key, value]) => [key, stable(value)]),
  };
  const cudaProjection = {
    final: stable(cuda.final, cudaGames.length > 0),
    games: cudaGames.map(([key, value]) => [key, stable(value)]),
  };
  const cpuJson = JSON.stringify(cpuProjection);
  const cudaJson = JSON.stringify(cudaProjection);
  if (cpuJson !== cudaJson) {
    throw new Error(`Deterministic checkpoint parity mismatch for ${players}P; timed speedup was not calculated.`);
  }
  return {
    match: true,
    gamesCompared: Math.max(cpuGames.length, cudaGames.length),
    checkpointLines: { cpu: cpu.lines, cuda: cuda.lines },
  };
}

function winners(final) {
  if (final.engineStandings && Array.isArray(final.engineStandings)) {
    return Object.fromEntries(final.engineStandings.map((item) => [item.engine, item.wins]));
  }
  return null;
}

function metrics(result, checkpoint, elapsedMs, discovery) {
  const candidate = result.candidateMetrics ?? {};
  const maxn = result.engineMetrics?.maxn ?? {};
  const games = number(result.games ?? checkpoint.final.completedGames);
  const cutoffs = number(result.cutoffs ?? checkpoint.final.cutoffs);
  const meanLatency = number(candidate.meanDecisionLatencyMs ?? result.meanDecisionLatencyMs);
  const reportedGamesPerSecond = number(result.gamesPerSecond);
  const gpu = result.gpuStats ?? result.gpu ?? {};
  const batch = result.batchStats ?? result.batch ?? {};
  const cudaBackend = /cuda/i.test(result.evaluatorBackend ?? "");
  const evaluatorTiming = gpu.evaluatorTimingMs ?? null;
  const reportedElapsedMs = number(result.elapsedMs);
  const evaluatorTotalMs = number(evaluatorTiming?.total);
  return {
    backend: result.backend ?? result.evaluatorBackend,
    device: cudaBackend
      ? result.device ?? result.evaluatorDevice ?? discovery.deviceName ?? null
      : "CPU",
    elapsedMs,
    reportedElapsedMs,
    gamesPerSecond: reportedGamesPerSecond ?? (games && elapsedMs > 0 ? games / (elapsedMs / 1_000) : null),
    terminalGames: number(result.terminalGames ?? checkpoint.final.terminalGames),
    winners: result.engineWins ?? winners(checkpoint.final),
    turns: { mean: number(result.meanTurns), total: number(result.totalTurns) },
    actions: { mean: number(result.meanActions), total: number(result.totalActions) },
    cutoffs,
    cutoffRate: number(result.cutoffRate) ?? (games && cutoffs !== null ? cutoffs / games : null),
    decisions: {
      total: number(candidate.decisions ?? result.decisions),
      search: number(maxn.searchSamples ?? result.searchDecisions),
      meanLatencyMs: meanLatency,
    },
    search: {
      totalNodes: number(result.searchNodes ?? maxn.searchNodes),
      meanNodes: number(maxn.meanSearchNodes ?? result.meanSearchNodes),
      meanDepth: number(maxn.meanSearchDepth ?? result.meanSearchDepth),
      meanRootActions: number(maxn.meanRootActions ?? result.meanRootActions),
      searchDeadlineRate: number(maxn.searchDeadlineShare ?? result.searchDeadlineShare),
    },
    particles: {
      configuredBelief: CONFIG.beliefParticles,
      configuredStrategic: CONFIG.strategicParticles,
      meanPosterior: number(maxn.meanPosteriorParticles ?? result.meanPosteriorParticles),
      meanStrategic: number(maxn.meanStrategicParticles ?? result.meanStrategicParticles),
    },
    batch: {
      averageSize: number(result.averageBatchSize ?? batch.averageBatchSize ?? batch.meanBatchSize ?? batch.averageSize),
      stats: Object.keys(batch).length > 0 ? batch : null,
    },
    gpu: cudaBackend ? {
      identity: gpu.name ?? result.gpuName ?? discovery.deviceName ?? null,
      utilizationPercent: number(gpu.utilizationPercent ?? gpu.utilization ?? result.gpuUtilizationPercent),
      memoryUsedBytes: number(gpu.memoryUsedBytes ?? result.gpuMemoryUsedBytes),
      memoryTotalBytes: number(gpu.memoryTotalBytes ?? result.gpuMemoryTotalBytes),
      evaluatorStatesPerSecond: number(gpu.evaluatorStatesPerSecond),
      searchCalls: number(gpu.searchCalls),
      linearSearchCalls: number(gpu.linearSearchCalls),
      deferredSearchCalls: number(gpu.deferredSearchCalls),
      streamedLeaves: number(gpu.streamedLeaves),
      streamFlushes: number(gpu.streamFlushes),
      evaluatorTimingMs: evaluatorTiming,
      searchTimingMs: gpu.searchTimingMs ?? null,
      rulesAndSearchMs: reportedElapsedMs !== null && evaluatorTotalMs !== null
        ? Math.max(0, reportedElapsedMs - evaluatorTotalMs)
        : null,
    } : null,
    build: {
      engineRevision: result.engineRevision ?? null,
      buildGitSha: result.buildGitSha ?? null,
      buildDirty: result.buildDirty ?? null,
    },
  };
}

function speedup(cpu, cuda) {
  const ratio = (left, right) => left !== null && right !== null && right > 0 ? left / right : null;
  return {
    elapsedCpuOverGpu: ratio(cpu.elapsedMs, cuda.elapsedMs),
    gamesPerSecondGpuOverCpu: ratio(cuda.gamesPerSecond, cpu.gamesPerSecond),
    meanDecisionLatencyCpuOverGpu: ratio(cpu.decisions.meanLatencyMs, cuda.decisions.meanLatencyMs),
  };
}

async function runBackend(players, backend, checkpoint, options, environment, discovery) {
  const result = await run(
    ARENA,
    arenaArgs(players, backend, checkpoint, options),
    { cwd: ENGINE_DIR, env: environment, capture: true, label: `${players}P ${backend} arena` },
  );
  const arena = parseJson(result.stdout, `${players}P ${backend} arena`);
  const checkpointResult = await checkpointData(checkpoint);
  validateRun(arena, checkpointResult, players, backend, options);
  return {
    metrics: metrics(arena, checkpointResult, result.elapsedMs, discovery),
    checkpoint: checkpointResult,
  };
}

async function main() {
  const options = readOptions(process.argv.slice(2));
  const revision = (await run("git", ["rev-parse", "HEAD"], { capture: true, label: "git revision" })).stdout.trim();
  const discovery = await discoverCuda();
  const environment = cudaEnvironment(discovery);
  if (options.build) {
    console.error("Building the CUDA-enabled release arena once…");
    await run(
      process.env.CARGO ?? "cargo",
      ["build", "--release", "-p", "colonist-catan-arena", "--features", "cuda-exact"],
      { cwd: ENGINE_DIR, env: environment, label: "CUDA-enabled arena build" },
    );
  }
  try {
    await access(ARENA);
  } catch {
    throw new Error(`${ARENA} is missing; the release arena was not built.`);
  }

  const stem = options.output.endsWith(".json") ? options.output.slice(0, -5) : options.output;
  const checkpointDirectory = `${stem}.checkpoints`;
  await mkdir(checkpointDirectory, { recursive: true });
  const lanes = [];
  for (const players of options.players) {
    const cpu = await runBackend(
      players,
      "cpu",
      resolve(checkpointDirectory, `${players}p-cpu.jsonl`),
      options,
      environment,
      discovery,
    );
    const cuda = await runBackend(
      players,
      "cuda",
      resolve(checkpointDirectory, `${players}p-cuda.jsonl`),
      options,
      environment,
      discovery,
    );
    const parity = compareCheckpoints(players, cpu.checkpoint, cuda.checkpoint);
    lanes.push({
      players,
      seed: options.seed,
      lineup: Array.from({ length: players }, () => "maxn"),
      cpu: { ...cpu.metrics, checkpoint: cpu.checkpoint.path },
      cuda: { ...cuda.metrics, checkpoint: cuda.checkpoint.path },
      parity,
      speedup: speedup(cpu.metrics, cuda.metrics),
    });
  }

  const report = {
    schemaVersion: 1,
    kind: "colonist-exact-gpu-arena-benchmark",
    generatedAt: new Date().toISOString(),
    profile: options.profile,
    build: { repositoryCommit: revision, builtOnce: options.build },
    evaluator: { kind: "exact-handcrafted", cpuOracle: "evaluate(state)", neural: false },
    cudaEnvironment: {
      discovery: "uv run --with torch --no-project",
      torchVersion: discovery.torchVersion,
      cudaVersion: discovery.cudaVersion,
      deviceName: discovery.deviceName,
      deviceCount: discovery.deviceCount,
      memoryFreeBytes: discovery.memoryFreeBytes,
      memoryTotalBytes: discovery.memoryTotalBytes,
      libraryDirs: discovery.libraryDirs,
      nvrtcLibraries: discovery.nvrtcLibraries,
      cudaRuntimeLibraries: discovery.cudaRuntimeLibraries,
    },
    configuration: {
      players: options.players,
      blocks: options.blocks,
      seed: options.seed,
      threads: options.threads,
      lineup: "all-maxn",
      ...CONFIG,
      playerTradesEnabled: false,
      maritimeTradesEnabled: true,
      validate: options.validate,
    },
    checkpointDirectory,
    parity: lanes.every((lane) => lane.parity.match),
    lanes,
  };
  await mkdir(dirname(options.output), { recursive: true });
  await writeFile(options.output, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ report: options.output, checkpointDirectory, parity: report.parity, lanes: lanes.map(({ players, speedup }) => ({ players, speedup })) }, null, 2));
}

main().catch((error) => {
  console.error(`Exact GPU arena benchmark failed: ${error.message}`);
  process.exitCode = 1;
});
