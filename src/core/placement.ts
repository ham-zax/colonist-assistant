import {
  RESOURCE_LABELS,
  RESOURCE_ORDER,
  RESOURCE_STRATEGIC_WEIGHTS,
  emptyResources,
  type Resource,
  type ResourceVector,
} from "./resources";
import { longestRoadFromEdges } from "./roads";

export type BoardAction =
  | "settlement"
  | "city"
  | "road"
  | "robber"
  | "discard"
  | "none";

export type KnownDevelopmentCard =
  | "knight"
  | "monopoly"
  | "road-building"
  | "year-of-plenty"
  | "victory-point";

export type DevelopmentCardVector = Record<KnownDevelopmentCard, number>;

export interface OwnDevelopmentCardState {
  cards: DevelopmentCardVector;
  playable: DevelopmentCardVector;
  boughtThisTurn: DevelopmentCardVector;
  hasPlayedThisTurn: boolean;
}

export interface ActiveTradeOffer {
  id: string;
  creator: string;
  tradeExecutor: string;
  give: ResourceVector;
  receive: ResourceVector;
  incoming: boolean;
  counterOffer: boolean;
  canAccept: boolean;
  acceptedPlayers?: string[];
  pendingPlayers?: string[];
  rejectedPlayers?: string[];
  responsesComplete?: boolean;
  myResponse?: "pending" | "accepted" | "rejected" | "embargoed";
}

export interface BoardPoint {
  x: number;
  y: number;
}

export interface BoardHex {
  id: string;
  resource?: Resource;
  number?: number;
  blocked?: boolean;
  screen?: BoardPoint;
}

export interface BoardVertex {
  id: string;
  label?: string;
  adjacentHexes: string[];
  adjacentVertices: string[];
  port?: Resource | "generic";
  building?: { player: string; kind: "settlement" | "city" };
  screen?: BoardPoint;
}

export interface BoardEdge {
  id: string;
  label?: string;
  vertices: [string, string];
  player?: string;
  screen?: BoardPoint;
}

export interface BoardPlayerPublicState {
  handSize: number;
  tradeRatios: ResourceVector;
  cardDiscardLimit: number;
  developmentCards?: number;
  playedKnights?: number;
  visiblePoints?: number;
  longestRoad?: number;
  hasLongestRoad?: boolean;
  hasLargestArmy?: boolean;
}

export interface BoardAssetMap {
  resources?: Partial<Record<Resource, string>>;
  pieces?: Partial<
    Record<
      | "road"
      | "settlement"
      | "city"
      | "development"
      | "robber"
      | "longestRoad"
      | "largestArmy"
      | "knight"
      | "monopoly"
      | "roadBuilding"
      | "yearOfPlenty"
      | "victoryPoint",
      string
    >
  >;
}

export interface BoardSnapshot {
  hexes: BoardHex[];
  vertices: BoardVertex[];
  edges: BoardEdge[];
  legalVertexIds?: string[];
  legalEdgeIds?: string[];
  buildableSettlementIds?: string[];
  buildableCityIds?: string[];
  buildableRoadIds?: string[];
  myPlayer?: string;
  ownHand?: ResourceVector;
  ownDevelopmentCards?: OwnDevelopmentCardState;
  activeTrades?: ActiveTradeOffer[];
  bank?: ResourceVector;
  bankVisible?: boolean;
  players?: Record<string, BoardPlayerPublicState>;
  playerOrder?: string[];
  assets?: BoardAssetMap;
  discardCount?: number;
  robberVictimSelection?: boolean;
  robberVictimPlayers?: string[];
  gameKey?: string;
  isMyTurn?: boolean;
  action?: BoardAction;
  initialPlacement?: boolean;
  picksUntilNext?: number;
  victoryTarget?: number;
  friendlyRobber?: boolean;
  privateGame?: boolean;
  botOnlyGame?: boolean;
  currentPlayer?: string;
  turn?: number;
  lastRoll?: number;
  hasRolled?: boolean;
  domesticTradeUsed?: boolean;
  gameOver?: boolean;
  winner?: string;
  observedAt?: number;
}

export interface PlacementContext {
  player: string;
  desiredResources?: Resource[];
  currentResources?: Resource[];
  currentNumbers?: number[];
  production?: Partial<ResourceVector>;
  opponentThreat?: Record<string, number>;
  stealPriority?: Record<string, number>;
  legalVertexIds?: string[];
  legalEdgeIds?: string[];
  preferredRoadTargetId?: string;
  initialPlacement?: boolean;
  picksUntilNext?: number;
  requireConnection?: boolean;
}

export interface PlacementRecommendation {
  id: string;
  label: string;
  score: number;
  reasons: string[];
  targetPlayer?: string;
  metrics?: {
    rawPips?: number;
    weightedPips?: number;
    strikeWays?: number;
    expectedPerRound?: number;
    roadsRequired?: number;
    targetId?: string;
    routeEdgeIds?: string[];
    targetRawPips?: number;
    purpose?: "opening" | "settlement" | "race" | "longest-road" | "none";
    strategicallyUseful?: boolean;
  };
}

export const NUMBER_PIPS: Record<number, number> = {
  2: 1,
  3: 2,
  4: 3,
  5: 4,
  6: 5,
  8: 5,
  9: 4,
  10: 3,
  11: 2,
  12: 1,
};

const byId = <T extends { id: string }>(items: T[]): Map<string, T> =>
  new Map(items.map((item) => [item.id, item]));

const vertexIsOpen = (
  vertex: BoardVertex,
  vertices: Map<string, BoardVertex>,
): boolean =>
  !vertex.building &&
  vertex.adjacentVertices.every((neighbor) => !vertices.get(neighbor)?.building);

/**
 * During each opening pair, Colonist only accepts a road adjacent to the
 * settlement just placed. That settlement is the user's sole opening building
 * without an adjacent owned road, including when the extension loads mid-prompt.
 */
export const openingRoadEdgeIds = (
  board: Pick<BoardSnapshot, "vertices" | "edges">,
  player: string,
): string[] | undefined => {
  const anchors = board.vertices.filter(
    (vertex) =>
      vertex.building?.player === player &&
      !board.edges.some(
        (edge) =>
          edge.player === player && edge.vertices.includes(vertex.id),
      ),
  );
  if (anchors.length !== 1) return undefined;
  return board.edges
    .filter(
      (edge) => !edge.player && edge.vertices.includes(anchors[0]!.id),
    )
    .map((edge) => edge.id);
};

const productionAt = (
  vertex: BoardVertex,
  hexes: Map<string, BoardHex>,
): Array<{ resource: Resource; number: number; pips: number; blocked: boolean }> =>
  vertex.adjacentHexes
    .map((id) => hexes.get(id))
    .filter(
      (hex): hex is BoardHex & { resource: Resource; number: number } =>
        Boolean(hex?.resource && hex.number && NUMBER_PIPS[hex.number]),
    )
    .map((hex) => ({
      resource: hex.resource,
      number: hex.number,
      pips: NUMBER_PIPS[hex.number] ?? 0,
      blocked: Boolean(hex.blocked),
    }));

const scoreProduction = (
  production: ReturnType<typeof productionAt>,
  desired: Set<Resource>,
): number =>
  production.reduce(
    (sum, item) =>
      sum +
      item.pips *
        RESOURCE_STRATEGIC_WEIGHTS[item.resource] *
        (desired.has(item.resource) ? 1.16 : 1) *
        (item.blocked ? 0.3 : 1),
    0,
  );

const boardScarcity = (board: BoardSnapshot): Map<Resource, number> => {
  const result = new Map<Resource, number>();
  for (const resource of RESOURCE_ORDER) {
    const totalPips = board.hexes
      .filter((hex) => hex.resource === resource)
      .reduce((sum, hex) => sum + (hex.number ? NUMBER_PIPS[hex.number] ?? 0 : 0), 0);
    result.set(resource, totalPips ? 1 / totalPips : 0);
  }
  return result;
};

interface SettlementAssessment {
  score: number;
  production: ReturnType<typeof productionAt>;
  resources: Set<Resource>;
  numbers: Set<number>;
  rawPips: number;
  weightedPips: number;
  strikeWays: number;
  newResources: Resource[];
  strongest?: ReturnType<typeof productionAt>[number];
}

const assessSettlement = (
  board: BoardSnapshot,
  vertex: BoardVertex,
  context: PlacementContext,
  hexes: Map<string, BoardHex>,
  vertices: Map<string, BoardVertex>,
  scarcity: Map<Resource, number>,
): SettlementAssessment => {
  const desired = new Set(context.desiredResources ?? []);
  const current = new Set(context.currentResources ?? []);
  const production = productionAt(vertex, hexes);
  const resources = new Set(production.map((item) => item.resource));
  const numbers = new Set(production.map((item) => item.number));
  const newResources = [...resources].filter((resource) => !current.has(resource));
  const rawPips = production.reduce((sum, item) => sum + item.pips, 0);
  const weightedPips = production.reduce(
    (sum, item) =>
      sum +
      item.pips *
        RESOURCE_STRATEGIC_WEIGHTS[item.resource] *
        (item.blocked ? 0.3 : 1),
    0,
  );
  const strikeWays = [...numbers].reduce(
    (sum, number) => sum + (NUMBER_PIPS[number] ?? 0),
    0,
  );
  const repeatedNumberPenalty = [...numbers].reduce((sum, number) => {
    const count = production.filter((item) => item.number === number).length;
    return sum + Math.max(0, count - 1) * (NUMBER_PIPS[number] ?? 0) * 0.9;
  }, 0);
  const existingNumbers = new Set(context.currentNumbers ?? []);
  const networkOverlapPenalty = [...numbers].reduce(
    (sum, number) =>
      sum +
      (existingNumbers.has(number) ? (NUMBER_PIPS[number] ?? 0) * 0.52 : 0),
    0,
  );
  const concentrationPenalty = rawPips
    ? Math.max(0, Math.max(...production.map((item) => item.pips)) / rawPips - 0.46) * 7
    : 0;
  const scarceValue = production.reduce(
    (sum, item) =>
      sum +
      item.pips *
        RESOURCE_STRATEGIC_WEIGHTS[item.resource] *
        (scarcity.get(item.resource) ?? 0) *
        6,
    0,
  );
  const ownedProduction = context.production ?? emptyResources();
  const matchedPortPips =
    vertex.port && vertex.port !== "generic"
      ? (ownedProduction[vertex.port] ?? 0) +
        production
          .filter((item) => item.resource === vertex.port)
          .reduce((sum, item) => sum + item.pips, 0)
      : 0;
  // A 2:1 port adds .25 wildcard-equivalent value per matching surplus
  // card versus baseline 4:1; a generic port adds .083.
  const portFit =
    vertex.port && vertex.port !== "generic"
      ? Math.max(0, matchedPortPips - 3) * 0.25 * 2.6
      : vertex.port === "generic"
        ? rawPips * 0.083 * 1.8
        : 0;
  const denialValue = vertex.adjacentVertices.reduce((sum, id) => {
    const adjacent = vertices.get(id);
    if (!adjacent || !vertexIsOpen(adjacent, vertices)) return sum;
    return sum + productionAt(adjacent, hexes).reduce((total, item) => total + item.pips, 0) * 0.08;
  }, 0);
  return {
    score:
      scoreProduction(production, desired) +
      strikeWays * 0.34 +
      resources.size * 1.15 +
      newResources.length * 2.6 +
      scarceValue +
      portFit +
      denialValue -
      repeatedNumberPenalty -
      networkOverlapPenalty -
      concentrationPenalty,
    production,
    resources,
    numbers,
    rawPips,
    weightedPips,
    strikeWays,
    newResources,
    strongest: [...production].sort((left, right) => right.pips - left.pips)[0],
  };
};

const projectedSecondSettlement = (
  board: BoardSnapshot,
  candidate: BoardVertex,
  context: PlacementContext,
  hexes: Map<string, BoardHex>,
  vertices: Map<string, BoardVertex>,
  scarcity: Map<Resource, number>,
): { score: number; pips: number } | undefined => {
  const blocked = new Set([candidate.id, ...candidate.adjacentVertices]);
  const available = (): BoardVertex[] =>
    board.vertices.filter(
      (vertex) => !blocked.has(vertex.id) && vertexIsOpen(vertex, vertices),
    );
  const neutralContext: PlacementContext = { player: context.player };
  const picks = Math.max(0, Math.min(6, context.picksUntilNext ?? 0));

  for (let index = 0; index < picks; index += 1) {
    const opponentPick = available()
      .map((vertex) => ({
        vertex,
        score: assessSettlement(
          board,
          vertex,
          neutralContext,
          hexes,
          vertices,
          scarcity,
        ).score,
      }))
      .sort((left, right) => right.score - left.score)[0]?.vertex;
    if (!opponentPick) break;
    blocked.add(opponentPick.id);
    for (const adjacent of opponentPick.adjacentVertices) blocked.add(adjacent);
  }

  const candidateResources = new Set([
    ...(context.currentResources ?? []),
    ...productionAt(candidate, hexes).map((item) => item.resource),
  ]);
  const followUpContext: PlacementContext = {
    ...context,
    currentResources: [...candidateResources],
    currentNumbers: [
      ...(context.currentNumbers ?? []),
      ...productionAt(candidate, hexes).map((item) => item.number),
    ],
    initialPlacement: false,
  };
  const followUp = available()
    .map((vertex) => ({
      vertex,
      assessment: assessSettlement(
        board,
        vertex,
        followUpContext,
        hexes,
        vertices,
        scarcity,
      ),
    }))
    .sort((left, right) => right.assessment.score - left.assessment.score)[0];
  if (!followUp) return undefined;
  return {
    score: followUp.assessment.score,
    pips: followUp.assessment.production.reduce((sum, item) => sum + item.pips, 0),
  };
};

export const scoreSettlementPlacements = (
  board: BoardSnapshot,
  context: PlacementContext,
): PlacementRecommendation[] => {
  const hexes = byId(board.hexes);
  const vertices = byId(board.vertices);
  const legal = context.legalVertexIds ? new Set(context.legalVertexIds) : undefined;
  const scarcity = boardScarcity(board);
  const ownedSettlements = board.vertices.filter(
    (vertex) => vertex.building?.player === context.player,
  ).length;

  return board.vertices
    .filter((vertex) => {
      if (!vertexIsOpen(vertex, vertices)) return false;
      if (legal) return legal.has(vertex.id);
      if (!context.requireConnection || context.initialPlacement) return true;
      return board.edges.some(
        (edge) => edge.player === context.player && edge.vertices.includes(vertex.id),
      );
    })
    .map((vertex) => {
      const assessment = assessSettlement(
        board,
        vertex,
        context,
        hexes,
        vertices,
        scarcity,
      );
      const lookahead =
        context.initialPlacement && ownedSettlements === 0
          ? projectedSecondSettlement(
              board,
              vertex,
              context,
              hexes,
              vertices,
              scarcity,
            )
          : undefined;
      const score = assessment.score + (lookahead?.score ?? 0) * 0.28;
      const reasons = [
        `${assessment.rawPips} pips with a ${assessment.strikeWays}/36 roll strike rate`,
      ];
      if (lookahead) {
        reasons.push(
          `Preserves a ${lookahead.pips}-pip complementary second site after ${Math.max(0, context.picksUntilNext ?? 0)} projected pick${context.picksUntilNext === 1 ? "" : "s"}`,
        );
      }
      reasons.push(
        `${assessment.resources.size} resource type${assessment.resources.size === 1 ? "" : "s"}`,
      );
      if (assessment.newResources.length) {
        reasons.push(
          `Adds ${assessment.newResources
            .map((resource) => RESOURCE_LABELS[resource].toLowerCase())
            .join(" and ")}`,
        );
      }
      if (!lookahead && assessment.strongest) {
        reasons.push(
          `Best tile: ${assessment.strongest.number} ${RESOURCE_LABELS[
            assessment.strongest.resource
          ].toLowerCase()}`,
        );
      }
      if (vertex.port) {
        reasons.push(
          vertex.port === "generic"
            ? "Immediate 3:1 port access"
            : `${RESOURCE_LABELS[vertex.port]} port converts your production`,
        );
      }
      return {
        id: vertex.id,
        label: vertex.label ?? vertex.id,
        score: Math.round(score * 10) / 10,
        reasons,
        metrics: {
          rawPips: assessment.rawPips,
          weightedPips: Math.round(assessment.weightedPips * 10) / 10,
          strikeWays: assessment.strikeWays,
          expectedPerRound:
            Math.round((assessment.rawPips / 9) * 100) / 100,
        },
      };
    })
    .sort((left, right) => right.score - left.score);
};

export const scoreCityPlacements = (
  board: BoardSnapshot,
  context: PlacementContext,
): PlacementRecommendation[] => {
  const hexes = byId(board.hexes);
  const desired = new Set(context.desiredResources ?? []);
  const legal = context.legalVertexIds ? new Set(context.legalVertexIds) : undefined;
  return board.vertices
    .filter(
      (vertex) =>
        vertex.building?.player === context.player &&
        vertex.building.kind === "settlement" &&
        (!legal || legal.has(vertex.id)),
    )
    .map((vertex) => {
      const production = productionAt(vertex, hexes);
      const activeProduction = production.filter((item) => !item.blocked);
      const blockedPips = production
        .filter((item) => item.blocked)
        .reduce((sum, item) => sum + item.pips, 0);
      const score = scoreProduction(production, desired);
      const rawPips = production.reduce((sum, item) => sum + item.pips, 0);
      const weightedPips = production.reduce(
        (sum, item) =>
          sum +
          item.pips *
            RESOURCE_STRATEGIC_WEIGHTS[item.resource] *
            (item.blocked ? 0.3 : 1),
        0,
      );
      const reasons = [
        `Adds ${activeProduction.reduce((sum, item) => sum + item.pips, 0)} active pips (${Math.round((rawPips / 36) * 100) / 100} cards per roll)`,
        `Doubles ${new Set(production.map((item) => item.resource)).size} resource type${new Set(production.map((item) => item.resource)).size === 1 ? "" : "s"}`,
      ];
      if (blockedPips) reasons.push(`${blockedPips} pips are blocked by the robber right now`);
      return {
        id: vertex.id,
        label: vertex.label ?? vertex.id,
        score: Math.round(score * 10) / 10,
        reasons,
        metrics: {
          rawPips,
          weightedPips: Math.round(weightedPips * 10) / 10,
          expectedPerRound: Math.round((rawPips / 9) * 100) / 100,
        },
      };
    })
    .sort((left, right) => right.score - left.score);
};

export const scoreRoadPlacements = (
  board: BoardSnapshot,
  context: PlacementContext,
): PlacementRecommendation[] => {
  const legal = context.legalEdgeIds ? new Set(context.legalEdgeIds) : undefined;
  const vertices = byId(board.vertices);
  const futureSettlements = new Map(
    scoreSettlementPlacements(board, {
      ...context,
      legalVertexIds: undefined,
      initialPlacement: false,
      requireConnection: false,
    }).map((item) => [item.id, item]),
  );
  const graph = new Map<string, Array<{ edge: BoardEdge; other: string }>>();
  for (const edge of board.edges) {
    const [left, right] = edge.vertices;
    graph.set(left, [...(graph.get(left) ?? []), { edge, other: right }]);
    graph.set(right, [...(graph.get(right) ?? []), { edge, other: left }]);
  }

  const nearestOpponentRoute = (
    targetId: string,
  ): { player: string; distance: number } | undefined => {
    const queue = [{ vertex: targetId, distance: 0 }];
    const visited = new Map([[targetId, 0]]);
    while (queue.length) {
      const current = queue.shift()!;
      const vertex = vertices.get(current.vertex);
      if (!vertex) continue;
      if (vertex.building && vertex.building.player !== context.player) {
        return {
          player: vertex.building.player,
          distance: current.distance,
        };
      }
      const opponentEdge = (graph.get(current.vertex) ?? []).find(
        ({ edge }) => edge.player && edge.player !== context.player,
      );
      if (opponentEdge?.edge.player) {
        return { player: opponentEdge.edge.player, distance: current.distance };
      }
      if (current.distance >= 3) continue;
      for (const next of graph.get(current.vertex) ?? []) {
        if (next.edge.player) continue;
        const distance = current.distance + 1;
        if (distance >= (visited.get(next.other) ?? Number.POSITIVE_INFINITY)) continue;
        visited.set(next.other, distance);
        queue.push({ vertex: next.other, distance });
      }
    }
    return undefined;
  };

  const connectedBeforePlacement = (vertexId: string): boolean => {
    const vertex = vertices.get(vertexId);
    if (vertex?.building?.player === context.player) return true;
    if (vertex?.building && vertex.building.player !== context.player) return false;
    return (graph.get(vertexId) ?? []).some(
      ({ edge }) => edge.player === context.player,
    );
  };

  const routeFromCandidate = (
    edge: BoardEdge,
  ):
    | {
        target: PlacementRecommendation;
        roadsRequired: number;
        routeEdgeIds: string[];
        value: number;
      }
    | undefined => {
    const connected = edge.vertices.map(connectedBeforePlacement);
    const frontierIds =
      connected[0] && !connected[1]
        ? [edge.vertices[1]]
        : connected[1] && !connected[0]
          ? [edge.vertices[0]]
          : connected[0] && connected[1]
            ? []
            : [...edge.vertices];
    let best:
      | {
          target: PlacementRecommendation;
          roadsRequired: number;
          routeEdgeIds: string[];
          value: number;
        }
      | undefined;

    for (const frontierId of frontierIds) {
      const queue = [
        {
          vertex: frontierId,
          roadsRequired: 1,
          routeEdgeIds: [edge.id],
        },
      ];
      const visited = new Map([[frontierId, 1]]);
      while (queue.length) {
        const current = queue.shift()!;
        const vertex = vertices.get(current.vertex);
        if (!vertex) continue;
        const target = futureSettlements.get(vertex.id);
        if (target) {
          const targetPips = target.metrics?.rawPips ?? 0;
          const portTarget = Boolean(vertex.port);
          const preferred =
            context.preferredRoadTargetId === vertex.id ? 12 : 0;
          const weakTargetPenalty =
            targetPips < 7 && !portTarget ? (7 - targetPips) * 2.8 : 0;
          const value =
            target.score * 0.62 -
            Math.max(0, current.roadsRequired - 1) * 6.4 -
            weakTargetPenalty +
            preferred;
          if (!best || value > best.value) {
            best = {
              target,
              roadsRequired: current.roadsRequired,
              routeEdgeIds: current.routeEdgeIds,
              value,
            };
          }
        }
        if (current.roadsRequired >= 4) continue;
        if (vertex.building && vertex.building.player !== context.player) {
          continue;
        }
        for (const next of graph.get(vertex.id) ?? []) {
          if (next.edge.id === edge.id || next.edge.player) continue;
          if (current.routeEdgeIds.includes(next.edge.id)) continue;
          const roadsRequired = current.roadsRequired + 1;
          if (
            roadsRequired >=
            (visited.get(next.other) ?? Number.POSITIVE_INFINITY)
          ) {
            continue;
          }
          visited.set(next.other, roadsRequired);
          queue.push({
            vertex: next.other,
            roadsRequired,
            routeEdgeIds: [...current.routeEdgeIds, next.edge.id],
          });
        }
      }
    }
    return best;
  };

  return board.edges
    .filter((edge) => {
      if (edge.player) return false;
      if (legal) return legal.has(edge.id);
      if (!context.requireConnection) return true;
      return edge.vertices.some((vertexId) => {
        const vertex = vertices.get(vertexId);
        if (vertex?.building?.player === context.player) return true;
        if (vertex?.building && vertex.building.player !== context.player) return false;
        return board.edges.some(
          (candidate) =>
            candidate.player === context.player &&
            candidate.id !== edge.id &&
            candidate.vertices.includes(vertexId),
        );
      });
    })
    .map((edge) => {
      const route = routeFromCandidate(edge);
      const openBranches = edge.vertices.reduce(
        (sum, vertex) =>
          sum +
          (graph.get(vertex) ?? []).filter(
            (candidate) => candidate.edge.id !== edge.id && !candidate.edge.player,
          ).length,
        0,
      );
      const opponentRoute = route
        ? nearestOpponentRoute(route.target.id)
        : undefined;
      const remainingOwnRoads = Math.max(0, (route?.roadsRequired ?? 1) - 1);
      const raceBonus =
        opponentRoute &&
        route &&
        opponentRoute.distance >= remainingOwnRoads &&
        opponentRoute.distance <= route.roadsRequired
          ? 6 + Math.max(0, route.value * 0.16)
          : 0;
      const likelyLostRacePenalty =
        opponentRoute &&
        route &&
        opponentRoute.distance < remainingOwnRoads
          ? 14
          : 0;
      const publicState = board.players?.[context.player];
      const exactCurrentLength = longestRoadFromEdges(board, context.player);
      const currentLength = Math.max(
        publicState?.longestRoad ?? 0,
        exactCurrentLength,
      );
      const boardAfterRoad: BoardSnapshot = {
        ...board,
        edges: board.edges.map((candidate) =>
          candidate.id === edge.id
            ? { ...candidate, player: context.player }
            : candidate,
        ),
      };
      const exactResultingLength = longestRoadFromEdges(
        boardAfterRoad,
        context.player,
      );
      const resultingLength =
        currentLength + Math.max(0, exactResultingLength - exactCurrentLength);
      const roadLengthGain = resultingLength - currentLength;
      const longestOpponent = Math.max(
        0,
        ...Object.entries(board.players ?? {})
          .filter(([player]) => player !== context.player)
          .map(([player, publicPlayer]) =>
            Math.max(
              publicPlayer.longestRoad ?? 0,
              longestRoadFromEdges(board, player),
            ),
          ),
      );
      const trophyTarget = Math.max(5, longestOpponent + 1);
      const winsOrClaimsTrophy =
        !publicState?.hasLongestRoad && resultingLength >= trophyTarget;
      const defendsTrophy =
        Boolean(publicState?.hasLongestRoad) &&
        roadLengthGain > 0 &&
        currentLength <= longestOpponent + 1;
      const trophyBonus = winsOrClaimsTrophy
        ? 18
        : defendsTrophy
          ? 8
          : roadLengthGain > 0 &&
              currentLength >= 4 &&
              trophyTarget - currentLength <= 2
            ? 3.5
            : 0;
      const targetRawPips = route?.target.metrics?.rawPips ?? 0;
      const immediateExpansion =
        Boolean(route) &&
        route!.roadsRequired === 1 &&
        (targetRawPips >= 7 ||
          Boolean(vertices.get(route!.target.id)?.port));
      const coherentTwoRoadRace =
        Boolean(route) &&
        route!.roadsRequired === 2 &&
        (raceBonus > 0 || targetRawPips >= 10);
      const strategicallyUseful =
        Boolean(context.initialPlacement) ||
        winsOrClaimsTrophy ||
        defendsTrophy ||
        immediateExpansion ||
        coherentTwoRoadRace;
      const purpose: NonNullable<
        NonNullable<PlacementRecommendation["metrics"]>["purpose"]
      > = context.initialPlacement
        ? "opening"
        : winsOrClaimsTrophy || defendsTrophy
          ? "longest-road"
          : raceBonus
            ? "race"
            : route
              ? "settlement"
              : "none";
      const deadRoadPenalty =
        route || winsOrClaimsTrophy || defendsTrophy ? 0 : 24;
      const lowPurposePenalty = strategicallyUseful ? 0 : 13;
      const score =
        (winsOrClaimsTrophy || defendsTrophy
          ? Math.max(0, route?.value ?? 0)
          : (route?.value ?? 0)) +
        openBranches * 0.65 +
        raceBonus +
        trophyBonus -
        deadRoadPenalty -
        lowPurposePenalty -
        likelyLostRacePenalty;
      const reasons = [
        route
          ? route.roadsRequired === 1
            ? `Immediately opens ${route.target.label}`
            : `Commits to one coherent route of ${route.roadsRequired} roads toward ${route.target.label}`
          : "Extends your network without a clear settlement target",
        opponentRoute && raceBonus
          ? `Contests a site ${opponentRoute.player} can reach in ${opponentRoute.distance} road${opponentRoute.distance === 1 ? "" : "s"}`
          : likelyLostRacePenalty && opponentRoute
            ? `${opponentRoute.player} is likely to arrive first`
            : route
              ? `${targetRawPips} target pips after ${route.roadsRequired} road${route.roadsRequired === 1 ? "" : "s"}`
          : `${openBranches} open branch${openBranches === 1 ? "" : "es"} remain`,
      ];
      if (winsOrClaimsTrophy) {
        reasons.unshift("Claims a two-point Longest Road swing");
      } else if (trophyBonus) {
        reasons.push("Keeps the Longest Road race live");
      } else if (!strategicallyUseful) {
        reasons.push(
          "Low-value vanity road: do not spend without an immediate conversion",
        );
      }
      return {
        id: edge.id,
        label: route ? `Route to ${route.target.label}` : edge.label ?? edge.id,
        score: Math.round(score * 10) / 10,
        reasons,
        metrics: {
          roadsRequired: route?.roadsRequired,
          targetId: route?.target.id,
          routeEdgeIds: route?.routeEdgeIds,
          targetRawPips,
          purpose,
          strategicallyUseful,
        },
      };
    })
    .sort((left, right) => right.score - left.score);
};

export const scoreRobberPlacements = (
  board: BoardSnapshot,
  context: PlacementContext,
): PlacementRecommendation[] => {
  const threat = context.opponentThreat ?? {};
  const stealPriority = context.stealPriority ?? {};
  return board.hexes
    .filter((hex) => {
      if (!hex.resource || !hex.number || hex.blocked) return false;
      if (!board.friendlyRobber) return true;
      return !board.vertices.some(
        (vertex) =>
          vertex.adjacentHexes.includes(hex.id) &&
          vertex.building &&
          (board.players?.[vertex.building.player]?.visiblePoints ?? 3) < 3,
      );
    })
    .map((hex) => {
      const pips = NUMBER_PIPS[hex.number!] ?? 0;
      const resourceWeight = RESOURCE_STRATEGIC_WEIGHTS[hex.resource!];
      const buildings = board.vertices
        .filter((vertex) => vertex.adjacentHexes.includes(hex.id) && vertex.building)
        .map((vertex) => vertex.building!);
      const players = new Map<string, number>();
      for (const building of buildings) {
        const impact = pips * (building.kind === "city" ? 2 : 1);
        players.set(building.player, (players.get(building.player) ?? 0) + impact);
      }
      const ownImpact = players.get(context.player) ?? 0;
      const targets = [...players.entries()]
        .filter(([player]) => player !== context.player)
        .map(([player, impact]) => ({
          player,
          impact,
          weightedImpact: impact * resourceWeight,
          score:
            impact * resourceWeight +
            (threat[player] ?? 0) * 1.05 +
            (stealPriority[player] ?? 0) +
            ((board.players?.[player]?.visiblePoints ?? 0) >=
            (board.victoryTarget ?? 10) - 2
              ? 7
              : 0),
        }))
        .sort((left, right) => right.score - left.score);
      const bestTarget = targets[0];
      const opponentImpact = targets.reduce(
        (sum, target) => sum + target.weightedImpact,
        0,
      );
      const score =
        opponentImpact +
        (bestTarget ? (threat[bestTarget.player] ?? 0) * 1.15 : 0) +
        (bestTarget ? stealPriority[bestTarget.player] ?? 0 : 0) -
        ownImpact * resourceWeight * 2.15;
      const reasons = [
        opponentImpact
          ? `Denies ${Math.round(opponentImpact * 10) / 10} weighted production pips`
          : "Does not block meaningful opponent production",
        ownImpact
          ? `Also costs you ${ownImpact} production pips`
          : "Leaves your production untouched",
      ];
      if (bestTarget) {
        reasons.push(
          `Best steal target here: ${bestTarget.player}${(board.players?.[bestTarget.player]?.visiblePoints ?? 0) >= (board.victoryTarget ?? 10) - 2 ? " — immediate win threat" : ""}`,
        );
      }
      return {
        id: hex.id,
        label: `${hex.number} ${RESOURCE_LABELS[hex.resource!].toLowerCase()}`,
        score: Math.round(score * 10) / 10,
        reasons,
        ...(bestTarget ? { targetPlayer: bestTarget.player } : {}),
        metrics: {
          rawPips: pips,
          weightedPips: Math.round(opponentImpact * 10) / 10,
        },
      };
    })
    .sort((left, right) => right.score - left.score);
};
