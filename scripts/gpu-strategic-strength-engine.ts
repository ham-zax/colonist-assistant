import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { spawn } from "node:child_process";
import { resolve } from "node:path";

import type { BoardSnapshot, DevelopmentCardVector } from "../src/core/placement";
import { RESOURCE_ORDER, emptyResources, type ResourceVector } from "../src/core/resources";
import type { PlayerMeta, TrackerState } from "../src/core/types";
import { buildDeepSearchRequest } from "../src/worker/deep-search";
import type {
  WasmAction,
  WasmActionStatistics,
  WasmRankedRoot,
  WasmRetainedRoot,
  WasmSearchResponse,
} from "../src/generated/wasm/colonist_search.js";

interface FixturePlayer {
  devCards: Array<{ boughtOnTurn: number }>;
  playedDevCards: DevelopmentCardVector;
  policyPosterior: PlayerMeta["opponentModel"]["policyPosterior"];
}

interface D68Fixture {
  schema: string;
  suite: string;
  fixtureId: string;
  description: string;
  sourceEvidence: Record<string, unknown>;
  rootPlayer: string;
  playerTradesEnabled: boolean;
  tracker: {
    worlds: TrackerState["worlds"];
    playerOrder: string[];
    eventCount: number;
    currentTurn: TrackerState["currentTurn"];
    players: Record<string, FixturePlayer>;
    recentEvents: TrackerState["recentEvents"];
  };
  board: BoardSnapshot;
  expected: {
    turn: number;
    lastRoll: number;
    ownHand: number[];
    bank: number[];
    publicVictoryPoints: number[];
    robberHex: string;
    ownSettlements: string[];
    ownRoads: string[];
    sourceResourceWorlds: number;
    strata: number;
    mergedParticles: number;
    mergedStratumMultiplicities: number[];
    effectiveSampleSize: number;
    semanticSeed: number;
    hiddenDevelopment: {
      player: string;
      totalCards: number;
      mixture: Record<string, number>;
    };
    strongRoad: string;
    weakRoad: string;
    immediateSettlement: string;
    effort: {
      decisionTimeMs: number;
      tacticalMaxDepth: number;
      tacticalNodeBudget: number;
      rootCap: number;
      rolloutBudget: number;
      rolloutSteps: number;
    };
  };
  historicalRecorded: Record<string, unknown>;
}

interface NativeHello {
  id: number;
  runtime?: string;
  protocolVersion?: number;
  stateSchemaVersion?: number;
  engineRevision?: string;
  device?: unknown;
  error?: string;
}

interface NativeAnalyze {
  id: number;
  response?: WasmSearchResponse;
  error?: string;
}

interface NativeVersions {
  protocolVersion: number;
  stateSchemaVersion: number;
}

interface RunArgs {
  repoRoot: string;
  fixturePath: string;
  binaryPath: string;
  horizon: number;
  reportOnly: boolean;
  outputPath?: string;
}

const DEVELOPMENT_REQUEST_ORDER = [
  "knight",
  "victory-point",
  "road-building",
  "year-of-plenty",
  "monopoly",
] as const;

const assertCondition = (condition: unknown, message: string): asserts condition => {
  if (!condition) throw new Error(message);
};

const parseArgs = (): RunArgs => {
  const args = process.argv.slice(2);
  const value = (name: string): string | undefined => {
    const prefix = `--${name}=`;
    return args.find((argument) => argument.startsWith(prefix))?.slice(prefix.length);
  };
  const repoRoot = value("repo-root");
  const fixturePath = value("fixture");
  const binaryPath = value("binary");
  assertCondition(repoRoot, "missing --repo-root");
  assertCondition(fixturePath, "missing --fixture");
  assertCondition(binaryPath, "missing --binary");
  const horizon = Number(value("horizon") ?? "96");
  assertCondition(Number.isInteger(horizon) && horizon > 0, `invalid --horizon=${horizon}`);
  return {
    repoRoot,
    fixturePath,
    binaryPath,
    horizon,
    reportOnly: args.includes("--report-only"),
    ...(value("output") ? { outputPath: value("output") } : {}),
  };
};

const vector = (resources: Partial<ResourceVector> | undefined): number[] =>
  RESOURCE_ORDER.map((resource) => resources?.[resource] ?? 0);

const zeroDevelopment = (): Record<string, number> => ({
  knight: 0,
  monopoly: 0,
  "road-building": 0,
  "year-of-plenty": 0,
  "victory-point": 0,
  unknown: 0,
});

const hydrateTracker = (fixture: D68Fixture): TrackerState => {
  const players = Object.fromEntries(
    fixture.tracker.playerOrder.map((name) => {
      const source = fixture.tracker.players[name];
      assertCondition(source, `fixture is missing tracker player ${name}`);
      const played = {
        ...zeroDevelopment(),
        ...source.playedDevCards,
      } as PlayerMeta["playedDevCards"];
      const player: PlayerMeta = {
        name,
        color: "",
        devCards: structuredClone(source.devCards),
        playedDevCards: played,
        builds: {
          road: 0,
          settlement: 0,
          city: 0,
          development: 0,
        },
        resourcesGained: emptyResources(),
        productionGained: emptyResources(),
        resourcesSpent: emptyResources(),
        opponentModel: {
          tradeAccepts: 0,
          tradeRejects: 0,
          offersMade: 0,
          countersMade: 0,
          policyPosterior: structuredClone(source.policyPosterior),
        },
      };
      return [name, player];
    }),
  );
  return {
    worlds: structuredClone(fixture.tracker.worlds),
    players,
    playerOrder: [...fixture.tracker.playerOrder],
    eventCount: fixture.tracker.eventCount,
    currentTurn: structuredClone(fixture.tracker.currentTurn),
    diceRolls: {},
    uncertaintyEvents: 0,
    possibilitiesTruncated: false,
    warnings: [],
    recentEvents: structuredClone(fixture.tracker.recentEvents),
    pendingTradeBehaviour: {},
    tradeEmbargoes: {},
  };
};

const readNativeVersions = (repoRoot: string): NativeVersions => {
  const source = readFileSync(resolve(repoRoot, "src/background/native-gpu.ts"), "utf8");
  const protocol = source.match(/const NATIVE_GPU_PROTOCOL_VERSION = (\d+);/);
  const state = source.match(/const NATIVE_GPU_STATE_SCHEMA_VERSION = (\d+);/);
  assertCondition(protocol && state, "could not read native GPU protocol/state versions");
  return {
    protocolVersion: Number(protocol[1]),
    stateSchemaVersion: Number(state[1]),
  };
};

const sha256 = (path: string): string =>
  createHash("sha256").update(readFileSync(path)).digest("hex");

class NativeHost {
  private readonly child;
  private buffer = Buffer.alloc(0);
  private readonly pending: Array<{
    resolve: (value: NativeHello | NativeAnalyze) => void;
    reject: (error: Error) => void;
  }> = [];
  private stderr = "";

  constructor(binaryPath: string) {
    this.child = spawn(binaryPath, [], { stdio: ["pipe", "pipe", "pipe"] });
    this.child.stderr.on("data", (chunk: Buffer) => {
      this.stderr += chunk.toString("utf8");
    });
    this.child.stdout.on("data", (chunk: Buffer) => {
      this.buffer = Buffer.concat([this.buffer, chunk]);
      this.drain();
    });
    this.child.once("error", (error) => this.failAll(error));
    this.child.once("exit", (code) => {
      if (this.pending.length) {
        this.failAll(new Error(`native GPU host exited with code ${code}: ${this.stderr}`));
      }
    });
  }

  private drain(): void {
    while (this.buffer.length >= 4) {
      const size = this.buffer.readUInt32LE(0);
      if (this.buffer.length < size + 4) return;
      const message = JSON.parse(
        this.buffer.subarray(4, size + 4).toString("utf8"),
      ) as NativeHello | NativeAnalyze;
      this.buffer = this.buffer.subarray(size + 4);
      const pending = this.pending.shift();
      pending?.resolve(message);
    }
  }

  private failAll(error: Error): void {
    while (this.pending.length) this.pending.shift()?.reject(error);
  }

  send(value: unknown): Promise<NativeHello | NativeAnalyze> {
    return new Promise((resolvePromise, reject) => {
      const payload = Buffer.from(JSON.stringify(value), "utf8");
      const header = Buffer.alloc(4);
      header.writeUInt32LE(payload.length, 0);
      this.pending.push({ resolve: resolvePromise, reject });
      this.child.stdin.write(Buffer.concat([header, payload]), (error) => {
        if (error) reject(error);
      });
    });
  }

  async close(): Promise<string> {
    this.child.stdin.end();
    if (this.child.exitCode === null) {
      await new Promise<void>((resolvePromise) => {
        this.child.once("exit", () => resolvePromise());
      });
    }
    return this.stderr;
  }
}

const applyNativeStrengthProfile = (
  request: ReturnType<typeof buildDeepSearchRequest>["request"],
  fixture: D68Fixture,
  horizon: number,
): void => {
  const effort = fixture.expected.effort;
  request.timeBudgetMs = Math.max(effort.decisionTimeMs, request.timeBudgetMs);
  request.effort = {
    ...request.effort,
    decisionTimeMs: Math.max(effort.decisionTimeMs, request.effort.decisionTimeMs),
    tactical: {
      maxDepth: Math.max(effort.tacticalMaxDepth, request.effort.tactical.maxDepth),
      nodeBudget: Math.max(effort.tacticalNodeBudget, request.effort.tactical.nodeBudget),
    },
    gpu: {
      rootCap: Math.max(effort.rootCap, request.effort.gpu.rootCap),
      rolloutBudget: Math.max(effort.rolloutBudget, request.effort.gpu.rolloutBudget),
      rolloutSteps:
        horizon === effort.rolloutSteps
          ? Math.max(effort.rolloutSteps, request.effort.gpu.rolloutSteps)
          : horizon,
    },
  };
  if (horizon !== effort.rolloutSteps) request.rolloutActions = horizon;
};

const effectiveSampleSize = (worlds: Array<{ weight: number }>): number => {
  const total = worlds.reduce((sum, world) => sum + Math.max(0, world.weight), 0);
  const squared = worlds.reduce((sum, world) => {
    const weight = Math.max(0, world.weight) / Math.max(Number.EPSILON, total);
    return sum + weight * weight;
  }, 0);
  return squared > 0 ? 1 / squared : 0;
};

const sortedNumbers = (values: number[]): number[] => [...values].sort((a, b) => a - b);
const near = (left: number, right: number, tolerance = 1e-6): boolean =>
  Math.abs(left - right) <= tolerance;

const semanticAction = (action: WasmAction | undefined, board: BoardSnapshot): string => {
  if (!action) return "none";
  if (action.kind === "build-road" || action.kind === "place-road") {
    return board.edges[action.first ?? -1]?.id ?? `${action.kind}:${action.first}`;
  }
  if (action.kind === "build-settlement" || action.kind === "place-settlement") {
    return board.vertices[action.first ?? -1]?.id ?? `${action.kind}:${action.first}`;
  }
  if (action.kind === "build-city") {
    return board.vertices[action.first ?? -1]?.id ?? `${action.kind}:${action.first}`;
  }
  return action.kind;
};

const actionIs = (action: WasmAction, kind: string, index: number): boolean =>
  action.kind === kind && action.first === index;

const rootSummary = (
  response: WasmSearchResponse,
  kind: "build-road" | "build-settlement",
  index: number,
  root: number,
  board: BoardSnapshot,
) => {
  const action = response.actions.find((row: WasmActionStatistics) =>
    actionIs(row.action, kind, index),
  );
  const ranked = response.rootProvenance.rankedRoots.find((row: WasmRankedRoot) =>
    actionIs(row.action, kind, index),
  );
  const retained = response.rootProvenance.retainedRoots.find((row: WasmRetainedRoot) =>
    actionIs(row.action, kind, index),
  );
  return {
    id: kind === "build-road" ? board.edges[index]?.id : board.vertices[index]?.id,
    admitted: Boolean(retained),
    preRank: ranked?.rank ?? retained?.preTruncationRank ?? null,
    prior: ranked?.prior ?? retained?.prior ?? action?.prior ?? null,
    finalRank: retained?.finalRank ?? null,
    weightedGpuResult: action
      ? {
          value: action.value[root],
          lowerConfidenceValue: action.lowerConfidenceValue[root],
          visits: action.visits,
          availability: action.availability,
          availabilityWeight: action.availabilityWeight,
          legalWeight: action.legalWeight,
        }
      : null,
    terminalOutcome: retained?.terminalOutcome ?? null,
    netTerminalOutcomeRate: retained?.terminalOutcome ?? null,
    terminalLowerBound: retained?.terminalLowerBound ?? null,
    terminalUpperBound: retained?.terminalUpperBound ?? null,
    victoryMargin: retained?.victoryMargin ?? null,
    victoryMarginLowerBound: retained?.victoryMarginLowerBound ?? null,
    victoryMarginUpperBound: retained?.victoryMarginUpperBound ?? null,
    meanTurn: retained?.meanTurn ?? null,
  };
};

const inspectSampling = (
  fixture: D68Fixture,
  request: ReturnType<typeof buildDeepSearchRequest>["request"],
) => {
  const worlds = request.state.worlds;
  const strata = fixture.expected.strata;
  const multiplicities = sortedNumbers(
    worlds.map((world) => Math.round(world.weight * strata)),
  );
  const ess = effectiveSampleSize(worlds);
  const hiddenPlayer = fixture.expected.hiddenDevelopment.player;
  const hiddenIndex = fixture.tracker.playerOrder.indexOf(hiddenPlayer);
  assertCondition(hiddenIndex >= 0, `unknown hidden-development player ${hiddenPlayer}`);
  const mixture = Object.fromEntries(
    DEVELOPMENT_REQUEST_ORDER.map((card, cardIndex) => {
      const mass = worlds.reduce(
        (sum, world) => sum + (world.development[hiddenIndex]?.[cardIndex] ? world.weight : 0),
        0,
      );
      return [card, mass];
    }),
  );
  return { worlds, multiplicities, ess, mixture };
};

const assertFixtureContract = (
  fixture: D68Fixture,
  board: BoardSnapshot,
  request: ReturnType<typeof buildDeepSearchRequest>["request"],
  root: number,
  sampling: ReturnType<typeof inspectSampling>,
): void => {
  const expected = fixture.expected;
  assertCondition(board.turn === expected.turn, `turn mismatch: ${board.turn}`);
  assertCondition(board.lastRoll === expected.lastRoll, `last roll mismatch: ${board.lastRoll}`);
  assertCondition(JSON.stringify(vector(board.ownHand)) === JSON.stringify(expected.ownHand), "own hand mismatch");
  assertCondition(JSON.stringify(vector(board.bank)) === JSON.stringify(expected.bank), "bank mismatch");
  const publicVp = fixture.tracker.playerOrder.map(
    (player) => board.players?.[player]?.visiblePoints ?? 0,
  );
  assertCondition(JSON.stringify(publicVp) === JSON.stringify(expected.publicVictoryPoints), "public VP mismatch");
  const robber = board.hexes.find((hex) => hex.blocked)?.id;
  assertCondition(robber === expected.robberHex, `robber mismatch: ${robber}`);
  const settlements = board.vertices
    .filter(
      (vertex) =>
        vertex.building?.player === fixture.rootPlayer &&
        vertex.building.kind === "settlement",
    )
    .map((vertex) => vertex.id)
    .sort();
  assertCondition(
    JSON.stringify(settlements) === JSON.stringify([...expected.ownSettlements].sort()),
    "own settlements mismatch",
  );
  const roads = board.edges
    .filter((edge) => edge.player === fixture.rootPlayer)
    .map((edge) => edge.id)
    .sort();
  assertCondition(JSON.stringify(roads) === JSON.stringify([...expected.ownRoads].sort()), "own roads mismatch");
  assertCondition(fixture.tracker.worlds.length === expected.sourceResourceWorlds, "source resource-world count mismatch");
  assertCondition(request.seed === expected.semanticSeed, `semantic seed mismatch: ${request.seed}`);
  assertCondition(sampling.worlds.length === expected.mergedParticles, `merged particle mismatch: ${sampling.worlds.length}`);
  assertCondition(
    JSON.stringify(sampling.multiplicities) === JSON.stringify(expected.mergedStratumMultiplicities),
    `24-strata merge mismatch: ${JSON.stringify(sampling.multiplicities)}`,
  );
  assertCondition(
    sampling.multiplicities.reduce((sum, count) => sum + count, 0) === expected.strata,
    "merged stratum multiplicities do not sum to 24",
  );
  assertCondition(near(sampling.ess, expected.effectiveSampleSize, 1e-9), `ESS mismatch: ${sampling.ess}`);
  for (const [card, mass] of Object.entries(expected.hiddenDevelopment.mixture)) {
    assertCondition(near(sampling.mixture[card] ?? 0, mass, 1e-9), `hidden development mixture mismatch for ${card}`);
  }
  assertCondition(
    Boolean(request.state.domesticTradeDisabled & (1 << root)),
    "D68 fixture must preserve the recorded local player-trades-disabled mode",
  );
};

const requestNative = async (
  host: NativeHost,
  id: number,
  request: ReturnType<typeof buildDeepSearchRequest>["request"],
): Promise<WasmSearchResponse> => {
  const message = (await host.send({ type: "analyze", id, request })) as NativeAnalyze;
  if (message.error) throw new Error(message.error);
  assertCondition(message.response, "native GPU host returned no response");
  return message.response;
};

const applyStrongRoadCounterfactual = (
  request: ReturnType<typeof buildDeepSearchRequest>["request"],
  root: number,
  roadIndex: number,
): ReturnType<typeof buildDeepSearchRequest>["request"] => {
  const next = structuredClone(request);
  assertCondition(next.state.roads[roadIndex] === -1, "strong road is not empty in D68 base state");
  next.state.roads[roadIndex] = root;
  next.state.players[root]!.roadsLeft = Math.max(0, next.state.players[root]!.roadsLeft - 1);
  const roadCost = [1, 1, 0, 0, 0] as const;
  next.state.players[root]!.resources = next.state.players[root]!.resources.map(
    (count, index) => count - roadCost[index]!,
  ) as [number, number, number, number, number];
  next.state.bank = next.state.bank.map(
    (count, index) => count + roadCost[index]!,
  ) as [number, number, number, number, number];
  for (const world of next.state.worlds) {
    world.hands[root] = world.hands[root]!.map(
      (count, index) => count - roadCost[index]!,
    ) as [number, number, number, number, number];
    world.bank = world.bank.map(
      (count, index) => count + roadCost[index]!,
    ) as [number, number, number, number, number];
  }
  return next;
};

const main = async (): Promise<void> => {
  const args = parseArgs();
  const fixture = JSON.parse(readFileSync(args.fixturePath, "utf8")) as D68Fixture;
  assertCondition(fixture.schema === "colonist-gpu-strategic-strength/1", `unsupported fixture schema ${fixture.schema}`);
  const state = hydrateTracker(fixture);
  const board = structuredClone(fixture.board);
  const built = buildDeepSearchRequest(
    state,
    board,
    fixture.rootPlayer,
    {},
    fixture.playerTradesEnabled,
    fixture.expected.strata,
  );
  applyNativeStrengthProfile(built.request, fixture, args.horizon);
  const sampling = inspectSampling(fixture, built.request);
  assertFixtureContract(fixture, board, built.request, built.root, sampling);

  const strongRoadIndex = board.edges.findIndex((edge) => edge.id === fixture.expected.strongRoad);
  const weakRoadIndex = board.edges.findIndex((edge) => edge.id === fixture.expected.weakRoad);
  const settlementIndex = board.vertices.findIndex((vertex) => vertex.id === fixture.expected.immediateSettlement);
  assertCondition(strongRoadIndex >= 0, "strong road is absent from board topology");
  assertCondition(weakRoadIndex >= 0, "weak road is absent from board topology");
  assertCondition(settlementIndex >= 0, "immediate settlement is absent from board topology");

  const versions = readNativeVersions(args.repoRoot);
  const host = new NativeHost(args.binaryPath);
  const hello = (await host.send({
    type: "hello",
    id: 1,
    protocolVersion: versions.protocolVersion,
    stateSchemaVersion: versions.stateSchemaVersion,
  })) as NativeHello;
  if (hello.error) throw new Error(hello.error);
  assertCondition(hello.runtime === "gpu-native", `unexpected native runtime ${hello.runtime}`);

  const response = await requestNative(host, 2, built.request);
  const selectedRoot = semanticAction(response.chosen, board);
  const strong = rootSummary(response, "build-road", strongRoadIndex, built.root, board);
  const weak = rootSummary(response, "build-road", weakRoadIndex, built.root, board);
  assertCondition(strong.admitted, `${fixture.expected.strongRoad} was not admitted`);
  assertCondition(weak.admitted, `${fixture.expected.weakRoad} was not admitted`);

  let counterfactual: Record<string, unknown> | null = null;
  let counterfactualPass: boolean | null = null;
  if (args.horizon === fixture.expected.effort.rolloutSteps) {
    const counterRequest = applyStrongRoadCounterfactual(
      built.request,
      built.root,
      strongRoadIndex,
    );
    const counterResponse = await requestNative(host, 3, counterRequest);
    const selectedSettlement = semanticAction(counterResponse.chosen, board);
    const settlement = rootSummary(
      counterResponse,
      "build-settlement",
      settlementIndex,
      built.root,
      board,
    );
    counterfactualPass =
      settlement.admitted &&
      (settlement.weightedGpuResult?.legalWeight ?? 0) > 0.999999 &&
      selectedSettlement === fixture.expected.immediateSettlement;
    counterfactual = {
      appliedRoad: fixture.expected.strongRoad,
      resultingOwnResources: counterRequest.state.players[built.root]!.resources,
      resultingBank: counterRequest.state.bank,
      selectedRoot: selectedSettlement,
      expectedSettlement: fixture.expected.immediateSettlement,
      settlement,
      rollouts: counterResponse.rollouts,
      rolloutHorizon: counterResponse.effectiveEffort.gpu.rolloutSteps,
      particleCount: counterResponse.rustSearchParticles,
      effectiveSampleSize: counterResponse.effectiveParticleCount,
      pass: counterfactualPass,
    };
  }

  const stderr = await host.close();
  const primaryPass = selectedRoot === fixture.expected.strongRoad;
  const acceptanceApplicable = args.horizon === fixture.expected.effort.rolloutSteps;
  const contractPass = acceptanceApplicable
    ? primaryPass && counterfactualPass === true
    : null;
  const knownWave4InvestigationTrigger = !acceptanceApplicable && !primaryPass;
  const report = {
    kind: "gpu-strategic-strength-regression",
    suite: fixture.suite,
    fixtureId: fixture.fixtureId,
    sourceEvidence: fixture.sourceEvidence,
    historicalRecorded: fixture.historicalRecorded,
    buildIdentity: {
      gitHead: process.env.COLONIST_BENCHMARK_GIT_HEAD ?? null,
      binaryPath: args.binaryPath,
      binarySha256: sha256(args.binaryPath),
      kernelPtxSha256: sha256(resolve(args.repoRoot, "engine/crates/catan-search/src/cuda/sim.ptx")),
      kernelSourceSha256: sha256(resolve(args.repoRoot, "engine/crates/catan-search/src/cuda/sim.cu")),
      nativeHello: hello,
    },
    beliefConstruction: {
      sourceResourceWorlds: fixture.tracker.worlds.length,
      requestedStrata: fixture.expected.strata,
      mergedParticles: sampling.worlds.length,
      mergedStratumMultiplicities: sampling.multiplicities,
      effectiveSampleSize: sampling.ess,
      semanticSeed: built.request.seed,
      hiddenDevelopment: {
        player: fixture.expected.hiddenDevelopment.player,
        mixture: sampling.mixture,
      },
      localPlayerTradesEnabled: fixture.playerTradesEnabled,
    },
    effort: {
      requested: fixture.expected.effort,
      effective: response.effectiveEffort,
      rolloutHorizon: response.effectiveEffort.gpu.rolloutSteps,
      rolloutCount: response.rollouts,
      particleCount: response.rustSearchParticles,
      effectiveSampleSize: response.effectiveParticleCount,
    },
    d68: {
      selectedRoot,
      expectedStrongRoot: fixture.expected.strongRoad,
      historicalWeakRoot: fixture.expected.weakRoad,
      strongRoad: strong,
      weakRoad: weak,
      primaryPass,
    },
    immediateCounterfactual: counterfactual,
    acceptance: {
      applicable: acceptanceApplicable,
      contractPass,
      primaryPass,
      counterfactualPass,
      knownWave4InvestigationTrigger,
      note: acceptanceApplicable
        ? "Production acceptance requires the strong eastern road at 96 steps and the immediate eastern settlement continuation."
        : "Non-96 horizons are diagnostic only; a weak-road selection is a Wave 4 investigation trigger and does not weaken the 96-step contract.",
    },
    nativeStderr: stderr || null,
  };
  const output = `${JSON.stringify(report, null, 2)}\n`;
  if (args.outputPath) writeFileSync(args.outputPath, output);
  process.stdout.write(output);
  if (acceptanceApplicable && contractPass === false && !args.reportOnly) {
    process.exitCode = 1;
  }
};

await main();
