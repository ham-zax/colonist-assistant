import type { DecisionSearchAttempt, DecisionTrace } from "./decision-trace";
import type { DeepSearchAction } from "./engine";
import type { BoardSnapshot } from "./placement";
import { RESOURCE_ORDER, type ResourceVector } from "./resources";
import type { StoredEvent } from "./types";

export const RECORD_RESOURCE_ORDER = [...RESOURCE_ORDER] as const;
export const RECORD_DEVELOPMENT_ORDER = [
  "knight",
  "monopoly",
  "road-building",
  "year-of-plenty",
  "victory-point",
] as const;

type Scalar = string | number | boolean | null;
type CompactCell = Scalar | number[] | string[];
type CompactRow = CompactCell[];

export interface CompactRecordContracts {
  resources: readonly string[];
  development: readonly string[];
  boardHexColumns: readonly string[];
  boardVertexColumns: readonly string[];
  boardEdgeColumns: readonly string[];
  frameColumns: readonly string[];
  buildingColumns: readonly string[];
  roadColumns: readonly string[];
  playerColumns: readonly string[];
  decisionColumns: readonly string[];
  decisionContextColumns: readonly string[];
  decisionTradeColumns: readonly string[];
  attemptColumns: readonly string[];
  candidateColumns: readonly string[];
  rootColumns: readonly string[];
  replacementColumns: readonly string[];
  beliefColumns: readonly string[];
  beliefSummaryColumns: readonly string[];
  beliefWorldColumns: readonly string[];
  archetypeColumns: readonly string[];
  eventColumns: readonly string[];
  unchanged: ".";
  unavailable: "~";
}

export interface CompactGameRecord {
  schema: "catan-evidence/1";
  status: "recording" | "completed" | "interrupted";
  scope: string;
  sessionId: string;
  gameKey?: string;
  startedAt: number;
  updatedAt: number;
  completedAt?: number;
  partialHistory: boolean;
  unmatchedCount: number;
  assistant: {
    engine: string;
    disablePlayerTrades: boolean;
    autopilot: boolean;
  };
  aliases: Record<string, string>;
  contracts: CompactRecordContracts;
  meta: {
    myPlayer?: string;
    victoryTarget?: number;
    friendlyRobber?: boolean;
    privateGame?: boolean;
    botOnlyGame?: boolean;
  };
  boardHexes: CompactRow[];
  boardVertices: CompactRow[];
  boardEdges: CompactRow[];
  frames: CompactRow[];
  buildings: CompactRow[];
  roads: CompactRow[];
  players: CompactRow[];
  decisions: CompactRow[];
  decisionContexts: CompactRow[];
  decisionTrades: CompactRow[];
  attempts: CompactRow[];
  candidates: CompactRow[];
  roots: CompactRow[];
  replacements: CompactRow[];
  beliefs: CompactRow[];
  beliefSummaries: CompactRow[];
  beliefWorlds: CompactRow[];
  archetypes: CompactRow[];
  handVectors: number[][];
  events: CompactRow[];
}

export interface CompactGameCapture {
  scope: string;
  sessionId: string;
  gameKey?: string;
  startedAt: number;
  partialHistory: boolean;
  unmatchedCount: number;
  assistant: CompactGameRecord["assistant"];
  events: StoredEvent[];
  decisions: DecisionTrace[];
  board?: BoardSnapshot;
}

const U = "." as const;
const NA = "~" as const;
const MAX_CANDIDATES_PER_DECISION = 12;
const MAX_ROOTS_PER_BUCKET = 12;
const MAX_ATTEMPTS_PER_DECISION = 8;
const MAX_EVENTS = 2400;

const compactDigest = (value: string): string => {
  let first = 2166136261;
  let second = 5381;
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    first = Math.imul(first ^ code, 16777619);
    second = Math.imul(second ^ code, 1597334677);
  }
  return `${(first >>> 0).toString(36)}:${(second >>> 0).toString(36)}:${value.length.toString(36)}`;
};

const compactStateId = (stateHash: string): string =>
  stateHash.length <= 96 ? stateHash : `legacy:${compactDigest(stateHash)}`;

const toResourceVector = (value?: Partial<ResourceVector>): number[] =>
  RESOURCE_ORDER.map((resource) => value?.[resource] ?? 0);

const developmentVector = (
  value?: Partial<Record<(typeof RECORD_DEVELOPMENT_ORDER)[number], number>>,
): number[] => RECORD_DEVELOPMENT_ORDER.map((card) => value?.[card] ?? 0);

const compactNumber = (value: number | undefined, digits = 4): number | null =>
  value === undefined || !Number.isFinite(value)
    ? null
    : Number(value.toFixed(digits));

const actionResourceVector = (value: unknown): number[] | undefined => {
  if (Array.isArray(value)) {
    return value.length === RESOURCE_ORDER.length &&
      value.every((entry) => typeof entry === "number" && Number.isFinite(entry))
      ? [...value]
      : undefined;
  }
  if (!value || typeof value !== "object") return undefined;
  const record = value as Record<string, unknown>;
  if (!RESOURCE_ORDER.every((resource) => typeof record[resource] === "number")) {
    return undefined;
  }
  return RESOURCE_ORDER.map((resource) => record[resource] as number);
};

const actionLabel = (
  action: unknown,
  alias: (name?: string) => string | null = () => null,
): string => {
  if (!action || typeof action !== "object") return String(action ?? "");
  const record = action as Record<string, unknown>;
  const kind = typeof record.kind === "string" ? record.kind : "action";
  const parts: string[] = [kind];
  const add = (key: string, value: unknown): void => {
    if (typeof value === "string" || typeof value === "number") {
      parts.push(`${key}=${value}`);
    } else if (typeof value === "boolean") {
      parts.push(`${key}=${value ? 1 : 0}`);
    }
  };
  const addVector = (key: string, value: unknown): void => {
    const vector = actionResourceVector(value);
    if (vector) parts.push(`${key}=${vector.join(",")}`);
  };
  const addAliasedPlayer = (key: string, value: unknown): void => {
    if (typeof value === "string") add(key, alias(value) ?? value);
  };

  add("t", record.targetId);
  add("t2", record.secondTargetId);
  add("r", record.resource);
  add("r2", record.otherResource);
  add("q", record.ratio);
  add("b", record.build);
  add("ctl", record.control);
  add("card", record.card);
  add("v", record.verdict);
  add("accept", record.accept);
  add("mode", record.mode);
  add("ba", record.boardAction);
  add("oi", record.offerIndex);
  add("tid", record.tradeId);
  add("ai", record.acceptedIndex);
  add("c", record.confidence);
  addAliasedPlayer("p", record.player);
  addAliasedPlayer("fp", record.followupPlayer);

  const point = record.point;
  if (point && typeof point === "object") {
    const { x, y } = point as Record<string, unknown>;
    if (typeof x === "number" && typeof y === "number") {
      parts.push(`pt=${x},${y}`);
    }
  }

  addVector("cards", record.cards);
  addVector("recv", record.receiveCards);
  addVector("give", record.give);
  addVector("get", record.receive);
  addVector("cg", record.counterGive);
  addVector("cr", record.counterReceive);
  addVector("eg", record.existingGive);
  addVector("er", record.existingReceive);

  if (Array.isArray(record.recipients)) {
    parts.push(
      `to=${record.recipients
        .map((player) =>
          typeof player === "string" ? alias(player) ?? player : String(player),
        )
        .join(",")}`,
    );
  }
  if (
    Array.isArray(record.followupResources) &&
    record.followupResources.every((resource) => typeof resource === "string")
  ) {
    parts.push(`fr=${record.followupResources.join(",")}`);
  }
  return parts.join("|");
};

const candidateActionLabel = (
  action: DeepSearchAction,
  alias: (name?: string) => string | null,
): string => actionLabel(action, alias);

const encodeEvent = (
  event: StoredEvent,
  startedAt: number,
  alias: (name?: string) => string | null,
): CompactRow => {
  const anchor = `E${compactDigest(event.id)}`;
  const dt = Math.max(0, event.timestamp - startedAt);
  const row = (...args: CompactCell[]): CompactRow => [
    anchor,
    dt,
    event.type,
    ...args,
  ];
  switch (event.type) {
    case "discover":
      return row(alias(event.player) ?? NA);
    case "gain":
      return row(alias(event.player) ?? NA, toResourceVector(event.cards), event.reason);
    case "spend":
      return row(alias(event.player) ?? NA, toResourceVector(event.cost), event.reason);
    case "transfer":
      return row(
        alias(event.from) ?? NA,
        alias(event.to) ?? NA,
        toResourceVector(event.cards),
        event.reason,
      );
    case "trade":
      return row(
        alias(event.player) ?? NA,
        alias(event.acceptingPlayer) ?? NA,
        toResourceVector(event.given),
        toResourceVector(event.received),
        event.bank,
      );
    case "trade-offered":
      return row(
        alias(event.player) ?? NA,
        event.recipients.map((player) => alias(player) ?? player),
        toResourceVector(event.give),
        toResourceVector(event.receive),
      );
    case "trade-accepted":
    case "trade-rejected":
      return row(
        alias(event.player) ?? NA,
        alias(event.creator) ?? NA,
        toResourceVector(event.give),
        toResourceVector(event.receive),
      );
    case "trade-countered":
      return row(
        alias(event.player) ?? NA,
        alias(event.creator) ?? NA,
        toResourceVector(event.give),
        toResourceVector(event.receive),
        toResourceVector(event.counterGive),
        toResourceVector(event.counterReceive),
      );
    case "trade-expired":
      return row(
        alias(event.player) ?? NA,
        event.recipients?.map((player) => alias(player) ?? player) ?? [],
        toResourceVector(event.give),
        toResourceVector(event.receive),
      );
    case "unknown-transfer":
      return row(alias(event.from) ?? NA, alias(event.to) ?? NA, event.count);
    case "unknown-discard":
      return row(alias(event.player) ?? NA, event.count);
    case "monopoly":
      return row(alias(event.player) ?? NA, event.resource, event.amount ?? null);
    case "buy-dev":
      return row(alias(event.player) ?? NA);
    case "play-dev":
      return row(alias(event.player) ?? NA, event.card);
    case "roll":
      return row(alias(event.player) ?? NA, event.dice ? [...event.dice] : []);
  }
};

const aliasManager = (existing?: Record<string, string>) => {
  const aliases: Record<string, string> = { ...(existing ?? {}) };
  const reverse = new Map(Object.entries(aliases).map(([alias, name]) => [name, alias]));
  let next = Object.keys(aliases).reduce((max, alias) => {
    const match = /^P(\d+)$/u.exec(alias);
    return Math.max(max, match ? Number(match[1]) + 1 : 0);
  }, 0);
  const alias = (name?: string): string | null => {
    if (!name) return null;
    const found = reverse.get(name);
    if (found) return found;
    const created = `P${next++}`;
    aliases[created] = name;
    reverse.set(name, created);
    return created;
  };
  return { aliases, alias };
};

const contracts = (): CompactRecordContracts => ({
  resources: [...RESOURCE_ORDER],
  development: [...RECORD_DEVELOPMENT_ORDER],
  boardHexColumns: ["hex", "resource", "number"],
  boardVertexColumns: ["vertex", "adjacentHexes", "adjacentVertices", "port"],
  boardEdgeColumns: ["edge", "v1", "v2"],
  frameColumns: [
    "f",
    "dtMs",
    "turn",
    "current",
    "phase",
    "myTurn",
    "rolled",
    "roll",
    "hand",
    "bank",
    "devCards",
    "devPlayable",
    "devBought",
    "devPlayed",
    "robber",
    "gameOver",
    "winner",
  ],
  buildingColumns: ["f", "op", "kind", "player", "vertex"],
  roadColumns: ["f", "op", "player", "edge"],
  playerColumns: [
    "f",
    "player",
    "vp",
    "hand",
    "dev",
    "roadLen",
    "longest",
    "largest",
    "playedDev",
    "discardLimit",
    "tradeRatios",
    "playedDevCards",
  ],
  decisionColumns: [
    "id",
    "dtMs",
    "turn",
    "phase",
    "state",
    "belief",
    "hand",
    "publicVP",
    "authority",
    "initialAuthority",
    "displaySource",
    "rootPlayer",
    "chosen",
    "display",
    "status",
    "engine",
    "runtime",
    "model",
    "runtimeReason",
    "engineRevision",
    "algorithm",
    "learnedModel",
    "tradeModel",
    "tacticalWinP",
    "tacticalProven",
    "exact",
    "selectedAtMs",
    "executionStartedAtMs",
    "executionFinishedAtMs",
    "executionOk",
    "executedBeforeResult",
    "totalMs",
    "searchMs",
    "nodes",
    "iterations",
    "depth",
    "rollouts",
    "sourceWorlds",
    "wasmParticles",
    "posteriorParticles",
    "searchParticles",
    "effectiveParticles",
    "seed",
    "deadline",
    "slowMs",
    "deepFailure",
    "mappingFailure",
    "executionFailure",
  ],
  decisionContextColumns: [
    "decision",
    "turn",
    "current",
    "phase",
    "rolled",
    "roll",
    "initial",
    "picksUntilNext",
    "discardCount",
    "robber",
    "victimSelection",
    "victims",
    "domesticTradeUsed",
    "buildSettlements",
    "buildCities",
    "buildRoads",
    "legalVertices",
    "legalEdges",
    "bank",
    "devCards",
    "devPlayable",
    "devBought",
    "devPlayed",
  ],
  decisionTradeColumns: [
    "decision",
    "trade",
    "creator",
    "executor",
    "incoming",
    "counter",
    "canAccept",
    "give",
    "receive",
    "accepted",
    "pending",
    "rejected",
    "complete",
    "myResponse",
  ],
  attemptColumns: [
    "decision",
    "attempt",
    "status",
    "latencyMs",
    "slowMs",
    "timedOut",
    "failure",
  ],
  candidateColumns: [
    "decision",
    "rank",
    "action",
    "value",
    "visits",
    "prior",
    "legalWeight",
    "availabilityWeight",
    "lcb",
  ],
  rootColumns: [
    "decision",
    "bucket",
    "rank",
    "action",
    "prior",
    "plannerValue",
    "completionMass",
    "allocatedNodes",
    "reason",
  ],
  replacementColumns: ["decision", "kind", "from", "to"],
  beliefColumns: [
    "belief",
    "digest",
    "sourceWorlds",
    "storedWorlds",
    "truncated",
    "players",
  ],
  beliefSummaryColumns: [
    "belief",
    "player",
    "expected",
    "pAtLeastOne",
    "minimum",
    "maximum",
  ],
  beliefWorldColumns: ["belief", "world", "weight", "handRefs..."],
  archetypeColumns: [
    "belief",
    "player",
    "balanced",
    "expansion",
    "cityDev",
    "tradeFlex",
    "tradeResist",
  ],
  eventColumns: ["event", "dtMs", "type", "args..."],
  unchanged: U,
  unavailable: NA,
});

interface SnapshotState {
  signature: string;
  capturedAt: number;
  ownHand?: number[];
  bank?: number[];
  ownCards?: number[];
  ownPlayable?: number[];
  ownBought?: number[];
  ownDevPlayed?: boolean;
  robber?: string;
  buildings: Map<string, { kind: string; player: string }>;
  roads: Map<string, string>;
  players: Map<string, string>;
}

const encodePlayerState = (
  frameIndex: number,
  playerAlias: string,
  player: NonNullable<BoardSnapshot["players"]>[string],
): CompactRow => [
  frameIndex,
  playerAlias,
  player.visiblePoints ?? null,
  player.handSize,
  player.developmentCards ?? null,
  player.longestRoad ?? null,
  player.hasLongestRoad ?? false,
  player.hasLargestArmy ?? false,
  player.hasPlayedDevelopmentThisTurn ?? false,
  player.cardDiscardLimit,
  toResourceVector(player.tradeRatios),
  player.playedDevelopmentCards
    ? developmentVector(player.playedDevelopmentCards)
    : NA,
];

const playerStateSignature = (row: CompactRow): string => row.slice(2).join("|");

export class CompactGameBuilder {
  private record?: CompactGameRecord;
  private snapshot?: SnapshotState;
  private eventIds = new Set<string>();
  private decisionIndexByState = new Map<string, number>();
  private beliefIdByDigest = new Map<string, string>();
  private handVectorIndex = new Map<string, number>();

  constructor(seed?: CompactGameRecord) {
    if (seed) {
      this.record = structuredClone(seed);
      this.eventIds = new Set(
        seed.events.flatMap((row) =>
          typeof row[0] === "string" ? [row[0]] : [],
        ),
      );
      seed.decisions.forEach((row, index) => {
        const state = row[4];
        if (typeof state === "string") this.decisionIndexByState.set(state, index);
      });
      for (const row of seed.beliefs) {
        const id = row[0];
        const digest = row[1];
        if (typeof id === "string" && typeof digest === "string") {
          this.beliefIdByDigest.set(digest, id);
        }
      }
      seed.handVectors.forEach((vector, index) => {
        this.handVectorIndex.set(vector.join(","), index);
      });
      this.snapshot = this.restoreSnapshot(seed);
    }
  }

  private restoreSnapshot(seed: CompactGameRecord): SnapshotState | undefined {
    if (!seed.frames.length) return undefined;
    const frameColumn = (name: string): number =>
      seed.contracts.frameColumns.indexOf(name);
    const lastDefined = <T extends CompactCell>(name: string): T | undefined => {
      const column = frameColumn(name);
      if (column < 0) return undefined;
      let value: T | undefined;
      for (const row of seed.frames) {
        const cell = row[column];
        if (cell === U) continue;
        if (cell === NA) {
          value = undefined;
          continue;
        }
        value = cell as T;
      }
      return value;
    };
    let capturedAt = seed.startedAt;
    for (const frame of seed.frames) {
      const delta = frame[1];
      if (typeof delta === "number") capturedAt += Math.max(0, delta);
    }
    const nameFor = (player: CompactCell | undefined): string | undefined =>
      typeof player === "string" ? seed.aliases[player] ?? player : undefined;
    const buildings = new Map<string, { kind: string; player: string }>();
    for (const row of seed.buildings) {
      const op = row[1];
      const kind = row[2];
      const player = nameFor(row[3]);
      const vertex = row[4];
      if (typeof vertex !== "string") continue;
      if (op === "-") {
        buildings.delete(vertex);
      } else if (typeof kind === "string" && player) {
        buildings.set(vertex, { kind, player });
      }
    }
    const roads = new Map<string, string>();
    for (const row of seed.roads) {
      const op = row[1];
      const player = nameFor(row[2]);
      const edge = row[3];
      if (typeof edge !== "string") continue;
      if (op === "-") roads.delete(edge);
      else if (player) roads.set(edge, player);
    }
    const players = new Map<string, string>();
    for (const row of seed.players) {
      const player = row[1];
      if (typeof player === "string") {
        players.set(player, playerStateSignature(row));
      }
    }
    return {
      // Force exactly one fresh frame after reload while retaining every delta
      // baseline, so continuity is re-established without replaying structures.
      signature: "",
      capturedAt,
      ownHand: lastDefined<number[]>("hand"),
      bank: lastDefined<number[]>("bank"),
      ownCards: lastDefined<number[]>("devCards"),
      ownPlayable: lastDefined<number[]>("devPlayable"),
      ownBought: lastDefined<number[]>("devBought"),
      ownDevPlayed: lastDefined<boolean>("devPlayed"),
      robber: lastDefined<string>("robber"),
      buildings,
      roads,
      players,
    };
  }

  get current(): CompactGameRecord | undefined {
    return this.record ? structuredClone(this.record) : undefined;
  }

  apply(input: CompactGameCapture, completed: boolean): CompactGameRecord {
    const now = Date.now();
    const aliasing = aliasManager(this.record?.aliases);
    const existing = this.record;
    const isCompleted = completed || existing?.status === "completed";
    if (!existing) {
      this.record = {
        schema: "catan-evidence/1",
        status: isCompleted ? "completed" : "recording",
        scope: input.scope,
        sessionId: input.sessionId,
        ...(input.gameKey ? { gameKey: input.gameKey } : {}),
        startedAt: input.startedAt,
        updatedAt: now,
        ...(isCompleted ? { completedAt: now } : {}),
        partialHistory: input.partialHistory,
        unmatchedCount: input.unmatchedCount,
        assistant: { ...input.assistant },
        aliases: aliasing.aliases,
        contracts: contracts(),
        meta: {},
        boardHexes: [],
        boardVertices: [],
        boardEdges: [],
        frames: [],
        buildings: [],
        roads: [],
        players: [],
        decisions: [],
        decisionContexts: [],
        decisionTrades: [],
        attempts: [],
        candidates: [],
        roots: [],
        replacements: [],
        beliefs: [],
        beliefSummaries: [],
        beliefWorlds: [],
        archetypes: [],
        handVectors: [],
        events: [],
      };
    } else {
      existing.status = isCompleted ? "completed" : "recording";
      existing.updatedAt = now;
      if (isCompleted) existing.completedAt ??= now;
      existing.partialHistory = input.partialHistory;
      existing.unmatchedCount = input.unmatchedCount;
      existing.assistant = { ...input.assistant };
      if (input.gameKey) existing.gameKey = input.gameKey;
    }

    const record = this.record!;
    record.aliases = aliasing.aliases;

    for (const event of input.events) {
      const row = encodeEvent(event, record.startedAt, aliasing.alias);
      const eventKey = String(row[0]);
      if (this.eventIds.has(eventKey)) continue;
      this.eventIds.add(eventKey);
      record.events.push(row);
    }
    if (record.events.length > MAX_EVENTS) {
      record.events.splice(0, record.events.length - MAX_EVENTS);
      record.partialHistory = true;
    }

    if (input.board) {
      this.appendBoard(input.board, aliasing.alias);
    }
    for (const trace of input.decisions) {
      this.appendDecision(trace, aliasing.alias);
    }
    return record;
  }

  private appendBoard(
    board: BoardSnapshot,
    alias: (name?: string) => string | null,
  ): void {
    const record = this.record!;
    const knownHexes = new Set(record.boardHexes.map((row) => String(row[0])));
    for (const hex of board.hexes) {
      if (knownHexes.has(hex.id)) continue;
      knownHexes.add(hex.id);
      record.boardHexes.push([
        hex.id,
        hex.resource ?? NA,
        hex.number ?? null,
      ]);
    }
    const knownVertices = new Set(
      record.boardVertices.map((row) => String(row[0])),
    );
    for (const vertex of board.vertices) {
      if (knownVertices.has(vertex.id)) continue;
      knownVertices.add(vertex.id);
      record.boardVertices.push([
        vertex.id,
        [...vertex.adjacentHexes],
        [...vertex.adjacentVertices],
        vertex.port ?? NA,
      ]);
    }
    const knownEdges = new Set(record.boardEdges.map((row) => String(row[0])));
    for (const edge of board.edges) {
      if (knownEdges.has(edge.id)) continue;
      knownEdges.add(edge.id);
      record.boardEdges.push([edge.id, edge.vertices[0], edge.vertices[1]]);
    }

    const frameIndex = record.frames.length;
    const capturedAt = board.observedAt ?? Date.now();
    const ownHand = board.ownHand ? toResourceVector(board.ownHand) : undefined;
    const bank = board.bankVisible && board.bank ? toResourceVector(board.bank) : undefined;
    const ownCards = board.ownDevelopmentCards
      ? developmentVector(board.ownDevelopmentCards.cards)
      : undefined;
    const ownPlayable = board.ownDevelopmentCards
      ? developmentVector(board.ownDevelopmentCards.playable)
      : undefined;
    const ownBought = board.ownDevelopmentCards
      ? developmentVector(board.ownDevelopmentCards.boughtThisTurn)
      : undefined;
    const ownDevPlayed = board.ownDevelopmentCards?.hasPlayedThisTurn;
    const robber = board.hexes.find((hex) => hex.blocked)?.id;
    const previous = this.snapshot;
    const signature = JSON.stringify({
      turn: board.turn,
      currentPlayer: board.currentPlayer,
      action: board.action,
      isMyTurn: board.isMyTurn,
      hasRolled: board.hasRolled,
      lastRoll: board.lastRoll,
      ownHand,
      bank,
      ownCards,
      ownPlayable,
      ownBought,
      ownDevPlayed,
      robber,
      gameOver: board.gameOver,
      winner: board.winner,
      players: board.players,
      buildings: board.vertices.flatMap((vertex) =>
        vertex.building
          ? [[vertex.id, vertex.building.player, vertex.building.kind]]
          : [],
      ),
      roads: board.edges.flatMap((edge) =>
        edge.player ? [[edge.id, edge.player]] : [],
      ),
    });
    if (previous?.signature === signature) return;
    const changed = <T>(value: T | undefined, before: T | undefined): T | typeof U | typeof NA => {
      if (value === undefined) return NA;
      return JSON.stringify(value) === JSON.stringify(before) ? U : value;
    };
    record.frames.push([
      frameIndex,
      previous ? Math.max(0, capturedAt - previous.capturedAt) : Math.max(0, capturedAt - record.startedAt),
      board.turn ?? null,
      alias(board.currentPlayer) ?? NA,
      board.action ?? "none",
      Boolean(board.isMyTurn),
      board.hasRolled ?? null,
      board.lastRoll ?? null,
      changed(ownHand, previous?.ownHand),
      changed(bank, previous?.bank),
      changed(ownCards, previous?.ownCards),
      changed(ownPlayable, previous?.ownPlayable),
      changed(ownBought, previous?.ownBought),
      changed(ownDevPlayed, previous?.ownDevPlayed),
      changed(robber, previous?.robber),
      Boolean(board.gameOver),
      alias(board.winner) ?? (board.winner ? board.winner : NA),
    ]);

    const currentBuildings = new Map(
      board.vertices.flatMap((vertex) =>
        vertex.building
          ? [[vertex.id, { kind: vertex.building.kind, player: vertex.building.player }] as const]
          : [],
      ),
    );
    const beforeBuildings = previous?.buildings ?? new Map();
    for (const [vertex, building] of currentBuildings) {
      const before = beforeBuildings.get(vertex);
      if (!before) {
        record.buildings.push([frameIndex, "+", building.kind, alias(building.player) ?? NA, vertex]);
      } else if (before.kind !== building.kind || before.player !== building.player) {
        record.buildings.push([frameIndex, "^", building.kind, alias(building.player) ?? NA, vertex]);
      }
    }
    for (const [vertex, before] of beforeBuildings) {
      if (!currentBuildings.has(vertex)) {
        record.buildings.push([frameIndex, "-", before.kind, alias(before.player) ?? NA, vertex]);
      }
    }

    const currentRoads = new Map(
      board.edges.flatMap((edge) => edge.player ? [[edge.id, edge.player] as const] : []),
    );
    const beforeRoads = previous?.roads ?? new Map();
    for (const [edge, player] of currentRoads) {
      const before = beforeRoads.get(edge);
      if (!before) record.roads.push([frameIndex, "+", alias(player) ?? NA, edge]);
      else if (before !== player) record.roads.push([frameIndex, "^", alias(player) ?? NA, edge]);
    }
    for (const [edge, player] of beforeRoads) {
      if (!currentRoads.has(edge)) record.roads.push([frameIndex, "-", alias(player) ?? NA, edge]);
    }

    const playerSignatures = new Map<string, string>();
    for (const [name, player] of Object.entries(board.players ?? {})) {
      const playerAlias = alias(name)!;
      const row = encodePlayerState(frameIndex, playerAlias, player);
      const signature = playerStateSignature(row);
      playerSignatures.set(playerAlias, signature);
      if (previous?.players.get(playerAlias) !== signature) record.players.push(row);
    }

    record.meta.myPlayer ??= alias(board.myPlayer) ?? undefined;
    record.meta.victoryTarget ??= board.victoryTarget;
    record.meta.friendlyRobber ??= board.friendlyRobber;
    record.meta.privateGame ??= board.privateGame;
    record.meta.botOnlyGame ??= board.botOnlyGame;
    this.snapshot = {
      signature,
      capturedAt,
      ownHand,
      bank,
      ownCards,
      ownPlayable,
      ownBought,
      ownDevPlayed,
      robber,
      buildings: currentBuildings,
      roads: currentRoads,
      players: playerSignatures,
    };
  }

  private handRef(hand?: Partial<ResourceVector>): number | typeof NA {
    if (!hand) return NA;
    const vector = toResourceVector(hand);
    const key = vector.join(",");
    const existing = this.handVectorIndex.get(key);
    if (existing !== undefined) return existing;
    const index = this.record!.handVectors.length;
    this.record!.handVectors.push(vector);
    this.handVectorIndex.set(key, index);
    return index;
  }

  private appendBelief(
    trace: DecisionTrace,
    alias: (name?: string) => string | null,
  ): string {
    const state = trace.replayState;
    if (!state?.worlds.length) return NA;
    const beliefPlayers = state.playerOrder.filter(
      (player) => player !== trace.rootPlayer,
    );
    const players = beliefPlayers.length ? beliefPlayers : [...state.playerOrder];
    const posterior = players.map((player) => {
      const model = state.players[player]?.opponentModel.policyPosterior;
      return [
        player,
        compactNumber(model?.balanced),
        compactNumber(model?.expansion),
        compactNumber(model?.cityDevelopment),
        compactNumber(model?.tradeFlexible),
        compactNumber(model?.tradeResistant),
      ];
    });
    const worlds = state.worlds.map((world) => [
      compactNumber(world.weight, 6),
      ...players.map((player) => toResourceVector(world.hands[player])),
    ]);
    const digest = compactDigest(
      JSON.stringify([players, posterior, trace.beliefSummary, worlds]),
    );
    const existing = this.beliefIdByDigest.get(digest);
    if (existing) return existing;

    const record = this.record!;
    const id = `B${record.beliefs.length + 1}`;
    const playerAliases = players.map((player) => alias(player) ?? player);
    record.beliefs.push([
      id,
      digest,
      trace.sourceWorldCount,
      state.worlds.length,
      trace.beliefSummary?.possibilitiesTruncated ?? false,
      playerAliases,
    ]);
    for (const summary of trace.beliefSummary?.players ?? []) {
      record.beliefSummaries.push([
        id,
        alias(summary.player) ?? summary.player,
        summary.expected.map((value) => compactNumber(value, 4) ?? 0),
        summary.pAtLeastOne.map((value) => compactNumber(value, 4) ?? 0),
        [...summary.minimum],
        [...summary.maximum],
      ]);
    }
    state.worlds.forEach((world, index) => {
      record.beliefWorlds.push([
        id,
        index + 1,
        compactNumber(world.weight, 6),
        ...players.map((player) => this.handRef(world.hands[player])),
      ]);
    });
    players.forEach((player) => {
      const model = state.players[player]?.opponentModel.policyPosterior;
      if (!model) return;
      record.archetypes.push([
        id,
        alias(player) ?? player,
        compactNumber(model.balanced),
        compactNumber(model.expansion),
        compactNumber(model.cityDevelopment),
        compactNumber(model.tradeFlexible),
        compactNumber(model.tradeResistant),
      ]);
    });
    this.beliefIdByDigest.set(digest, id);
    return id;
  }

  private appendDecisionContext(
    decisionId: string,
    board: BoardSnapshot,
    alias: (name?: string) => string | null,
  ): void {
    const record = this.record!;
    if (!record.decisionContexts.some((row) => row[0] === decisionId)) {
      record.decisionContexts.push([
        decisionId,
        board.turn ?? null,
        alias(board.currentPlayer) ?? NA,
        board.action ?? "none",
        board.hasRolled ?? null,
        board.lastRoll ?? null,
        board.initialPlacement ?? false,
        board.picksUntilNext ?? null,
        board.discardCount ?? null,
        board.hexes.find((hex) => hex.blocked)?.id ?? NA,
        board.robberVictimSelection ?? false,
        board.robberVictimPlayers?.map((player) => alias(player) ?? player) ?? [],
        board.domesticTradeUsed ?? false,
        [...(board.buildableSettlementIds ?? [])],
        [...(board.buildableCityIds ?? [])],
        [...(board.buildableRoadIds ?? [])],
        [...(board.legalVertexIds ?? [])],
        [...(board.legalEdgeIds ?? [])],
        board.bankVisible && board.bank ? toResourceVector(board.bank) : NA,
        board.ownDevelopmentCards
          ? developmentVector(board.ownDevelopmentCards.cards)
          : NA,
        board.ownDevelopmentCards
          ? developmentVector(board.ownDevelopmentCards.playable)
          : NA,
        board.ownDevelopmentCards
          ? developmentVector(board.ownDevelopmentCards.boughtThisTurn)
          : NA,
        board.ownDevelopmentCards?.hasPlayedThisTurn ?? null,
      ]);
    }
    if (record.decisionTrades.some((row) => row[0] === decisionId)) return;
    for (const trade of board.activeTrades ?? []) {
      record.decisionTrades.push([
        decisionId,
        `T${compactDigest(trade.id)}`,
        alias(trade.creator) ?? trade.creator,
        alias(trade.tradeExecutor) ?? trade.tradeExecutor,
        trade.incoming,
        trade.counterOffer,
        trade.canAccept,
        toResourceVector(trade.creatorGive),
        toResourceVector(trade.creatorReceive),
        trade.acceptedPlayers?.map((player) => alias(player) ?? player) ?? [],
        trade.pendingPlayers?.map((player) => alias(player) ?? player) ?? [],
        trade.rejectedPlayers?.map((player) => alias(player) ?? player) ?? [],
        trade.responsesComplete ?? false,
        trade.myResponse ?? NA,
      ]);
    }
  }

  private appendDecision(
    trace: DecisionTrace,
    alias: (name?: string) => string | null,
  ): void {
    const record = this.record!;
    const stateId = compactStateId(trace.stateHash);
    const existingIndex = this.decisionIndexByState.get(stateId);
    const existingRow =
      existingIndex === undefined ? undefined : record.decisions[existingIndex];
    const id =
      existingRow && typeof existingRow[0] === "string"
        ? existingRow[0]
        : `D${record.decisions.length + 1}`;
    const existingBelief =
      existingRow && typeof existingRow[5] === "string"
        ? existingRow[5]
        : undefined;
    const belief =
      existingBelief && existingBelief !== NA
        ? existingBelief
        : this.appendBelief(trace, alias);
    const relative = (timestamp?: number): number | null =>
      timestamp === undefined
        ? null
        : Math.max(0, Math.round(timestamp - record.startedAt));
    const row: CompactRow = [
      id,
      Math.max(0, trace.recordedAt - record.startedAt),
      trace.turn,
      trace.phase,
      stateId,
      belief,
      [...trace.hand],
      [...trace.publicVictoryPoints],
      trace.rustAuthority ?? trace.finalActionSource ?? NA,
      trace.authorityTrace?.initialAuthority ?? NA,
      trace.finalActionSource ?? NA,
      alias(trace.rootPlayer) ?? trace.rootPlayer ?? NA,
      trace.deepChosenAction ? actionLabel(trace.deepChosenAction, alias) : NA,
      trace.finalAction ? actionLabel(trace.finalAction, alias) : NA,
      trace.deepStatus,
      trace.engine ?? NA,
      trace.runtime ?? NA,
      trace.decisionModel ?? NA,
      trace.runtimeReason ?? NA,
      trace.engineRevision ?? NA,
      trace.algorithm ?? NA,
      trace.learnedModelVersion ?? NA,
      trace.tradeModelVersion ?? NA,
      compactNumber(trace.tacticalWinProbability),
      trace.tacticalProven ?? false,
      trace.exactDecision ?? false,
      relative(trace.finalActionSelectedAt),
      relative(trace.executionStartedAt),
      relative(trace.executionFinishedAt),
      trace.executionSucceeded ?? null,
      trace.executedBeforeDeepResult,
      compactNumber(trace.deepLatencyMs, 1),
      compactNumber(trace.searchElapsedMs, 1),
      trace.nodes ?? null,
      trace.iterations ?? null,
      trace.deepestDecisionDepth ?? null,
      trace.rollouts ?? null,
      trace.sourceWorldCount,
      trace.wasmParticleCount ?? null,
      trace.rustPosteriorParticleCount ?? null,
      trace.rustSearchParticleCount ?? null,
      compactNumber(trace.effectiveParticleCount),
      trace.seed ?? null,
      trace.deepTimedOut,
      compactNumber(trace.deepSlowWarningAtMs, 1),
      trace.deepFailureReason ?? NA,
      trace.mappingFailureReason ?? NA,
      trace.executionFailureReason ?? NA,
    ];
    if (existingIndex === undefined) {
      this.decisionIndexByState.set(stateId, record.decisions.length);
      record.decisions.push(row);
    } else {
      record.decisions[existingIndex] = row;
    }
    if (trace.replayBoard) {
      this.appendDecisionContext(id, trace.replayBoard, alias);
    }

    record.attempts = record.attempts.filter((entry) => entry[0] !== id);
    record.candidates = record.candidates.filter((entry) => entry[0] !== id);
    record.roots = record.roots.filter((entry) => entry[0] !== id);
    record.replacements = record.replacements.filter((entry) => entry[0] !== id);

    (trace.deepAttempts ?? [])
      .slice(-MAX_ATTEMPTS_PER_DECISION)
      .forEach((attempt: DecisionSearchAttempt, index) => {
        record.attempts.push([
          id,
          index + 1,
          attempt.status,
          compactNumber(attempt.latencyMs, 1),
          compactNumber(attempt.slowWarningAtMs, 1),
          attempt.timedOut,
          attempt.failureReason ?? NA,
        ]);
      });
    (trace.deepCandidates ?? [])
      .slice(0, MAX_CANDIDATES_PER_DECISION)
      .forEach((candidate, index) => {
        record.candidates.push([
          id,
          index + 1,
          candidateActionLabel(candidate.action, alias),
          compactNumber(candidate.value),
          candidate.visits ?? null,
          compactNumber(candidate.prior),
          compactNumber(candidate.legalWeight),
          compactNumber(candidate.availabilityWeight),
          compactNumber(candidate.lowerConfidenceValue),
        ]);
      });

    const provenance = trace.rootProvenance;
    provenance?.rankedRoots
      .slice(0, MAX_ROOTS_PER_BUCKET)
      .forEach((root) => {
        record.roots.push([
          id,
          "ranked",
          root.rank,
          actionLabel(root.action, alias),
          compactNumber(root.prior),
          compactNumber(root.plannerValue),
          compactNumber(root.plannerCompletionMass),
          null,
          NA,
        ]);
      });
    provenance?.retainedRoots
      .slice(0, MAX_ROOTS_PER_BUCKET)
      .forEach((root) => {
        record.roots.push([
          id,
          "retained",
          root.preTruncationRank ?? null,
          actionLabel(root.action, alias),
          compactNumber(root.prior),
          compactNumber(root.plannerValue),
          compactNumber(root.plannerCompletionMass),
          root.allocatedNodes,
          NA,
        ]);
      });
    provenance?.prunedRoots
      .slice(0, MAX_ROOTS_PER_BUCKET)
      .forEach((root) => {
        record.roots.push([
          id,
          "pruned",
          root.preTruncationRank ?? null,
          actionLabel(root.action, alias),
          null,
          null,
          null,
          null,
          root.reason,
        ]);
      });

    const exactReplacement =
      trace.authorityTrace?.exactFamilyReplacement ??
      provenance?.exactFamilyReplacement;
    if (trace.authorityTrace?.exactFamily) {
      record.replacements.push([
        id,
        "exact-family",
        trace.authorityTrace.exactFamily,
        NA,
      ]);
    }
    if (exactReplacement) {
      record.replacements.push([
        id,
        "exact-replacement",
        actionLabel(exactReplacement.from, alias),
        actionLabel(exactReplacement.to, alias),
      ]);
    }
    const safetyReplacement =
      trace.authorityTrace?.safetyReplacement ?? provenance?.safetyReplacement;
    if (safetyReplacement) {
      record.replacements.push([
        id,
        "safety-replacement",
        actionLabel(safetyReplacement.from, alias),
        actionLabel(safetyReplacement.to, alias),
      ]);
    }
  }
}

export const formatCompactGameRecord = (record: CompactGameRecord): string => {
  const lines: string[] = [
    `@schema=${record.schema}`,
    `@status=${record.status}`,
    `@scope=${JSON.stringify(record.scope)}`,
    `@started=${record.startedAt}`,
    `@updated=${record.updatedAt}`,
    `@partial=${record.partialHistory ? 1 : 0}`,
    `@unmatched=${record.unmatchedCount}`,
    `@resources=[${record.contracts.resources.join(",")}]`,
    `@development=[${record.contracts.development.join(",")}]`,
    `@symbols=${JSON.stringify({ ".": "unchanged", "~": "unavailable" })}`,
    `@ops=${JSON.stringify({ "+": "add", "^": "replace-or-upgrade", "-": "remove" })}`,
    `@time=${JSON.stringify({ frames: "dtMs since previous frame; first since start", decisions: "dtMs since start", events: "dtMs since start" })}`,
    `@actionKeys=${JSON.stringify({ t: "targetId", t2: "secondTargetId", r: "resource", r2: "otherResource", q: "ratio", b: "build", ctl: "control", card: "development card", v: "verdict", accept: "boolean", mode: "trade mode", ba: "board action", oi: "offer index", tid: "trade id", ai: "accepted-player index", c: "confidence", p: "player alias", fp: "follow-up player alias", pt: "screen point x,y", cards: "resource vector", recv: "receive resource vector", give: "give resource vector", get: "receive resource vector", cg: "counter give resource vector", cr: "counter receive resource vector", eg: "existing give resource vector", er: "existing receive resource vector", to: "recipient aliases", fr: "follow-up resource sequence" })}`,
    `@eventArgs=${JSON.stringify({ discover: "[P]", gain: "[P,R,reason]", spend: "[P,R,reason]", transfer: "[from,to,R,reason]", trade: "[P,acceptor,giveR,getR,bank]", "trade-offered": "[P,recipients,giveR,getR]", "trade-accepted": "[P,creator,giveR,getR]", "trade-rejected": "[P,creator,giveR,getR]", "trade-countered": "[P,creator,giveR,getR,counterGiveR,counterGetR]", "trade-expired": "[P,recipients,giveR,getR]", "unknown-transfer": "[from,to,count]", "unknown-discard": "[P,count]", monopoly: "[P,resource,amount]", "buy-dev": "[P]", "play-dev": "[P,card]", roll: "[P,dice]" })}`,
    `@beliefWorlds=${JSON.stringify("handRefs follow the player order declared by each @beliefs row")}`,
    `@aliases=${JSON.stringify(record.aliases)}`,
    `@assistant=${JSON.stringify(record.assistant)}`,
    `@meta=${JSON.stringify(record.meta)}`,
  ];
  const section = (name: string, columns: readonly string[], rows: CompactRow[]) => {
    if (!rows.length) return;
    lines.push("", `@${name}_schema=[${columns.join(",")}]`, `@${name}`);
    for (const row of rows) lines.push(JSON.stringify(row));
  };
  section("boardHexes", record.contracts.boardHexColumns, record.boardHexes);
  section(
    "boardVertices",
    record.contracts.boardVertexColumns,
    record.boardVertices,
  );
  section("boardEdges", record.contracts.boardEdgeColumns, record.boardEdges);
  section("frames", record.contracts.frameColumns, record.frames);
  section("buildings", record.contracts.buildingColumns, record.buildings);
  section("roads", record.contracts.roadColumns, record.roads);
  section("players", record.contracts.playerColumns, record.players);
  section("decisions", record.contracts.decisionColumns, record.decisions);
  section(
    "decisionContexts",
    record.contracts.decisionContextColumns,
    record.decisionContexts,
  );
  section(
    "decisionTrades",
    record.contracts.decisionTradeColumns,
    record.decisionTrades,
  );
  section("attempts", record.contracts.attemptColumns, record.attempts);
  section("candidates", record.contracts.candidateColumns, record.candidates);
  section("roots", record.contracts.rootColumns, record.roots);
  section("replacements", record.contracts.replacementColumns, record.replacements);
  section("beliefs", record.contracts.beliefColumns, record.beliefs);
  section(
    "beliefSummaries",
    record.contracts.beliefSummaryColumns,
    record.beliefSummaries,
  );
  if (record.handVectors.length) {
    lines.push("", "@handVectors_schema=[ref,L,B,W,G,O]", "@handVectors");
    record.handVectors.forEach((vector, index) => {
      lines.push(JSON.stringify([index, ...vector]));
    });
  }
  section("beliefWorlds", record.contracts.beliefWorldColumns, record.beliefWorlds);
  section("archetypes", record.contracts.archetypeColumns, record.archetypes);
  section("events", record.contracts.eventColumns, record.events);
  return lines.join("\n");
};
