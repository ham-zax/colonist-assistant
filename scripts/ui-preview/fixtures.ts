import type { DecisionAnalysis, DeepSearchResult } from "../../src/core/engine";
import type {
  ActiveTradeOffer,
  BoardEdge,
  BoardHex,
  BoardPlayerPublicState,
  BoardSnapshot,
  BoardVertex,
  DevelopmentCardVector,
} from "../../src/core/placement";
import {
  RESOURCE_LABELS,
  emptyResources,
  type Resource,
  type ResourceVector,
} from "../../src/core/resources";
import { createTrackerState, reduceTracker } from "../../src/core/tracker";
import type { TrackerEvent, TrackerState } from "../../src/core/types";

/** Realistic 4-player table. The second opponent exercises long-name layout. */
export const ME = "Hamza#1029";
export const LONG = "Bouton#4976_TheLongestName";
export const MIRA = "Mira#2231";
export const KASPER = "Kasper#8810";
export const PLAYERS = [ME, LONG, MIRA, KASPER] as const;
export const PLAYER_COLORS: Record<string, string> = {
  [ME]: "#e8a33d",
  [LONG]: "#d8534f",
  [MIRA]: "#5aa469",
  [KASPER]: "#8b6fd1",
};

export const res = (values: Partial<ResourceVector> = {}): ResourceVector => ({
  ...emptyResources(),
  ...values,
});

const dev = (values: Partial<DevelopmentCardVector> = {}): DevelopmentCardVector => ({
  knight: 0,
  monopoly: 0,
  "road-building": 0,
  "year-of-plenty": 0,
  "victory-point": 0,
  ...values,
});

/* ------------------------------------------------------------------ board */

export const BOARD_CENTER = { x: 530, y: 440 };
const HEX_SIZE = 56;

// Row-major over axial (q, r) with radius 2: r=-2..2.
const RESOURCE_LAYOUT: Array<Resource | "desert"> = [
  "ore", "wool", "lumber",
  "grain", "brick", "lumber", "wool",
  "wool", "grain", "desert", "ore", "brick",
  "lumber", "grain", "wool", "ore",
  "brick", "lumber", "grain",
];
const NUMBER_LAYOUT = [
  10, 2, 9,
  12, 6, 4, 10,
  9, 11, 3, 8,
  8, 3, 4, 5,
  5, 6, 11,
];
const PIPS: Record<number, number> = {
  2: 1, 3: 2, 4: 3, 5: 4, 6: 5, 8: 5, 9: 4, 10: 3, 11: 2, 12: 1,
};

export interface GeneratedBoard {
  hexes: BoardHex[];
  vertices: BoardVertex[];
  edges: BoardEdge[];
}

export const generateBoard = (): GeneratedBoard => {
  const hexes: BoardHex[] = [];
  const vertices: BoardVertex[] = [];
  const edges: BoardEdge[] = [];
  const vertexByKey = new Map<string, BoardVertex>();
  const edgeByKey = new Map<string, BoardEdge>();
  let layoutIndex = 0;
  let numberIndex = 0;
  for (let r = -2; r <= 2; r += 1) {
    for (let q = Math.max(-2, -r - 2); q <= Math.min(2, -r + 2); q += 1) {
      const kind = RESOURCE_LAYOUT[layoutIndex++]!;
      const cx = BOARD_CENTER.x + HEX_SIZE * Math.sqrt(3) * (q + r / 2);
      const cy = BOARD_CENTER.y + HEX_SIZE * 1.5 * r;
      const hex: BoardHex = {
        id: `h${hexes.length}`,
        screen: { x: Math.round(cx), y: Math.round(cy) },
        ...(kind === "desert"
          ? { blocked: true }
          : { resource: kind, number: NUMBER_LAYOUT[numberIndex++]! }),
      };
      hexes.push(hex);
      const corners: BoardVertex[] = [];
      for (let i = 0; i < 6; i += 1) {
        const angle = ((60 * i - 30) * Math.PI) / 180;
        const x = Math.round(cx + HEX_SIZE * Math.cos(angle));
        const y = Math.round(cy + HEX_SIZE * Math.sin(angle));
        const key = `${x},${y}`;
        let vertex = vertexByKey.get(key);
        if (!vertex) {
          vertex = {
            id: `v${vertices.length}`,
            adjacentHexes: [],
            adjacentVertices: [],
            screen: { x, y },
          };
          vertexByKey.set(key, vertex);
          vertices.push(vertex);
        }
        vertex.adjacentHexes.push(hex.id);
        corners.push(vertex);
      }
      for (let i = 0; i < 6; i += 1) {
        const a = corners[i]!;
        const b = corners[(i + 1) % 6]!;
        const key = [a.id, b.id].sort().join("|");
        if (edgeByKey.has(key)) continue;
        const edge: BoardEdge = {
          id: `e${edges.length}`,
          vertices: [a.id, b.id],
          screen: {
            x: Math.round((a.screen!.x + b.screen!.x) / 2),
            y: Math.round((a.screen!.y + b.screen!.y) / 2),
          },
        };
        edgeByKey.set(key, edge);
        edges.push(edge);
        a.adjacentVertices.push(b.id);
        b.adjacentVertices.push(a.id);
      }
    }
  }
  const hexById = new Map(hexes.map((hex) => [hex.id, hex]));
  for (const vertex of vertices) {
    const producing = vertex.adjacentHexes
      .map((id) => hexById.get(id)!)
      .filter((hex) => hex.resource && hex.number)
      .sort((left, right) => (PIPS[right.number!] ?? 0) - (PIPS[left.number!] ?? 0));
    vertex.label = producing
      .map((hex) => `${hex.number} ${RESOURCE_LABELS[hex.resource!].toLowerCase()}`)
      .join(" / ");
  }
  return { hexes, vertices, edges };
};

export interface Placement {
  player: string;
  /** Vertex id from generateBoard(), which is deterministic. */
  vertex: string;
  /** Adjacent vertex the owned road leads to. */
  roadTo: string;
  city?: boolean;
}

/** Applies buildings/roads; fails loudly if a fixture breaks the distance rule. */
export const populateBoard = (
  board: GeneratedBoard,
  placements: Placement[],
): GeneratedBoard => {
  const byId = new Map(board.vertices.map((vertex) => [vertex.id, vertex]));
  for (const placement of placements) {
    const vertex = byId.get(placement.vertex);
    if (!vertex) throw new Error(`Unknown vertex ${placement.vertex}`);
    if (
      vertex.building ||
      vertex.adjacentVertices.some((id) => byId.get(id)?.building)
    ) {
      throw new Error(`${placement.player} at ${placement.vertex} breaks the distance rule`);
    }
    vertex.building = {
      player: placement.player,
      kind: placement.city ? "city" : "settlement",
    };
    const edge = board.edges.find(
      (candidate) =>
        candidate.vertices.includes(vertex.id) &&
        candidate.vertices.includes(placement.roadTo),
    );
    if (!edge) throw new Error(`No edge ${placement.vertex}-${placement.roadTo}`);
    edge.player = placement.player;
  }
  return board;
};

export const openVertexIds = (board: GeneratedBoard): string[] => {
  const byId = new Map(board.vertices.map((vertex) => [vertex.id, vertex]));
  return board.vertices
    .filter(
      (vertex) =>
        !vertex.building &&
        vertex.adjacentVertices.every((id) => !byId.get(id)?.building),
    )
    .map((vertex) => vertex.id);
};

export const freeEdgesTouching = (
  board: GeneratedBoard,
  player: string,
): string[] => {
  const owned = new Set<string>();
  for (const edge of board.edges) {
    if (edge.player === player) edge.vertices.forEach((id) => owned.add(id));
  }
  for (const vertex of board.vertices) {
    if (vertex.building?.player === player) owned.add(vertex.id);
  }
  return board.edges
    .filter((edge) => !edge.player && edge.vertices.some((id) => owned.has(id)))
    .map((edge) => edge.id);
};

/** Mid-game position: two buildings and a road each; Bouton holds a city. */
export const midGamePlacements = (): Placement[] => [
  { player: ME, vertex: "v2", roadTo: "v14" },
  { player: ME, vertex: "v20", roadTo: "v21" },
  { player: LONG, vertex: "v38", roadTo: "v42" },
  { player: LONG, vertex: "v8", roadTo: "v18", city: true },
  { player: MIRA, vertex: "v23", roadTo: "v22" },
  { player: MIRA, vertex: "v26", roadTo: "v27" },
  { player: KASPER, vertex: "v44", roadTo: "v50" },
  { player: KASPER, vertex: "v29", roadTo: "v19" },
];

/* ---------------------------------------------------------------- tracker */

/** True hands behind the tracker evidence; Bouton's extra card was robbed from Mira. */
const opponentHands = {
  [LONG]: res({ lumber: 2, brick: 2, wool: 1, ore: 1 }),
  [MIRA]: res({ lumber: 1, brick: 1, wool: 1, grain: 2 }),
  [KASPER]: res({ lumber: 1, brick: 1, wool: 2, grain: 1, ore: 1 }),
};

const DICE: Array<[number, number]> = [
  [3, 4], [2, 6], [4, 4], [5, 1], [3, 3], [6, 2], [1, 5], [4, 3], [2, 4], [6, 5],
  [3, 5], [1, 1], [4, 2], [6, 6], [5, 3], [2, 5], [4, 6], [3, 2], [5, 4], [1, 6],
  [2, 3], [6, 1], [4, 1], [5, 2], [3, 6], [2, 2], [6, 3], [1, 4], [5, 5], [3, 1],
];

export const buildTracker = (options: {
  ownHand: ResourceVector;
  rolls?: number;
  /** Seating order; defaults to PLAYERS. */
  order?: readonly string[];
  /** Opening placement: nobody holds cards yet. */
  setup?: boolean;
}): TrackerState => {
  const events: TrackerEvent[] = [];
  for (const player of options.order ?? PLAYERS) {
    events.push({ type: "discover", player, color: PLAYER_COLORS[player] });
  }
  if (options.setup) {
    let state = createTrackerState();
    for (const event of events) state = reduceTracker(state, event);
    return state;
  }
  events.push({ type: "gain", player: ME, cards: options.ownHand, reason: "production" });
  events.push({
    type: "gain", player: LONG, reason: "production",
    cards: res({ lumber: 2, brick: 1, wool: 1, ore: 1 }),
  });
  events.push({
    type: "gain", player: MIRA, reason: "production",
    cards: res({ lumber: 1, brick: 1, wool: 1, grain: 2, ore: 0, }),
  });
  // One unexplained card moves Mira -> Bouton (e.g. a robber steal), leaving
  // both players with genuine hidden-card ranges.
  events.push({
    type: "gain", player: MIRA, reason: "production", cards: res({ brick: 1 }),
  });
  events.push({ type: "unknown-transfer", from: MIRA, to: LONG, count: 1 });
  events.push({
    type: "gain", player: KASPER, reason: "production", cards: opponentHands[KASPER],
  });
  const rolls = options.rolls ?? 0;
  for (let index = 0; index < rolls; index += 1) {
    events.push({
      type: "roll",
      player: PLAYERS[index % PLAYERS.length]!,
      dice: DICE[index % DICE.length]!,
    });
  }
  let state = createTrackerState();
  for (const event of events) state = reduceTracker(state, event);
  return state;
};

export const publicPlayers = (
  ownHand: ResourceVector,
  overrides: Record<string, Partial<BoardPlayerPublicState>> = {},
): Record<string, BoardPlayerPublicState> => {
  const sum = (hand: ResourceVector): number =>
    hand.lumber + hand.brick + hand.wool + hand.grain + hand.ore;
  const base = (
    handSize: number,
    points: number,
    extra: Partial<BoardPlayerPublicState> = {},
  ): BoardPlayerPublicState => ({
    handSize,
    tradeRatios: res({ lumber: 4, brick: 4, wool: 4, grain: 4, ore: 4 }),
    cardDiscardLimit: 7,
    developmentCards: 0,
    playedDevelopmentCards: dev(),
    hasPlayedDevelopmentThisTurn: false,
    visiblePoints: points,
    ...extra,
  });
  return {
    [ME]: { ...base(sum(ownHand), 2, { longestRoad: 2 }), ...overrides[ME] },
    [LONG]: {
      ...base(6, 5, {
        developmentCards: 2,
        hasLargestArmy: true,
        playedDevelopmentCards: dev({ knight: 3 }),
      }),
      ...overrides[LONG],
    },
    [MIRA]: {
      ...base(5, 4, {
        hasLongestRoad: true,
        tradeRatios: res({ lumber: 3, brick: 3, wool: 3, grain: 3, ore: 3 }),
        longestRoad: 5,
      }),
      ...overrides[MIRA],
    },
    [KASPER]: { ...base(6, 2, { developmentCards: 1 }), ...overrides[KASPER] },
  };
};

export const bankFor = (ownHand: ResourceVector): ResourceVector => {
  const total = res();
  for (const hand of [ownHand, ...Object.values(opponentHands)]) {
    for (const key of Object.keys(total) as Resource[]) total[key] += hand[key];
  }
  const bank = res();
  for (const key of Object.keys(bank) as Resource[]) bank[key] = 19 - total[key];
  return bank;
};

export const RESOLVED_SEAT = {
  seatSource: "gameController.myColor+currentUserId+gameUserStates" as const,
  identity: {
    status: "resolved" as const,
    reason: "cross-checked" as const,
    source: "controller+account-user-id+store-roster" as const,
    currentUserIdAvailable: true,
    currentUserMatchColors: [1],
    myColor: 1,
    currentUserColor: 1,
  },
};

export interface BoardOptions {
  geometry: GeneratedBoard;
  ownHand: ResourceVector;
  overrides?: Partial<BoardSnapshot>;
  playerOverrides?: Record<string, Partial<BoardPlayerPublicState>>;
}

export const makeBoard = (options: BoardOptions): BoardSnapshot => ({
  hexes: options.geometry.hexes,
  vertices: options.geometry.vertices,
  edges: options.geometry.edges,
  gameKey: "ui-preview-game",
  myPlayer: ME,
  playerOrder: [...PLAYERS],
  currentPlayer: ME,
  isMyTurn: true,
  action: "none",
  hasRolled: true,
  turn: 14,
  victoryTarget: 10,
  diceMode: "random",
  initialPlacement: false,
  ownHand: options.ownHand,
  players: publicPlayers(options.ownHand, options.playerOverrides),
  bank: bankFor(options.ownHand),
  bankVisible: true,
  localSeatDiagnostics: RESOLVED_SEAT,
  ...options.overrides,
});

export const incomingTrade = (
  overrides: Partial<ActiveTradeOffer> = {},
): ActiveTradeOffer => ({
  id: "preview-trade-1",
  creator: LONG,
  tradeExecutor: LONG,
  creatorGive: res({ brick: 1 }),
  creatorReceive: res({ ore: 1 }),
  incoming: true,
  counterOffer: false,
  canAccept: true,
  myResponse: "pending",
  ...overrides,
});

/* --------------------------------------------------------------- analysis */

export const winEstimates = (
  probabilities: number[],
): DecisionAnalysis["players"] =>
  PLAYERS.map((player, index) => ({
    player,
    probability: probabilities[index]!,
    etaTurns: [9, 7, 10, 11][index]!,
    samples: 480,
    confidence: "medium" as const,
    reasons: [],
  }));

export const makeAnalysis = (
  probabilities: number[],
  deepSearch?: DeepSearchResult,
): DecisionAnalysis => ({
  engine: "deep-search",
  runtime: "background-wasm",
  players: winEstimates(probabilities),
  actionScores: { road: 0.3, settlement: 0.4, city: 0.6, development: 0.35 },
  simulations: 480,
  model: "Deep MaxN, 4-player search, 2,000 ms budget",
  ...(deepSearch ? { deepSearch } : {}),
});

type ActionStatistics = DeepSearchResult["actions"][number];

export const actionStats = (
  action: ActionStatistics["action"],
  value: number,
  legalWeight = 1,
): ActionStatistics => ({
  action,
  visits: 240,
  availability: 1,
  availabilityWeight: 1,
  legalWeight,
  prior: 0.25,
  value: [value, 0, 0, 0],
  lowerConfidenceValue: [value - 0.04, 0, 0, 0],
});

/** Fixture-only DeepSearchResult: the overlay reads a small subset of fields. */
export const makeDeepSearch = (
  chosen: ActionStatistics["action"],
  actions: ActionStatistics[],
): DeepSearchResult =>
  ({
    engineRevision: "ui-preview",
    diceMode: "random",
    chanceModel: "fair-iid-2d6",
    requestedStochasticModel: "m0-fair-iid-2d6-v1",
    stochasticModel: "m0-fair-iid-2d6-v1",
    stochasticBeliefParticleCount: 64,
    rootIndex: 0,
    algorithm: "deep-maxn",
    authority: "deep-maxn",
    chosen,
    rootValue: [actions[0]?.value[0] ?? 0, 0, 0, 0],
    tacticalWinProbability: 0.31,
    tacticalLowerBound: 0.27,
    tacticalProven: false,
    exactDecision: false,
    exactWorlds: 1,
    tacticalLine: [chosen],
    actions,
    iterations: 4200,
    nodes: 18500,
    deepestDecisionDepth: 3,
    rollouts: 0,
    particles: 64,
    sourceWorldCount: 64,
    wasmParticleCount: 64,
    rustPosteriorParticleCount: 64,
    rustSearchParticleCount: 64,
    rootProvenance: {
      rankedRootCount: actions.length,
      rankedRoots: [],
      retainedRoots: [],
      prunedRootCount: 0,
      prunedRoots: [],
      rootEvidence: [],
    },
    authorityTrace: { initialAuthority: "deep-maxn" },
    effectiveParticleCount: 58,
    elapsedMs: 820,
    seed: 1,
  }) as unknown as DeepSearchResult;
