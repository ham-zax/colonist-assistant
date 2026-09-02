import initWasm, {
  analyze as analyzeWasm,
  engine_version as engineVersion,
  type WasmAction,
  type WasmActionReplacement,
  type WasmActionStatistics,
  type WasmAuthorityTrace,
  type WasmRootProvenance,
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
import type { TrackerState } from "../core/types";
import type {
  DeepSearchAction,
  DeepSearchResult,
  DecisionAnalysis,
  DecisionEngine,
  DecisionSearchConstraints,
} from "../core/engine";
import { isFullySpecifiedTrade } from "../core/trade-guard";

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
const MAX_INTERACTIVE_PARTICLES = 24;
const LIVE_WASM_DECISION_TIME_MS = 2_000;
const LIVE_WASM_TRADE_DECISION_TIME_MS = 1_500;
const LIVE_WASM_OPENING_DECISION_TIME_MS = 2_500;
const LIVE_WASM_PONDER_DECISION_TIME_MS = 3_000;

export type DeepSearchExecutor = (
  request: unknown,
) => Promise<WasmSearchResponse>;

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

const mixDimensionSeed = (seed: number, dimension: number): number => {
  let value = (seed ^ Math.imul(dimension + 1, 0x9e3779b1)) >>> 0;
  value ^= value >>> 16;
  value = Math.imul(value, 0x7feb352d) >>> 0;
  value ^= value >>> 15;
  value = Math.imul(value, 0x846ca68b) >>> 0;
  value ^= value >>> 16;
  return value >>> 0;
};

const greatestCommonDivisor = (left: number, right: number): number => {
  let a = Math.abs(Math.trunc(left));
  let b = Math.abs(Math.trunc(right));
  while (b) {
    const next = a % b;
    a = b;
    b = next;
  }
  return a;
};

const stratifiedUnit = (
  seed: number,
  stratum: number,
  strata: number,
  dimension: number,
): number => {
  const count = Math.max(1, Math.floor(strata));
  if (count === 1) {
    return mixDimensionSeed(seed, dimension) / 0x1_0000_0000;
  }
  let multiplier = (mixDimensionSeed(seed, dimension) % count) || 1;
  while (greatestCommonDivisor(multiplier, count) !== 1) {
    multiplier = (multiplier + 1) % count || 1;
  }
  const shift = mixDimensionSeed(seed, dimension ^ 0x85ebca6b) % count;
  const offset =
    mixDimensionSeed(seed, dimension ^ 0xc2b2ae35) / 0x1_0000_0000;
  const bucket = (multiplier * stratum + shift) % count;
  return (bucket + offset) / count;
};

const RESOURCE_WORLD_DIMENSION = 0x1000;
const RESOURCE_SLOT_DIMENSION = 0x2000;
const DEVELOPMENT_SLOT_DIMENSION = 0x4000;

const normalizedWorldWeight = (
  weight: number | undefined,
  fallback: number,
): number =>
  Number.isFinite(weight) && (weight ?? 0) > 0
    ? weight!
    : fallback;

const resourceWorldKey = (
  world: TrackerState["worlds"][number],
  players: string[],
): string =>
  JSON.stringify(
    players.map((player) => resources(world.hands[player])),
  );

const normalizedResourceWorlds = (
  source: TrackerState["worlds"],
  players: string[],
): TrackerState["worlds"] => {
  if (!source.length) return [];
  const fallback = 1 / source.length;
  const merged = new Map<string, TrackerState["worlds"][number]>();
  for (const world of source) {
    const key = resourceWorldKey(world, players);
    const weight = normalizedWorldWeight(world.weight, fallback);
    const existing = merged.get(key);
    if (existing) {
      existing.weight += weight;
    } else {
      merged.set(key, {
        ...world,
        weight,
        hands: Object.fromEntries(
          Object.entries(world.hands).map(([player, hand]) => [
            player,
            { ...hand },
          ]),
        ),
      });
    }
  }
  const total = [...merged.values()]
    .reduce((sum, world) => sum + world.weight, 0)
    || 1;
  return [...merged.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([, world]) => ({
      ...world,
      weight: world.weight / total,
    }));
};

const weightedIndex = (weights: number[], unit: number): number => {
  const total = weights.reduce((sum, weight) => sum + Math.max(0, weight), 0);
  if (total <= 0) return 0;
  let target = Math.max(0, Math.min(1 - Number.EPSILON, unit)) * total;
  for (let index = 0; index < weights.length; index += 1) {
    target -= Math.max(0, weights[index] ?? 0);
    if (target < 0) return index;
  }
  return weights.length - 1;
};

/**
 * Lossy resource-only sampling for tests/replay. Production joint sampling does
 * not pre-cap resources. When sampling is required, each final stratum carries
 * exactly 1/N mass; omitted mass is never reassigned to a nearest different
 * world.
 */
export const selectRepresentativeWorlds = (
  source: TrackerState["worlds"],
  players: string[],
  maximum = MAX_PARTICLES,
  seed = 0,
): TrackerState["worlds"] => {
  const normalized = normalizedResourceWorlds(source, players);
  const limit = Math.min(
    MAX_PARTICLES,
    Math.max(1, Math.floor(maximum)),
  );
  if (normalized.length <= limit) return normalized;
  const weights = normalized.map((world) => world.weight);
  const selected = new Map<string, TrackerState["worlds"][number]>();
  for (let stratum = 0; stratum < limit; stratum += 1) {
    const index = weightedIndex(
      weights,
      stratifiedUnit(seed, stratum, limit, RESOURCE_WORLD_DIMENSION),
    );
    const world = normalized[index]!;
    const key = resourceWorldKey(world, players);
    const existing = selected.get(key);
    if (existing) {
      existing.weight += 1 / limit;
    } else {
      selected.set(key, {
        ...world,
        weight: 1 / limit,
        hands: Object.fromEntries(
          Object.entries(world.hands).map(([player, hand]) => [
            player,
            { ...hand },
          ]),
        ),
      });
    }
  }
  return [...selected.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([, world]) => world);
};

const sampledCountIndex = (counts: number[], unit: number): number =>
  weightedIndex(counts, unit);

const publicDevelopmentEvidence = (
  state: TrackerState,
  board?: BoardSnapshot,
): [number, number, number, number, number] => {
  const result = [0, 0, 0, 0, 0] as [
    number,
    number,
    number,
    number,
    number,
  ];
  const playerNames = new Set([
    ...Object.keys(state.players),
    ...Object.keys(board?.players ?? {}),
  ]);
  for (const playerName of playerNames) {
    const tracked = state.players[playerName]?.playedDevCards;
    const publicPlayed = board?.players?.[playerName]?.playedDevelopmentCards;
    DEVELOPMENT_ORDER.forEach((card, index) => {
      result[index] =
        (result[index] ?? 0) +
        Math.max(tracked?.[card] ?? 0, publicPlayed?.[card] ?? 0);
    });
  }
  return result;
};

const developmentStateIntegrityError = (detail: string): Error =>
  new Error(`Deep Search development-card state integrity error: ${detail}`);

type CardTuple = [number, number, number, number, number];

interface DevelopmentSlot {
  player: number;
  playerName: string;
  boughtThisTurn: boolean;
}

interface DevelopmentSamplingBase {
  hands: CardTuple[];
  bought: CardTuple[];
  remaining: CardTuple;
  slots: DevelopmentSlot[];
}

const developmentSamplingBase = (
  state: TrackerState,
  board: BoardSnapshot,
  players: string[],
): DevelopmentSamplingBase => {
  const played = publicDevelopmentEvidence(state, board);
  const remaining = DEVELOPMENT_TOTAL.map((total, index) => {
    const playedCount = played[index] ?? 0;
    const available = total - playedCount;
    if (available < 0) {
      throw developmentStateIntegrityError(
        `${DEVELOPMENT_ORDER[index]} public plays (${playedCount}) exceed the base-deck count (${total})`,
      );
    }
    return available;
  }) as CardTuple;
  const hands = players.map(() => [0, 0, 0, 0, 0] as CardTuple);
  const bought = players.map(() => [0, 0, 0, 0, 0] as CardTuple);
  const ownIndex = board.myPlayer ? players.indexOf(board.myPlayer) : -1;
  if (ownIndex >= 0) {
    const exact = development(board.ownDevelopmentCards?.cards);
    const exactBought = development(board.ownDevelopmentCards?.boughtThisTurn);
    hands[ownIndex] = exact;
    bought[ownIndex] = exactBought;
    for (let index = 0; index < exact.length; index += 1) {
      const exactCount = exact[index] ?? 0;
      const boughtCount = exactBought[index] ?? 0;
      const available = remaining[index] ?? 0;
      if (boughtCount > exactCount) {
        throw developmentStateIntegrityError(
          `exact local bought-this-turn ${DEVELOPMENT_ORDER[index]} count (${boughtCount}) exceeds the held count (${exactCount})`,
        );
      }
      if (exactCount > available) {
        throw developmentStateIntegrityError(
          `exact local ${DEVELOPMENT_ORDER[index]} holdings (${exactCount}) exceed the ${available} cards left after public plays`,
        );
      }
      remaining[index] = available - exactCount;
    }
  }

  const slots: DevelopmentSlot[] = [];
  for (let player = 0; player < players.length; player += 1) {
    if (player === ownIndex) continue;
    const playerName = players[player]!;
    const count = board.players?.[playerName]?.developmentCards ?? 0;
    if (!Number.isInteger(count) || count < 0) {
      throw developmentStateIntegrityError(
        `${playerName} has invalid public hidden-card count ${count}`,
      );
    }
    const boughtThisTurn = Math.min(
      count,
      state.players[playerName]?.devCards.filter(
        (card) => card.boughtOnTurn >= state.currentTurn.sequence,
      ).length ?? 0,
    );
    const ready = count - boughtThisTurn;
    for (let slot = 0; slot < ready; slot += 1) {
      slots.push({ player, playerName, boughtThisTurn: false });
    }
    for (let slot = 0; slot < boughtThisTurn; slot += 1) {
      slots.push({ player, playerName, boughtThisTurn: true });
    }
  }

  const hiddenAvailable = remaining.reduce((sum, count) => sum + count, 0);
  if (slots.length > hiddenAvailable) {
    throw developmentStateIntegrityError(
      `${slots.length} hidden opponent cards are required but only ${hiddenAvailable} development cards remain after public plays and the exact local hand`,
    );
  }
  return { hands, bought, remaining, slots };
};

const sampledDevelopmentWorld = (
  base: DevelopmentSamplingBase,
  seed: number,
  stratum: number,
  strata: number,
): {
  hands: CardTuple[];
  bought: CardTuple[];
  deck: CardTuple;
} => {
  const remaining = [...base.remaining] as CardTuple;
  const hands = base.hands.map((hand) => [...hand] as CardTuple);
  const bought = base.bought.map((hand) => [...hand] as CardTuple);
  for (let slotIndex = 0; slotIndex < base.slots.length; slotIndex += 1) {
    const slot = base.slots[slotIndex]!;
    const card = sampledCountIndex(
      remaining,
      stratifiedUnit(
        seed,
        stratum,
        strata,
        DEVELOPMENT_SLOT_DIMENSION + slotIndex,
      ),
    );
    if ((remaining[card] ?? 0) <= 0) {
      throw developmentStateIntegrityError(
        `sampler exhausted the development deck while assigning ${slot.playerName}`,
      );
    }
    hands[slot.player]![card] = (hands[slot.player]![card] ?? 0) + 1;
    if (slot.boughtThisTurn) {
      bought[slot.player]![card] = (bought[slot.player]![card] ?? 0) + 1;
    }
    remaining[card] = (remaining[card] ?? 0) - 1;
  }
  return { hands, bought, deck: remaining };
};

const playerNames = (
  state: TrackerState,
  board: BoardSnapshot,
): string[] => {
  if (board.playerOrder?.length) {
    return [...new Set(board.playerOrder)];
  }
  const names = [
    ...state.playerOrder,
    ...Object.keys(board.players ?? {}),
    ...board.vertices.flatMap((vertex) =>
      vertex.building ? [vertex.building.player] : [],
    ),
    ...board.edges.flatMap((edge) => (edge.player ? [edge.player] : [])),
  ];
  if (board.myPlayer) names.push(board.myPlayer);
  return [...new Set(names)];
};

const currentPlayerIndex = (
  state: TrackerState,
  board: BoardSnapshot,
  players: string[],
): number => {
  const current =
    (board.action === "discard" ? state.currentTurn.player : undefined) ??
    board.currentPlayer ??
    (board.isMyTurn ? board.myPlayer : state.currentTurn.player) ??
    board.myPlayer;
  if (!current) {
    throw new Error("Deep Search could not resolve the current player");
  }
  const index = players.indexOf(current);
  if (index < 0) {
    throw new Error(`Deep Search current player is unknown: ${current}`);
  }
  return index;
};

const isProtocolActiveTrade = (
  trade: NonNullable<BoardSnapshot["activeTrades"]>[number],
): boolean =>
  isFullySpecifiedTrade(trade) &&
  ((trade.incoming && (!trade.myResponse || trade.myResponse === "pending")) ||
    (!trade.incoming &&
      trade.responsesComplete === true &&
      Boolean(
        trade.acceptedPlayers?.length ||
          trade.rejectedPlayers?.length ||
          trade.embargoedPlayers?.length,
      )));

const inferPhase = (
  board: BoardSnapshot,
  actingPlayer?: string,
): { phase: string; parameter?: number } => {
  if (board.initialPlacement && board.action === "settlement") {
    return { phase: "setup-settlement" };
  }
  if (board.initialPlacement && board.action === "road") {
    const anchor = actingPlayer
      ? board.vertices.findIndex(
          (vertex) =>
            vertex.building?.player === actingPlayer &&
            !board.edges.some(
              (edge) =>
                edge.player === actingPlayer &&
                edge.vertices.includes(vertex.id),
            ),
        )
      : -1;
    // Colonist can publish the previous player's road prompt for one snapshot
    // after control has advanced. Never construct an invalid setup-road state
    // for a player who does not yet own its settlement anchor.
    return anchor >= 0
      ? { phase: "setup-road", parameter: anchor }
      : { phase: "setup-settlement" };
  }
  if (board.action === "discard") return { phase: "discard" };
  if (board.action === "robber") return { phase: "move-robber" };
  if (board.activeTrades?.some(isProtocolActiveTrade)) {
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
  const activeTradeId = board.activeTrades?.find(isProtocolActiveTrade)?.id;
  const tradeBound =
    action.kind === "respond-trade" ||
    action.kind === "counter-trade" ||
    action.kind === "confirm-trade" ||
    action.kind === "cancel-trade";
  return {
    kind: action.kind,
    ...(tradeBound && activeTradeId ? { tradeId: activeTradeId } : {}),
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

const mapActionReplacement = (
  replacement: WasmActionReplacement | undefined,
  players: string[],
  board: BoardSnapshot,
) =>
  replacement
    ? {
        from: mapAction(replacement.from, players, board),
        to: mapAction(replacement.to, players, board),
      }
    : undefined;

const mapRootProvenance = (
  provenance: WasmRootProvenance,
  players: string[],
  board: BoardSnapshot,
) => {
  const exactFamilyReplacement = mapActionReplacement(
    provenance.exactFamilyReplacement,
    players,
    board,
  );
  const safetyReplacement = mapActionReplacement(
    provenance.safetyReplacement,
    players,
    board,
  );
  return {
    rankedRootCount: provenance.rankedRootCount,
    rankedRoots: provenance.rankedRoots.map((candidate) => ({
      action: mapAction(candidate.action, players, board),
      rank: candidate.rank,
      prior: candidate.prior,
      ...(candidate.plannerValue !== undefined
        ? { plannerValue: candidate.plannerValue }
        : {}),
      ...(candidate.plannerCompletionMass !== undefined
        ? { plannerCompletionMass: candidate.plannerCompletionMass }
        : {}),
    })),
    retainedRoots: provenance.retainedRoots.map((candidate) => ({
      action: mapAction(candidate.action, players, board),
      ...(candidate.preTruncationRank !== undefined
        ? { preTruncationRank: candidate.preTruncationRank }
        : {}),
      prior: candidate.prior,
      nodeBudgetPerParticle: candidate.nodeBudgetPerParticle,
      allocatedNodes: candidate.allocatedNodes,
      ...(candidate.plannerValue !== undefined
        ? { plannerValue: candidate.plannerValue }
        : {}),
      ...(candidate.plannerCompletionMass !== undefined
        ? { plannerCompletionMass: candidate.plannerCompletionMass }
        : {}),
      ...(typeof candidate.finalRank === "number"
        ? { finalRank: candidate.finalRank }
        : {}),
      ...(typeof candidate.terminalOutcome === "number"
        ? { terminalOutcome: candidate.terminalOutcome }
        : {}),
      ...(typeof candidate.terminalLowerBound === "number"
        ? { terminalLowerBound: candidate.terminalLowerBound }
        : {}),
      ...(typeof candidate.terminalUpperBound === "number"
        ? { terminalUpperBound: candidate.terminalUpperBound }
        : {}),
      ...(typeof candidate.victoryMargin === "number"
        ? { victoryMargin: candidate.victoryMargin }
        : {}),
      ...(typeof candidate.victoryMarginLowerBound === "number"
        ? { victoryMarginLowerBound: candidate.victoryMarginLowerBound }
        : {}),
      ...(typeof candidate.victoryMarginUpperBound === "number"
        ? { victoryMarginUpperBound: candidate.victoryMarginUpperBound }
        : {}),
      ...(typeof candidate.meanTurn === "number"
        ? { meanTurn: candidate.meanTurn }
        : {}),
    })),
    prunedRootCount: provenance.prunedRootCount,
    prunedRoots: provenance.prunedRoots.map((candidate) => ({
      action: mapAction(candidate.action, players, board),
      ...(candidate.preTruncationRank !== undefined
        ? { preTruncationRank: candidate.preTruncationRank }
        : {}),
      reason: candidate.reason,
    })),
    ...(exactFamilyReplacement ? { exactFamilyReplacement } : {}),
    ...(safetyReplacement ? { safetyReplacement } : {}),
  };
};

const mapAuthorityTrace = (
  trace: WasmAuthorityTrace,
  players: string[],
  board: BoardSnapshot,
) => {
  const exactFamilyReplacement = mapActionReplacement(
    trace.exactFamilyReplacement,
    players,
    board,
  );
  const safetyReplacement = mapActionReplacement(
    trace.safetyReplacement,
    players,
    board,
  );
  return {
    initialAuthority: trace.initialAuthority,
    ...(trace.exactFamily ? { exactFamily: trace.exactFamily } : {}),
    ...(exactFamilyReplacement ? { exactFamilyReplacement } : {}),
    ...(safetyReplacement ? { safetyReplacement } : {}),
  };
};

interface SampledResourceWorld {
  hands: CardTuple[];
  bank: CardTuple;
}

interface JointParticleWorld extends SampledResourceWorld {
  weight: number;
  development: CardTuple[];
  boughtDevelopment: CardTuple[];
  developmentDeck: CardTuple;
}

const resourceWorldFeasible = (
  world: TrackerState["worlds"][number],
  players: string[],
  board: BoardSnapshot,
): boolean => {
  const own = resources(board.ownHand);
  const remaining = RESOURCE_ORDER.map((resource, index) => {
    const bank = board.bankVisible && board.bank ? board.bank[resource] : 0;
    return 19 - bank - (own[index] ?? 0);
  });
  if (remaining.some((count) => count < 0)) return false;
  let missingTotal = 0;
  for (let player = 0; player < players.length; player += 1) {
    const playerName = players[player]!;
    if (playerName === board.myPlayer) continue;
    const known = resources(world.hands[playerName]);
    const target = clampCard(
      board.players?.[playerName]?.handSize ?? resourceTotal(world.hands[playerName] ?? emptyResources()),
    );
    const knownTotal = known.reduce((sum, count) => sum + count, 0);
    if (knownTotal > target) return false;
    for (let resource = 0; resource < known.length; resource += 1) {
      if ((known[resource] ?? 0) > (remaining[resource] ?? 0)) return false;
      remaining[resource] = (remaining[resource] ?? 0) - (known[resource] ?? 0);
    }
    missingTotal += target - knownTotal;
  }
  const available = remaining.reduce((sum, count) => sum + count, 0);
  return missingTotal <= available &&
    (!board.bankVisible || !board.bank || missingTotal === available);
};

const resourceCompletionRequired = (
  world: TrackerState["worlds"][number],
  players: string[],
  board: BoardSnapshot,
): boolean =>
  players.some((player) => {
    if (player === board.myPlayer) return false;
    const known = resourceTotal(world.hands[player] ?? emptyResources());
    const target = board.players?.[player]?.handSize ?? known;
    return known < target;
  });

const sampledResourceWorld = (
  world: TrackerState["worlds"][number],
  players: string[],
  board: BoardSnapshot,
  seed: number,
  stratum: number,
  strata: number,
): SampledResourceWorld | undefined => {
  const own = resources(board.ownHand);
  const remaining = RESOURCE_ORDER.map((resource, index) => {
    const bank = board.bankVisible && board.bank ? board.bank[resource] : 0;
    return 19 - bank - (own[index] ?? 0);
  }) as CardTuple;
  if (remaining.some((count) => count < 0)) return undefined;

  const hands = players.map((player) =>
    player === board.myPlayer
      ? ([...own] as CardTuple)
      : resources(world.hands[player]),
  );
  const missing = new Array<number>(players.length).fill(0);
  for (let player = 0; player < players.length; player += 1) {
    const playerName = players[player]!;
    if (playerName === board.myPlayer) continue;
    const hand = hands[player]!;
    const target = clampCard(
      board.players?.[playerName]?.handSize ?? hand.reduce((sum, count) => sum + count, 0),
    );
    const knownTotal = hand.reduce((sum, count) => sum + count, 0);
    if (knownTotal > target) return undefined;
    missing[player] = target - knownTotal;
    for (let resource = 0; resource < hand.length; resource += 1) {
      const count = hand[resource] ?? 0;
      if (count > (remaining[resource] ?? 0)) return undefined;
      remaining[resource] = (remaining[resource] ?? 0) - count;
    }
  }
  const required = missing.reduce((sum, count) => sum + count, 0);
  const available = remaining.reduce((sum, count) => sum + count, 0);
  if (
    required > available ||
    (board.bankVisible && board.bank && required !== available)
  ) {
    return undefined;
  }

  for (let player = 0; player < players.length; player += 1) {
    for (let slot = 0; slot < (missing[player] ?? 0); slot += 1) {
      const resource = sampledCountIndex(
        remaining,
        stratifiedUnit(
          seed,
          stratum,
          strata,
          RESOURCE_SLOT_DIMENSION + player * 64 + slot,
        ),
      );
      if ((remaining[resource] ?? 0) <= 0) return undefined;
      hands[player]![resource] = (hands[player]![resource] ?? 0) + 1;
      remaining[resource] = (remaining[resource] ?? 0) - 1;
    }
  }
  const bank = board.bankVisible && board.bank
    ? resources(board.bank)
    : ([...remaining] as CardTuple);
  return { hands, bank };
};

const jointWorldKey = (world: Omit<JointParticleWorld, "weight">): string =>
  JSON.stringify([
    world.hands,
    world.development,
    world.boughtDevelopment,
    world.developmentDeck,
    world.bank,
  ]);

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
  searchConstraints: DecisionSearchConstraints = {},
  playerTradesEnabled = true,
  particleLimit = MAX_INTERACTIVE_PARTICLES,
) => {
  const players = playerNames(state, board);
  if (players.length < 2 || players.length > 4) {
    throw new Error("Deep Search supports standard 2–4 player games");
  }
  const discardLimits = [
    ...new Set(
      players
        .map((player) => board.players?.[player]?.cardDiscardLimit)
        .filter(
          (limit): limit is number =>
            typeof limit === "number" && Number.isInteger(limit),
        ),
    ),
  ];
  if (discardLimits.length > 1) {
    throw new Error("Deep Search received inconsistent card discard limits");
  }
  const cardDiscardLimit = discardLimits[0] ?? 7;
  const playerIndex = new Map(players.map((player, index) => [player, index]));
  const vertexIndex = new Map(
    board.vertices.map((vertex, index) => [vertex.id, index]),
  );
  const hexIndex = new Map(board.hexes.map((hex, index) => [hex.id, index]));
  const root = players.indexOf(rootPlayer);
  if (root < 0) {
    throw new Error(`Deep Search root player is unknown: ${rootPlayer}`);
  }
  const current = currentPlayerIndex(state, board, players);

  for (const vertex of board.vertices) {
    for (const hexId of vertex.adjacentHexes) {
      if (!hexIndex.has(hexId)) {
        throw new Error(
          `Deep Search topology has unknown hex ${hexId} adjacent to vertex ${vertex.id}`,
        );
      }
    }
    for (const vertexId of vertex.adjacentVertices) {
      if (!vertexIndex.has(vertexId)) {
        throw new Error(
          `Deep Search topology has unknown vertex ${vertexId} adjacent to vertex ${vertex.id}`,
        );
      }
    }
  }
  for (const edge of board.edges) {
    for (const vertexId of edge.vertices) {
      if (!vertexIndex.has(vertexId)) {
        throw new Error(
          `Deep Search edge ${edge.id} has unknown vertex endpoint ${vertexId}`,
        );
      }
    }
  }
  const blockedHexes = board.hexes
    .map((hex, index) => ({ hex, index }))
    .filter(({ hex }) => hex.blocked);
  if (blockedHexes.length !== 1) {
    throw new Error(
      `Deep Search requires exactly one robber location; found ${blockedHexes.length}`,
    );
  }
  const phase = inferPhase(board, players[current] ?? rootPlayer);
  const activeTrade = board.activeTrades?.find(isProtocolActiveTrade);
  const requirePlayerIndex = (name: string, context: string): number => {
    const index = playerIndex.get(name);
    if (index === undefined) {
      throw new Error(`Deep Search ${context} references unknown player: ${name}`);
    }
    return index;
  };
  const bitset = (
    names: string[] | undefined,
    context = "trade player",
  ): number =>
    (names ?? []).reduce(
      (mask, name) => mask | (1 << requirePlayerIndex(name, context)),
      0,
    );
  const tradeCreator = activeTrade
    ? requirePlayerIndex(activeTrade.creator, "trade creator")
    : 0;
  const embargoBit = (embargoer: number, blocked: number): number =>
    1 << (embargoer * 4 + blocked);
  let domesticTradeEmbargoes = 0;
  for (const [embargoer, blockedPlayers] of Object.entries(state.tradeEmbargoes ?? {})) {
    const embargoerIndex = playerIndex.get(embargoer);
    if (embargoerIndex === undefined) continue;
    for (const blocked of blockedPlayers) {
      const blockedIndex = playerIndex.get(blocked);
      if (blockedIndex === undefined || blockedIndex === embargoerIndex) continue;
      domesticTradeEmbargoes |= embargoBit(embargoerIndex, blockedIndex);
    }
  }
  for (const trade of board.activeTrades ?? []) {
    const creatorIndex = playerIndex.get(trade.creator);
    if (creatorIndex === undefined) continue;
    for (const embargoer of trade.embargoedPlayers ?? []) {
      const embargoerIndex = playerIndex.get(embargoer);
      if (embargoerIndex === undefined || embargoerIndex === creatorIndex) continue;
      domesticTradeEmbargoes |= embargoBit(embargoerIndex, creatorIndex);
    }
  }
  const observedTradeRecipients = activeTrade
    ? bitset([
        ...(activeTrade.acceptedPlayers ?? []),
        ...(activeTrade.pendingPlayers ?? []),
        ...(activeTrade.rejectedPlayers ?? []),
        ...(activeTrade.embargoedPlayers ?? []),
      ]) & ~(1 << tradeCreator)
    : 0;
  const tradeRecipients = activeTrade
    ? observedTradeRecipients || (activeTrade.incoming ? 1 << root : 0)
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
                    ...(activeTrade.embargoedPlayers ?? []),
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
  const robberHex = blockedHexes[0]!.index;
  const playedDevelopment = publicDevelopmentEvidence(state, board);
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
          publicState?.playedDevelopmentCards?.knight ?? 0,
          state.players[player]?.playedDevCards.knight ?? 0,
        ),
      ),
      roadsLeft: Math.max(0, 15 - pieces.roads),
      settlementsLeft: Math.max(0, 5 - pieces.settlements),
      citiesLeft: Math.max(0, 4 - pieces.cities),
      hasLongestRoad: Boolean(publicState?.hasLongestRoad),
      hasLargestArmy: Boolean(publicState?.hasLargestArmy),
      playedDevelopmentThisTurn:
        isOwn && board.ownDevelopmentCards
          ? Boolean(board.ownDevelopmentCards.hasPlayedThisTurn)
          : Boolean(publicState?.hasPlayedDevelopmentThisTurn),
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
    searchConstraints,
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
  if (!state.worlds.length) {
    throw new Error("Deep Search has no resource worlds consistent with public evidence");
  }
  const finalParticleLimit = Math.min(
    MAX_PARTICLES,
    Math.max(1, Math.floor(particleLimit)),
  );
  const normalizedResources = normalizedResourceWorlds(state.worlds, players);
  const feasibleResources = normalizedResources.filter((world) =>
    resourceWorldFeasible(world, players, board),
  );
  if (!feasibleResources.length) {
    throw new Error("Deep Search could not construct a resource world consistent with public evidence");
  }
  const feasibleMass = feasibleResources.reduce(
    (sum, world) => sum + world.weight,
    0,
  );
  const sourceWorlds = feasibleResources.map((world) => ({
    ...world,
    weight: world.weight / Math.max(Number.EPSILON, feasibleMass),
  }));
  const sourceWeights = sourceWorlds.map((world) => world.weight);
  const developmentBase = developmentSamplingBase(state, board, players);
  const hasHiddenDevelopmentIdentity = developmentBase.slots.length > 0;
  const hasIncompleteResourceWorld = sourceWorlds.some((world) =>
    resourceCompletionRequired(world, players, board),
  );
  const exactJointPosterior =
    !hasHiddenDevelopmentIdentity &&
    !hasIncompleteResourceWorld &&
    sourceWorlds.length <= finalParticleLimit;
  const mergedWorlds = new Map<string, JointParticleWorld>();
  const addParticle = (particle: JointParticleWorld): void => {
    const key = jointWorldKey(particle);
    const existing = mergedWorlds.get(key);
    if (existing) existing.weight += particle.weight;
    else mergedWorlds.set(key, particle);
  };

  if (exactJointPosterior) {
    const developmentWorld = sampledDevelopmentWorld(
      developmentBase,
      seed,
      0,
      1,
    );
    for (const world of sourceWorlds) {
      const resourceWorld = sampledResourceWorld(
        world,
        players,
        board,
        seed,
        0,
        1,
      );
      if (!resourceWorld) continue;
      addParticle({
        weight: world.weight,
        ...resourceWorld,
        development: developmentWorld.hands.map((hand) => [...hand] as CardTuple),
        boughtDevelopment: developmentWorld.bought.map(
          (hand) => [...hand] as CardTuple,
        ),
        developmentDeck: [...developmentWorld.deck] as CardTuple,
      });
    }
  } else {
    for (let stratum = 0; stratum < finalParticleLimit; stratum += 1) {
      const resourceIndex = weightedIndex(
        sourceWeights,
        stratifiedUnit(
          seed,
          stratum,
          finalParticleLimit,
          RESOURCE_WORLD_DIMENSION,
        ),
      );
      const resourceWorld = sampledResourceWorld(
        sourceWorlds[resourceIndex]!,
        players,
        board,
        seed,
        stratum,
        finalParticleLimit,
      );
      if (!resourceWorld) {
        throw new Error("Deep Search joint sampler selected an invalid resource world");
      }
      const developmentWorld = sampledDevelopmentWorld(
        developmentBase,
        seed,
        stratum,
        finalParticleLimit,
      );
      addParticle({
        weight: 1 / finalParticleLimit,
        ...resourceWorld,
        development: developmentWorld.hands,
        boughtDevelopment: developmentWorld.bought,
        developmentDeck: developmentWorld.deck,
      });
    }
  }
  if (!mergedWorlds.size) {
    throw new Error("Deep Search joint sampler produced no valid particles");
  }
  const mergedMass = [...mergedWorlds.values()].reduce(
    (sum, world) => sum + world.weight,
    0,
  );
  const worlds = [...mergedWorlds.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([, world]) => ({
      ...world,
      weight: world.weight / Math.max(Number.EPSILON, mergedMass),
    }));
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
        ...(phase.parameter !== undefined && phase.parameter >= 0
          ? { phaseParameter: phase.parameter }
          : {}),
        turn: Math.max(0, board.turn ?? state.currentTurn.sequence),
        lastRoll: Math.max(0, board.lastRoll ?? 0),
        victoryTarget: board.victoryTarget ?? 10,
        cardDiscardLimit,
        friendlyRobber: Boolean(board.friendlyRobber),
        setupStep: Math.min(
          players.length * 2,
          board.edges.filter((edge) => edge.player).length,
        ),
        discardRemaining: players
          .map((player, index) => {
            if (board.action !== "discard" || index < root) return 0;
            if (index === root && board.discardCount !== undefined) {
              return board.discardCount;
            }
            const publicState = board.players?.[player];
            const handSize = publicState?.handSize ?? 0;
            const discardLimit = publicState?.cardDiscardLimit ?? 7;
            return handSize > discardLimit ? Math.floor(handSize / 2) : 0;
          })
          .concat([0, 0, 0, 0])
          .slice(0, 4),
        discardCursor: board.action === "discard" ? root : 0,
        robberReturnPhase: board.hasRolled === false ? "pre-roll" : "main",
        domesticTradeUsed,
        // `disablePlayerTrades` is an assistant policy, not a table rule. Keep
        // domestic trading enabled in the simulated game so opponents can
        // trade with each other, and disable only the local/root seat.
        playerTradesEnabled: true,
        domesticTradeDisabled: playerTradesEnabled ? 0 : 1 << root,
        domesticTradeEmbargoes,
        ...(activeTrade
          ? {
              trade: {
                creator: tradeCreator,
                recipients: tradeRecipients,
                // Active trades are stored creator-relative, matching the rules engine.
                give: resources(activeTrade.creatorGive),
                receive: resources(activeTrade.creatorReceive),
                accepted: bitset(activeTrade.acceptedPlayers),
                // Embargo is distinct belief evidence, but it is still a
                // completed negative response for the live trade protocol.
                rejected: bitset([
                  ...(activeTrade.rejectedPlayers ?? []),
                  ...(activeTrade.embargoedPlayers ?? []),
                ]),
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
      // Give WASM Strategist enough room to finish a materially stronger
      // iterative search while keeping a bounded live-decision window.
      // Native CUDA receives its own larger deadline floor in the background.
      iterations: players.length >= 3 ? 320 : 384,
      maxNodes: 8_000,
      rolloutActions: players.length >= 3 ? 96 : 108,
      tacticalDepth: 14,
      tacticalNodes: 900,
      timeBudgetMs: LIVE_WASM_DECISION_TIME_MS,
      effort: {
        decisionTimeMs: LIVE_WASM_DECISION_TIME_MS,
        tactical: { maxDepth: 14, nodeBudget: 900 },
        cpu: { maxDepth: 5, rootCap: 10, nodesPerDepthWave: 8_000 },
        gpu: {
          rootCap: 8,
          rolloutBudget: players.length >= 3 ? 320 : 384,
          rolloutSteps: players.length >= 3 ? 96 : 108,
        },
      },
      seed,
      mode: "maxn",
      depth: 5,
      branchCap: 10,
      ponder: false,
      ...(searchConstraints.lastRejectedTrade
        ? {
            lastRejectedTrade: {
              give: resources(searchConstraints.lastRejectedTrade.give),
              receive: resources(searchConstraints.lastRejectedTrade.receive),
            },
          }
        : {}),
      ...(searchConstraints.rootExclusions?.length
        ? {
            rootExclusions: searchConstraints.rootExclusions.map((exclusion) => ({
              kind: exclusion.kind,
              give: resources(exclusion.give),
              receive: resources(exclusion.receive),
            })),
          }
        : {}),
    },
  };
};

export const analyzeDeepSearch = async (
  state: TrackerState,
  board: BoardSnapshot,
  rootPlayer: string,
  fallback: DecisionAnalysis,
  searchConstraints: DecisionSearchConstraints = {},
  playerTradesEnabled = true,
  engine: DecisionEngine = "deep-search",
  executor?: DeepSearchExecutor,
): Promise<DecisionAnalysis> => {
  if (!executor) await ensureWasm();
  const { request, players, root } = buildDeepSearchRequest(
    state,
    board,
    rootPlayer,
    searchConstraints,
    playerTradesEnabled,
  );
  request.mode = engine === "weighted" ? "weighted" : "maxn";
  if (board.initialPlacement) {
    // Setup is a fully public sequential draft. Spend a larger cumulative
    // budget here than on an ordinary turn: setup occurs only a handful of
    // times and dominates long-horizon outcomes.
    request.maxNodes = 12_000;
    request.timeBudgetMs = LIVE_WASM_OPENING_DECISION_TIME_MS;
    request.depth = Math.min(4, Math.max(2, players.length));
    request.branchCap = 12;
  }
  request.branchCap = board.initialPlacement
    ? request.branchCap
    : 10;
  if (request.state.phase === "trade-responses") {
    request.iterations = 64;
    request.maxNodes = 4_000;
    request.rolloutActions = 48;
    request.tacticalNodes = 600;
    request.timeBudgetMs = LIVE_WASM_TRADE_DECISION_TIME_MS;
    request.depth = 4;
    request.branchCap = 8;
  } else if (!board.isMyTurn) {
    request.ponder = true;
    request.iterations = 96;
    request.maxNodes = 8_000;
    request.rolloutActions = 64;
    request.tacticalNodes = 600;
    request.timeBudgetMs = LIVE_WASM_PONDER_DECISION_TIME_MS;
    // Background opening/pondering can keep accumulating while opponents act.
    if (board.initialPlacement) {
      request.maxNodes = 18_000;
      request.timeBudgetMs = 4_000;
    }
  }
  // The wire contract is backend-specific even while legacy top-level knobs
  // remain available to offline tooling. Live WASM/native execution consumes
  // this explicit effort object so CPU nodes and GPU rollouts cannot be
  // mistaken for the same unit.
  request.effort = {
    decisionTimeMs: request.timeBudgetMs,
    tactical: {
      maxDepth: request.tacticalDepth,
      nodeBudget: request.tacticalNodes,
    },
    cpu: {
      maxDepth: request.depth,
      rootCap: request.branchCap,
      nodesPerDepthWave: request.maxNodes,
    },
    gpu: {
      rootCap: request.branchCap,
      rolloutBudget: request.iterations,
      rolloutSteps: request.rolloutActions,
    },
  };
  const startedAt = performance.now();
  const response = executor
    ? await executor(request)
    : (analyzeWasm(request) as WasmSearchResponse);
  const elapsedMs = performance.now() - startedAt;
  const mappingFailureReason =
    response.chosen && !matchingPrompt(response.chosen, board)
      ? "rust-chosen-action-does-not-match-live-prompt"
      : undefined;
  const selected = mappingFailureReason ? undefined : response.chosen;
  const effectiveSearchEffort: DeepSearchResult["effectiveSearchEffort"] =
    response.algorithm === "gpu-root-rollout"
      ? {
          backend: "gpu",
          timeBudgetMs: response.effectiveEffort.decisionTimeMs,
          tacticalMaxDepth: response.effectiveEffort.tactical.maxDepth,
          tacticalNodeBudget: response.effectiveEffort.tactical.nodeBudget,
          rootCap: response.effectiveEffort.gpu.rootCap,
          rolloutBudget: response.effectiveEffort.gpu.rolloutBudget,
          rolloutSteps: response.effectiveEffort.gpu.rolloutSteps,
        }
      : {
          backend: "cpu",
          timeBudgetMs: response.effectiveEffort.decisionTimeMs,
          tacticalMaxDepth: response.effectiveEffort.tactical.maxDepth,
          tacticalNodeBudget: response.effectiveEffort.tactical.nodeBudget,
          maxDepth: response.effectiveEffort.cpu.maxDepth,
          rootCap: response.effectiveEffort.cpu.rootCap,
          nodesPerDepthWave: response.effectiveEffort.cpu.nodesPerDepthWave,
        };
  const search: DeepSearchResult = {
    engineRevision: response.engineRevision,
    rootIndex: root,
    learnedModelVersion: response.learnedModelVersion,
    tradeModelVersion: response.tradeModelVersion,
    algorithm: response.algorithm,
    authority: response.authority,
    effectiveSearchEffort,
    ...(selected ? { chosen: mapAction(selected, players, board) } : {}),
    rootValue: response.rootValue.slice(0, players.length),
    tacticalWinProbability: response.tacticalWinProbability,
    tacticalLowerBound: response.tacticalLowerBound,
    tacticalProven: response.tacticalProven,
    exactDecision: response.exactDecision,
    exactWorlds: response.exactWorlds,
    exactActions: (response.exactActions ?? []).map((candidate) => ({
      action: mapAction(candidate.action, players, board),
      value: candidate.value.slice(0, players.length),
      lowerBound: candidate.lowerBound.slice(0, players.length),
      legalWeight: candidate.legalWeight,
      decisionScore: candidate.decisionScore,
      lowerScore: candidate.lowerScore,
      comparatorScore: candidate.comparatorScore,
    })),
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
    particles: response.wasmParticles,
    sourceWorldCount: state.worlds.length,
    wasmParticleCount: response.wasmParticles,
    rustPosteriorParticleCount: response.rustPosteriorParticles,
    rustSearchParticleCount: response.rustSearchParticles,
    ...(response.searchStages ? { searchStages: response.searchStages } : {}),
    rootProvenance: mapRootProvenance(
      response.rootProvenance,
      players,
      board,
    ),
    authorityTrace: mapAuthorityTrace(
      response.authorityTrace,
      players,
      board,
    ),
    ...(mappingFailureReason ? { mappingFailureReason } : {}),
    effectiveParticleCount: response.effectiveParticleCount,
    deadlineReached: response.deadlineReached,
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
    0.68,
    0.44 + visibleProgress * 0.24,
  );
  const deepPlayers =
    engine === "weighted" || response.algorithm === "gpu-root-rollout"
      ? fallback.players
      : deepValueTotal > Number.EPSILON
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
                    response.algorithm === "maxn"
                      ? "Deep MaxN belief search"
                      : response.algorithm === "gpu-root-rollout"
                        ? "GPU resident root-rollout search"
                        : response.algorithm
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
    engine,
    actionScores: {
      ...fallback.actionScores,
      ...deepActionScores(response.actions, root),
    },
    players: deepPlayers,
    simulations: response.rollouts,
    model:
      engine === "weighted"
        ? "Weighted heuristic policy (top-5 action-prior sampling)"
        : response.algorithm === "gpu-root-rollout"
          ? `Observation-safe GPU root-rollout search (${response.particles} particles, ${response.rollouts.toLocaleString()} resident rollouts)`
          : `Observation-safe weighted-belief Deep MaxN (${response.particles} particles, ${response.nodes.toLocaleString()} nodes, depth ${response.deepestDecisionDepth})`,
    deepSearch: search,
  };
};
