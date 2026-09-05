import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

import initWasm, {
  analyze,
  inspect_stochastic,
  type WasmReferenceControllerSnapshot,
  type WasmSearchResponse,
  type WasmStochasticInspectionResponse,
} from "../src/generated/wasm/colonist_search.js";
import {
  M0_FAIR_IID_2D6_V1,
  MREF_COLONIST_LINKED_2024_V1,
  PUBLIC_HISTORY_BELIEF_V1,
  appendPublicDiceRoll,
  buildReferenceStochasticInput,
  createDiceHistoryState,
  observeLogCoverage,
} from "../src/core/dice-history.ts";
import { replayEvents } from "../src/core/tracker.ts";
import type { BoardSnapshot } from "../src/core/placement.ts";
import type { ResourceVector } from "../src/core/resources.ts";
import type { StoredEvent } from "../src/core/types.ts";
import { buildDeepSearchRequest } from "../src/worker/deep-search.ts";

const REQUIRED_SOURCE_SHA256 =
  "e542827c9aefb35f47db836c50455db4076bc8fabe17b1cd4d0b0ce8efe107a4";
const DEFAULT_SOURCE =
  "/mnt/c/Users/Hamza/Downloads/colonist-evidence-hand2325-1-2026-09-04T10-41-53-871Z (1).txt";
const ROOT = process.env.COLONIST_REPO_ROOT
  ? path.resolve(process.env.COLONIST_REPO_ROOT)
  : path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const DEFAULT_OUTPUT = path.join(ROOT, "tests/fixtures/balanced-dice-hand2325.json");

const STATES = {
  D29: {
    turn: 27,
    expectedRolls: 24,
    belief: "B16",
    expectedSeed: 4_215_863_924,
    sourceWorlds: 12,
    wasmParticles: 16,
    effectiveParticles: 13.7143,
    hand: [3, 0, 2, 1, 2],
    phase: "main",
    robber: "h:1,1",
    expectedController: {
      cardsLeft: 12,
      recentTotals: [8, 11, 6, 10, 7],
      sevenCounts: [2, 1],
      sevenStreakOwner: 1,
      sevenStreakCount: 1,
    },
    m0Regression: {
      action: "3:1 Lumber -> Grain",
      authority: "deep-maxn",
      evidenceEscalation: "none",
    },
  },
  D31: {
    turn: 29,
    expectedRolls: 26,
    belief: "B18",
    expectedSeed: 872_288_473,
    sourceWorlds: 15,
    wasmParticles: 22,
    effectiveParticles: 20.5714,
    hand: [3, 0, 1, 0, 1],
    phase: "pre-roll",
    robber: "h:1,1",
    expectedController: {
      cardsLeft: 34,
      recentTotals: [6, 10, 7, 10, 7],
      sevenCounts: [2, 2],
      sevenStreakOwner: 1,
      sevenStreakCount: 2,
    },
    m0Regression: {
      action: "Roll",
      authority: "deep-maxn",
      evidenceEscalation: "triggered/completed/strengthened",
      baselineRealizedNodes: 7_984,
      escalationNodes: 23_149,
      totalNodes: 31_133,
      deadline: false,
      exactKnightFamily: {
        selected: "h:-2,0 -> P0",
        selectedComparator: 13.0667858,
        strongestChallenger: "h:0,2 -> P0",
        challengerComparator: 8.3313026,
      },
    },
  },
  D34: {
    turn: 31,
    expectedRolls: 28,
    belief: "B20",
    expectedSeed: 3_040_454_875,
    sourceWorlds: 11,
    wasmParticles: 20,
    effectiveParticles: 18,
    hand: [4, 0, 2, 0, 1],
    phase: "pre-roll",
    robber: "h:-2,0",
    expectedController: {
      cardsLeft: 32,
      recentTotals: [7, 10, 7, 9, 5],
      sevenCounts: [2, 2],
      sevenStreakOwner: 1,
      sevenStreakCount: 2,
    },
    m0Regression: {
      action: "Knight h:1,1 -> P0",
      authority: "exact-family",
      evidenceEscalation: "none",
    },
  },
} as const;

type StateName = keyof typeof STATES;
type Row = unknown[];

const fail = (message: string): never => {
  throw new Error(message);
};

const assert = (condition: unknown, message: string): asserts condition => {
  if (!condition) fail(message);
};

const parseScalar = (text: string, key: string): unknown => {
  const prefix = `@${key}=`;
  const line = text
    .split("\n")
    .find((candidate) => candidate.startsWith(prefix));
  if (!line) fail(`missing @${key}`);
  return JSON.parse(line.slice(prefix.length));
};

const parseSchema = (text: string, name: string): string[] => {
  const prefix = `@${name}_schema=`;
  const line = text
    .split("\n")
    .find((candidate) => candidate.startsWith(prefix));
  if (!line) fail(`missing ${prefix}`);
  const raw = line.slice(prefix.length).trim();
  assert(raw.startsWith("[") && raw.endsWith("]"), `invalid ${name} schema`);
  return raw
    .slice(1, -1)
    .split(",")
    .map((value) => value.trim());
};

const parseRows = (text: string, name: string): Row[] => {
  const marker = `@${name}\n`;
  const start = text.indexOf(marker);
  if (start < 0) fail(`missing @${name}`);
  const bodyStart = start + marker.length;
  const next = text.indexOf("\n@", bodyStart);
  const body = text.slice(bodyStart, next < 0 ? text.length : next);
  return body
    .split("\n")
    .filter((line) => line.startsWith("["))
    .map((line) => JSON.parse(line) as Row);
};

const rowObject = (
  schema: readonly string[],
  row: readonly unknown[],
): Record<string, unknown> =>
  Object.fromEntries(schema.map((column, index) => [column, row[index]]));

const decodeDeltaRows = (
  schema: readonly string[],
  rows: readonly Row[],
): Array<Record<string, unknown>> => {
  let previous: unknown[] | undefined;
  return rows.map((row) => {
    const decoded = row.map((value, index) =>
      value === "." && previous ? previous[index] : value,
    );
    previous = decoded;
    return rowObject(schema, decoded);
  });
};

const resources = (value: unknown): ResourceVector => {
  const vector = Array.isArray(value) ? value : [];
  return {
    lumber: Number(vector[0] ?? 0),
    brick: Number(vector[1] ?? 0),
    wool: Number(vector[2] ?? 0),
    grain: Number(vector[3] ?? 0),
    ore: Number(vector[4] ?? 0),
  };
};

const development = (value: unknown): Record<string, number> => {
  const vector = Array.isArray(value) ? value : [];
  return {
    knight: Number(vector[0] ?? 0),
    monopoly: Number(vector[1] ?? 0),
    "road-building": Number(vector[2] ?? 0),
    "year-of-plenty": Number(vector[3] ?? 0),
    "victory-point": Number(vector[4] ?? 0),
    unknown: 0,
  };
};

const storedEvent = (row: Row, index: number): StoredEvent => {
  const [id, dtMs, type, ...args] = row;
  const base = {
    id: String(id),
    index,
    timestamp: Number(dtMs),
    raw: "",
  };
  switch (type) {
    case "spend":
      return {
        ...base,
        type,
        player: String(args[0]),
        cost: resources(args[1]),
        reason: String(args[2]),
      } as StoredEvent;
    case "gain":
      return {
        ...base,
        type,
        player: String(args[0]),
        cards: resources(args[1]),
        reason: String(args[2]),
      } as StoredEvent;
    case "roll":
      return {
        ...base,
        type,
        player: String(args[0]),
        dice: args[1],
      } as StoredEvent;
    case "trade":
      return {
        ...base,
        type,
        player: String(args[0]),
        ...(args[1] !== "~" ? { acceptingPlayer: String(args[1]) } : {}),
        given: resources(args[2]),
        received: resources(args[3]),
        bank: Boolean(args[4]),
      } as StoredEvent;
    case "buy-dev":
      return { ...base, type, player: String(args[0]) } as StoredEvent;
    case "play-dev":
      return {
        ...base,
        type,
        player: String(args[0]),
        card: String(args[1]),
      } as StoredEvent;
    case "transfer":
      return {
        ...base,
        type,
        from: String(args[0]),
        to: String(args[1]),
        cards: resources(args[2]),
        reason: String(args[3]),
      } as StoredEvent;
    case "discover":
      return { ...base, type, player: String(args[0]) } as StoredEvent;
    default:
      return fail(`unsupported pre-fixture event ${String(type)}`);
  }
};

const effectiveParticleCount = (worlds: Array<{ weight?: number }>): number => {
  const total = worlds.reduce((sum, world) => sum + Math.max(0, world.weight ?? 0), 0);
  if (total <= Number.EPSILON) return worlds.length;
  const squared = worlds.reduce((sum, world) => {
    const probability = Math.max(0, world.weight ?? 0) / total;
    return sum + probability * probability;
  }, 0);
  return 1 / Math.max(Number.EPSILON, squared);
};

const stableSearchResult = (response: WasmSearchResponse) => ({
  chosen: response.chosen ?? null,
  authority: response.authority,
  stochasticModel: response.stochasticModel ?? M0_FAIR_IID_2D6_V1,
  beliefPolicy: response.beliefPolicy ?? null,
  diceHistoryProvenance: response.diceHistoryProvenance ?? null,
  publicHistoryDigest: response.publicHistoryDigest ?? null,
  stochasticBeliefDigest: response.stochasticBeliefDigest ?? null,
  stochasticBeliefParticleCount: response.stochasticBeliefParticleCount ?? 1,
});

const publicRollsThrough = (
  frames: readonly Record<string, unknown>[],
  turnCutoff: number,
): Array<{ turn: number; actor: string; total: number; frame: number }> => {
  const seenTurns = new Set<number>();
  const rolls: Array<{ turn: number; actor: string; total: number; frame: number }> = [];
  for (const frame of frames) {
    const turn = Number(frame.turn);
    if (!Number.isInteger(turn) || turn > turnCutoff) continue;
    if (frame.rolled !== true || !Number.isInteger(frame.roll)) continue;
    if (seenTurns.has(turn)) continue;
    const actor = String(frame.current);
    assert(/^P\d+$/u.test(actor), `turn ${turn} has invalid public actor ${actor}`);
    const total = Number(frame.roll);
    assert(total >= 2 && total <= 12, `turn ${turn} has invalid public total ${total}`);
    seenTurns.add(turn);
    rolls.push({ turn, actor, total, frame: Number(frame.f) });
  }
  return rolls;
};

const controllerSummary = (controller: WasmReferenceControllerSnapshot) => ({
  remainingCounts: controller.remainingCounts,
  cardsLeft: controller.cardsLeft,
  recentTotals: controller.recentTotals,
  initializedPlayerMask: controller.initializedPlayerMask,
  sevenCounts: controller.sevenCounts,
  sevenStreakOwner: controller.sevenStreakOwner ?? null,
  sevenStreakCount: controller.sevenStreakCount,
});

const verifyController = (
  state: StateName,
  controller: WasmReferenceControllerSnapshot,
): void => {
  const expected = STATES[state].expectedController;
  assert(
    controller.cardsLeft === expected.cardsLeft,
    `${state}: cardsLeft ${controller.cardsLeft} != ${expected.cardsLeft}`,
  );
  assert(
    JSON.stringify(controller.recentTotals) === JSON.stringify(expected.recentTotals),
    `${state}: recent totals differ`,
  );
  assert(
    JSON.stringify(controller.sevenCounts.slice(0, 2)) ===
      JSON.stringify(expected.sevenCounts),
    `${state}: seven counts differ`,
  );
  assert(
    controller.sevenStreakOwner === expected.sevenStreakOwner,
    `${state}: seven streak owner differs`,
  );
  assert(
    controller.sevenStreakCount === expected.sevenStreakCount,
    `${state}: seven streak count differs`,
  );
};

const main = async (): Promise<void> => {
  const sourcePath = process.argv[2] ?? DEFAULT_SOURCE;
  const outputPath = process.argv[3] ?? DEFAULT_OUTPUT;
  const source = await readFile(sourcePath);
  const sourceSha256 = createHash("sha256").update(source).digest("hex");
  assert(
    sourceSha256 === REQUIRED_SOURCE_SHA256,
    `source SHA-256 ${sourceSha256} != required ${REQUIRED_SOURCE_SHA256}`,
  );
  const text = source.toString("utf8");
  const meta = parseScalar(text, "meta") as Record<string, unknown>;
  const scope = String(parseScalar(text, "scope"));
  assert(scope === "/|hand2325|1", `unexpected source scope ${scope}`);
  assert(meta.friendlyRobber === true, "source does not prove Friendly Robber enabled");
  assert(meta.victoryTarget === 15, "source does not prove victory target 15");
  assert(meta.playerCount === 2, "fixture requires the recorded two-player game");

  const aliases = parseScalar(text, "aliases") as Record<string, string>;
  const playerMapping = Object.keys(aliases).sort(
    (left, right) => Number(left.slice(1)) - Number(right.slice(1)),
  );
  assert(
    JSON.stringify(playerMapping) === JSON.stringify(["P0", "P1"]),
    `unexpected player mapping ${JSON.stringify(playerMapping)}`,
  );

  const frameSchema = parseSchema(text, "frames");
  const frameRows = parseRows(text, "frames");
  const frames = decodeDeltaRows(frameSchema, frameRows);
  let cumulativeFrameMs = 0;
  const frameTimes = frames.map((frame) => {
    cumulativeFrameMs += Number(frame.dtMs ?? 0);
    return { frame: Number(frame.f), elapsedMs: cumulativeFrameMs };
  });
  const decisionSchema = parseSchema(text, "decisions");
  const decisions = new Map(
    parseRows(text, "decisions").map((row) => {
      const value = rowObject(decisionSchema, row);
      return [String(value.id), value];
    }),
  );
  const contextSchema = parseSchema(text, "decisionContexts");
  const contexts = new Map(
    parseRows(text, "decisionContexts").map((row) => {
      const value = rowObject(contextSchema, row);
      return [String(value.decision), value];
    }),
  );
  const buildingRows = parseRows(text, "buildings");
  const roadRows = parseRows(text, "roads");
  const playerRows = parseRows(text, "players");
  const beliefRows = parseRows(text, "beliefs");
  const beliefWorldRows = parseRows(text, "beliefWorlds");
  const handVectorRows = parseRows(text, "handVectors");
  const archetypeRows = parseRows(text, "archetypes");
  const eventRows = parseRows(text, "events");
  const topology = {
    hexes: parseRows(text, "boardHexes").map((row) => ({
      id: String(row[0]),
      ...(row[1] !== "~" ? { resource: String(row[1]) } : {}),
      ...(row[2] !== null ? { number: Number(row[2]) } : {}),
    })),
    vertices: parseRows(text, "boardVertices").map((row) => ({
      id: String(row[0]),
      adjacentHexes: row[1] as string[],
      adjacentVertices: row[2] as string[],
      ...(row[3] !== "~" ? { port: String(row[3]) } : {}),
    })),
    edges: parseRows(text, "boardEdges").map((row) => ({
      id: String(row[0]),
      vertices: [String(row[1]), String(row[2])] as [string, string],
    })),
  };

  const wasmBytes = await readFile(
    path.join(ROOT, "src/generated/wasm/colonist_search_bg.wasm"),
  );
  await initWasm({ module_or_path: wasmBytes });

  const replayStateAt = (
    decisionTimeMs: number,
    beliefId: string,
    rootHand: readonly number[],
  ) => {
    const events = eventRows
      .filter((row) => Number(row[1]) <= decisionTimeMs)
      .map(storedEvent);
    const state = replayEvents(events);
    const belief = beliefRows.find((row) => row[0] === beliefId) ??
      fail(`missing belief ${beliefId}`);
    const beliefPlayers = belief[5] as string[];
    state.worlds = beliefWorldRows
      .filter((row) => row[0] === beliefId)
      .map((row) => {
        const hands: Record<string, ResourceVector> = {
          P1: resources(rootHand),
        };
        beliefPlayers.forEach((player, playerIndex) => {
          const handRef = row[3 + playerIndex];
          const hand = handVectorRows.find((candidate) => candidate[0] === handRef) ??
            fail(`missing hand vector ${String(handRef)}`);
          hands[player] = resources(hand.slice(1));
        });
        return { weight: Number(row[2]), hands };
      });
    for (const row of archetypeRows.filter((candidate) => candidate[0] === beliefId)) {
      const player = String(row[1]);
      const model = state.players[player]?.opponentModel ??
        fail(`missing replayed opponent model for ${player}`);
      model.policyPosterior = {
        balanced: Number(row[2]),
        expansion: Number(row[3]),
        cityDevelopment: Number(row[4]),
        tradeFlexible: Number(row[5]),
        tradeResistant: Number(row[6]),
      };
    }
    return state;
  };

  const dynamicAt = (frameIndex: number) => {
    const buildings = new Map<string, { player: string; kind: "settlement" | "city" }>();
    const roads = new Map<string, string>();
    for (const row of buildingRows.filter((candidate) => Number(candidate[0]) <= frameIndex)) {
      const [, operation, kind, player, vertex] = row;
      if (operation === "-") buildings.delete(String(vertex));
      else {
        buildings.set(String(vertex), {
          player: String(player),
          kind: String(kind) as "settlement" | "city",
        });
      }
    }
    for (const row of roadRows.filter((candidate) => Number(candidate[0]) <= frameIndex)) {
      const [, operation, player, edge] = row;
      if (operation === "-") roads.delete(String(edge));
      else roads.set(String(edge), String(player));
    }
    const players = new Map<string, Record<string, unknown>>();
    for (const row of playerRows.filter((candidate) => Number(candidate[0]) <= frameIndex)) {
      players.set(String(row[1]), {
        visiblePoints: Number(row[2] ?? 0),
        handSize: Number(row[3] ?? 0),
        developmentCards: Number(row[4] ?? 0),
        ...(row[5] !== null && row[5] !== "~" ? { longestRoad: Number(row[5]) } : {}),
        hasLongestRoad: Boolean(row[6]),
        hasLargestArmy: Boolean(row[7]),
        hasPlayedDevelopmentThisTurn: Boolean(row[8]),
        cardDiscardLimit: Number(row[9] ?? 9),
        tradeRatios: resources(row[10]),
        ...(row[11] !== "~" ? { playedDevelopmentCards: development(row[11]) } : {}),
      });
    }
    return { buildings, roads, players };
  };

  const materialized: Record<string, unknown> = {};
  for (const stateName of Object.keys(STATES) as StateName[]) {
    const spec = STATES[stateName];
    const decision = decisions.get(stateName) ?? fail(`missing ${stateName} decision`);
    const context = contexts.get(stateName) ?? fail(`missing ${stateName} decision context`);
    assert(decision.playerTradesDisabled === true, `${stateName}: player trades not disabled`);
    assert(context.bank === "~", `${stateName}: public bank unexpectedly visible`);
    assert(decision.rootPlayer === "P1", `${stateName}: unexpected root player`);

    const rolls = publicRollsThrough(frames, spec.turn);
    assert(
      rolls.length === spec.expectedRolls,
      `${stateName}: ${rolls.length} gameplay rolls != ${spec.expectedRolls}`,
    );
    const history = createDiceHistoryState();
    observeLogCoverage(
      history,
      Array.from({ length: rolls.length }, (_, ordinal) => ordinal),
    );
    for (const [ordinal, roll] of rolls.entries()) {
      appendPublicDiceRoll(history, {
        actor: roll.actor,
        total: roll.total,
        eventId: `source-frame-${roll.frame}-turn-${roll.turn}-roll-${ordinal}`,
      });
    }
    assert(
      history.provenance === "complete-from-first-gameplay-roll",
      `${stateName}: materialized history is not complete`,
    );
    const stochastic = buildReferenceStochasticInput(history, playerMapping);
    const inspection = inspect_stochastic({
      numPlayers: playerMapping.length,
      stochastic,
    }) as WasmStochasticInspectionResponse;
    assert(
      inspection.stochasticModel === MREF_COLONIST_LINKED_2024_V1,
      `${stateName}: unexpected effective stochastic model`,
    );
    assert(
      inspection.beliefPolicy === PUBLIC_HISTORY_BELIEF_V1,
      `${stateName}: unexpected belief policy`,
    );
    assert(
      inspection.stochasticBeliefParticleCount === 1 &&
        inspection.controllers.length === 1,
      `${stateName}: complete history did not reconstruct one exact controller`,
    );
    verifyController(stateName, inspection.controllers[0]!);

    assert(decision.belief === spec.belief, `${stateName}: unexpected resource belief`);
    assert(
      JSON.stringify(decision.hand) === JSON.stringify(spec.hand),
      `${stateName}: unexpected root hand`,
    );
    assert(context.robber === spec.robber, `${stateName}: unexpected robber hex`);
    const nextFrame = frameTimes.find(
      (candidate) => candidate.elapsedMs > Number(decision.dtMs),
    ) ?? fail(`${stateName}: no frame follows decision`);
    const dynamic = dynamicAt(nextFrame.frame);
    const trackerState = replayStateAt(Number(decision.dtMs), spec.belief, spec.hand);
    const hexes = structuredClone(topology.hexes).map((hex) =>
      hex.id === context.robber ? { ...hex, blocked: true } : hex,
    );
    const vertices = structuredClone(topology.vertices).map((vertex) => {
      const building = dynamic.buildings.get(vertex.id);
      return building ? { ...vertex, building } : vertex;
    });
    const edges = structuredClone(topology.edges).map((edge) => {
      const player = dynamic.roads.get(edge.id);
      return player ? { ...edge, player } : edge;
    });
    const board = {
      hexes,
      vertices,
      edges,
      myPlayer: "P1",
      ownHand: resources(spec.hand),
      ownDevelopmentCards: {
        cards: development(context.devCards),
        playable: development(context.devPlayable),
        boughtThisTurn: development(context.devBought),
        hasPlayedThisTurn: Boolean(context.devPlayed),
      },
      activeTrades: [],
      bankVisible: false,
      players: Object.fromEntries(dynamic.players),
      playerOrder: ["P0", "P1"],
      gameKey: scope,
      isMyTurn: true,
      action: context.phase,
      initialPlacement: Boolean(context.initial),
      ...(context.picksUntilNext !== null
        ? { picksUntilNext: Number(context.picksUntilNext) }
        : {}),
      ...(context.discardCount !== null
        ? { discardCount: Number(context.discardCount) }
        : {}),
      robberVictimSelection: Boolean(context.victimSelection),
      robberVictimPlayers: context.victims as string[],
      victoryTarget: 15,
      diceMode: meta.diceMode,
      friendlyRobber: true,
      privateGame: Boolean(meta.privateGame),
      botOnlyGame: Boolean(meta.botOnlyGame),
      currentPlayer: String(context.current),
      turn: Number(context.turn),
      ...(context.roll !== null ? { lastRoll: Number(context.roll) } : {}),
      hasRolled: Boolean(context.rolled),
      domesticTradeUsed: Boolean(context.domesticTradeUsed),
      ...((context.buildSettlements as unknown[])?.length
        ? { buildableSettlementIds: context.buildSettlements as string[] }
        : {}),
      ...((context.buildCities as unknown[])?.length
        ? { buildableCityIds: context.buildCities as string[] }
        : {}),
      ...((context.buildRoads as unknown[])?.length
        ? { buildableRoadIds: context.buildRoads as string[] }
        : {}),
      ...((context.legalVertices as unknown[])?.length
        ? { legalVertexIds: context.legalVertices as string[] }
        : {}),
      ...((context.legalEdges as unknown[])?.length
        ? { legalEdgeIds: context.legalEdges as string[] }
        : {}),
    } as unknown as BoardSnapshot;

    const m0Built = buildDeepSearchRequest(trackerState, board, "P1", {}, false);
    const m0Request = m0Built.request;
    assert(m0Built.root === 1, `${stateName}: unexpected root index ${m0Built.root}`);
    assert(m0Request.state.phase === spec.phase, `${stateName}: M0 phase mismatch`);
    assert(m0Request.seed === spec.expectedSeed, `${stateName}: M0 seed changed to ${m0Request.seed}`);
    assert(
      trackerState.worlds.length === spec.sourceWorlds,
      `${stateName}: source resource-world count changed`,
    );
    assert(
      m0Request.state.worlds.length === spec.wasmParticles,
      `${stateName}: WASM resource-particle count changed`,
    );
    assert(
      Math.abs(effectiveParticleCount(m0Request.state.worlds) - spec.effectiveParticles) < 0.0002,
      `${stateName}: effective resource-particle count changed`,
    );
    assert(
      m0Request.state.domesticTradeDisabled === 2,
      `${stateName}: no-player-trade root representation changed`,
    );
    assert(!("stochastic" in m0Request), `${stateName}: M0 wire request gained stochastic bytes`);
    const mrefBuilt = buildDeepSearchRequest(
      trackerState,
      board,
      "P1",
      {},
      false,
      undefined,
      stochastic,
    );
    assert(
      mrefBuilt.request.seed !== m0Request.seed,
      `${stateName}: M_ref incorrectly reused the M0 search seed`,
    );
    assert(
      mrefBuilt.request.stochastic?.model === MREF_COLONIST_LINKED_2024_V1,
      `${stateName}: M_ref stochastic input missing from WASM request`,
    );
    const mrefFirst = analyze(structuredClone(mrefBuilt.request)) as WasmSearchResponse;
    const mrefSecond = analyze(structuredClone(mrefBuilt.request)) as WasmSearchResponse;
    const mrefFirstStable = stableSearchResult(mrefFirst);
    const mrefSecondStable = stableSearchResult(mrefSecond);
    assert(
      JSON.stringify(mrefFirstStable) === JSON.stringify(mrefSecondStable),
      `${stateName}: M_ref result is not deterministic across identical runs: ${JSON.stringify({ first: mrefFirstStable, second: mrefSecondStable })}`,
    );
    assert(
      mrefFirst.stochasticModel === MREF_COLONIST_LINKED_2024_V1,
      `${stateName}: M_ref did not execute with the reference model`,
    );
    assert(
      mrefFirst.beliefPolicy === PUBLIC_HISTORY_BELIEF_V1,
      `${stateName}: M_ref belief policy changed`,
    );
    assert(
      mrefFirst.publicHistoryDigest === inspection.publicHistoryDigest &&
        mrefFirst.stochasticBeliefDigest === inspection.stochasticBeliefDigest,
      `${stateName}: M_ref execution identity differs from reconstruction identity`,
    );

    materialized[stateName] = {
      turnCutoff: spec.turn,
      stateIdentity: decision.state,
      rootPlayer: decision.rootPlayer,
      hiddenBank: {
        publicVisibility: "hidden",
        recordedValue: context.bank,
      },
      friendlyRobber: meta.friendlyRobber,
      victoryTarget: meta.victoryTarget,
      rootDomesticTradeDisabled: decision.playerTradesDisabled,
      resourceBeliefIdentity: decision.belief,
      m0Seed: m0Request.seed,
      publicHistory: {
        provenance: history.provenance,
        rolls: stochastic.rolls,
        rollCount: rolls.length,
        digest: stochastic.diceHistoryDigest,
      },
      stochasticIdentity: {
        m0Model: M0_FAIR_IID_2D6_V1,
        referenceModel: MREF_COLONIST_LINKED_2024_V1,
        beliefPolicy: PUBLIC_HISTORY_BELIEF_V1,
      },
      referenceController: controllerSummary(inspection.controllers[0]!),
      stochasticBeliefDigest: inspection.stochasticBeliefDigest,
      stochasticBeliefParticleCount: inspection.stochasticBeliefParticleCount,
      m0Regression: spec.m0Regression,
      mrefSeed: mrefBuilt.request.seed,
      mrefResult: mrefFirstStable,
    };
  }

  const fixture = {
    schema: "balanced-dice-hand2325/1",
    sourceRecording: {
      file: path.basename(sourcePath),
      sha256: sourceSha256,
    },
    modelDisclaimer:
      "mref-colonist-linked-2024-v1 is the named linked/public reference controller hypothesis; it is not a claim about Colonist's current live server algorithm or hidden server state.",
    players: playerMapping.map((alias) => ({ alias, publicName: aliases[alias] })),
    states: materialized,
  };
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, "utf8");
  process.stdout.write(`${outputPath}\n`);
};

await main();
