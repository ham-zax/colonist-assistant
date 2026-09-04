import type { DiceMode } from "../core/placement";

export interface ColonistDiceModeObservation {
  mode: DiceMode;
  /** Forensic-only value for positively observed unsupported numeric settings. */
  rawUnsupportedSetting?: number;
}

/**
 * Terminate Colonist's numeric dice-setting contract at the page boundary.
 * Missing state is unobserved; every other non-production value fails closed.
 */
export const observeColonistDiceMode = (
  diceSetting: unknown,
): ColonistDiceModeObservation => {
  if (diceSetting === undefined || diceSetting === null) {
    return { mode: "unknown" };
  }
  if (diceSetting === 0) return { mode: "random" };
  if (diceSetting === 1) return { mode: "balanced" };
  return {
    mode: "unsupported",
    ...(typeof diceSetting === "number" && Number.isFinite(diceSetting)
      ? { rawUnsupportedSetting: diceSetting }
      : {}),
  };
};
