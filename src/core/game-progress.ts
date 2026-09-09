// Public completed-turn counts include both rounds of snake-order setup.
// A setup flag with a later turn count is a contradictory hydration frame.
export const isSetupTurn = (
  completedTurns: number | undefined,
  playerCount: number,
): boolean =>
  typeof completedTurns === "number" &&
  Number.isSafeInteger(completedTurns) &&
  completedTurns >= 0 &&
  playerCount >= 2 &&
  completedTurns <= 2 * playerCount;
