import type {
  BoardAction,
  BoardPoint,
  BoardSnapshot,
} from "./placement";
import type { TrackerState } from "./types";

export type PlaceableBoardAction = Exclude<
  BoardAction,
  "none" | "discard"
>;

export interface PendingBoardPlacement {
  action: PlaceableBoardAction;
  targetId: string;
  point: BoardPoint;
  gameKey?: string;
  startedAt: number;
}

export const PLACEMENT_SYNC_TIMEOUT_MS = 10_000;

const targetNowOccupied = (
  pending: PendingBoardPlacement,
  board: BoardSnapshot,
): boolean => {
  if (pending.action === "road") {
    return Boolean(
      board.edges.find((edge) => edge.id === pending.targetId)?.player,
    );
  }
  if (pending.action === "robber") {
    return Boolean(
      board.hexes.find((hex) => hex.id === pending.targetId)?.blocked,
    );
  }
  const building = board.vertices.find(
    (vertex) => vertex.id === pending.targetId,
  )?.building;
  return pending.action === "city"
    ? building?.kind === "city"
    : Boolean(building);
};

/**
 * Colonist updates its canvas and its public action state on separate ticks.
 * Treat either signal as confirmation so an accepted placement never remains
 * highlighted while the other signal catches up.
 */
export const placementHasAdvanced = (
  pending: PendingBoardPlacement,
  board: BoardSnapshot,
): boolean =>
  Boolean(
    (pending.gameKey &&
      board.gameKey &&
      pending.gameKey !== board.gameKey) ||
      board.action !== pending.action ||
      targetNowOccupied(pending, board),
  );

export const placementIsAwaitingSync = (
  pending: PendingBoardPlacement | undefined,
  board: BoardSnapshot | undefined,
  now = Date.now(),
): pending is PendingBoardPlacement =>
  Boolean(
    pending &&
      board &&
      !placementHasAdvanced(pending, board) &&
      now - pending.startedAt < PLACEMENT_SYNC_TIMEOUT_MS,
  );

const spendReasonForPlacement = (
  action: PlaceableBoardAction,
): "road" | "settlement" | "city" | undefined =>
  action === "robber" ? undefined : action;

export const placementConfirmedByPublicLog = (
  pending: PendingBoardPlacement,
  state: TrackerState,
  player: string,
): boolean => {
  const reason = spendReasonForPlacement(pending.action);
  if (!reason) return false;
  return state.recentEvents.some(
    (event) =>
      event.timestamp >= pending.startedAt - 250 &&
      event.type === "spend" &&
      event.player === player &&
      event.reason === reason,
  );
};

/**
 * If Colonist's action enum lags behind its public game log, advance a local
 * copy just far enough to calculate the next legal opening prompt. A later
 * authoritative snapshot always replaces this patch.
 */
export const applyConfirmedPlacement = (
  pending: PendingBoardPlacement,
  board: BoardSnapshot,
  player: string,
): BoardSnapshot => {
  if (pending.action === "settlement" || pending.action === "city") {
    const vertices = board.vertices.map((vertex) =>
      vertex.id === pending.targetId
        ? {
            ...vertex,
            building: {
              player,
              kind:
                pending.action === "city"
                  ? ("city" as const)
                  : ("settlement" as const),
            },
          }
        : vertex,
    );
    const legalEdgeIds =
      pending.action === "settlement" && board.initialPlacement
        ? board.edges
            .filter(
              (edge) =>
                !edge.player && edge.vertices.includes(pending.targetId),
            )
            .map((edge) => edge.id)
        : undefined;
    return {
      ...board,
      vertices,
      action:
        pending.action === "settlement" && board.initialPlacement
          ? "road"
          : "none",
      legalVertexIds: undefined,
      legalEdgeIds,
      observedAt: Date.now(),
    };
  }
  if (pending.action === "road") {
    return {
      ...board,
      edges: board.edges.map((edge) =>
        edge.id === pending.targetId ? { ...edge, player } : edge,
      ),
      action: "none",
      legalEdgeIds: undefined,
      observedAt: Date.now(),
    };
  }
  return board;
};
