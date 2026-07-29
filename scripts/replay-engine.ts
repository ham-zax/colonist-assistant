import { readFile, writeFile } from "node:fs/promises";

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
  replayState?: TrackerState;
  replayBoard?: BoardSnapshot;
  deepChosenAction?: unknown;
  finalAction?: unknown;
  finalActionSource?: string;
  executionSucceeded?: boolean;
  executedBeforeDeepResult?: boolean;
  engine?: string;
  runtime?: string;
  learnedModelVersion?: string;
  tradeModelVersion?: string;
  deepLatencyMs?: number;
}

const [input, output, wasmPath] = process.argv.slice(2);
if (!input || !output || !wasmPath) {
  throw new Error("replay-engine needs input, output, and WASM paths");
}
const bytes = await readFile(wasmPath);
await initWasm({ module_or_path: bytes });
const parsed = JSON.parse(await readFile(input, "utf8")) as
  | ReplayTrace[]
  | { traces: ReplayTrace[] };
const traces = Array.isArray(parsed) ? parsed : parsed.traces;
const configurations = [
  {
    name: "maxn-authoritative",
    mode: "maxn",
    depth: 4,
    branchCap: 12,
    maxNodes: 48_000,
    iterations: 1,
  },
  {
    name: "maxn-10x-wide",
    mode: "maxn",
    depth: 6,
    branchCap: 32,
    maxNodes: 250_000,
    iterations: 1,
  },
  {
    name: "puct-authoritative",
    mode: "puct",
    maxNodes: 60_000,
    iterations: 2_400,
  },
  {
    name: "puct-10x",
    mode: "puct",
    maxNodes: 250_000,
    iterations: 24_000,
  },
] as const;

const reports = [];
for (const trace of traces) {
  if (!trace.replayState || !trace.replayBoard || !trace.rootPlayer) continue;
  const built = buildDeepSearchRequest(
    trace.replayState,
    trace.replayBoard,
    trace.rootPlayer,
  );
  const configurationsForState = [];
  for (const configuration of configurations) {
    const request = {
      ...structuredClone(built.request),
      ...configuration,
    };
    const started = performance.now();
    const response = analyzeWasm(request) as WasmSearchResponse;
    configurationsForState.push({
      name: configuration.name,
      chosen: response.chosen,
      rootValue: response.rootValue,
      actions: response.actions.slice(0, 12),
      nodes: response.nodes,
      iterations: response.iterations,
      exactDecision: response.exactDecision,
      tacticalProven: response.tacticalProven,
      latencyMs: performance.now() - started,
    });
  }

  const oracleWorlds = built.request.state.worlds.slice(0, 8).map((world) => {
    const request = structuredClone(built.request);
    request.state.worlds = [{ ...world, weight: 1 }];
    request.mode = "maxn";
    request.depth = 6;
    request.branchCap = 32;
    request.maxNodes = 250_000;
    const response = analyzeWasm(request) as WasmSearchResponse;
    return {
      weight: world.weight,
      chosen: response.chosen,
      rootValue: response.rootValue,
      nodes: response.nodes,
    };
  });
  reports.push({
    stateHash: trace.stateHash,
    actualLivePolicy: {
      engine: trace.engine,
      runtime: trace.runtime,
      learnedModelVersion: trace.learnedModelVersion,
      tradeModelVersion: trace.tradeModelVersion,
      deepChosen: trace.deepChosenAction,
      finalAction: trace.finalAction,
      finalActionSource: trace.finalActionSource,
      executionSucceeded: trace.executionSucceeded,
      executedBeforeDeepResult: trace.executedBeforeDeepResult,
      latencyMs: trace.deepLatencyMs,
    },
    configurations: configurationsForState,
    completeInformationOracle: oracleWorlds,
  });
}
await writeFile(
  output,
  `${JSON.stringify(
    {
      schemaVersion: 1,
      traces: reports.length,
      reports,
    },
    null,
    2,
  )}\n`,
);
