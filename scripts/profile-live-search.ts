#!/usr/bin/env node

import { readFile } from "node:fs/promises";

import type { BoardSnapshot } from "../src/core/placement";
import type { TrackerState } from "../src/core/types";
import { buildDeepSearchRequest } from "../src/worker/deep-search";
import initWasm, {
  analyze as analyzeWasm,
  type WasmSearchResponse,
} from "../src/generated/wasm/colonist_search.js";

interface ReplayTrace {
  stateHash: string;
  rootPlayer?: string;
  phase?: string;
  deepLatencyMs?: number;
  deepChosenAction?: { kind?: string };
  replayState?: TrackerState;
  replayBoard?: BoardSnapshot;
}

const [tracePath, maximumArgument = "20", turnArgument] =
  process.argv.slice(2);
if (!tracePath) {
  throw new Error(
    "Usage: vite-node scripts/profile-live-search.ts <trace.json> [samples] [exact-turn|all]",
  );
}
const maximum = Math.max(1, Number.parseInt(maximumArgument, 10) || 20);
const includeAllPhases = turnArgument === "all";
const [requestedTurnText, requestedPhase] = (turnArgument ?? "").split(":");
const requestedTurn =
  turnArgument === undefined || includeAllPhases
    ? undefined
    : Number.parseInt(requestedTurnText, 10);
const wasmPath = new URL(
  "../src/generated/wasm/colonist_search_bg.wasm",
  import.meta.url,
);
await initWasm({ module_or_path: await readFile(wasmPath) });

const parsed = JSON.parse(await readFile(tracePath, "utf8")) as
  | ReplayTrace[]
  | { traces: ReplayTrace[] };
const allTraces = Array.isArray(parsed) ? parsed : parsed.traces;
const strategic = allTraces
  .filter(
    (trace) =>
      trace.replayState &&
      trace.rootPlayer &&
      (includeAllPhases
        ? Boolean(trace.replayBoard)
        : requestedTurn === undefined
        ? trace.replayBoard?.isMyTurn &&
          (trace.replayBoard.action ?? "none") === "none"
        : trace.replayBoard?.turn === requestedTurn &&
          (!requestedPhase ||
            (trace.replayBoard.action ?? "none") === requestedPhase)),
  )
  .sort(
    (left, right) =>
      (right.deepLatencyMs ?? 0) - (left.deepLatencyMs ?? 0),
  );
const stride = Math.max(1, Math.floor(strategic.length / maximum));
const traces = strategic
  .filter((_, index) => index % stride === 0)
  .slice(0, maximum);

const configurations = [
  { name: "live-depth4-4k", depth: 4, maxNodes: 4_000, tacticalNodes: 900 },
  { name: "live-depth4-3k", depth: 4, maxNodes: 3_000, tacticalNodes: 760 },
  { name: "live-depth4-2400", depth: 4, maxNodes: 2_400, tacticalNodes: 640 },
  { name: "live-depth4-2k", depth: 4, maxNodes: 2_000, tacticalNodes: 560 },
] as const;

const results = new Map<
  string,
  Array<{
    stateHash: string;
    turn?: number;
    phase?: string;
    latencyMs: number;
    action: unknown;
    topActions: unknown[];
    nodes: number;
    particles: number;
  }>
>();
for (const configuration of configurations) {
  results.set(configuration.name, []);
}

for (const trace of traces) {
  const built = buildDeepSearchRequest(
    trace.replayState!,
    trace.replayBoard!,
    trace.rootPlayer!,
  );
  for (const configuration of configurations) {
    const request = {
      ...structuredClone(built.request),
      mode: "maxn",
      depth: configuration.depth,
      maxNodes: configuration.maxNodes,
      tacticalNodes: configuration.tacticalNodes,
    };
    const startedAt = performance.now();
    const response = analyzeWasm(request) as WasmSearchResponse;
    results.get(configuration.name)!.push({
      stateHash: trace.stateHash,
      turn: trace.replayBoard?.turn,
      phase: trace.replayBoard?.action,
      latencyMs: performance.now() - startedAt,
      action: response.chosen,
      topActions: response.actions.slice(0, 5).map((candidate) => ({
        action: candidate.action,
        value: candidate.value,
        lowerConfidenceValue: candidate.lowerConfidenceValue,
        visits: candidate.visits,
      })),
      nodes: response.nodes,
      particles: response.particles,
    });
  }
}

const percentile = (values: number[], fraction: number): number | undefined => {
  if (!values.length) return undefined;
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[
    Math.min(
      sorted.length - 1,
      Math.floor((sorted.length - 1) * fraction),
    )
  ];
};
const reference = results.get(configurations[0].name) ?? [];
const report = configurations.map((configuration) => {
  const entries = results.get(configuration.name) ?? [];
  return {
    ...configuration,
    samples: entries.length,
    latencyMs: {
      p50: percentile(entries.map((entry) => entry.latencyMs), 0.5),
      p95: percentile(entries.map((entry) => entry.latencyMs), 0.95),
      maximum: Math.max(0, ...entries.map((entry) => entry.latencyMs)),
    },
    underOneSecond:
      entries.filter((entry) => entry.latencyMs < 1_000).length /
      Math.max(1, entries.length),
    actionAgreementWithCurrent:
      entries.filter(
        (entry, index) =>
          JSON.stringify(entry.action) ===
          JSON.stringify(reference[index]?.action),
      ).length / Math.max(1, entries.length),
    averageNodes:
      entries.reduce((sum, entry) => sum + entry.nodes, 0) /
      Math.max(1, entries.length),
    averageParticles:
      entries.reduce((sum, entry) => sum + entry.particles, 0) /
      Math.max(1, entries.length),
    slowest: [...entries]
      .sort((left, right) => right.latencyMs - left.latencyMs)
      .slice(0, 2)
      .map(({ stateHash: _stateHash, ...entry }) => entry),
    disagreements: entries.flatMap((entry, index) =>
      JSON.stringify(entry.action) === JSON.stringify(reference[index]?.action)
        ? []
        : [
            {
              turn: entry.turn,
              selected: entry.action,
              reference: reference[index]?.action,
            },
          ],
    ),
  };
});

console.log(
  JSON.stringify(
    {
      tracePath,
      availableStrategicStates: strategic.length,
      sampledStates: traces.length,
      sampledInputs: traces.map((trace) => {
        const built = buildDeepSearchRequest(
          trace.replayState!,
          trace.replayBoard!,
          trace.rootPlayer!,
        );
        return {
          turn: trace.replayBoard?.turn,
          boardAction: trace.replayBoard?.action,
          hasRolled: trace.replayBoard?.hasRolled,
          requestPhase: built.request.state.phase,
          recordedLatencyMs: trace.deepLatencyMs,
          recordedAction: trace.deepChosenAction?.kind,
          playableDevelopment:
            trace.replayBoard?.ownDevelopmentCards?.playable,
        };
      }),
      configurations: report,
    },
    null,
    2,
  ),
);
