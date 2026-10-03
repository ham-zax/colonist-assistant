import type { RootTradeActionExclusion } from "./engine";
import {
  MaritimeCycleMemory,
  type MaritimeCycleObservation,
} from "./maritime-cycle";
import type { BoardSnapshot } from "./placement";
import { BUILD_COSTS, emptyResources, type ResourceVector } from "./resources";

/** Only exact, resolved observations can establish a bank conversion segment. */
export const maritimeCycleObservation = (
  board?: BoardSnapshot,
): MaritimeCycleObservation | undefined => {
  const player = board?.myPlayer;
  const own = player ? board?.players?.[player] : undefined;
  if (
    !board || board.gameOver || !board.isMyTurn || !player || !board.gameKey ||
    board.turn === undefined || !board.ownHand || !own ||
    board.localSeatDiagnostics?.identity.status !== "resolved"
  ) return undefined;
  return {
    gameKey: board.gameKey,
    turn: board.turn,
    player,
    isMyTurn: true,
    hand: board.ownHand,
    ratios: own.tradeRatios,
    ...(board.bankVisible && board.bank ? { bank: board.bank } : {}),
    assetSignature: JSON.stringify({
      action: board.action,
      rolled: board.hasRolled,
      buildings: board.vertices.flatMap(({ id, building }) => building
        ? [[id, building.player, building.kind]] : []).sort(),
      roads: board.edges.filter((edge) => edge.player)
        .map((edge) => [edge.id, edge.player]).sort(),
      ports: board.vertices.filter((vertex) => vertex.port)
        .map((vertex) => [vertex.id, vertex.port]).sort(),
      robber: board.hexes.filter((hex) => hex.blocked).map((hex) => hex.id).sort(),
      cards: board.ownDevelopmentCards,
      // A domestic transfer can imitate a bank-shaped own-hand delta. Its
      // partner's public hand total changes too, invalidating this segment.
      rivals: Object.entries(board.players ?? {})
        .filter(([name]) => name !== player)
        .map(([name, publicState]) => [name, publicState.handSize, publicState.developmentCards]).sort(),
    }),
  };
};

/** Costs are offered only for known placement targets and remaining pieces. */
const recoveryCosts = (
  board: BoardSnapshot,
): Partial<ResourceVector>[] | undefined => {
  if (!board.buildableRoadIds || !board.buildableSettlementIds || !board.buildableCityIds) return undefined;
  const player = board.myPlayer;
  const costs: Partial<ResourceVector>[] = [];
  const roads = board.edges.filter((edge) => edge.player === player).length;
  const buildings = board.vertices.flatMap(({ building }) =>
    building && building.player === player ? [building.kind] : []);
  if (roads < 15 && board.buildableRoadIds.length) costs.push(BUILD_COSTS.road);
  if (buildings.filter((kind) => kind === "settlement").length < 5 && board.buildableSettlementIds.length) costs.push(BUILD_COSTS.settlement);
  if (buildings.filter((kind) => kind === "city").length < 4 && board.buildableCityIds.length) costs.push(BUILD_COSTS.city);
  // Card identities stay hidden. Public unplayed/played totals can establish
  // deck exhaustion; missing totals conservatively preserve purchase recovery.
  const publicPlayers = Object.values(board.players ?? {});
  const deckKnown = publicPlayers.every((publicState) =>
    publicState.developmentCards !== undefined && publicState.playedDevelopmentCards !== undefined);
  const used = publicPlayers.reduce((sum, publicState) =>
    sum + (publicState.developmentCards ?? 0) + Object.values(publicState.playedDevelopmentCards ?? {})
      .reduce((sum, count) => sum + count, 0), 0);
  if (!deckKnown || used < 25) costs.push(BUILD_COSTS.development);
  return costs;
};

export const maritimeCycleRootExclusions = (
  memory: MaritimeCycleMemory,
  board?: BoardSnapshot,
): RootTradeActionExclusion[] => {
  const observation = maritimeCycleObservation(board);
  if (!observation || !board) return [];
  const costs = recoveryCosts(board);
  if (!costs) return [];
  return memory.exclusions(observation, costs, board.players?.[observation.player!]?.cardDiscardLimit).map((exclusion) => {
    const give = emptyResources();
    const receive = emptyResources();
    give[exclusion.give] = exclusion.ratio;
    receive[exclusion.receive] = 1;
    return { kind: "maritime-trade", give, receive };
  });
};
