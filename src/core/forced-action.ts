import type { BoardAction, BoardSnapshot } from "./placement";
import {
  BUILD_COSTS,
  RESOURCE_ORDER,
  hasResources,
  type Resource,
} from "./resources";

const hasPlayableDevelopment = (
  board: BoardSnapshot,
): boolean => {
  const playable = board.ownDevelopmentCards?.playable;
  return Boolean(
    playable &&
      (
        playable.knight > 0 ||
        playable.monopoly > 0 ||
        playable["road-building"] > 0 ||
        playable["year-of-plenty"] > 0
      ),
  );
};

const canBuildAnything = (board: BoardSnapshot): boolean => {
  const hand = board.ownHand;
  return Boolean(
    hand &&
      Object.values(BUILD_COSTS).some((cost) =>
        hasResources(hand, cost),
      ),
  );
};

const ownTradeRatio = (
  board: BoardSnapshot,
  resource: Resource,
): number =>
  Math.max(
    2,
    board.myPlayer
      ? board.players?.[board.myPlayer]?.tradeRatios[resource] ?? 4
      : 4,
  );

const canTradeWithBank = (board: BoardSnapshot): boolean => {
  const hand = board.ownHand;
  if (!hand) return false;
  return RESOURCE_ORDER.some((give) => {
    if (hand[give] < ownTradeRatio(board, give)) return false;
    return RESOURCE_ORDER.some(
      (receive) =>
        receive !== give &&
        (!board.bankVisible || (board.bank?.[receive] ?? 0) > 0),
    );
  });
};

/**
 * Colonist's global action state can report `discard` while a different player
 * is resolving their seven. Only the local card-picker proves that this client
 * owns the mandatory discard action.
 */
export const resolveLocalBoardAction = (
  action: BoardAction,
  localDiscardPromptVisible: boolean,
): BoardAction => {
  if (localDiscardPromptVisible) return "discard";
  return action === "discard" ? "none" : action;
};

export const shouldFastTrackRoll = (
  board: BoardSnapshot,
  visibleControl: "roll" | "end" | undefined,
): boolean =>
  !board.gameOver &&
  Boolean(board.isMyTurn) &&
  (board.action ?? "none") === "none" &&
  visibleControl === "roll" &&
  !hasPlayableDevelopment(board) &&
  !(board.activeTrades ?? []).some(
    (trade) =>
      trade.incoming &&
      (!trade.myResponse || trade.myResponse === "pending"),
  );

/**
 * Ends only when the public/private snapshot proves that no meaningful
 * decision remains. This is deliberately stricter than a heuristic: a player
 * trade, bank conversion, build, or development-card choice keeps search on.
 */
export const shouldFastTrackEndTurn = (
  board: BoardSnapshot,
  visibleControl: "roll" | "end" | undefined,
): boolean =>
  !board.gameOver &&
  Boolean(board.isMyTurn) &&
  (board.action ?? "none") === "none" &&
  board.hasRolled !== false &&
  visibleControl === "end" &&
  Boolean(board.ownHand) &&
  board.domesticTradeUsed === true &&
  !(board.activeTrades?.length) &&
  !hasPlayableDevelopment(board) &&
  !canBuildAnything(board) &&
  !canTradeWithBank(board);
