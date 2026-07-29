import initWasm, {
  analyze as analyzeWasm,
  engine_version as engineVersion,
  type WasmAction,
  type WasmActionStatistics,
  type WasmSearchResponse,
} from "../generated/wasm/colonist_search.js";
import {
  NUMBER_PIPS,
  type BoardSnapshot,
  type DevelopmentCardVector,
} from "../core/placement";
import {
  RESOURCE_ORDER,
  emptyResources,
  resourceTotal,
  type BuildKind,
  type Resource,
  type ResourceVector,
} from "../core/resources";
import type { DevCardKind, TrackerState } from "../core/types";
import type {
  DeepSearchAction,
  DeepSearchResult,
  DecisionAnalysis,
} from "../core/engine";

const RESOURCE_CODE = new Map<Resource, number>(
  RESOURCE_ORDER.map((resource, index) => [resource, index]),
);
const DEVELOPMENT_ORDER = [
  "knight",
  "victory-point",
  "road-building",
  "year-of-plenty",
  "monopoly",
] as const;
const DEVELOPMENT_TOTAL = [14, 5, 2, 2, 2] as const;
const MAX_PARTICLES = 96;
const MAX_INTERACTIVE_PARTICLES = 32;

let wasmReady: Promise<void> | undefined;

const ensureWasm = async (): Promise<void> => {
  wasmReady ??= initWasm({
    module_or_path: chrome.runtime.getURL("colonist_search_bg.wasm"),
  })
    .then(() => undefined)
    .catch((error: unknown) => {
      wasmReady = undefined;
      throw error;
    });
  return wasmReady;
};

export const warmDeepSearchEngine = async (): Promise<{
  engineRevision: string;
  initializationMs: number;
}> => {
  const startedAt = performance.now();
  await ensureWasm();
  return {
    engineRevision: engineVersion(),
    initializationMs: performance.now() - startedAt,
  };
};

const clampCard = (value: number): number =>
  Math.max(0, Math.min(255, Math.round(Number.isFinite(value) ? value : 0)));

const resources = (
  vector: Partial<ResourceVector> | undefined,
): [number, number, number, number, number] =>
  RESOURCE_ORDER.map((resource) =>
    clampCard(vector?.[resource] ?? 0),
  ) as [number, number, number, number, number];

const development = (
  vector: Partial<DevelopmentCardVector> | undefined,
): [number, number, number, number, number] =>
  DEVELOPMENT_ORDER.map((card) =>
    clampCard(vector?.[card] ?? 0),
  ) as [number, number, number, number, number];

const hashString = (value: string): number => {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
};

const mulberry32 = (seed: number): (() => number) => {
  let value = seed >>> 0;
  return () => {
    value += 0x6d2b79f5;
    let result = value;
    result = Math.imul(result ^ (result >>> 15), result | 1);
    result ^= result + Math.imul(result ^ (result >>> 7), result | 61);
    return ((result ^ (result >>> 14)) >>> 0) / 4294967296;
  };
};

const normalizedWorldWeight = (
  weight: number | undefined,
  fallback: number,
): number =>
  Number.isFinite(weight) && (weight ?? 0) > 0
    ? weight!
    : fallback;

const strategicWorldSignature = (
  world: TrackerState["worlds"][number],
  players: string[],
): string =>
  players
    .map((player) => {
      const hand = world.hands[player] ?? emptyResources();
      const affordable = [
        hand.lumber >= 1 && hand.brick >= 1,
        hand.lumber >= 1 &&
          hand.brick >= 1 &&
          hand.wool >= 1 &&
          hand.grain >= 1,
        hand.grain >= 2 && hand.ore >= 3,
        hand.wool >= 1 && hand.grain >= 1 && hand.ore >= 1,
      ]
        .map(Number)
        .join("");
      return `${resourceTotal(hand)}:${affordable}:${RESOURCE_ORDER.map(
        (resource) => Math.min(3, hand[resource]),
      ).join("")}`;
    })
    .join("|");

const worldDistance = (
  left: TrackerState["worlds"][number],
  right: TrackerState["worlds"][number],
  players: string[],
): number =>
  players.reduce((sum, player) => {
    const a = left.hands[player] ?? emptyResources();
    const b = right.hands[player] ?? emptyResources();
    return (
      sum +
      RESOURCE_ORDER.reduce(
        (resourceSum, resource) =>
          resourceSum + Math.abs(a[resource] - b[resource]),
        0,
      )
    );
  }, 0);

/**
 * Deterministic weighted stratification. It preserves strategically distinct
 * affordability worlds, then assigns every omitted world's posterior mass to
 * its nearest representative instead of silently dropping that mass.
 */
export const selectRepresentativeWorlds = (
  source: TrackerState["worlds"],
  players: string[],
  maximum = MAX_PARTICLES,
): TrackerState["worlds"] => {
  if (!source.length) return [];
  const rawTotal = source.reduce(
    (sum, world) =>
      sum + normalizedWorldWeight(world.weight, 1 / source.length),
    0,
  );
  const normalized = source.map((world) => ({
    ...world,
    weight:
      normalizedWorldWeight(world.weight, 1 / source.length) /
      Math.max(Number.EPSILON, rawTotal),
  }));
  if (normalized.length <= maximum) return normalized;

  const selected = new Set<number>();
  const buckets = new Map<string, number[]>();
  normalized.forEach((world, index) => {
    const signature = strategicWorldSignature(world, players);
    const bucket = buckets.get(signature) ?? [];
    bucket.push(index);
    buckets.set(signature, bucket);
  });
  const bucketLeaders = [...buckets.values()]
    .map((indices) =>
      [...indices].sort(
        (left, right) =>
          normalized[right]!.weight - normalized[left]!.weight ||
          left - right,
      )[0]!,
    )
    .sort(
      (left, right) =>
        normalized[right]!.weight - normalized[left]!.weight ||
        left - right,
    );
  for (const index of bucketLeaders.slice(0, Math.ceil(maximum / 2))) {
    selected.add(index);
  }

  const sorted = normalized
    .map((world, index) => ({ world, index }))
    .sort(
      (left, right) =>
        strategicWorldSignature(left.world, players).localeCompare(
          strategicWorldSignature(right.world, players),
        ) || left.index - right.index,
    );
  let cursor = 0;
  let cumulative = sorted[0]!.world.weight;
  for (let stratum = 0; selected.size < maximum; stratum += 1) {
    const target = (stratum + 0.5) / maximum;
    while (cursor < sorted.length - 1 && cumulative < target) {
      cursor += 1;
      cumulative += sorted[cursor]!.world.weight;
    }
    selected.add(sorted[cursor]!.index);
    if (stratum > maximum * 4 && selected.size < maximum) {
      const fallback = normalized.findIndex((_, index) => !selected.has(index));
      if (fallback < 0) break;
      selected.add(fallback);
    }
  }

  const indices = [...selected].slice(0, maximum);
  const representatives = indices.map((index) => ({
    ...normalized[index]!,
    weight: 0,
  }));
  for (const world of normalized) {
    let nearest = 0;
    let distance = Number.POSITIVE_INFINITY;
    representatives.forEach((representative, index) => {
      const candidate = worldDistance(world, representative, players);
      if (candidate < distance) {
        distance = candidate;
        nearest = index;
      }
    });
    representatives[nearest]!.weight += world.weight;
  }
  return representatives.filter((world) => world.weight > 0);
};

const sampleIndex = (counts: number[], random: () => number): number => {
  const total = counts.reduce((sum, count) => sum + count, 0);
  if (!total) return 0;
  let cursor = random() * total;
  for (let index = 0; index < counts.length; index += 1) {
    cursor -= counts[index] ?? 0;
    if (cursor < 0) return index;
  }
  return counts.length - 1;
};

const publicDevelopmentEvidence = (
  state: TrackerState,
): [number, number, number, number, number] => {
  const result = [0, 0, 0, 0, 0] as [
    number,
    number,
    number,
    number,
    number,
  ];
  const indexByCard: Record<DevCardKind, number | undefined> = {
    knight: 0,
    "victory-point": 1,
    "road-building": 2,
    "year-of-plenty": 3,
    monopoly: 4,
    unknown: undefined,
  };
  for (const player of Object.values(state.players)) {
    for (const [card, count] of Object.entries(player.playedDevCards) as Array<
      [DevCardKind, number]
    >) {
      const index = indexByCard[card];
      if (index !== undefined) {
        result[index] = (result[index] ?? 0) + clampCard(count);
      }
    }
  }
  return result;
};

const sampledDevelopmentWorld = (
  state: TrackerState,
  board: BoardSnapshot,
  players: string[],
  random: () => number,
): {
  hands: Array<[number, number, number, number, number]>;
  deck: [number, number, number, number, number];
} => {
  const played = publicDevelopmentEvidence(state);
  const remaining = DEVELOPMENT_TOTAL.map((total, index) =>
    Math.max(0, total - (played[index] ?? 0)),
  );
  const hands = players.map(
    () => [0, 0, 0, 0, 0] as [number, number, number, number, number],
  );
  const ownIndex = board.myPlayer ? players.indexOf(board.myPlayer) : -1;
  if (ownIndex >= 0) {
    const exact = development(board.ownDevelopmentCards?.cards);
    hands[ownIndex] = exact;
    for (let index = 0; index < exact.length; index += 1) {
      remaining[index] = Math.max(
        0,
        (remaining[index] ?? 0) - (exact[index] ?? 0),
      );
    }
  }
  for (let player = 0; player < players.length; player += 1) {
    if (player === ownIndex) continue;
    const count = clampCard(board.players?.[players[player]!]?.developmentCards ?? 0);
    for (let card = 0; card < count; card += 1) {
      const index = sampleIndex(remaining, random);
      if ((remaining[index] ?? 0) <= 0) break;
      hands[player]![index] = (hands[player]![index] ?? 0) + 1;
      remaining[index] = (remaining[index] ?? 0) - 1;
    }
  }
  return {
    hands,
    deck: remaining as [number, number, number, number, number],
  };
};

const playerNames = (
  state: TrackerState,
  board: BoardSnapshot,
): string[] => {
  const names = [
    ...(board.playerOrder ?? []),
    ...state.playerOrder,
    ...Object.keys(board.players ?? {}),
    ...board.vertices.flatMap((vertex) =>
      vertex.building ? [vertex.building.player] : [],
    ),
    ...board.edges.flatMap((edge) => (edge.player ? [edge.player] : [])),
  ];
  if (board.myPlayer) names.push(board.myPlayer);
  return [...new Set(names)].slice(0, 4);
};

const currentPlayerIndex = (
  state: TrackerState,
  board: BoardSnapshot,
  players: string[],
): number => {
  const current =
    board.currentPlayer ??
    (board.isMyTurn ? board.myPlayer : state.currentTurn.player) ??
    board.myPlayer;
  const index = current ? players.indexOf(current) : -1;
  return index >= 0 ? index : 0;
};

const inferPhase = (
  board: BoardSnapshot,
): { phase: string; parameter?: number } => {
  if (board.initialPlacement && board.action === "settlement") {
    return { phase: "setup-settlement" };
  }
  if (board.initialPlacement && board.action === "road") {
    return { phase: "setup-road" };
  }
  if (board.action === "discard") return { phase: "discard" };
  if (board.action === "robber") return { phase: "move-robber" };
  if (
    board.activeTrades?.some(
      (trade) =>
        (
          trade.incoming &&
          (!trade.myResponse || trade.myResponse === "pending")
        ) ||
        (
          !trade.incoming &&
          Boolean(
            trade.responsesComplete &&
            trade.acceptedPlayers?.length,
          )
        ),
    )
  ) {
    return { phase: "trade-responses" };
  }
  if (board.isMyTurn && board.hasRolled === false) return { phase: "pre-roll" };
  return { phase: "main" };
};

const matchingPrompt = (
  action: WasmAction,
  board: BoardSnapshot,
): boolean => {
  if (board.action === "road") {
    return action.kind === (board.initialPlacement ? "place-road" : "build-road");
  }
  if (board.action === "settlement") {
    return action.kind ===
      (board.initialPlacement ? "place-settlement" : "build-settlement");
  }
  if (board.action === "city") return action.kind === "build-city";
  if (board.action === "robber") {
    return action.kind === "move-robber" || action.kind === "play-knight";
  }
  if (board.action === "discard") return action.kind === "discard";
  return true;
};

const mapAction = (
  action: WasmAction,
  players: string[],
  board: BoardSnapshot,
): DeepSearchAction => {
  const first =
    action.kind === "place-road" ||
    action.kind === "build-road" ||
    action.kind === "play-road-building"
      ? action.first === undefined
        ? undefined
        : board.edges[action.first]?.id
      : action.kind === "move-robber" || action.kind === "play-knight"
        ? action.first === undefined
          ? undefined
          : board.hexes[action.first]?.id
        : action.kind === "place-settlement" ||
            action.kind === "build-settlement" ||
            action.kind === "build-city"
          ? action.first === undefined
            ? undefined
            : board.vertices[action.first]?.id
          : undefined;
  const second =
    action.second === undefined ? undefined : board.edges[action.second]?.id;
  const resource =
    action.resource === undefined ? undefined : RESOURCE_ORDER[action.resource];
  const otherResource =
    action.otherResource === undefined
      ? undefined
      : RESOURCE_ORDER[action.otherResource];
  return {
    kind: action.kind,
    ...(first ? { targetId: first } : {}),
    ...(second ? { secondTargetId: second } : {}),
    ...(action.player !== undefined && players[action.player]
      ? { player: players[action.player] }
      : {}),
    ...(resource ? { resource } : {}),
    ...(otherResource ? { otherResource } : {}),
    ...(action.kind === "maritime-trade" && action.first !== undefined
      ? { ratio: action.first }
      : {}),
    ...(action.cards ? { cards: action.cards } : {}),
    ...(action.receiveCards ? { receiveCards: action.receiveCards } : {}),
    ...(action.kind === "offer-trade" && action.first !== undefined
      ? {
          recipients: players.filter(
            (_, index) => ((action.first ?? 0) & (1 << index)) !== 0,
          ),
        }
      : {}),
    ...(action.accept !== undefined ? { accept: action.accept } : {}),
  };
};

const actionBuildKind = (kind: string): BuildKind | undefined => {
  if (kind === "build-road" || kind === "place-road") return "road";
  if (kind === "build-settlement" || kind === "place-settlement") {
    return "settlement";
  }
  if (kind === "build-city") return "city";
  if (kind === "buy-development") return "development";
  return undefined;
};

const deepActionScores = (
  actions: WasmActionStatistics[],
  root: number,
): Partial<Record<BuildKind, number>> => {
  const buckets = new Map<BuildKind, { value: number; visits: number }>();
  for (const statistic of actions) {
    const kind = actionBuildKind(statistic.action.kind);
    if (!kind) continue;
    const weight = Math.max(1, statistic.visits);
    const current = buckets.get(kind) ?? { value: 0, visits: 0 };
    current.value += (statistic.value[root] ?? 0) * weight;
    current.visits += weight;
    buckets.set(kind, current);
  }
  const means = [...buckets.entries()].map(([kind, bucket]) => ({
    kind,
    value: bucket.value / bucket.visits,
  }));
  const minimum = Math.min(...means.map((item) => item.value));
  const maximum = Math.max(...means.map((item) => item.value));
  return Object.fromEntries(
    means.map((item) => [
      item.kind,
      maximum > minimum
        ? ((item.value - minimum) / (maximum - minimum)) * 100
        : 50,
    ]),
  );
};

export const buildDeepSearchRequest = (
  state: TrackerState,
  board: BoardSnapshot,
  rootPlayer: string,
) => {
  const players = playerNames(state, board);
  if (players.length < 2 || players.length > 4) {
    throw new Error("Deep Search supports standard 2–4 player games");
  }
  const playerIndex = new Map(players.map((player, index) => [player, index]));
  const vertexIndex = new Map(
    board.vertices.map((vertex, index) => [vertex.id, index]),
  );
  const hexIndex = new Map(board.hexes.map((hex, index) => [hex.id, index]));
  let current = currentPlayerIndex(state, board, players);
  const root = Math.max(0, players.indexOf(rootPlayer));
  const phase = inferPhase(board);
  if (phase.phase.startsWith("setup") || board.action === "discard") {
    current = root;
  }
  const activeTrade = board.activeTrades?.find(
    (trade) =>
      (
        trade.incoming &&
        (!trade.myResponse || trade.myResponse === "pending")
      ) ||
      (
        !trade.incoming &&
        Boolean(
          trade.responsesComplete &&
          trade.acceptedPlayers?.length,
        )
      ),
  );
  const bitset = (names: string[] | undefined): number =>
    (names ?? []).reduce((mask, name) => {
      const index = players.indexOf(name);
      return index >= 0 ? mask | (1 << index) : mask;
    }, 0);
  const tradeCreator = activeTrade
    ? Math.max(0, players.indexOf(activeTrade.creator))
    : 0;
  const tradeRecipients = activeTrade
    ? activeTrade.incoming
      ? 1 << root
      : bitset([
          ...(activeTrade.acceptedPlayers ?? []),
          ...(activeTrade.pendingPlayers ?? []),
          ...(activeTrade.rejectedPlayers ?? []),
        ])
    : 0;
  const tradeCursor = activeTrade
    ? activeTrade.responsesComplete
      ? tradeCreator
      : (
          activeTrade.incoming
            ? root
            : players.findIndex((_, index) =>
                Boolean(tradeRecipients & (1 << index)) &&
                !Boolean(
                  bitset([
                    ...(activeTrade.acceptedPlayers ?? []),
                    ...(activeTrade.rejectedPlayers ?? []),
                  ]) & (1 << index),
                ),
              )
        )
    : 0;
  const adjacentEdges = new Map<string, number[]>(
    board.vertices.map((vertex) => [vertex.id, []]),
  );
  board.edges.forEach((edge, index) => {
    adjacentEdges.get(edge.vertices[0])?.push(index);
    adjacentEdges.get(edge.vertices[1])?.push(index);
  });
  const robberHex = Math.max(
    0,
    board.hexes.findIndex((hex) => hex.blocked),
  );
  const playedDevelopment = publicDevelopmentEvidence(state);
  const piecesByPlayer = players.map((player) => ({
    roads: board.edges.filter((edge) => edge.player === player).length,
    settlements: board.vertices.filter(
      (vertex) =>
        vertex.building?.player === player &&
        vertex.building.kind === "settlement",
    ).length,
    cities: board.vertices.filter(
      (vertex) =>
        vertex.building?.player === player && vertex.building.kind === "city",
    ).length,
  }));
  const exactOwnDevelopment = development(board.ownDevelopmentCards?.cards);
  const exactOwnBought = development(board.ownDevelopmentCards?.boughtThisTurn);
  const phaseParameter =
    phase.phase === "setup-road"
      ? board.vertices.findIndex(
          (vertex) =>
            vertex.building?.player === rootPlayer &&
            !board.edges.some(
              (edge) =>
                edge.player === rootPlayer &&
                edge.vertices.includes(vertex.id),
            ),
        )
      : undefined;
  const basePlayers = players.map((player, index) => {
    const pieces = piecesByPlayer[index]!;
    const publicState = board.players?.[player];
    const derivedPoints =
      pieces.settlements +
      pieces.cities * 2 +
      (publicState?.hasLongestRoad ? 2 : 0) +
      (publicState?.hasLargestArmy ? 2 : 0);
    const isOwn = player === board.myPlayer;
    return {
      resources: isOwn ? resources(board.ownHand) : [0, 0, 0, 0, 0],
      development: isOwn ? exactOwnDevelopment : [0, 0, 0, 0, 0],
      boughtDevelopment: isOwn ? exactOwnBought : [0, 0, 0, 0, 0],
      publicVictoryPoints: clampCard(
        Math.max(derivedPoints, publicState?.visiblePoints ?? 0),
      ),
      playedKnights: clampCard(
        Math.max(
          publicState?.playedKnights ?? 0,
          state.players[player]?.playedDevCards.knight ?? 0,
        ),
      ),
      roadsLeft: Math.max(0, 15 - pieces.roads),
      settlementsLeft: Math.max(0, 5 - pieces.settlements),
      citiesLeft: Math.max(0, 4 - pieces.cities),
      hasLongestRoad: Boolean(publicState?.hasLongestRoad),
      hasLargestArmy: Boolean(publicState?.hasLargestArmy),
      playedDevelopmentThisTurn:
        isOwn && Boolean(board.ownDevelopmentCards?.hasPlayedThisTurn),
      policyProfile: (() => {
        const posterior = state.players[player]?.opponentModel.policyPosterior;
        const probabilities = posterior
          ? [
              posterior.balanced,
              posterior.expansion,
              posterior.cityDevelopment,
              posterior.tradeFlexible,
              posterior.tradeResistant,
            ]
          : [0.2, 0.2, 0.2, 0.2, 0.2];
        const total = probabilities.reduce((sum, value) => sum + value, 0);
        return probabilities.map((value) =>
          Math.max(0, Math.min(255, Math.round((value / Math.max(Number.EPSILON, total)) * 255))),
        ) as [number, number, number, number, number];
      })(),
    };
  });
  const signature = JSON.stringify({
    game: board.gameKey,
    event: state.eventCount,
    turn: state.currentTurn.sequence,
    action: board.action,
    hand: board.ownHand,
    trades: board.activeTrades?.map((trade) => ({
      id: trade.id,
      incoming: trade.incoming,
      accepted: trade.acceptedPlayers,
      pending: trade.pendingPlayers,
      rejected: trade.rejectedPlayers,
      complete: trade.responsesComplete,
    })),
  });
  const seed = hashString(signature);
  const lastOwnRoll = state.recentEvents
    .map((event) => event.type === "roll" && event.player === rootPlayer)
    .lastIndexOf(true);
  const domesticTradeUsed =
    Boolean(board.domesticTradeUsed) ||
    Boolean(board.activeTrades?.some((trade) => !trade.incoming)) ||
    state.recentEvents
      .slice(lastOwnRoll >= 0 ? lastOwnRoll : state.recentEvents.length)
      .some(
        (event) =>
          event.type === "trade" &&
          !event.bank &&
          (event.player === rootPlayer ||
            event.acceptingPlayer === rootPlayer),
      );
  const hiddenDevelopmentCards = players.reduce(
    (sum, player) =>
      player === board.myPlayer
        ? sum
        : sum + (board.players?.[player]?.developmentCards ?? 0),
    0,
  );
  const sourceWorldLimit =
    hiddenDevelopmentCards > 0
      ? Math.floor(MAX_PARTICLES / 2)
      : MAX_PARTICLES;
  const sourceWorlds = state.worlds.length
    ? selectRepresentativeWorlds(
        state.worlds,
        players,
        sourceWorldLimit,
      )
    : [{
        hands: Object.fromEntries(
          players.map((player) => [player, emptyResources()]),
        ),
        weight: 1,
      }];
  const developmentSamples =
    hiddenDevelopmentCards > 0
      ? Math.max(
          2,
          Math.min(
            8,
            Math.floor(MAX_PARTICLES / Math.max(1, sourceWorlds.length)),
          ),
        )
      : 1;
  const rawWorlds = sourceWorlds.flatMap((world, worldIndex) =>
    Array.from({ length: developmentSamples }, (_, developmentSampleIndex) => {
    const random = mulberry32(
      seed ^
      Math.imul(worldIndex + 1, 0x9e3779b1) ^
      Math.imul(developmentSampleIndex + 1, 0x85ebca6b),
    );
    const developmentWorld = sampledDevelopmentWorld(
      state,
      board,
      players,
      random,
    );
    const own = resources(board.ownHand);
    const remainingPool = board.bankVisible && board.bank
      ? RESOURCE_ORDER.map((resource, index) =>
          Math.max(
            0,
            19 -
              board.bank![resource] -
              (own[index] ?? 0),
          ),
        )
      : RESOURCE_ORDER.map((_, index) =>
          Math.max(0, 19 - (own[index] ?? 0)),
        );
    const hands = players.map((player) => {
      if (player === board.myPlayer && board.ownHand) return own;
      const known = resources(world.hands[player]);
      const target = clampCard(
        board.players?.[player]?.handSize ??
          known.reduce((sum, count) => sum + count, 0),
      );
      const knownTotal = known.reduce((sum, count) => sum + count, 0);
      const sampled = knownTotal <= target
        ? [...known]
        : [0, 0, 0, 0, 0];
      for (let index = 0; index < sampled.length; index += 1) {
        remainingPool[index] = Math.max(
          0,
          (remainingPool[index] ?? 0) - (sampled[index] ?? 0),
        );
      }
      for (
        let missing = sampled.reduce((sum, count) => sum + count, 0);
        missing < target;
        missing += 1
      ) {
        const choices = remainingPool.some((count) => count > 0)
          ? remainingPool
          : [1, 1, 1, 1, 1];
        const index = sampleIndex(choices, random);
        sampled[index] = (sampled[index] ?? 0) + 1;
        remainingPool[index] = Math.max(
          0,
          (remainingPool[index] ?? 0) - 1,
        );
      }
      return sampled as [number, number, number, number, number];
    });
    const inferredBank = RESOURCE_ORDER.map((_, resourceIndex) =>
      Math.max(
        0,
        19 -
          hands.reduce(
            (sum, hand) => sum + (hand[resourceIndex] ?? 0),
            0,
          ),
      ),
    ) as [number, number, number, number, number];
    return {
      weight: world.weight / developmentSamples,
      hands,
      development: developmentWorld.hands,
      developmentDeck: developmentWorld.deck,
      bank:
        board.bankVisible && board.bank ? resources(board.bank) : inferredBank,
    };
  }));
  // Monte-Carlo development determinizations can collide. Merge identical
  // particles so ESS and downstream search effort reflect actual diversity.
  const mergedWorlds = new Map<string, (typeof rawWorlds)[number]>();
  for (const world of rawWorlds) {
    const key = JSON.stringify([
      world.hands,
      world.development,
      world.developmentDeck,
      world.bank,
    ]);
    const existing = mergedWorlds.get(key);
    if (existing) existing.weight += world.weight;
    else mergedWorlds.set(key, world);
  }
  let worlds = [...mergedWorlds.values()];
  if (worlds.length > MAX_INTERACTIVE_PARTICLES) {
    // Preserve the weighted posterior without sending dozens of near-duplicate
    // development-card determinizations through every interactive WASM call.
    // Deterministic systematic resampling covers the complete cumulative mass
    // (rather than taking the first N worlds) and merges repeated selections.
    const totalWeight = worlds.reduce(
      (sum, world) => sum + Math.max(0, world.weight),
      0,
    );
    const quantum = totalWeight / MAX_INTERACTIVE_PARTICLES;
    const selected = new Map<number, (typeof worlds)[number]>();
    let cursor = 0;
    let cumulative = Math.max(0, worlds[0]?.weight ?? 0);
    for (let sample = 0; sample < MAX_INTERACTIVE_PARTICLES; sample += 1) {
      const target = (sample + 0.5) * quantum;
      while (
        cursor < worlds.length - 1 &&
        cumulative + Number.EPSILON < target
      ) {
        cursor += 1;
        cumulative += Math.max(0, worlds[cursor]?.weight ?? 0);
      }
      const existing = selected.get(cursor);
      if (existing) {
        existing.weight += quantum;
      } else {
        selected.set(cursor, {
          ...worlds[cursor]!,
          weight: quantum,
        });
      }
    }
    worlds = [...selected.values()];
  }
  const baseWorld = worlds[0]!;
  const buildings = board.vertices.map((vertex) => {
    if (!vertex.building) return -1;
    const owner = playerIndex.get(vertex.building.player);
    if (owner === undefined) return -1;
    return owner * 2 + (vertex.building.kind === "city" ? 1 : 0);
  });
  const roads = board.edges.map((edge) =>
    edge.player === undefined ? -1 : (playerIndex.get(edge.player) ?? -1),
  );
  return {
    players,
    root,
    request: {
      state: {
        board: {
          hexes: board.hexes.map((hex) => ({
            resource:
              hex.resource === undefined
                ? -1
                : (RESOURCE_CODE.get(hex.resource) ?? -1),
            number: hex.number ?? 0,
          })),
          vertices: board.vertices.map((vertex) => ({
            adjacentHexes: vertex.adjacentHexes.flatMap((id) => {
              const index = hexIndex.get(id);
              return index === undefined ? [] : [index];
            }),
            adjacentVertices: vertex.adjacentVertices.flatMap((id) => {
              const index = vertexIndex.get(id);
              return index === undefined ? [] : [index];
            }),
            adjacentEdges: adjacentEdges.get(vertex.id) ?? [],
            port:
              vertex.port === undefined
                ? -1
                : vertex.port === "generic"
                  ? 5
                  : (RESOURCE_CODE.get(vertex.port) ?? -1),
          })),
          edges: board.edges.map((edge) => {
            const vertices = edge.vertices.map(
              (id) => vertexIndex.get(id) ?? 0,
            ) as [number, number];
            const adjacentHexes = board.vertices[vertices[0]]!.adjacentHexes
              .filter((id) =>
                board.vertices[vertices[1]]!.adjacentHexes.includes(id),
              )
              .flatMap((id) => {
                const index = hexIndex.get(id);
                return index === undefined ? [] : [index];
              });
            return { vertices, adjacentHexes };
          }),
        },
        players: basePlayers,
        worlds,
        buildings,
        roads,
        bank: baseWorld.bank,
        bankVisible: Boolean(board.bankVisible),
        developmentDeck: baseWorld.developmentDeck,
        playedDevelopment,
        robberHex,
        currentPlayer: current,
        phase: phase.phase,
        ...(phaseParameter !== undefined && phaseParameter >= 0
          ? { phaseParameter }
          : {}),
        turn: Math.max(0, board.turn ?? state.currentTurn.sequence),
        lastRoll: Math.max(0, board.lastRoll ?? 0),
        victoryTarget: board.victoryTarget ?? 10,
        setupStep: Math.min(
          players.length * 2,
          board.edges.filter((edge) => edge.player).length,
        ),
        discardRemaining: players
          .map((_, index) =>
            board.action === "discard" && index === root
              ? board.discardCount ?? 0
              : 0,
          )
          .concat([0, 0, 0, 0])
          .slice(0, 4),
        discardCursor: board.action === "discard" ? root : 0,
        robberReturnPhase: "main",
        domesticTradeUsed,
        ...(activeTrade
          ? {
              trade: {
                creator: tradeCreator,
                recipients: tradeRecipients,
                // Board evidence is normalized to the user's perspective.
                // The rules engine stores creator -> recipient orientation.
                give: resources(
                  activeTrade.incoming
                    ? activeTrade.receive
                    : activeTrade.give,
                ),
                receive: resources(
                  activeTrade.incoming
                    ? activeTrade.give
                    : activeTrade.receive,
                ),
                accepted: bitset(activeTrade.acceptedPlayers),
                rejected: bitset(activeTrade.rejectedPlayers),
              },
            }
          : {}),
        tradeCursor: Math.max(0, tradeCursor),
        longestRoadHolder:
          basePlayers.findIndex((player) => player.hasLongestRoad) >= 0
            ? basePlayers.findIndex((player) => player.hasLongestRoad)
            : undefined,
        largestArmyHolder:
          basePlayers.findIndex((player) => player.hasLargestArmy) >= 0
            ? basePlayers.findIndex((player) => player.hasLargestArmy)
            : undefined,
      },
      // Keep live decisions interactive. Longer searches belong in the
      // native arena/training pipeline; exact mandatory and tactical solvers
      // still run ahead of this bounded strategic search.
      iterations: players.length >= 3 ? 112 : 128,
      maxNodes: 2_000,
      rolloutActions: players.length >= 3 ? 72 : 84,
      tacticalDepth: 14,
      tacticalNodes: 560,
      seed,
      mode: "maxn",
      depth: 3,
      branchCap: 12,
      ponder: false,
    },
  };
};

export const analyzeDeepSearch = async (
  state: TrackerState,
  board: BoardSnapshot,
  rootPlayer: string,
  fallback: DecisionAnalysis,
  algorithm: "maxn" | "alpha-beta" | "puct" = "maxn",
): Promise<DecisionAnalysis> => {
  await ensureWasm();
  const { request, players, root } = buildDeepSearchRequest(
    state,
    board,
    rootPlayer,
  );
  request.mode = algorithm;
  if (algorithm === "maxn" || algorithm === "alpha-beta") {
    // One depth unit is a completed turn, not a UI click. A player-count
    // horizon therefore covers one complete table rotation. The previous
    // `players + 1` setting spent roughly twice the interactive latency for
    // almost identical live choices.
    request.depth = Math.min(4, Math.max(2, players.length));
    request.maxNodes = 2_000;
    request.tacticalNodes = 560;
  }
  request.branchCap = 12;
  if (request.state.phase === "trade-responses") {
    request.iterations = 48;
    request.maxNodes = 2_000;
    request.rolloutActions = 48;
    request.tacticalNodes = 560;
  } else if (!board.isMyTurn) {
    request.ponder = true;
    request.iterations = 64;
    request.maxNodes = 2_000;
    request.rolloutActions = 56;
    request.tacticalNodes = 480;
  }
  const startedAt = performance.now();
  const response = analyzeWasm(request) as WasmSearchResponse;
  const elapsedMs = performance.now() - startedAt;
  const allowed = response.actions.filter((statistic) =>
    matchingPrompt(statistic.action, board),
  );
  const selected =
    (matchingPrompt(response.chosen ?? { kind: "" }, board)
      ? response.chosen
      : undefined) ??
    allowed.sort(
      (left, right) =>
        right.visits - left.visits ||
        (right.value[root] ?? 0) - (left.value[root] ?? 0),
    )[0]?.action;
  const search: DeepSearchResult = {
    engineRevision: response.engineRevision,
    learnedModelVersion: response.learnedModelVersion,
    tradeModelVersion: response.tradeModelVersion,
    algorithm: response.algorithm,
    ...(selected ? { chosen: mapAction(selected, players, board) } : {}),
    rootValue: response.rootValue.slice(0, players.length),
    tacticalWinProbability: response.tacticalWinProbability,
    tacticalLowerBound: response.tacticalLowerBound,
    tacticalProven: response.tacticalProven,
    exactDecision: response.exactDecision,
    exactWorlds: response.exactWorlds,
    tacticalLine: response.tacticalLine.map((action) =>
      mapAction(action, players, board),
    ),
    actions: response.actions.map((statistics) => ({
      action: mapAction(statistics.action, players, board),
      visits: statistics.visits,
      availability: statistics.availability,
      availabilityWeight: statistics.availabilityWeight,
      legalWeight: statistics.legalWeight,
      prior: statistics.prior,
      value: statistics.value.slice(0, players.length),
      lowerConfidenceValue: statistics.lowerConfidenceValue.slice(
        0,
        players.length,
      ),
    })),
    iterations: response.iterations,
    nodes: response.nodes,
    deepestDecisionDepth: response.deepestDecisionDepth,
    rollouts: response.rollouts,
    particles: response.particles,
    effectiveParticleCount: response.effectiveParticleCount,
    elapsedMs,
    seed: request.seed,
  };
  const deepValueTotal = response.rootValue
    .slice(0, players.length)
    .reduce((sum, value) => sum + Math.max(0, value), 0);
  const victoryTarget = Math.max(1, board.victoryTarget ?? 10);
  const visibleProgress =
    Math.max(
      0,
      ...players.map(
        (player) => board.players?.[player]?.visiblePoints ?? 0,
      ),
    ) / victoryTarget;
  // Root utilities are strong action-ranking signals but are not calibrated
  // probabilities. Blend them with the independent rollout/ETA estimate,
  // trusting the strategic search more as public victory progress accumulates.
  const strategicProbabilityWeight = Math.min(
    algorithm === "puct" ? 0.62 : 0.76,
    (algorithm === "puct" ? 0.38 : 0.52) + visibleProgress * 0.24,
  );
  const deepPlayers =
    deepValueTotal > Number.EPSILON
      ? fallback.players.map((estimate) => {
          const player = players.indexOf(estimate.player);
          return player < 0
            ? estimate
            : {
                ...estimate,
                probability:
                  estimate.probability *
                    (1 - strategicProbabilityWeight) +
                  (
                    Math.max(0, response.rootValue[player] ?? 0) /
                    deepValueTotal
                  ) *
                    strategicProbabilityWeight,
                reasons: [
                  `${
                    response.algorithm === "puct"
                      ? "Belief PUCT"
                      : response.algorithm === "alpha-beta"
                        ? "Belief AlphaBeta"
                        : "Belief MaxN"
                  } relative race value from the current live board`,
                  ...estimate.reasons.filter(
                    (reason) =>
                      !reason.toLowerCase().includes("simulation"),
                  ),
                ],
              };
        })
      : fallback.players;
  return {
    ...fallback,
    engine:
      algorithm === "puct"
        ? "deep-puct"
        : algorithm === "alpha-beta"
          ? "deep-alpha-beta"
          : "deep-search",
    actionScores: {
      ...fallback.actionScores,
      ...deepActionScores(response.actions, root),
    },
    players: deepPlayers,
    simulations: response.rollouts,
    model:
      response.algorithm === "maxn"
        ? `Belief-aggregated multiplayer MaxN (${response.particles} particles, ${response.nodes.toLocaleString()} nodes, depth ${response.deepestDecisionDepth})`
        : response.algorithm === "alpha-beta"
          ? `Belief-aggregated paranoid AlphaBeta (${response.particles} particles, ${response.nodes.toLocaleString()} nodes, depth ${response.deepestDecisionDepth})`
          : `Belief-sampled multiplayer PUCT (${response.particles} particles, ${response.nodes.toLocaleString()} nodes, depth ${response.deepestDecisionDepth})`,
    deepSearch: search,
  };
};
