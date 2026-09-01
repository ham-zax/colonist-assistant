import {
  BUILD_COSTS,
  RESOURCE_LABELS,
  RESOURCE_ORDER,
  RESOURCE_STRATEGIC_WEIGHTS,
  cloneResources,
  emptyResources,
  hasResources,
  type BuildKind,
  type Resource,
  type ResourceVector,
} from "./resources";
import {
  NUMBER_PIPS,
  scoreRoadPlacements,
  scoreRobberPlacements,
  type BoardSnapshot,
  type KnownDevelopmentCard,
  type PlacementContext,
} from "./placement";
import { longestRoadFromEdges } from "./roads";
import {
  getPlayerEstimate,
} from "./tracker";
import {
  playerBoardProfile,
  strategicThreatScore,
} from "./strategy";
import type { TrackerState } from "./types";

export interface DevelopmentTimingContext {
  primaryKind: BuildKind;
  primaryDeficit: ResourceVector;
  phase: "opening" | "middle" | "closing";
  stealTarget?: string;
  neededStealChance?: number;
}

export interface DevelopmentCardRecommendation {
  card: KnownDevelopmentCard;
  useNow: boolean;
  score: number;
  title: string;
  reason: string;
  detail: string;
  resource?: Resource;
  resources?: [Resource, Resource];
  targetPlayer?: string;
  routeEdgeIds?: string[];
}

export interface DevelopmentTimingReport {
  primary?: DevelopmentCardRecommendation;
  cards: DevelopmentCardRecommendation[];
}

const CARD_LABELS: Record<KnownDevelopmentCard, string> = {
  knight: "Knight",
  monopoly: "Monopoly",
  "road-building": "Road Building",
  "year-of-plenty": "Year of Plenty",
  "victory-point": "Victory Point",
};

const buildUtility = (
  kind: BuildKind,
  board: BoardSnapshot,
  player: string,
): number => {
  const profile = playerBoardProfile(board, player);
  if (
    (kind === "settlement" &&
      (!profile.remaining.settlements || !profile.openSettlementSites)) ||
    (kind === "city" &&
      (!profile.remaining.cities || !profile.settlements)) ||
    (kind === "road" && !profile.remaining.roads)
  ) {
    return -100;
  }
  if (kind === "city") return 28;
  if (kind === "settlement") return 24;
  if (kind === "development") {
    return profile.knightsToLargest <= 2 ? 18 : 12;
  }
  return profile.roadsToLongest <= 2 ? 17 : 5;
};

const cardsNeededFor = (
  hand: ResourceVector,
  kind: BuildKind,
): ResourceVector => {
  const result = emptyResources();
  for (const resource of RESOURCE_ORDER) {
    result[resource] = Math.max(
      0,
      ((BUILD_COSTS[kind] as Partial<ResourceVector>)[resource] ?? 0) -
        hand[resource],
    );
  }
  return result;
};

const pairAvailableFromBank = (
  board: BoardSnapshot,
  left: Resource,
  right: Resource,
): boolean => {
  if (!board.bankVisible || !board.bank) return true;
  if (left === right) return board.bank[left] >= 2;
  return board.bank[left] >= 1 && board.bank[right] >= 1;
};

const yearOfPlentyRecommendation = (
  board: BoardSnapshot,
  player: string,
  context: DevelopmentTimingContext,
): DevelopmentCardRecommendation => {
  const hand = board.ownHand ?? emptyResources();
  let best:
    | {
        pair: [Resource, Resource];
        kind: BuildKind;
        score: number;
      }
    | undefined;
  for (let leftIndex = 0; leftIndex < RESOURCE_ORDER.length; leftIndex += 1) {
    for (
      let rightIndex = leftIndex;
      rightIndex < RESOURCE_ORDER.length;
      rightIndex += 1
    ) {
      const left = RESOURCE_ORDER[leftIndex]!;
      const right = RESOURCE_ORDER[rightIndex]!;
      if (!pairAvailableFromBank(board, left, right)) continue;
      const after = cloneResources(hand);
      after[left] += 1;
      after[right] += 1;
      for (const kind of Object.keys(BUILD_COSTS) as BuildKind[]) {
        if (hasResources(hand, BUILD_COSTS[kind])) continue;
        if (!hasResources(after, BUILD_COSTS[kind])) continue;
        const primaryBonus = kind === context.primaryKind ? 18 : 0;
        const scarcityBonus =
          RESOURCE_STRATEGIC_WEIGHTS[left] +
          RESOURCE_STRATEGIC_WEIGHTS[right];
        const score =
          buildUtility(kind, board, player) +
          primaryBonus +
          scarcityBonus +
          (context.phase === "closing" && kind !== "road" ? 8 : 0);
        if (!best || score > best.score) {
          best = { pair: [left, right], kind, score };
        }
      }
    }
  }
  if (!best) {
    return {
      card: "year-of-plenty",
      useNow: false,
      score: 0,
      title: "Hold Year of Plenty",
      reason: "It does not complete a worthwhile build yet",
      detail:
        "Use it only when both bank cards convert immediately into a settlement, city, development card, or decisive road.",
    };
  }
  return {
    card: "year-of-plenty",
    useNow: true,
    score: best.score,
    title: "Play Year of Plenty now",
    reason: `Take ${RESOURCE_LABELS[best.pair[0]].toLowerCase()} and ${RESOURCE_LABELS[best.pair[1]].toLowerCase()}`,
    detail: `Those two cards immediately complete your ${best.kind === "development" ? "development card" : best.kind}.`,
    resources: best.pair,
  };
};

const monopolyRecommendation = (
  state: TrackerState,
  board: BoardSnapshot,
  player: string,
  context: DevelopmentTimingContext,
): DevelopmentCardRecommendation => {
  const candidates = RESOURCE_ORDER.map((resource) => {
    let guaranteed = 0;
    let expected = 0;
    for (const opponent of state.playerOrder) {
      if (opponent === player) continue;
      const estimate = getPlayerEstimate(state, opponent);
      guaranteed += estimate.minimum[resource];
      expected += estimate.average[resource];
    }
    const completesPrimary =
      context.primaryDeficit[resource] > 0 &&
      guaranteed >= context.primaryDeficit[resource];
    const strategicValue =
      expected *
        RESOURCE_STRATEGIC_WEIGHTS[resource] *
        (completesPrimary ? 1.5 : 1) +
      guaranteed * 0.7;
    return {
      resource,
      guaranteed,
      expected,
      completesPrimary,
      strategicValue,
    };
  }).sort((left, right) => right.strategicValue - left.strategicValue);
  const best = candidates[0]!;
  const useNow =
    best.guaranteed >= 3 ||
    best.expected >= 4 ||
    (best.completesPrimary &&
      (context.primaryKind === "city" ||
        context.primaryKind === "settlement")) ||
    (context.phase === "closing" && best.expected >= 2.5);
  return {
    card: "monopoly",
    useNow,
    score: useNow
      ? best.strategicValue * 9 +
        (best.completesPrimary ? 18 : 0)
      : best.strategicValue,
    title: useNow ? "Play Monopoly now" : "Hold Monopoly",
    reason: useNow
      ? `Call ${RESOURCE_LABELS[best.resource]}`
      : `Wait for more ${RESOURCE_LABELS[best.resource].toLowerCase()} to accumulate`,
    detail: `${best.guaranteed} guaranteed and about ${best.expected.toFixed(1)} expected across opponents${best.completesPrimary ? `; the guaranteed cards complete your ${context.primaryKind}` : ""}.`,
    resource: best.resource,
  };
};

const roadBuildingRecommendation = (
  board: BoardSnapshot,
  player: string,
): DevelopmentCardRecommendation => {
  const production = emptyResources();
  const currentNumbers: number[] = [];
  for (const vertex of board.vertices) {
    if (vertex.building?.player !== player) continue;
    const multiplier = vertex.building.kind === "city" ? 2 : 1;
    for (const hexId of vertex.adjacentHexes) {
      const hex = board.hexes.find((candidate) => candidate.id === hexId);
      if (!hex?.resource || !hex.number) continue;
      production[hex.resource] += (NUMBER_PIPS[hex.number] ?? 0) * multiplier;
      currentNumbers.push(hex.number);
    }
  }
  const placementContext: PlacementContext = {
    player,
    production,
    currentNumbers,
    currentResources: RESOURCE_ORDER.filter(
      (resource) => production[resource] > 0,
    ),
    legalEdgeIds: board.buildableRoadIds,
    requireConnection: true,
  };
  const route = scoreRoadPlacements(board, placementContext).find(
    (candidate) =>
      candidate.metrics?.strategicallyUseful &&
      (candidate.metrics.routeEdgeIds?.length ?? 0) >= 2,
  );
  const profile = playerBoardProfile(board, player);
  if (!route || profile.remaining.roads < 2) {
    return {
      card: "road-building",
      useNow: false,
      score: 0,
      title: "Hold Road Building",
      reason: "No coherent two-road conversion is available",
      detail:
        "Wait until two connected free roads secure a settlement race or create a real Longest Road swing.",
    };
  }
  const firstTwo = route.metrics!.routeEdgeIds!.slice(0, 2);
  const boardAfter: BoardSnapshot = {
    ...board,
    edges: board.edges.map((edge) =>
      firstTwo.includes(edge.id) ? { ...edge, player } : edge,
    ),
  };
  const beforeLongest = longestRoadFromEdges(board, player);
  const afterLongest = longestRoadFromEdges(boardAfter, player);
  const opponentLongest = Math.max(
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
  const claimsLongest =
    !profile.hasLongestRoad &&
    afterLongest >= Math.max(5, opponentLongest + 1);
  const securesExpansion =
    (route.metrics?.targetRawPips ?? 0) >= 8 &&
    (route.metrics?.roadsRequired ?? 99) <= 2;
  const useNow = claimsLongest || securesExpansion;
  return {
    card: "road-building",
    useNow,
    score:
      (claimsLongest ? 54 : 0) +
      (securesExpansion ? 30 : 0) +
      Math.max(0, afterLongest - beforeLongest) * 3,
    title: useNow ? "Play Road Building now" : "Hold Road Building",
    reason: claimsLongest
      ? "The two connected roads claim Longest Road"
      : useNow
        ? `Use both roads on the route to ${route.label.replace(/^Route to /u, "")}`
        : "The current route does not convert enough value",
    detail: `${route.reasons[0]}. Both road placements stay on the same route.`,
    routeEdgeIds: firstTwo,
  };
};

const knightRecommendation = (
  state: TrackerState,
  board: BoardSnapshot,
  player: string,
  context: DevelopmentTimingContext,
): DevelopmentCardRecommendation => {
  const profile = playerBoardProfile(board, player);
  const blockedWeightedPips = board.vertices
    .filter((vertex) => vertex.building?.player === player)
    .reduce((total, vertex) => {
      const multiplier = vertex.building?.kind === "city" ? 2 : 1;
      return (
        total +
        vertex.adjacentHexes.reduce((sum, id) => {
          const hex = board.hexes.find((candidate) => candidate.id === id);
          return (
            sum +
            (hex?.blocked && hex.number && hex.resource
              ? (NUMBER_PIPS[hex.number] ?? 0) *
                multiplier *
                RESOURCE_STRATEGIC_WEIGHTS[hex.resource]
              : 0)
          );
        }, 0)
      );
    }, 0);
  const threat = Object.fromEntries(
    state.playerOrder
      .filter((candidate) => candidate !== player)
      .map((candidate) => [
        candidate,
        strategicThreatScore(
          board,
          candidate,
          getPlayerEstimate(state, candidate).average,
          board.players?.[candidate]?.developmentCards ?? 0,
          board.players?.[candidate]?.playedDevelopmentCards?.knight ?? 0,
        ),
      ]),
  );
  const robberTarget = scoreRobberPlacements(board, {
    player,
    opponentThreat: threat,
    stealPriority:
      context.stealTarget && context.neededStealChance
        ? {
            [context.stealTarget]:
              Math.min(10, context.neededStealChance * 10),
          }
        : {},
  })[0];
  const armySwing = !profile.hasLargestArmy && profile.knightsToLargest === 1;
  const tacticalSteal =
    Boolean(context.stealTarget) &&
    (context.neededStealChance ?? 0) >= 0.48 &&
    context.phase !== "opening";
  const removesSeriousBlock = blockedWeightedPips >= 4.5;
  const endgameTempo =
    context.phase === "closing" &&
    profile.knightsToLargest <= 2 &&
    Boolean(robberTarget?.targetPlayer);
  const useNow =
    armySwing || removesSeriousBlock || tacticalSteal || endgameTempo;
  const targetPlayer =
    robberTarget?.targetPlayer ?? context.stealTarget;
  return {
    card: "knight",
    useNow,
    score:
      (armySwing ? 56 : 0) +
      (removesSeriousBlock ? blockedWeightedPips * 5 : 0) +
      (tacticalSteal ? 18 : 0) +
      (endgameTempo ? 12 : 0),
    title: useNow ? "Play a Knight now" : "Hold your Knight",
    reason: armySwing
      ? "It claims a two-point Largest Army swing"
      : removesSeriousBlock
        ? `It frees ${blockedWeightedPips.toFixed(1)} weighted production pips`
        : tacticalSteal
          ? `Rob ${targetPlayer ?? "the recommended opponent"} for your missing build card`
          : "Save it for robber relief, a decisive steal, or Largest Army timing",
    detail: robberTarget
      ? `${robberTarget.label} is the strongest current block${targetPlayer ? `; steal from ${targetPlayer}` : ""}.`
      : "No robber move currently creates enough denial or steal value.",
    ...(targetPlayer ? { targetPlayer } : {}),
  };
};

const inactiveRecommendation = (
  card: KnownDevelopmentCard,
  reason: string,
): DevelopmentCardRecommendation => ({
  card,
  useNow: false,
  score: -1,
  title: `Hold ${CARD_LABELS[card]}`,
  reason,
  detail: "Colonist does not currently allow this card to be played.",
});

export const recommendDevelopmentCards = (
  state: TrackerState,
  board: BoardSnapshot,
  player: string,
  context: DevelopmentTimingContext,
): DevelopmentTimingReport => {
  const own = board.ownDevelopmentCards;
  if (!own) return { cards: [] };
  const profile = playerBoardProfile(board, player);
  const recommendations: DevelopmentCardRecommendation[] = [];
  for (const card of Object.keys(own.cards) as KnownDevelopmentCard[]) {
    if (own.cards[card] <= 0) continue;
    if (card === "victory-point") {
      const wins =
        profile.visiblePoints + own.cards["victory-point"] >=
        profile.victoryTarget;
      recommendations.push({
        card,
        useNow: wins,
        score: wins ? 100 : 0,
        title: wins ? "Reveal Victory Points and win" : "Keep Victory Points hidden",
        reason: wins
          ? "Your public points plus hidden Victory Points reach the target"
          : "They score automatically and are strongest while concealed",
        detail: `${profile.visiblePoints} visible + ${own.cards["victory-point"]} hidden toward ${profile.victoryTarget}.`,
      });
      continue;
    }
    if (own.playable[card] <= 0) {
      const boughtNow = own.boughtThisTurn[card] > 0;
      recommendations.push(
        inactiveRecommendation(
          card,
          boughtNow
            ? "You bought it this turn"
            : own.hasPlayedThisTurn
              ? "You already played a development card this turn"
              : "Wait for your turn",
        ),
      );
      continue;
    }
    if (card === "knight") {
      recommendations.push(
        knightRecommendation(state, board, player, context),
      );
    } else if (card === "monopoly") {
      recommendations.push(
        monopolyRecommendation(state, board, player, context),
      );
    } else if (card === "road-building") {
      recommendations.push(roadBuildingRecommendation(board, player));
    } else if (card === "year-of-plenty") {
      recommendations.push(
        yearOfPlentyRecommendation(board, player, context),
      );
    }
  }
  recommendations.sort((left, right) => {
    if (left.useNow !== right.useNow) return left.useNow ? -1 : 1;
    return right.score - left.score;
  });
  return {
    primary: recommendations.find((recommendation) => recommendation.useNow),
    cards: recommendations,
  };
};
