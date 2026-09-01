import { readFile, writeFile } from "node:fs/promises";

import type { BoardSnapshot } from "../src/core/placement";
import type { TrackerState } from "../src/core/types";
import { buildDeepSearchRequest } from "../src/worker/deep-search";
import initWasm, {
  analyze as analyzeWasm,
  type WasmAction,
  type WasmSearchResponse,
} from "../src/generated/wasm/colonist_search.js";

interface ReplayTrace {
  stateHash: string;
  fixtureId?: string;
  tags?: string[];
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
const particleLimits = [24, 48, 96] as const;
const searchConfiguration = {
  mode: "maxn",
  depth: 4,
  branchCap: 8,
  maxNodes: 4_000,
  timeBudgetMs: 350,
  iterations: 1,
} as const;
const regretThreshold = 0.02;
const task15Configurations = [
  {
    name: "live",
    mode: "maxn",
    depth: 4,
    branchCap: 8,
    maxNodes: 4_000,
    timeBudgetMs: 350,
    iterations: 1,
  },
  {
    name: "reference-medium",
    mode: "maxn",
    depth: 6,
    branchCap: 32,
    maxNodes: 64_000,
    timeBudgetMs: 10_000,
    iterations: 1,
  },
  {
    name: "reference-max",
    mode: "maxn",
    depth: 6,
    branchCap: 32,
    maxNodes: 250_000,
    timeBudgetMs: 10_000,
    iterations: 1,
  },
] as const;

const actionKey = (action: WasmAction | undefined): string =>
  action
    ? JSON.stringify([
        action.kind,
        action.first,
        action.second,
        action.player,
        action.resource,
        action.otherResource,
        action.cards,
        action.receiveCards,
        action.accept,
      ])
    : "none";

const actionFamily = (action: WasmAction | undefined): string | undefined =>
  action?.kind;

const actionValue = (
  response: WasmSearchResponse,
  action: WasmAction | undefined,
  root: number,
): number | undefined => {
  if (!action) return undefined;
  const key = actionKey(action);
  return response.actions.find((candidate) => actionKey(candidate.action) === key)
    ?.value[root];
};

const rankedRoot = (
  response: WasmSearchResponse,
  action: WasmAction | undefined,
) => {
  if (!action) return undefined;
  const key = actionKey(action);
  return response.rootProvenance.rankedRoots.find(
    (candidate) => actionKey(candidate.action) === key,
  );
};

const retainedRoot = (
  response: WasmSearchResponse,
  action: WasmAction | undefined,
) => {
  if (!action) return undefined;
  const key = actionKey(action);
  return response.rootProvenance.retainedRoots.find(
    (candidate) => actionKey(candidate.action) === key,
  );
};

const reports = [];
for (const trace of traces) {
  if (!trace.replayState || !trace.replayBoard || !trace.rootPlayer) continue;
  const runs = [] as Array<{
    particleLimit: number;
    root: number;
    response: WasmSearchResponse;
    latencyMs: number;
    constructedParticles: number;
    requestSeed: number;
  }>;
  for (const particleLimit of particleLimits) {
    const built = buildDeepSearchRequest(
      trace.replayState,
      trace.replayBoard,
      trace.rootPlayer,
      {},
      true,
      particleLimit,
    );
    const request = {
      ...structuredClone(built.request),
      ...searchConfiguration,
    };
    const started = performance.now();
    const response = analyzeWasm(request) as WasmSearchResponse;
    runs.push({
      particleLimit,
      root: built.root,
      response,
      latencyMs: performance.now() - started,
      constructedParticles: request.state.worlds.length,
      requestSeed: request.seed,
    });
  }

  const live = runs[0]!;
  const medium = runs[1]!;
  const large = runs[2]!;
  const regretAgainst = (reference: (typeof runs)[number]): number | undefined => {
    const referenceChosen = actionValue(
      reference.response,
      reference.response.chosen,
      reference.root,
    );
    const liveChosen = actionValue(
      reference.response,
      live.response.chosen,
      reference.root,
    );
    if (referenceChosen === undefined || liveChosen === undefined) return undefined;
    return Math.max(0, referenceChosen - liveChosen);
  };
  const regret48 = regretAgainst(medium);
  const regret96 = regretAgainst(large);
  const liveFamily = actionFamily(live.response.chosen);
  const familyUnsafe =
    liveFamily !== actionFamily(medium.response.chosen) &&
    liveFamily !== actionFamily(large.response.chosen);
  const regretUnsafe =
    regret48 !== undefined &&
    regret96 !== undefined &&
    regret48 > regretThreshold &&
    regret96 > regretThreshold;
  const gatePassed = !familyUnsafe && !regretUnsafe;
  const calibrationRuns = [
    {
      configuration: task15Configurations[0],
      root: live.root,
      response: live.response,
      latencyMs: live.latencyMs,
      constructedParticles: live.constructedParticles,
    },
  ];
  for (const configuration of task15Configurations.slice(1)) {
    const built = buildDeepSearchRequest(
      trace.replayState,
      trace.replayBoard,
      trace.rootPlayer,
      {},
      true,
      24,
    );
    const request = {
      ...structuredClone(built.request),
      ...configuration,
    };
    const started = performance.now();
    const response = analyzeWasm(request) as WasmSearchResponse;
    calibrationRuns.push({
      configuration,
      root: built.root,
      response,
      latencyMs: performance.now() - started,
      constructedParticles: request.state.worlds.length,
    });
  }
  const referenceMedium = calibrationRuns[1]!;
  const referenceMax = calibrationRuns[2]!;
  const stableReferenceAction =
    actionKey(referenceMedium.response.chosen) ===
    actionKey(referenceMax.response.chosen)
      ? referenceMedium.response.chosen
      : undefined;
  const stableReferenceLiveRank = rankedRoot(
    live.response,
    stableReferenceAction,
  );
  const stableReferenceLiveAdmission = retainedRoot(
    live.response,
    stableReferenceAction,
  );
  const outsideLiveTopEight =
    stableReferenceLiveRank !== undefined && stableReferenceLiveRank.rank > 8;
  const materialF9Omission =
    stableReferenceAction !== undefined &&
    outsideLiveTopEight &&
    stableReferenceLiveAdmission === undefined;
  const rootRegretAgainst = (
    reference: (typeof calibrationRuns)[number],
  ): number | undefined => {
    const referenceChosen = actionValue(
      reference.response,
      reference.response.chosen,
      reference.root,
    );
    const liveChosen = actionValue(
      reference.response,
      live.response.chosen,
      reference.root,
    );
    if (referenceChosen === undefined || liveChosen === undefined) return undefined;
    return Math.max(0, referenceChosen - liveChosen);
  };
  const seedStability = trace.tags?.includes("seed-sensitivity")
    ? (() => {
        const seedRuns = [];
        for (let seedIndex = 0; seedIndex < 8; seedIndex += 1) {
          const board = structuredClone(trace.replayBoard!);
          board.gameKey = `${board.gameKey ?? trace.fixtureId ?? trace.stateHash}:seed-${seedIndex}`;
          const built = buildDeepSearchRequest(
            trace.replayState!,
            board,
            trace.rootPlayer!,
            {},
            true,
            24,
          );
          const request = {
            ...structuredClone(built.request),
            ...searchConfiguration,
          };
          const response = analyzeWasm(request) as WasmSearchResponse;
          seedRuns.push({
            seedIndex,
            seed: request.seed,
            chosen: response.chosen,
            chosenFamily: actionFamily(response.chosen),
          });
        }
        const chosenKeys = new Set(seedRuns.map((run) => actionKey(run.chosen)));
        return {
          stable: chosenKeys.size === 1,
          distinctChosenActions: chosenKeys.size,
          runs: seedRuns,
        };
      })()
    : undefined;

  reports.push({
    fixtureId: trace.fixtureId ?? trace.stateHash,
    stateHash: trace.stateHash,
    tags: trace.tags ?? [],
    sourceWorldCount: trace.replayState.worlds.length,
    semanticSeed: live.requestSeed,
    task14Gate: {
      passed: gatePassed,
      familyUnsafe,
      regretUnsafe,
      regretThreshold,
      regret48,
      regret96,
    },
    ...(seedStability ? { seedStability } : {}),
    task15Calibration: {
      actionFamilyAgreement:
        new Set(
          calibrationRuns.map((run) => actionFamily(run.response.chosen)),
        ).size === 1,
      stableReferenceAction,
      stableReferenceFamily: actionFamily(stableReferenceAction),
      stableReferenceLiveRank: stableReferenceLiveRank?.rank,
      stableReferenceLivePrior: stableReferenceLiveRank?.prior,
      stableReferenceAdmittedLive: stableReferenceLiveAdmission !== undefined,
      outsideLiveTopEight,
      materialF9Omission,
      liveChosenRegretMedium: rootRegretAgainst(referenceMedium),
      liveChosenRegretMax: rootRegretAgainst(referenceMax),
      runs: calibrationRuns.map((run) => {
        const chosenRank = rankedRoot(run.response, run.response.chosen);
        const chosenAdmission = retainedRoot(run.response, run.response.chosen);
        return {
          name: run.configuration.name,
          depth: run.configuration.depth,
          branchCap: run.configuration.branchCap,
          maxNodes: run.configuration.maxNodes,
          timeBudgetMs: run.configuration.timeBudgetMs,
          constructedParticles: run.constructedParticles,
          chosen: run.response.chosen,
          chosenFamily: actionFamily(run.response.chosen),
          chosenPreTruncationRank: chosenRank?.rank,
          chosenPrior: chosenRank?.prior,
          chosenAdmitted: chosenAdmission !== undefined,
          chosenBackedUpValue: actionValue(
            run.response,
            run.response.chosen,
            run.root,
          ),
          nodes: run.response.nodes,
          deadlineReached: run.response.deadlineReached,
          latencyMs: run.latencyMs,
        };
      }),
    },
    particleComparisons: runs.map((run) => ({
      particleLimit: run.particleLimit,
      constructedParticles: run.constructedParticles,
      wasmParticles: run.response.wasmParticles,
      rustPosteriorParticles: run.response.rustPosteriorParticles,
      rustSearchParticles: run.response.rustSearchParticles,
      chosen: run.response.chosen,
      chosenFamily: actionFamily(run.response.chosen),
      chosenValue: actionValue(run.response, run.response.chosen, run.root),
      authority: run.response.authority,
      rootValue: run.response.rootValue,
      rootProvenance: run.response.rootProvenance,
      nodes: run.response.nodes,
      deadlineReached: run.response.deadlineReached,
      latencyMs: run.latencyMs,
    })),
  });
}
const failedFixtures = reports
  .filter((report) => !report.task14Gate.passed)
  .map((report) => report.fixtureId);
const materialF9Fixtures = reports
  .filter((report) => report.task15Calibration.materialF9Omission)
  .map((report) => report.fixtureId);
await writeFile(
  output,
  `${JSON.stringify(
    {
      schemaVersion: 2,
      searchConfiguration,
      particleLimits,
      task14Gate: {
        passed: failedFixtures.length === 0,
        regretThreshold,
        failedFixtures,
      },
      task15Configurations,
      task15Disposition: {
        materialOmissionFound: materialF9Fixtures.length > 0,
        materialF9Fixtures,
        rootWidth8Retained: materialF9Fixtures.length === 0,
      },
      traces: reports.length,
      reports,
    },
    null,
    2,
  )}\n`,
);
