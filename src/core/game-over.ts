const TERMINAL_HEADING =
  /^(?:victory|defeat|game over|well played|you won|you lost)[!.]*$/iu;

/**
 * Match only an actual end-of-game heading. Colonist keeps labels such as
 * "Victory Points" visible throughout play, so substring matching can stop
 * the executor and live benchmark many turns too early.
 */
export const isTerminalGameHeading = (
  value: string | null | undefined,
): boolean => TERMINAL_HEADING.test(value?.trim() ?? "");
