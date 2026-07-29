import type { BoardSnapshot } from "./placement";

const playablePreRollDevelopment = (
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

export const shouldFastTrackRoll = (
  board: BoardSnapshot,
  visibleControl: "roll" | "end" | undefined,
): boolean =>
  !board.gameOver &&
  Boolean(board.isMyTurn) &&
  (board.action ?? "none") === "none" &&
  visibleControl === "roll" &&
  !playablePreRollDevelopment(board) &&
  !(board.activeTrades ?? []).some(
    (trade) =>
      trade.incoming &&
      (!trade.myResponse || trade.myResponse === "pending"),
  );
