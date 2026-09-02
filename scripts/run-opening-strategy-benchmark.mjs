#!/usr/bin/env node

import { spawn } from "node:child_process";
import { existsSync, createReadStream, createWriteStream } from "node:fs";
import { access, cp, mkdir, mkdtemp, readFile, rm, unlink, writeFile } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import { delimiter, join, resolve } from "node:path";
import process from "node:process";
import { pipeline } from "node:stream/promises";
import { createGzip } from "node:zlib";

const ROOT = resolve(import.meta.dirname, "..");
const ARENA_SOURCE = resolve(ROOT, "engine/crates/catan-arena");
const CORE_SOURCE = resolve(ROOT, "engine/crates/catan-core");
const SEARCH_SOURCE = resolve(ROOT, "engine/crates/catan-search");
const ANALYZER = resolve(ROOT, "scripts/analyze-opening-strategy.py");

const PROFILES = {
  fast: {
    iterations: 4,
    maxnNodes: 128,
    maxnDepth: 1,
    maxnBranch: 6,
    beliefParticles: 1,
    strategicParticles: 1,
    tradeNodes: 64,
    openingNodes: 1_000,
  },
  deep: {
    iterations: 10,
    maxnNodes: 1_000,
    maxnDepth: 2,
    maxnBranch: 8,
    beliefParticles: 4,
    strategicParticles: 4,
    tradeNodes: 500,
    openingNodes: 2_000,
  },
};

function parseArgs(args) {
  const options = {
    profile: "fast",
    blocks: 50,
    seed: 20_261_000,
    threads: 4,
    maxTurns: 160,
    output: resolve(ROOT, "benchmark-results/opening-strategy-latest"),
    keepTemp: false,
  };
  for (let index = 0; index < args.length; index += 1) {
    const flag = args[index];
    const value = () => {
      const next = args[++index];
      if (next === undefined) throw new Error(`${flag} requires a value`);
      return next;
    };
    switch (flag) {
      case "--profile": options.profile = value(); break;
      case "--blocks": options.blocks = Number(value()); break;
      case "--seed": options.seed = Number(value()); break;
      case "--threads": options.threads = Number(value()); break;
      case "--max-turns": options.maxTurns = Number(value()); break;
      case "--output": options.output = resolve(value()); break;
      case "--keep-temp": options.keepTemp = true; break;
      case "--help":
      case "-h":
        console.log(`Usage: node scripts/run-opening-strategy-benchmark.mjs [options]\n\n` +
          `Runs the frozen CUDA-assisted MaxN-vs-weighted opening/strategy cohort.\n` +
          `The script creates an instrumented arena copy in /tmp; production source is not patched.\n\n` +
          `  --profile fast|deep   Search profile (default: fast)\n` +
          `  --blocks N            Matched blocks; 4 games each (default: 50)\n` +
          `  --seed N              Board/chance seed base (default: 20261000)\n` +
          `  --threads N           Arena worker threads (default: 4)\n` +
          `  --max-turns N         Game cutoff (default: 160)\n` +
          `  --output PATH         Output directory\n` +
          `  --keep-temp           Preserve the temporary instrumented arena\n`);
        process.exit(0);
        break;
      default: throw new Error(`Unknown option: ${flag}`);
    }
  }
  if (!(options.profile in PROFILES)) throw new Error("--profile must be fast or deep");
  for (const [name, number] of Object.entries({ blocks: options.blocks, seed: options.seed, threads: options.threads, maxTurns: options.maxTurns })) {
    if (!Number.isSafeInteger(number) || number < 1) throw new Error(`${name} must be a positive integer`);
  }
  return options;
}

function run(command, args, { cwd = ROOT, env = process.env, capture = false } = {}) {
  return new Promise((resolvePromise, reject) => {
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
      child.stdout.on("data", (chunk) => { stdout += chunk; });
      child.stderr.on("data", (chunk) => { stderr += chunk; });
    }
    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (code === 0) resolvePromise({ stdout, stderr });
      else reject(new Error(`${command} exited ${code ?? signal}${stderr ? `\n${stderr}` : ""}`));
    });
  });
}

function replaceExactly(source, oldText, newText, label) {
  const first = source.indexOf(oldText);
  if (first < 0 || source.indexOf(oldText, first + oldText.length) >= 0) {
    throw new Error(`Arena source changed: expected one ${label} anchor`);
  }
  return source.replace(oldText, newText);
}

async function cargoExecutable() {
  if (process.env.CARGO) return process.env.CARGO;
  const local = join(homedir(), ".cargo/bin/cargo");
  try {
    await access(local);
    return local;
  } catch {
    return "cargo";
  }
}

function hasNvrtc(directory) {
  if (!directory) return false;
  return ["libnvrtc.so", "libnvrtc.so.13", "libnvrtc.so.12", "libnvrtc.so.11"]
    .some((name) => existsSync(join(directory, name)));
}

async function discoverCudaLibraryPath() {
  const current = (process.env.LD_LIBRARY_PATH ?? "").split(delimiter).filter(Boolean);
  const common = [
    "/usr/local/cuda/lib64",
    "/usr/lib/x86_64-linux-gnu",
    "/usr/lib/wsl/lib",
  ];
  let nvrtc = [...current, ...common].find(hasNvrtc);
  if (!nvrtc) {
    const cacheRoot = join(homedir(), ".cache/uv");
    if (existsSync(cacheRoot)) {
      const result = await run(
        "find",
        [cacheRoot, "-type", "f", "-name", "libnvrtc.so*", "-print", "-quit"],
        { capture: true },
      );
      const found = result.stdout.trim();
      if (found) nvrtc = resolve(found, "..");
    }
  }
  if (!nvrtc) {
    throw new Error("CUDA NVRTC was not found. Set LD_LIBRARY_PATH to the directory containing libnvrtc.so.");
  }
  return [...new Set([nvrtc, "/usr/lib/wsl/lib", ...current])].join(delimiter);
}

async function gzipFile(path) {
  const destination = `${path}.gz`;
  await pipeline(createReadStream(path), createGzip({ level: 9 }), createWriteStream(destination));
  await unlink(path);
  return destination;
}

async function gitMetadata() {
  const sha = (await run("git", ["rev-parse", "HEAD"], { capture: true })).stdout.trim();
  const status = (await run("git", ["status", "--porcelain"], { capture: true })).stdout;
  return { sha, dirty: status.trim().length > 0, status: status.trim().split("\n").filter(Boolean) };
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const profile = PROFILES[options.profile];
  const temporary = await mkdtemp(join(tmpdir(), "colonist-opening-strategy-arena-"));
  try {
    await cp(ARENA_SOURCE, temporary, { recursive: true });
    const mainPath = join(temporary, "src/main.rs");
    let source = await readFile(mainPath, "utf8");
    source = replaceExactly(
      source,
      `            if config\n                .lineup\n                .as_ref()\n                .is_none_or(|lineup| lineup.iter().any(|engine| *engine != Engine::MaxN))\n            {\n                eprintln!("--evaluator cuda currently requires an all-maxn --lineup");\n                std::process::exit(2);\n            }\n`,
      "",
      "CUDA all-MaxN restriction",
    );
    source = replaceExactly(
      source,
      `    actor: u8,\n    engines: Vec<&'static str>,\n    public_victory_points: Vec<u8>,\n`,
      `    actor: u8,\n    engines: Vec<&'static str>,\n    policy_profiles: Vec<[u8; 5]>,\n    public_victory_points: Vec<u8>,\n`,
      "trajectory policy profile field",
    );
    source = replaceExactly(source, `    development_hand: Vec<u8>,\n`, `    development_hand: Vec<u8>,\n    development_bought: Vec<u32>,\n    maritime_trades: Vec<u32>,\n`, "trajectory build counters");
    source = replaceExactly(source, `        engines: engines.iter().map(|engine| engine.as_str()).collect(),\n`, `        engines: engines.iter().map(|engine| engine.as_str()).collect(),\n        policy_profiles: state.players[..players].iter().map(|player| player.policy_profile).collect(),\n`, "trajectory policy profile capture");
    source = replaceExactly(source, `        development_hand: state.players[..players]\n            .iter()\n            .map(|player| player.development.iter().copied().sum())\n            .collect(),\n`, `        development_hand: state.players[..players]\n            .iter()\n            .map(|player| player.development.iter().copied().sum())\n            .collect(),\n        development_bought: metrics.development_bought[..players].to_vec(),\n        maritime_trades: metrics.maritime_trades[..players].to_vec(),\n`, "trajectory build counter capture");
    await writeFile(mainPath, source);

    const cargoToml = `[package]\nname = "colonist-catan-arena"\nversion = "0.1.0"\nedition = "2024"\nlicense = "MIT"\nrust-version = "1.90"\n\n[[bin]]\nname = "colonist-arena"\npath = "src/main.rs"\n\n[features]\ncuda-exact = ["colonist-catan-search/cuda-exact"]\n\n[dependencies]\ncolonist-catan-core = { path = "${CORE_SOURCE}" }\ncolonist-catan-search = { path = "${SEARCH_SOURCE}", features = ["benchmark-profile"] }\nserde = { version = "1", features = ["derive"] }\nserde_json = "1"\n\n[profile.release]\nopt-level = 3\n`;
    await writeFile(join(temporary, "Cargo.toml"), cargoToml);

    const cargo = await cargoExecutable();
    await run(cargo, ["build", "--release", "--features", "cuda-exact", "--bin", "colonist-arena"], { cwd: temporary });

    await mkdir(options.output, { recursive: true });
    const checkpointPath = join(options.output, "checkpoints.jsonl");
    const trajectoryPath = join(options.output, "trajectories.jsonl");
    const summaryPath = join(options.output, "summary.json");
    const arenaArgs = [
      "--players", "4",
      "--blocks", String(options.blocks),
      "--seed", String(options.seed),
      "--candidate", "maxn",
      "--baseline", "weighted",
      "--evaluator", "cuda",
      "--iterations", String(profile.iterations),
      "--maxn-nodes", String(profile.maxnNodes),
      "--maxn-time-ms", "0",
      "--maxn-depth", String(profile.maxnDepth),
      "--maxn-branch", String(profile.maxnBranch),
      "--belief-particles", String(profile.beliefParticles),
      "--strategic-particles", String(profile.strategicParticles),
      "--trade-response-nodes", String(profile.tradeNodes),
      "--trade-response-time-ms", "0",
      "--opening-nodes", String(profile.openingNodes),
      "--opening-time-ms", "0",
      "--max-turns", String(options.maxTurns),
      "--threads", String(options.threads),
      "--checkpoint-output", checkpointPath,
      "--trajectory-output", trajectoryPath,
      "--json",
    ];
    const env = { ...process.env, LD_LIBRARY_PATH: await discoverCudaLibraryPath() };
    const binary = join(temporary, "target/release/colonist-arena");
    const result = await run(binary, arenaArgs, { cwd: ROOT, env, capture: true });
    if (result.stderr.trim()) process.stderr.write(result.stderr);
    await writeFile(summaryPath, result.stdout.trim() + "\n");

    const [checkpointGzip, trajectoryGzip] = await Promise.all([
      gzipFile(checkpointPath),
      gzipFile(trajectoryPath),
    ]);
    const analysis = await run("python3", [ANALYZER, checkpointGzip, trajectoryGzip], { capture: true });
    await writeFile(join(options.output, "analysis.txt"), analysis.stdout);

    const metadata = {
      schemaVersion: 1,
      kind: "colonist-opening-strategy-benchmark",
      createdAt: new Date().toISOString(),
      repository: await gitMetadata(),
      temporaryArenaPatch: {
        mixedCudaLineup: true,
        trajectoryPolicyProfiles: true,
        trajectoryDevelopmentBought: true,
        trajectoryMaritimeTrades: true,
      },
      options,
      profile,
      command: [binary, ...arenaArgs],
      outputs: {
        summary: "summary.json",
        checkpoints: "checkpoints.jsonl.gz",
        trajectories: "trajectories.jsonl.gz",
        analysis: "analysis.txt",
      },
    };
    await writeFile(join(options.output, "metadata.json"), JSON.stringify(metadata, null, 2) + "\n");
    process.stdout.write(analysis.stdout);
    console.log(`\nSaved benchmark to ${options.output}`);
    if (options.keepTemp) console.log(`Temporary arena preserved at ${temporary}`);
  } finally {
    if (!options.keepTemp) await rm(temporary, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
