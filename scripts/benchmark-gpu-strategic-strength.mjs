#!/usr/bin/env node

import { execFileSync, spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import process from "node:process";
import { build } from "esbuild";

const args = process.argv.slice(2);
const value = (name) => {
  const prefix = `--${name}=`;
  return args.find((argument) => argument.startsWith(prefix))?.slice(prefix.length);
};

if (args.includes("--help") || args.includes("-h")) {
  console.log(
    "benchmark-gpu-strategic-strength\n" +
      "\nRuns the Wave 4 D68/D71 native-GPU strategic-strength regression.\n" +
      "The 96-step run is the acceptance contract; other horizons are diagnostic only.\n" +
      "\nOptions:\n" +
      "  --binary=PATH       Native colonist-assistant-gpu binary (required)\n" +
      "  --horizon=N         Rollout horizon (default: 96)\n" +
      "  --fixture=PATH      Fixture override\n" +
      "  --output=PATH       Also write the JSON report to PATH\n" +
      "  --report-only       Report a failed 96-step contract without exiting nonzero\n",
  );
  process.exit(0);
}

const binary = value("binary");
if (!binary) {
  console.error("Missing --binary=PATH. Use --help for usage.");
  process.exit(2);
}

const horizon = Number(value("horizon") ?? "96");
if (!Number.isInteger(horizon) || horizon <= 0) {
  console.error(`Invalid --horizon=${value("horizon")}`);
  process.exit(2);
}

const root = resolve(import.meta.dirname, "..");
const fixture = resolve(
  value("fixture") ?? resolve(root, "tests/fixtures/gpu-strategic-strength-d68.json"),
);
const temporary = await mkdtemp(resolve(tmpdir(), "colonist-gpu-strength-"));
const bundle = resolve(temporary, "gpu-strategic-strength-engine.mjs");

try {
  await build({
    entryPoints: [resolve(root, "scripts/gpu-strategic-strength-engine.ts")],
    outfile: bundle,
    bundle: true,
    platform: "node",
    format: "esm",
    target: "node22",
    external: [],
  });
  const gitHead = execFileSync("git", ["rev-parse", "HEAD"], {
    cwd: root,
    encoding: "utf8",
  }).trim();
  const engineArgs = [
    bundle,
    `--repo-root=${root}`,
    `--fixture=${fixture}`,
    `--binary=${resolve(binary)}`,
    `--horizon=${horizon}`,
    ...(args.includes("--report-only") ? ["--report-only"] : []),
    ...(value("output") ? [`--output=${resolve(value("output"))}`] : []),
  ];
  const exitCode = await new Promise((resolvePromise, reject) => {
    const child = spawn(process.execPath, engineArgs, {
      cwd: root,
      stdio: "inherit",
      env: {
        ...process.env,
        COLONIST_BENCHMARK_GIT_HEAD: gitHead,
      },
    });
    child.once("error", reject);
    child.once("exit", (code) => resolvePromise(code ?? 1));
  });
  process.exitCode = exitCode;
} finally {
  await rm(temporary, { recursive: true, force: true });
}
