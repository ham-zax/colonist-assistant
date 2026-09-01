#!/usr/bin/env node

import { mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { spawn } from "node:child_process";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import process from "node:process";
import { build } from "esbuild";

const input = process.argv[2];
const rerun = process.argv.includes("--rerun");
const outputArgument = process.argv.find((value) => value.startsWith("--output="));
if (!input) {
  console.error("Usage: node scripts/replay-decisions.mjs <decision-traces.json>");
  process.exitCode = 2;
} else {
  if (rerun) {
    const root = resolve(import.meta.dirname, "..");
    const temporary = await mkdtemp(resolve(tmpdir(), "colonist-replay-"));
    const bundle = resolve(temporary, "replay-engine.mjs");
    const output =
      outputArgument?.slice("--output=".length) ??
      resolve(root, "benchmark-results", "decision-replay-latest.json");
    await mkdir(dirname(output), { recursive: true });
    await build({
      entryPoints: [resolve(root, "scripts/replay-engine.ts")],
      outfile: bundle,
      bundle: true,
      platform: "node",
      format: "esm",
      target: "node22",
      external: [],
    });
    await new Promise((resolvePromise, reject) => {
      const child = spawn(
        process.execPath,
        [
          bundle,
          resolve(input),
          output,
          resolve(root, "src/generated/wasm/colonist_search_bg.wasm"),
        ],
        { stdio: "inherit" },
      );
      child.once("error", reject);
      child.once("exit", (code) => {
        if (code === 0) resolvePromise();
        else reject(new Error(`replay runner exited with code ${code}`));
      });
    });
    await rm(temporary, { recursive: true, force: true });
    const replay = JSON.parse(await readFile(output, "utf8"));
    console.log(
      JSON.stringify(
        {
          replay: output,
          ...(replay.task14Gate ? { task14Gate: replay.task14Gate } : {}),
          ...(replay.task15Disposition
            ? { task15Disposition: replay.task15Disposition }
            : {}),
        },
        null,
        2,
      ),
    );
    process.exit(replay.task14Gate?.passed === false ? 1 : 0);
  }
  const parsed = JSON.parse(await readFile(input, "utf8"));
  const traces = Array.isArray(parsed) ? parsed : parsed.traces;
  if (!Array.isArray(traces)) {
    throw new Error("Expected an array or { traces: [...] }");
  }

  const rustAuthoritySources = new Set([
    "exact-mandatory",
    "tactical-proven",
    "deep-maxn",
    "exact-family",
    "safety-override",
  ]);
  const sourceCounts = new Map();
  const failures = [];
  const overrides = [];
  const latencies = [];
  for (const trace of traces) {
    const source = trace.finalActionSource ?? "missing";
    sourceCounts.set(source, (sourceCounts.get(source) ?? 0) + 1);
    if (Number.isFinite(trace.deepLatencyMs)) latencies.push(trace.deepLatencyMs);
    if (trace.executedBeforeDeepResult) {
      failures.push({
        turn: trace.turn,
        phase: trace.phase,
        problem: "executed-before-deep",
      });
    }
    if (trace.executionSucceeded === false) {
      failures.push({
        turn: trace.turn,
        phase: trace.phase,
        problem: trace.executionFailureReason ?? "execution-failed",
      });
    }
    if (
      trace.deepChosenAction &&
      trace.finalAction &&
      !rustAuthoritySources.has(source) &&
      source !== "mandatory" &&
      source !== "incoming-trade-evaluator"
    ) {
      overrides.push({
        turn: trace.turn,
        phase: trace.phase,
        source,
        deep: trace.deepChosenAction,
        final: trace.finalAction,
      });
    }
  }

  latencies.sort((left, right) => left - right);
  const percentile = (value) =>
    latencies.length
      ? latencies[Math.min(
          latencies.length - 1,
          Math.floor((latencies.length - 1) * value),
        )]
      : undefined;

  console.log(
    JSON.stringify(
      {
        traces: traces.length,
        deepAuthority:
          traces.length === 0
            ? 0
            : traces.filter(
                (trace) =>
                  rustAuthoritySources.has(trace.finalActionSource) ||
                  trace.finalActionSource === "mandatory",
              ).length / traces.length,
        fallbackRate:
          traces.length === 0
            ? 0
            : traces.filter((trace) =>
                String(trace.finalActionSource).includes("fallback"),
              ).length / traces.length,
        sources: Object.fromEntries(sourceCounts),
        latencyMs: {
          p50: percentile(0.5),
          p95: percentile(0.95),
          maximum: latencies.at(-1),
        },
        failures,
        overrides,
      },
      null,
      2,
    ),
  );
}
