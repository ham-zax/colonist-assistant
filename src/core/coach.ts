import {
  BUILD_COSTS,
  RESOURCE_LABELS,
  RESOURCE_ORDER,
  RESOURCE_STRATEGIC_WEIGHTS,
  emptyResources,
  resourceTotal,
  type BuildKind,
  type Resource,
  type ResourceVector,
} from "./resources";
import {
  NUMBER_PIPS,
  scoreCityPlacements,
  scoreRoadPlacements,
  scoreSettlementPlacements,
  type BoardSnapshot,
  type PlacementContext,
} from "./placement";
import { affordability, getPlayerEstimate } from "./tracker";
import type { TrackerState } from "./types";
import {
  estimateBuildTempo,
  estimateDevelopmentDeck,
  likelyUpgradePath,
  playerBoardProfile,
  strategicThreatScore,
  type DevelopmentDeckEstimate,
} from "./strategy";
import {
  recommendDevelopmentCards,
  type DevelopmentTimingReport,
} from "./development";
import { generateTradeProposals } from "./trades";
import {
  analyzePublicEstimate,
  type DecisionAnalysis,
} from "./engine";

export type GamePhase = "opening" | "middle" | "closing";
export type StrategyArchetype =
  | "expansion"
  | "city-engine"
  | "development-control"
  | "balanced";

export interface BuildRecommendation {
  kind: BuildKind;
  label: string;
  score: number;
  confidence: number;
  progress: number;
  affordableProbability: number;
  deficit: ResourceVector;
  reasons: string[];
}

export interface TradeRecommendation {
  give: ResourceVector;
  receive: ResourceVector;
  partner: string;
  acceptanceProbability: number;
  score: number;
  ownTempoGain: number;
  opponentTempoGain: number;
  reason: string;
}

export interface StealRecommendation {
  player: string;
  score: number;
  neededCardChance: number;
  likelyResources: Array<{ resource: Resource; chance: number }>;
  reasons: string[];
}

export interface CoachReport {
  player: string;
  phase: GamePhase;
  strategy: StrategyArchetype;
  primary: BuildRecommendation;
  alternatives: BuildRecommendation[];
  trade?: TradeRecommendation;
  steal?: StealRecommendation;
  developmentDeck: DevelopmentDeckEstimate;
  developmentTiming?: DevelopmentTimingReport;
  decisionAnalysis?: DecisionAnalysis;
  winWindow?: string;
  alerts: string[];
}

const BUILD_LABELS: Record<BuildKind, string> = {
  road: "Road",
  settlement: "Settlement",
  city: "City",
  development: "Development card",
};

const STRATEGY_REASONS: Record<StrategyArchetype, Record<BuildKind, string>> = {
  expansion: {
    road: "Keeps your expansion route moving",
    settlement: "Turns strong road production into another point and more income",
    city: "Improves production without extending your network",
    development: "Adds tactical options while your road engine develops",
  },
  "city-engine": {
    road: "Opens a future settlement when your city route stalls",
    settlement: "Adds a city target and broadens production",
    city: "Converts your grain and ore strength into the largest production gain",
    development: "Uses the same grain-and-ore engine to contest Largest Army",
  },
  "development-control": {
    road: "Creates an expansion route without abandoning robber control",
    settlement: "Adds production before the development-card push",
    city: "Accelerates the grain, ore, and wool engine",
    development: "Supports robber control, Largest Army, and hidden points",
  },
  balanced: {
    road: "Preserves flexible expansion options",
    settlement: "Adds a point and a broader stream of resources",
    city: "Raises production without needing a new location",
    development: "Adds tactical flexibility and a possible hidden point",
  },
};

const buildCost = (kind: BuildKind, resource: Resource): number =>
  (BUILD_COSTS[kind] as Partial<ResourceVector>)[resource] ?? 0;

const phaseFor = (
  state: TrackerState,
  player: string,
  board?: BoardSnapshot,
): GamePhase => {
  if (board) {
    const profile = playerBoardProfile(board, player);
    const maximumVisible = Math.max(
      profile.visiblePoints,
      ...Object.values(board.players ?? {}).map(
        (candidate) => candidate.visiblePoints ?? 0,
      ),
    );
    if (
      maximumVisible >= profile.victoryTarget - 3 ||
      profile.visiblePoints >= profile.victoryTarget - 3
    ) {
      return "closing";
    }
    if (profile.cities || profile.settlements >= 3 || state.currentTurn.sequence >= 18) {
      return "middle";
    }
  }
  if (state.currentTurn.sequence < 12) return "opening";
  if (state.currentTurn.sequence < 36) return "middle";
  return "closing";
};

const boardProductionFor = (
  board: BoardSnapshot | undefined,
  player: string,
): ResourceVector | undefined => {
  if (!board) return undefined;
  const production = emptyResources();
  for (const vertex of board.vertices) {
    if (vertex.building?.player !== player) continue;
    const multiplier = vertex.building.kind === "city" ? 2 : 1;
    for (const id of vertex.adjacentHexes) {
      const hex = board.hexes.find((candidate) => candidate.id === id);
      if (!hex?.resource || !hex.number) continue;
      production[hex.resource] += (NUMBER_PIPS[hex.number] ?? 0) * multiplier;
    }
  }
  return production;
};

const strategyFor = (
  state: TrackerState,
  player: string,
  board?: BoardSnapshot,
): StrategyArchetype => {
  const production =
    boardProductionFor(board, player) ??
    state.players[player]?.productionGained ??
    emptyResources();
  const weighted = (resource: Resource) =>
    production[resource] * RESOURCE_STRATEGIC_WEIGHTS[resource];
  const expansion = weighted("lumber") + weighted("brick");
  const city = weighted("grain") + weighted("ore");
  const development = weighted("wool") + weighted("grain") + weighted("ore");
  const total = Math.max(
    1,
    RESOURCE_ORDER.reduce((sum, resource) => sum + weighted(resource), 0),
  );

  if (city / total >= 0.66 && production.ore > 0) return "city-engine";
  if (
    development / total >= 0.72 &&
    production.wool >= Math.min(production.grain, production.ore)
  ) {
    return "development-control";
  }
  if (expansion / total >= 0.45) return "expansion";
  return "balanced";
};

const deficitFor = (
  state: TrackerState,
  player: string,
  kind: BuildKind,
): ResourceVector => {
  const estimate = getPlayerEstimate(state, player);
  const result = emptyResources();
  for (const resource of RESOURCE_ORDER) {
    result[resource] = Math.max(
      0,
      buildCost(kind, resource) - Math.floor(estimate.average[resource]),
    );
  }
  return result;
};

const exactnessFor = (state: TrackerState, player: string): number => {
  const estimate = getPlayerEstimate(state, player);
  const width = RESOURCE_ORDER.reduce(
    (sum, resource) => sum + estimate.maximum[resource] - estimate.minimum[resource],
    0,
  );
  return Math.max(0.45, Math.min(1, 1 - width / Math.max(5, estimate.totalMaximum)));
};

const phaseFit = (phase: GamePhase, kind: BuildKind): number => {
  const weights: Record<GamePhase, Record<BuildKind, number>> = {
    opening: { road: 16, settlement: 24, city: 8, development: 9 },
    middle: { road: 8, settlement: 17, city: 22, development: 16 },
    closing: { road: 3, settlement: 13, city: 24, development: 22 },
  };
  return weights[phase][kind];
};

const strategyFit = (strategy: StrategyArchetype, kind: BuildKind): number => {
  const weights: Record<StrategyArchetype, Record<BuildKind, number>> = {
    expansion: { road: 17, settlement: 22, city: 7, development: 6 },
    "city-engine": { road: 3, settlement: 10, city: 24, development: 16 },
    "development-control": { road: 3, settlement: 8, city: 18, development: 24 },
    balanced: { road: 8, settlement: 17, city: 17, development: 13 },
  };
  return weights[strategy][kind];
};

const formatDeficit = (deficit: ResourceVector): string => {
  const values = RESOURCE_ORDER.filter((resource) => deficit[resource] > 0).map(
    (resource) => `${deficit[resource]} ${RESOURCE_LABELS[resource].toLowerCase()}`,
  );
  return values.length ? `Need ${values.join(" · ")}` : "Affordable now";
};

const boardFitFor = (
  kind: BuildKind,
  board: BoardSnapshot | undefined,
  player: string,
): number => {
  if (!board) return 0;
  const production = boardProductionFor(board, player) ?? emptyResources();
  const context: PlacementContext = {
    player,
    production,
    currentResources: RESOURCE_ORDER.filter((resource) => production[resource] > 0),
    currentNumbers: board.vertices
      .filter((vertex) => vertex.building?.player === player)
      .flatMap((vertex) =>
        vertex.adjacentHexes.flatMap((id) => {
          const number = board.hexes.find((hex) => hex.id === id)?.number;
          return number ? [number] : [];
        }),
      ),
    desiredResources: RESOURCE_ORDER.filter(
      (resource) => buildCost(kind, resource) > 0,
    ),
    legalVertexIds:
      kind === "settlement"
        ? board.buildableSettlementIds
        : kind === "city"
          ? board.buildableCityIds
          : undefined,
    legalEdgeIds: kind === "road" ? board.buildableRoadIds : undefined,
    initialPlacement: board.initialPlacement,
    picksUntilNext: board.picksUntilNext,
    requireConnection: true,
  };
  if (kind === "settlement") {
    const best = scoreSettlementPlacements(board, context)[0];
    if (!best) return -24;
    const rawPips = best.metrics?.rawPips ?? 0;
    return (
      Math.min(15, best.score * 0.42) +
      (rawPips >= 10 ? 9 : rawPips >= 8 ? 5.5 : rawPips <= 6 ? -5 : 0)
    );
  }
  if (kind === "city") {
    const best = scoreCityPlacements(board, context)[0];
    if (!best) return -24;
    const rawPips = best.metrics?.rawPips ?? 0;
    return Math.min(16, best.score * 0.75) + (rawPips >= 10 ? 5 : rawPips <= 6 ? -3 : 0);
  }
  if (kind === "road") {
    const best = scoreRoadPlacements(board, context)[0];
    if (!best) return -24;
    if (!best.metrics?.strategicallyUseful) return -34;
    if (
      best.metrics.purpose === "settlement" &&
      (best.metrics.roadsRequired ?? 99) > 1
    ) {
      return -19;
    }
    if (best.score <= 0) return -24;
    return Math.min(18, best.score * 0.42);
  }
  const developmentPips = production.wool + production.grain + production.ore;
  return Math.min(12, developmentPips * 0.35);
};

const developmentDeckFor = (
  state: TrackerState,
  board?: BoardSnapshot,
): DevelopmentDeckEstimate => {
  const players = Object.values(state.players);
  const playedCount = (
    player: (typeof players)[number],
    card: "knight" | "monopoly" | "road-building" | "year-of-plenty" | "victory-point",
  ): number =>
    Math.max(
      player.playedDevCards[card],
      board?.players?.[player.name]?.playedDevelopmentCards?.[card] ?? 0,
    );
  return estimateDevelopmentDeck({
    purchased: players.reduce((sum, player) => {
      const trackedPlayed = Object.values(player.playedDevCards).reduce(
        (total, count) => total + count,
        0,
      );
      const publicHeld = board?.players?.[player.name]?.developmentCards ?? 0;
      const mergedPlayed =
        playedCount(player, "knight") +
        playedCount(player, "monopoly") +
        playedCount(player, "road-building") +
        playedCount(player, "year-of-plenty") +
        playedCount(player, "victory-point");
      return (
        sum +
        Math.max(
          player.devCards.length + trackedPlayed,
          publicHeld + mergedPlayed,
        )
      );
    }, 0),
    playedKnights: players.reduce(
      (sum, player) => sum + playedCount(player, "knight"),
      0,
    ),
    playedMonopoly: players.reduce(
      (sum, player) => sum + playedCount(player, "monopoly"),
      0,
    ),
    playedRoadBuilding: players.reduce(
      (sum, player) => sum + playedCount(player, "road-building"),
      0,
    ),
    playedYearOfPlenty: players.reduce(
      (sum, player) => sum + playedCount(player, "year-of-plenty"),
      0,
    ),
    revealedVictoryPoints: players.reduce(
      (sum, player) => sum + playedCount(player, "victory-point"),
      0,
    ),
  });
};

const buildRecommendations = (
  state: TrackerState,
  player: string,
  phase: GamePhase,
  strategy: StrategyArchetype,
  board?: BoardSnapshot,
  actionScores?: Record<BuildKind, number>,
): BuildRecommendation[] => {
  const estimate = getPlayerEstimate(state, player);
  const exactness = exactnessFor(state, player);
  const profile = board ? playerBoardProfile(board, player) : undefined;
  const handRisk =
    estimate.totalMaximum > (profile?.cardDiscardLimit ?? 7) ? 10 : 0;
  const developmentDeck = developmentDeckFor(state, board);
  const pointsRemaining = profile
    ? Math.max(0, profile.victoryTarget - profile.visiblePoints)
    : Number.POSITIVE_INFINITY;

  return (Object.keys(BUILD_COSTS) as BuildKind[])
    .map((kind) => {
      const deficit = deficitFor(state, player, kind);
      const costTotal = resourceTotal(BUILD_COSTS[kind]);
      const missing = resourceTotal(deficit);
      const progress = Math.max(0, 1 - missing / costTotal);
      const affordableProbability = affordability(state, player, kind);
      const tempo = profile
        ? estimateBuildTempo(estimate.average, kind, profile)
        : undefined;
      const pieceBlocked =
        profile &&
        ((kind === "road" && profile.remaining.roads <= 0) ||
          (kind === "settlement" &&
            (profile.remaining.settlements <= 0 ||
              profile.openSettlementSites <= 0)) ||
          (kind === "city" &&
            (profile.remaining.cities <= 0 || profile.settlements <= 0)));
      const armySwing =
        kind === "development" && profile
          ? profile.knightsToLargest <= 1
            ? 16 * developmentDeck.next.knight
            : profile.knightsToLargest === 2
              ? 8 * developmentDeck.next.knight
              : 0
          : 0;
      const hiddenPointSwing =
        kind === "development"
          ? developmentDeck.next.victoryPoint *
            (pointsRemaining <= 1 ? 28 : pointsRemaining <= 2 ? 15 : 6)
          : 0;
      const longestRoadSwing =
        kind === "road" && profile
          ? profile.roadsToLongest === 1
            ? 19
            : profile.roadsToLongest === 2
              ? 6
              : 0
          : 0;
      const closingPointBias =
        phase === "closing"
          ? kind === "city"
            ? 11
            : kind === "settlement"
              ? 7
              : kind === "development"
                ? 8
                : longestRoadSwing
          : 0;
      const score = Math.round(
        progress * 42 +
          affordableProbability * 18 +
          phaseFit(phase, kind) +
          strategyFit(strategy, kind) +
          boardFitFor(kind, board, player) +
          (affordableProbability > 0 && handRisk ? handRisk : 0) +
          (tempo ? Math.max(-12, 18 - tempo.rolls * 1.15) : 0) +
          (pieceBlocked ? -80 : 0) +
          armySwing +
          hiddenPointSwing +
          longestRoadSwing +
          closingPointBias +
          (actionScores ? (actionScores[kind] - 50) * 0.24 : 0) +
          (state.playerOrder.length === 2 && kind === "development" ? 7 : 0) +
          (state.playerOrder.length === 2 && kind === "city" ? 4 : 0),
      );
      const reasons = [
        formatDeficit(deficit),
        STRATEGY_REASONS[strategy][kind],
      ];
      if (kind === "development") {
        reasons[1] =
          `${Math.round(developmentDeck.next.knight * 100)}% knight · ${Math.round(developmentDeck.next.victoryPoint * 100)}% hidden point from the tracked deck`;
        if (profile?.knightsToLargest === 1) {
          reasons.push(
            "One more played knight can create a two-point Largest Army swing",
          );
        }
      }
      if (kind === "road" && profile?.roadsToLongest === 1) {
        reasons[1] = "This road can create a two-point Longest Road swing";
      }
      if (handRisk && affordableProbability > 0) {
        reasons.push("Spending now reduces your seven-roll exposure");
      }
      if (tempo && !tempo.affordable) {
        reasons.push(
          `About ${Math.max(1, Math.round(tempo.rolls))} expected rolls at current production and port rates`,
        );
      }
      if (pieceBlocked) {
        reasons.push("No legal piece or upgrade site is currently available");
      }
      return {
        kind,
        label: BUILD_LABELS[kind],
        score: Math.min(100, score),
        confidence: Math.round(exactness * 100),
        progress,
        affordableProbability,
        deficit,
        reasons,
      };
    })
    .sort((left, right) => right.score - left.score);
};

const resourceStealProbability = (
  state: TrackerState,
  player: string,
  resource: Resource,
): number => {
  if (!state.worlds.length) return 0;
  return (
    state.worlds.reduce((sum, world) => {
      const hand = world.hands[player] ?? emptyResources();
      const total = resourceTotal(hand);
      return sum + (total ? hand[resource] / total : 0);
    }, 0) / state.worlds.length
  );
};

const resourcePresenceProbability = (
  state: TrackerState,
  player: string,
  resource: Resource,
): number => {
  if (!state.worlds.length) return 0;
  return (
    state.worlds.filter(
      (world) => (world.hands[player]?.[resource] ?? 0) > 0,
    ).length / state.worlds.length
  );
};

const fallbackTradeRecommendation = (
  state: TrackerState,
  player: string,
  primary: BuildRecommendation,
): TradeRecommendation | undefined => {
  const hand = getPlayerEstimate(state, player).average;
  const receive = RESOURCE_ORDER.filter(
    (resource) => primary.deficit[resource] > 0,
  ).sort(
    (left, right) =>
      primary.deficit[right] * RESOURCE_STRATEGIC_WEIGHTS[right] -
      primary.deficit[left] * RESOURCE_STRATEGIC_WEIGHTS[left],
  )[0];
  if (!receive) return undefined;
  const cost = BUILD_COSTS[primary.kind] as Partial<ResourceVector>;
  const give = RESOURCE_ORDER.filter(
    (resource) =>
      resource !== receive &&
      hand[resource] > (cost[resource] ?? 0),
  ).sort(
    (left, right) =>
      hand[right] -
        (cost[right] ?? 0) -
        (hand[left] - (cost[left] ?? 0)),
  )[0];
  if (!give) return undefined;
  const partner = state.playerOrder
    .filter((candidate) => candidate !== player)
    .map((candidate) => ({
      player: candidate,
      probability: resourcePresenceProbability(
        state,
        candidate,
        receive,
      ),
    }))
    .sort((left, right) => right.probability - left.probability)[0];
  if (!partner || partner.probability <= 0) return undefined;
  const giveVector = emptyResources();
  giveVector[give] = 1;
  const receiveVector = emptyResources();
  receiveVector[receive] = 1;
  return {
    give: giveVector,
    receive: receiveVector,
    partner: partner.player,
    acceptanceProbability: partner.probability * 0.5,
    score: partner.probability,
    ownTempoGain: 0,
    opponentTempoGain: 0,
    reason: `${Math.round(partner.probability * 100)}% tracked chance they hold ${RESOURCE_LABELS[receive].toLowerCase()}; board data is needed to model acceptance`,
  };
};

const visibleThreat = (
  state: TrackerState,
  player: string,
  board?: BoardSnapshot,
): number => {
  const meta = state.players[player];
  if (!meta) return 0;
  if (board) {
    return strategicThreatScore(
      board,
      player,
      getPlayerEstimate(state, player).average,
      board.players?.[player]?.developmentCards ?? meta.devCards.length,
      board.players?.[player]?.playedDevelopmentCards?.knight ??
        meta.playedDevCards.knight,
    );
  }
  const visiblePoints = Math.max(2, meta.builds.settlement + meta.builds.city);
  const army = meta.playedDevCards.knight;
  return visiblePoints + army * 0.55 + meta.devCards.length * 0.25;
};

const recommendSteal = (
  state: TrackerState,
  player: string,
  primary: BuildRecommendation,
  board?: BoardSnapshot,
): StealRecommendation | undefined => {
  const needed = RESOURCE_ORDER.filter((resource) => primary.deficit[resource] > 0);
  const targets = state.playerOrder
    .filter((candidate) => candidate !== player)
    .map((candidate) => {
      const estimate = getPlayerEstimate(state, candidate);
      const likelyResources = RESOURCE_ORDER.map((resource) => ({
        resource,
        chance: resourceStealProbability(state, candidate, resource),
      })).sort((left, right) => right.chance - left.chance);
      const neededCardChance = likelyResources
        .filter(({ resource }) => needed.includes(resource))
        .reduce((sum, { chance }) => sum + chance, 0);
      const threat = visibleThreat(state, candidate, board);
      const path = board
        ? likelyUpgradePath(board, candidate, estimate.average)
        : undefined;
      const score =
        neededCardChance * 58 +
        Math.min(10, (estimate.totalMinimum + estimate.totalMaximum) / 2) * 2 +
        threat * 2.8;
      const reasons = needed.length
        ? [`${Math.round(neededCardChance * 100)}% tracked chance of stealing a needed card`]
        : ["Best combination of visible hand size and public threat"];
      reasons.push(
        `${estimate.totalMinimum === estimate.totalMaximum ? estimate.totalMinimum : `${estimate.totalMinimum}–${estimate.totalMaximum}`} cards in hand`,
      );
      if (threat >= 5) reasons.push("Also slows a visible leader");
      if (path) {
        reasons.push(
          `${path.kind === "development" ? "Development card" : path.kind} is their fastest visible upgrade path`,
        );
      }
      return {
        player: candidate,
        score: Math.round(score),
        neededCardChance,
        likelyResources: likelyResources.slice(0, 3),
        reasons,
      };
    })
    .filter((target) => target.score > 0)
    .sort((left, right) => right.score - left.score);
  return targets[0];
};

export const resolveCoachPlayer = (
  state: TrackerState,
  preferred?: string,
): string | undefined => {
  if (preferred && state.players[preferred]) return preferred;
  if (state.players.You) return "You";
  return undefined;
};

export const createCoachReport = (
  state: TrackerState,
  preferredPlayer?: string,
  board?: BoardSnapshot,
  preparedAnalysis?: DecisionAnalysis,
): CoachReport | undefined => {
  const player = resolveCoachPlayer(state, preferredPlayer);
  if (!player) return undefined;
  const phase = phaseFor(state, player, board);
  const strategy = strategyFor(state, player, board);
  const developmentDeck = developmentDeckFor(state, board);
  const decisionAnalysis = board
    ? preparedAnalysis ?? analyzePublicEstimate(state, board, player)
    : undefined;
  const recommendations = buildRecommendations(
    state,
    player,
    phase,
    strategy,
    board,
    decisionAnalysis?.actionScores,
  );
  const [primary, ...alternatives] = recommendations;
  if (!primary) return undefined;
  const estimate = getPlayerEstimate(state, player);
  const alerts: string[] = [];
  const discardLimit = board?.players?.[player]?.cardDiscardLimit ?? 7;
  if (estimate.totalMaximum > discardLimit) {
    alerts.push(
      `Seven risk: you may be holding ${estimate.totalMinimum === estimate.totalMaximum ? estimate.totalMaximum : `${estimate.totalMinimum}–${estimate.totalMaximum}`} cards`,
    );
  }
  if (state.possibilitiesTruncated) {
    alerts.push("Recommendation confidence is reduced because the possibility set was compressed");
  }
  if (board) {
    const profile = playerBoardProfile(board, player);
    const pointsRemaining = profile.victoryTarget - profile.visiblePoints;
    if (pointsRemaining <= 2) {
      alerts.push(
        pointsRemaining <= 1
          ? "Win window: evaluate every legal one-point conversion before trading"
          : "Two-point window: city, trophy steals, and concealed development points dominate ordinary value",
      );
    }
    const blockedPips = board.vertices
      .filter((vertex) => vertex.building?.player === player)
      .reduce((total, vertex) => {
        const multiplier = vertex.building?.kind === "city" ? 2 : 1;
        return (
          total +
          vertex.adjacentHexes.reduce((sum, id) => {
            const hex = board.hexes.find((candidate) => candidate.id === id);
            return sum + (hex?.blocked && hex.number ? (NUMBER_PIPS[hex.number] ?? 0) * multiplier : 0);
          }, 0)
        );
      }, 0);
    if (blockedPips) {
      alerts.push(`The robber is suppressing ${blockedPips} of your production pips`);
    }
  }
  const profile = board ? playerBoardProfile(board, player) : undefined;
  const pointsRemaining = profile
    ? profile.victoryTarget - profile.visiblePoints
    : Number.POSITIVE_INFINITY;
  const winWindow =
    pointsRemaining <= 1
      ? "One point from victory"
      : pointsRemaining === 2
        ? "A city or trophy swing can win"
        : profile?.roadsToLongest === 1
          ? "One road from a two-point trophy swing"
          : profile?.knightsToLargest === 1
            ? "One played knight from a two-point trophy swing"
          : undefined;
  const trade = board
    ? generateTradeProposals(state, board, player, {
        primaryKind: primary.kind,
        primaryDeficit: primary.deficit,
        phase,
      })[0]
    : fallbackTradeRecommendation(state, player, primary);
  const steal = recommendSteal(state, player, primary, board);
  const developmentTiming = board
    ? recommendDevelopmentCards(state, board, player, {
        primaryKind: primary.kind,
        primaryDeficit: primary.deficit,
        phase,
        stealTarget: steal?.player,
        neededStealChance: steal?.neededCardChance,
      })
    : undefined;
  return {
    player,
    phase,
    strategy,
    primary,
    alternatives: alternatives.slice(0, 2),
    trade,
    steal,
    developmentDeck,
    ...(decisionAnalysis ? { decisionAnalysis } : {}),
    ...(developmentTiming ? { developmentTiming } : {}),
    ...(winWindow ? { winWindow } : {}),
    alerts,
  };
};
