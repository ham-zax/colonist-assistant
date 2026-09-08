import { readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";

import type { BoardSnapshot } from "../src/core/placement";
import { RESOURCE_ORDER } from "../src/core/resources";
import type {
  DeepSearchStrategyPolicy,
  CanonicalEngineRequest,
  DecisionSearchConstraints,
} from "../src/core/engine";
import type { PublicStochasticInput } from "../src/core/dice-history";
import type { TrackerState } from "../src/core/types";
import { buildDeepSearchRequest } from "../src/worker/deep-search";
import initWasm, {
  analyze as analyzeWasm,
  engine_version as engineVersion,
  type WasmAction,
  type WasmSearchResponse,
} from "../src/generated/wasm/colonist_search.js";

interface ReplayTrace {
  engineRevision?: string;
  canonicalRequest?: CanonicalEngineRequest;
  stateHash: string;
  fixtureId?: string;
  tags?: string[];
  rootPlayer?: string;
  replayState?: TrackerState;
  replayBoard?: BoardSnapshot;
  replayRequestContext?: {
    representation: "reconstructed-request-v1";
    playerTradesEnabled: boolean;
    stochastic: PublicStochasticInput;
    searchConstraints: DecisionSearchConstraints;
    strategyPolicy?: DeepSearchStrategyPolicy;
  };
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
const wasmSha256 = createHash("sha256").update(bytes).digest("hex");
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
  tacticalDepth: 14,
  tacticalNodes: 900,
  evidenceEscalationMs: 0,
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
    tacticalDepth: 14,
    tacticalNodes: 900,
    evidenceEscalationMs: 0,
  },
  {
    name: "reference-medium",
    mode: "maxn",
    depth: 6,
    branchCap: 32,
    maxNodes: 64_000,
    timeBudgetMs: 10_000,
    iterations: 1,
    tacticalDepth: 14,
    tacticalNodes: 900,
    evidenceEscalationMs: 0,
  },
  {
    name: "reference-max",
    mode: "maxn",
    depth: 6,
    branchCap: 32,
    maxNodes: 250_000,
    timeBudgetMs: 10_000,
    iterations: 1,
    tacticalDepth: 14,
    tacticalNodes: 900,
    evidenceEscalationMs: 0,
  },
] as const;

type EngineRequest = ReturnType<typeof buildDeepSearchRequest>["request"];
type SearchConfiguration = {
  mode: "maxn";
  depth: number;
  branchCap: number;
  maxNodes: number;
  timeBudgetMs: number;
  iterations: number;
  tacticalDepth: number;
  tacticalNodes: number;
  evidenceEscalationMs: number;
};

const configureRequest = (
  base: EngineRequest,
  configuration: SearchConfiguration,
): EngineRequest => {
  const request = structuredClone(base);
  request.mode = configuration.mode;
  request.depth = configuration.depth;
  request.branchCap = configuration.branchCap;
  request.maxNodes = configuration.maxNodes;
  request.timeBudgetMs = configuration.timeBudgetMs;
  request.iterations = configuration.iterations;
  request.tacticalDepth = configuration.tacticalDepth;
  request.tacticalNodes = configuration.tacticalNodes;
  request.effort = {
    decisionTimeMs: configuration.timeBudgetMs,
    tactical: {
      maxDepth: configuration.tacticalDepth,
      nodeBudget: configuration.tacticalNodes,
    },
    cpu: {
      maxDepth: configuration.depth,
      rootCap: configuration.branchCap,
      nodesPerDepthWave: configuration.maxNodes,
      evidenceEscalationMs: configuration.evidenceEscalationMs,
    },
    gpu: {
      rootCap: Math.min(24, Math.max(2, configuration.branchCap)),
      rolloutBudget: Math.min(50_000, Math.max(16, configuration.iterations)),
      rolloutSteps: Math.min(160, Math.max(24, request.rolloutActions ?? 96)),
    },
  };
  return request;
};

const assertEffectiveEffort = (
  response: WasmSearchResponse,
  request: EngineRequest,
  label: string,
): void => {
  const expected = request.effort;
  if (!expected) throw new Error(`${label}: request is missing authoritative effort`);
  const actual = response.effectiveEffort;
  const normalizedExpected = {
    decisionTimeMs: expected.decisionTimeMs,
    tactical: expected.tactical,
    cpu: {
      ...expected.cpu,
      evidenceEscalationMs: expected.cpu.evidenceEscalationMs ?? 0,
    },
    gpu: expected.gpu,
  };
  const normalizedActual = {
    decisionTimeMs: actual.decisionTimeMs,
    tactical: actual.tactical,
    cpu: {
      ...actual.cpu,
      evidenceEscalationMs: actual.cpu.evidenceEscalationMs ?? 0,
    },
    gpu: actual.gpu,
  };
  if (JSON.stringify(normalizedActual) !== JSON.stringify(normalizedExpected)) {
    throw new Error(
      `${label}: effective effort mismatch: requested=${JSON.stringify(normalizedExpected)} returned=${JSON.stringify(normalizedActual)}`,
    );
  }
};

const buildReplayRequest = (
  trace: ReplayTrace,
  particleLimit: number,
) => {
  const context = trace.replayRequestContext;
  const built = buildDeepSearchRequest(
    trace.replayState!,
    trace.replayBoard!,
    trace.rootPlayer!,
    context?.searchConstraints ?? {},
    context?.playerTradesEnabled ?? true,
    particleLimit,
    context?.stochastic,
    context?.strategyPolicy,
  );
  return { built };

};

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

const semanticActionKey = (
  action: unknown,
  board: BoardSnapshot,
  players: readonly string[],
): string | undefined => {
  if (!action || typeof action !== "object") return undefined;
  const value = action as Record<string, unknown>;
  if (typeof value.kind !== "string") return undefined;
  const kind = value.kind;
  const numericFirst = typeof value.first === "number" ? value.first : undefined;
  const targetId =
    typeof value.targetId === "string"
      ? value.targetId
      : numericFirst === undefined
        ? undefined
        : kind === "place-road" || kind === "build-road" || kind === "play-road-building"
          ? board.edges[numericFirst]?.id
          : kind === "move-robber" || kind === "play-knight"
            ? board.hexes[numericFirst]?.id
            : kind === "place-settlement" || kind === "build-settlement" || kind === "build-city"
              ? board.vertices[numericFirst]?.id
              : undefined;
  const numericSecond = typeof value.second === "number" ? value.second : undefined;
  const secondTargetId =
    typeof value.secondTargetId === "string"
      ? value.secondTargetId
      : numericSecond === undefined
        ? undefined
        : board.edges[numericSecond]?.id;
  const player =
    typeof value.player === "string"
      ? value.player
      : typeof value.player === "number"
        ? players[value.player]
        : undefined;
  const resource =
    typeof value.resource === "string"
      ? value.resource
      : typeof value.resource === "number"
        ? RESOURCE_ORDER[value.resource]
        : undefined;
  const otherResource =
    typeof value.otherResource === "string"
      ? value.otherResource
      : typeof value.otherResource === "number"
        ? RESOURCE_ORDER[value.otherResource]
        : undefined;
  const ratio =
    typeof value.ratio === "number"
      ? value.ratio
      : kind === "maritime-trade"
        ? numericFirst
        : undefined;
  return JSON.stringify([
    kind,
    targetId,
    secondTargetId,
    player,
    resource,
    otherResource,
    ratio,
    value.cards,
    value.receiveCards,
    value.recipients,
    value.accept,
  ]);
};

const actionValueBySemanticKey = (
  response: WasmSearchResponse,
  action: unknown,
  root: number,
  board: BoardSnapshot,
  players: readonly string[],
): number | undefined => {
  const key = semanticActionKey(action, board, players);
  if (!key) return undefined;
  return response.actions.find(
    (candidate) => semanticActionKey(candidate.action, board, players) === key,
  )?.value[root];
};

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

const canonicalReplays = [];
const reports = [];
for (const trace of traces) {
  if (trace.canonicalRequest) {
    const captured = trace.canonicalRequest;
    if (captured.representation !== "canonical-engine-request-v1" ||
        captured.algorithm !== "maxn" || !captured.request ||
        !Array.isArray(captured.players) || !Number.isInteger(captured.root) ||
        captured.root < 0 || captured.root >= captured.players.length) {
      throw new Error(`${trace.stateHash}: unsupported canonical request contract`);
    }
    const request = structuredClone(captured.request) as EngineRequest;
    const reproduced = analyzeWasm(request) as WasmSearchResponse;
    assertEffectiveEffort(reproduced, request, "canonical-reproduction");
    const sameArtifact = captured.wasmSha256 === wasmSha256 &&
      captured.engineRevision === engineVersion();
    // The reference experiment starts from identical particles and rules; only
    // its explicit effort differs. Never call that experiment production replay.
    const referenceRequest = configureRequest(request, task15Configurations[1]);
    const reference = analyzeWasm(referenceRequest) as WasmSearchResponse;
    assertEffectiveEffort(reference, referenceRequest, "canonical-reference-medium");
    const historicalValue = trace.replayBoard ? actionValueBySemanticKey(
      reference, trace.finalAction, captured.root, trace.replayBoard, captured.players,
    ) : undefined;
    const referenceValue = actionValue(reference, reference.chosen, captured.root);
    canonicalReplays.push({
      stateHash: trace.stateHash,
      fidelity: sameArtifact ? "exact-request-and-artifact" : "exact-request-current-artifact",
      limitation: sameArtifact
        ? "Identical request and artifact; wall-clock scheduling can still change completed work."
        : "Request preserved; source artifact differs or was not recorded. This is a current-engine counterfactual.",
      sourceWasmSha256: captured.wasmSha256 ?? null,
      replayWasmSha256: wasmSha256,
      effectiveEffort: reproduced.effectiveEffort,
      historicalExecuted: trace.executionSucceeded ? trace.finalAction : null,
      historicalRecommended: trace.finalAction,
      historicalSearchChosen: trace.deepChosenAction,
      reproducedProduction: reproduced.chosen,
      referenceSearch: reference.chosen,
      historicalReferenceScoreDifferenceProxy: historicalValue !== undefined && referenceValue !== undefined
        ? Math.max(0, referenceValue - historicalValue) : null,
      historicalReferenceScoreLimitation: historicalValue === undefined
        ? "Historical action cannot be mapped to the reference action table." : null,
      nodes: reproduced.nodes, depth: reproduced.deepestDecisionDepth,
      deadlineReached: reproduced.deadlineReached,
    });
    continue;
  }
  if (!trace.replayState || !trace.replayBoard || !trace.rootPlayer) continue;
  const replayFidelity = "approximate-legacy";
  const replayLimitation = "No canonical request: tracker worlds/history may be truncated and production profile adjustments are not reproduced.";
  const runs = [] as Array<{
    particleLimit: number;
    root: number;
    response: WasmSearchResponse;
    latencyMs: number;
    constructedParticles: number;
    requestSeed: number;
    players: string[];
  }>;
  for (const particleLimit of particleLimits) {
    const { built } = buildReplayRequest(trace, particleLimit);
    const request = configureRequest(built.request, searchConfiguration);
    const started = performance.now();
    const response = analyzeWasm(request) as WasmSearchResponse;
    assertEffectiveEffort(response, request, `particle-${particleLimit}`);
    runs.push({
      particleLimit,
      root: built.root,
      response,
      latencyMs: performance.now() - started,
      constructedParticles: request.state.worlds.length,
      requestSeed: request.seed,
      players: built.players,
    });
  }

  const live = runs[0]!;
  const medium = runs[1]!;
  const large = runs[2]!;
  const reproducedScoreDifferenceAgainst = (
    reference: (typeof runs)[number],
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
  const scoreDifference48 = reproducedScoreDifferenceAgainst(medium);
  const scoreDifference96 = reproducedScoreDifferenceAgainst(large);
  const liveFamily = actionFamily(live.response.chosen);
  const familyUnsafe =
    liveFamily !== actionFamily(medium.response.chosen) &&
    liveFamily !== actionFamily(large.response.chosen);
  const scoreDifferenceUnsafe =
    scoreDifference48 !== undefined &&
    scoreDifference96 !== undefined &&
    scoreDifference48 > regretThreshold &&
    scoreDifference96 > regretThreshold;
  const gatePassed = !familyUnsafe && !scoreDifferenceUnsafe;
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
    const { built } = buildReplayRequest(trace, 24);
    const request = configureRequest(built.request, configuration);
    const started = performance.now();
    const response = analyzeWasm(request) as WasmSearchResponse;
    assertEffectiveEffort(response, request, configuration.name);
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
  const referenceBestValue = actionValue(
    referenceMedium.response,
    referenceMedium.response.chosen,
    referenceMedium.root,
  );
  const historicalExecutedReferenceValue = actionValueBySemanticKey(
    referenceMedium.response,
    trace.finalAction,
    referenceMedium.root,
    trace.replayBoard,
    live.players,
  );
  const historicalReferenceScoreDifference =
    referenceBestValue === undefined || historicalExecutedReferenceValue === undefined
      ? undefined
      : Math.max(0, referenceBestValue - historicalExecutedReferenceValue);
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
  const reproducedScoreDifferenceProxyAgainst = (
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
          const context = trace.replayRequestContext;
          const built = buildDeepSearchRequest(
            trace.replayState!,
            board,
            trace.rootPlayer!,
            context?.searchConstraints ?? {},
            context?.playerTradesEnabled ?? true,
            24,
            context?.stochastic,
            context?.strategyPolicy,
          );
          const request = configureRequest(built.request, searchConfiguration);
          const response = analyzeWasm(request) as WasmSearchResponse;
          assertEffectiveEffort(response, request, `seed-${seedIndex}`);
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
    replayFidelity,
    ...(replayLimitation ? { replayLimitation } : {}),
    actions: {
      historicalExecuted: trace.finalAction,
      historicalSearchChosen: trace.deepChosenAction,
      reconstructedDiagnostic: live.response.chosen,
      referenceSearch: referenceMedium.response.chosen,
      historicalActionPresentInReference:
        historicalExecutedReferenceValue !== undefined,
      ...(historicalReferenceScoreDifference !== undefined
        ? {
            historicalReferenceScoreDifferenceProxy:
              historicalReferenceScoreDifference,
          }
        : {
            historicalReferenceScoreDifferenceProxy: null,
            historicalReferenceScoreLimitation:
              "historical executed action is absent from the reference action table or is not an engine-comparable action",
          }),
    },
    task14Gate: {
      passed: gatePassed,
      familyUnsafe,
      scoreDifferenceUnsafe,
      scoreDifferenceProxyThreshold: regretThreshold,
      scoreDifference48,
      scoreDifference96,
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
      reproducedScoreDifferenceProxyMedium:
        reproducedScoreDifferenceProxyAgainst(referenceMedium),
      reproducedScoreDifferenceProxyMax:
        reproducedScoreDifferenceProxyAgainst(referenceMax),
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
          effectiveEffort: run.response.effectiveEffort,
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
      schemaVersion: 4,
      engineRevision: engineVersion(),
      scoreDifferenceSemantics:
        "Reference-search score differences are diagnostic proxies, not optimal regret or proof that the reference action wins.",
      searchConfiguration,
      particleLimits,
      task14Gate: {
        passed: failedFixtures.length === 0,
        scoreDifferenceProxyThreshold: regretThreshold,
        failedFixtures,
      },
      task15Configurations,
      task15Disposition: {
        materialOmissionFound: materialF9Fixtures.length > 0,
        materialF9Fixtures,
        rootWidth8Retained: materialF9Fixtures.length === 0,
      },
      traces: reports.length,
      faithfulTraces: canonicalReplays.filter((report) => report.fidelity === "exact-request-and-artifact").length,
      canonicalReplays,
      approximateLegacyTraces: reports.filter(
        (report) => report.replayFidelity === "approximate-legacy",
      ).length,
      reports,
    },
    null,
    2,
  )}\n`,
);
