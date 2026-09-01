import {
  BUILD_COSTS,
  RESOURCE_ORDER,
  RESOURCE_STRATEGIC_WEIGHTS,
  emptyResources,
  hasResources,
  resourceTotal,
  type BuildKind,
  type Resource,
  type ResourceVector,
} from "./resources";
import {
  NUMBER_PIPS,
  type BoardSnapshot,
} from "./placement";
import { longestRoadFromEdges } from "./roads";

export interface ProductionMetrics {
  rawPips: number;
  activePips: number;
  weightedPips: number;
  activeWeightedPips: number;
  strikeWays: number;
  activeStrikeWays: number;
  expectedCardsPerRoll: number;
  activeExpectedCardsPerRoll: number;
  concentration: number;
}

export interface PlayerBoardProfile {
  player: string;
  production: ResourceVector;
  activeProduction: ResourceVector;
  tradeRatios: ResourceVector;
  settlements: number;
  cities: number;
  roads: number;
  remaining: {
    settlements: number;
    cities: number;
    roads: number;
  };
  visiblePoints: number;
  victoryTarget: number;
  cardDiscardLimit: number;
  blockedPips: number;
  openSettlementSites: number;
  metrics: ProductionMetrics;
  longestRoad: number;
  playedKnights: number;
  developmentCards: number;
  hasLongestRoad: boolean;
  hasLargestArmy: boolean;
  roadsToLongest: number;
  knightsToLargest: number;
}

export interface BuildTempo {
  kind: BuildKind;
  rolls: number;
  missing: ResourceVector;
  affordable: boolean;
}

export interface DevelopmentDeckEvidence {
  purchased: number;
  playedKnights: number;
  playedMonopoly: number;
  playedRoadBuilding: number;
  playedYearOfPlenty: number;
  revealedVictoryPoints?: number;
}

export interface DevelopmentDeckEstimate {
  remainingCards: number;
  expectedComposition: {
    knight: number;
    victoryPoint: number;
    progress: number;
  };
  next: {
    knight: number;
    victoryPoint: number;
    progress: number;
  };
  atLeastOneVictoryPoint: (draws: number) => number;
  atLeastOneKnight: (draws: number) => number;
}

const standardTradeRatios = (): ResourceVector => ({
  lumber: 4,
  brick: 4,
  wool: 4,
  grain: 4,
  ore: 4,
});

const choose = (n: number, k: number): number => {
  if (k < 0 || k > n) return 0;
  const count = Math.min(k, n - k);
  let result = 1;
  for (let index = 1; index <= count; index += 1) {
    result = (result * (n - count + index)) / index;
  }
  return result;
};

const atLeastOne = (population: number, successes: number, draws: number): number => {
  const sample = Math.max(0, Math.min(draws, population));
  if (!sample || !successes || !population) return 0;
  return 1 - choose(population - successes, sample) / choose(population, sample);
};

/**
 * The identities of bought-but-unplayed cards stay hidden. By exchangeability,
 * an unseen next card keeps the same marginal type distribution after those
 * unknown draws; the known played/revealed cards are the evidence that changes
 * the posterior.
 */
export const estimateDevelopmentDeck = (
  evidence: DevelopmentDeckEvidence,
): DevelopmentDeckEstimate => {
  const knownKnights = Math.min(14, Math.max(0, evidence.playedKnights));
  const knownVictoryPoints = Math.min(
    5,
    Math.max(0, evidence.revealedVictoryPoints ?? 0),
  );
  const knownProgress = Math.min(
    6,
    Math.max(
      0,
      evidence.playedMonopoly +
        evidence.playedRoadBuilding +
        evidence.playedYearOfPlenty,
    ),
  );
  const posteriorPopulation = Math.max(
    0,
    25 - knownKnights - knownVictoryPoints - knownProgress,
  );
  const knights = Math.max(0, 14 - knownKnights);
  const victoryPoints = Math.max(0, 5 - knownVictoryPoints);
  const progress = Math.max(0, 6 - knownProgress);
  const hiddenPurchased = Math.max(
    0,
    evidence.purchased - knownKnights - knownVictoryPoints - knownProgress,
  );
  const remainingCards = Math.max(0, posteriorPopulation - hiddenPurchased);
  const remainingFraction = posteriorPopulation
    ? remainingCards / posteriorPopulation
    : 0;
  const probability = (count: number) =>
    posteriorPopulation ? count / posteriorPopulation : 0;

  return {
    remainingCards,
    expectedComposition: {
      knight: knights * remainingFraction,
      victoryPoint: victoryPoints * remainingFraction,
      progress: progress * remainingFraction,
    },
    next: {
      knight: probability(knights),
      victoryPoint: probability(victoryPoints),
      progress: probability(progress),
    },
    atLeastOneVictoryPoint: (draws) =>
      atLeastOne(
        posteriorPopulation,
        victoryPoints,
        Math.min(draws, remainingCards),
      ),
    atLeastOneKnight: (draws) =>
      atLeastOne(posteriorPopulation, knights, Math.min(draws, remainingCards)),
  };
};

export const productionMetricsFor = (
  board: BoardSnapshot,
  player: string,
): ProductionMetrics => {
  let rawPips = 0;
  let activePips = 0;
  let weightedPips = 0;
  let activeWeightedPips = 0;
  const numbers = new Set<number>();
  const activeNumbers = new Set<number>();
  const byNumber = new Map<number, number>();

  for (const vertex of board.vertices) {
    if (vertex.building?.player !== player) continue;
    const multiplier = vertex.building.kind === "city" ? 2 : 1;
    for (const id of vertex.adjacentHexes) {
      const hex = board.hexes.find((candidate) => candidate.id === id);
      if (!hex?.resource || !hex.number) continue;
      const pips = (NUMBER_PIPS[hex.number] ?? 0) * multiplier;
      rawPips += pips;
      weightedPips += pips * RESOURCE_STRATEGIC_WEIGHTS[hex.resource];
      numbers.add(hex.number);
      byNumber.set(hex.number, (byNumber.get(hex.number) ?? 0) + pips);
      if (!hex.blocked) {
        activePips += pips;
        activeWeightedPips += pips * RESOURCE_STRATEGIC_WEIGHTS[hex.resource];
        activeNumbers.add(hex.number);
      }
    }
  }
  const strikeWays = [...numbers].reduce(
    (sum, number) => sum + (NUMBER_PIPS[number] ?? 0),
    0,
  );
  const activeStrikeWays = [...activeNumbers].reduce(
    (sum, number) => sum + (NUMBER_PIPS[number] ?? 0),
    0,
  );
  const largestNumberStack = Math.max(0, ...byNumber.values());

  return {
    rawPips,
    activePips,
    weightedPips,
    activeWeightedPips,
    strikeWays,
    activeStrikeWays,
    expectedCardsPerRoll: rawPips / 36,
    activeExpectedCardsPerRoll: activePips / 36,
    concentration: rawPips ? largestNumberStack / rawPips : 0,
  };
};

export const playerBoardProfile = (
  board: BoardSnapshot,
  player: string,
): PlayerBoardProfile => {
  const production = emptyResources();
  const activeProduction = emptyResources();
  let settlements = 0;
  let cities = 0;
  let blockedPips = 0;

  for (const vertex of board.vertices) {
    if (vertex.building?.player !== player) continue;
    const multiplier = vertex.building.kind === "city" ? 2 : 1;
    if (vertex.building.kind === "city") cities += 1;
    else settlements += 1;
    for (const id of vertex.adjacentHexes) {
      const hex = board.hexes.find((candidate) => candidate.id === id);
      if (!hex?.resource || !hex.number) continue;
      const pips = (NUMBER_PIPS[hex.number] ?? 0) * multiplier;
      production[hex.resource] += pips;
      if (hex.blocked) blockedPips += pips;
      else activeProduction[hex.resource] += pips;
    }
  }

  const roads = board.edges.filter((edge) => edge.player === player).length;
  const publicState = board.players?.[player];
  const tradeRatios = publicState?.tradeRatios ?? standardTradeRatios();
  const byVertex = new Map(board.vertices.map((vertex) => [vertex.id, vertex]));
  const openSettlementSites = board.vertices.filter((vertex) => {
    if (vertex.building) return false;
    if (
      vertex.adjacentVertices.some(
        (neighbor) => byVertex.get(neighbor)?.building,
      )
    ) {
      return false;
    }
    return board.edges.some(
      (edge) => edge.player === player && edge.vertices.includes(vertex.id),
    );
  }).length;
  const longestRoad = Math.max(
    publicState?.longestRoad ?? 0,
    longestRoadFromEdges(board, player),
  );
  const otherLongest = Math.max(
    0,
    ...Object.entries(board.players ?? {})
      .filter(([candidate]) => candidate !== player)
      .map(([candidate, publicCandidate]) =>
        Math.max(
          publicCandidate.longestRoad ?? 0,
          longestRoadFromEdges(board, candidate),
        ),
      ),
  );
  const otherArmy = Math.max(
    0,
    ...Object.entries(board.players ?? {})
      .filter(([candidate]) => candidate !== player)
      .map(([, candidate]) => candidate.playedKnights ?? 0),
  );
  const playedKnights = publicState?.playedKnights ?? 0;

  return {
    player,
    production,
    activeProduction,
    tradeRatios,
    settlements,
    cities,
    roads,
    remaining: {
      settlements: Math.max(0, 5 - settlements),
      cities: Math.max(0, 4 - cities),
      roads: Math.max(0, 15 - roads),
    },
    visiblePoints:
      publicState?.visiblePoints ?? settlements + cities * 2,
    victoryTarget: board.victoryTarget ?? 10,
    cardDiscardLimit: publicState?.cardDiscardLimit ?? 7,
    blockedPips,
    openSettlementSites,
    metrics: productionMetricsFor(board, player),
    longestRoad,
    playedKnights,
    developmentCards: publicState?.developmentCards ?? 0,
    hasLongestRoad: Boolean(publicState?.hasLongestRoad),
    hasLargestArmy: Boolean(publicState?.hasLargestArmy),
    roadsToLongest: Math.max(
      0,
      Math.max(5, otherLongest + (publicState?.hasLongestRoad ? 0 : 1)) -
        longestRoad,
    ),
    knightsToLargest: Math.max(
      0,
      Math.max(3, otherArmy + (publicState?.hasLargestArmy ? 0 : 1)) -
        playedKnights,
    ),
  };
};

const effectivePips = (
  resource: Resource,
  profile: PlayerBoardProfile,
): number => {
  const direct = profile.activeProduction[resource];
  let converted = 0;
  for (const source of RESOURCE_ORDER) {
    if (source === resource) continue;
    converted +=
      profile.activeProduction[source] /
      Math.max(2, profile.tradeRatios[source]);
  }
  return direct + converted * 0.58;
};

export const estimateBuildTempo = (
  hand: ResourceVector,
  kind: BuildKind,
  profile: PlayerBoardProfile,
): BuildTempo => {
  const missing = emptyResources();
  const cost = BUILD_COSTS[kind] as Partial<ResourceVector>;
  let slowest = 0;
  let aggregate = 0;
  for (const resource of RESOURCE_ORDER) {
    missing[resource] = Math.max(0, (cost[resource] ?? 0) - hand[resource]);
    if (!missing[resource]) continue;
    const pips = effectivePips(resource, profile);
    const expectedRolls = pips > 0 ? (36 * missing[resource]) / pips : 54;
    slowest = Math.max(slowest, expectedRolls);
    aggregate += expectedRolls;
  }
  return {
    kind,
    missing,
    affordable: hasResources(hand, cost),
    rolls: slowest + aggregate * 0.18,
  };
};

const pieceBlocked = (kind: BuildKind, profile: PlayerBoardProfile): boolean => {
  if (kind === "road") return profile.remaining.roads <= 0;
  if (kind === "settlement") {
    return profile.remaining.settlements <= 0 || profile.openSettlementSites <= 0;
  }
  if (kind === "city") {
    return profile.remaining.cities <= 0 || profile.settlements <= 0;
  }
  return false;
};

export const likelyUpgradePath = (
  board: BoardSnapshot,
  player: string,
  hand: ResourceVector,
): BuildTempo => {
  const profile = playerBoardProfile(board, player);
  const tempo = (Object.keys(BUILD_COSTS) as BuildKind[])
    .map((kind) => estimateBuildTempo(hand, kind, profile))
    .filter((item) => !pieceBlocked(item.kind, profile))
    .sort((left, right) => {
      if (left.affordable !== right.affordable) return left.affordable ? -1 : 1;
      const utility = (kind: BuildKind) => {
        if (kind === "city") return 2.6;
        if (kind === "settlement") return profile.openSettlementSites ? 2.2 : 0;
        if (kind === "development") {
          return 1.2 + (profile.knightsToLargest <= 2 ? 0.9 : 0);
        }
        return profile.roadsToLongest <= 2 ? 1.3 : 0.15;
      };
      return (
        left.rolls - utility(left.kind) -
        (right.rolls - utility(right.kind))
      );
    });
  return (
    tempo[0] ?? {
      kind: "development",
      rolls: 54,
      missing: emptyResources(),
      affordable: false,
    }
  );
};

export const strategicThreatScore = (
  board: BoardSnapshot,
  player: string,
  hand: ResourceVector,
  developmentCards = 0,
  playedKnights = 0,
): number => {
  const profile = playerBoardProfile(board, player);
  const path = likelyUpgradePath(board, player, hand);
  const pointsRemaining = Math.max(0, profile.victoryTarget - profile.visiblePoints);
  const handPressure = Math.min(12, resourceTotal(hand)) * 0.3;
  const tempoPressure = Math.max(0, 7 - path.rolls * 0.18);
  const engine = profile.metrics.activeWeightedPips * 0.065;
  const portPower = RESOURCE_ORDER.reduce(
    (sum, resource) =>
      sum +
      Math.max(0, 4 - profile.tradeRatios[resource]) *
        profile.activeProduction[resource] *
        0.055,
    0,
  );
  const concealedPoints = Math.min(2.4, developmentCards * 0.38);
  const armySwing =
    profile.knightsToLargest <= 1 || profile.hasLargestArmy ? 1.7 : 0;
  const roadSwing =
    profile.roadsToLongest <= 1 || profile.hasLongestRoad ? 1.35 : 0;
  const winWindow =
    pointsRemaining <= 1
      ? 8
      : pointsRemaining <= 2
        ? 4.5
        : pointsRemaining <= 3
          ? 2
          : 0;
  return (
    profile.visiblePoints * 1.75 +
    engine +
    handPressure +
    tempoPressure +
    concealedPoints +
    playedKnights * 0.5 +
    portPower +
    armySwing +
    roadSwing +
    winWindow
  );
};
