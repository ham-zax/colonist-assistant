import type { DecisionEngine } from "../core/engine";
import { isExtensionContextInvalidatedError } from "./extension-context";

export type AutopilotDelaySeconds = 0 | 1 | 3 | 5;

export const AUTOPILOT_DELAY_OPTIONS: readonly AutopilotDelaySeconds[] = [
  0, 1, 3, 5,
];

export const normalizeAutopilotDelaySeconds = (
  value: unknown,
): AutopilotDelaySeconds =>
  AUTOPILOT_DELAY_OPTIONS.includes(value as AutopilotDelaySeconds)
    ? (value as AutopilotDelaySeconds)
    : 0;

export const normalizeDecisionEngine = (value: unknown): DecisionEngine =>
  value === "weighted" ? "weighted" : "deep-search";

export interface AssistantSettings {
  enabled: boolean;
  startCollapsed: boolean;
  engine: DecisionEngine;
  highlightNextAction: boolean;
  disablePlayerTrades: boolean;
  recordGame: boolean;
  /** Legacy key name; enables autopilot in any Colonist game. */
  autonomousPrivateGames: boolean;
  autopilotDelaySeconds: AutopilotDelaySeconds;
}

export interface OverlayPosition {
  left?: number;
  top?: number;
}

export const DEFAULT_SETTINGS: AssistantSettings = {
  enabled: true,
  startCollapsed: false,
  engine: "deep-search",
  highlightNextAction: true,
  disablePlayerTrades: false,
  recordGame: false,
  autonomousPrivateGames: false,
  autopilotDelaySeconds: 0,
};

export const SETTINGS_KEY = "colonistAssistantSettings";
export const POSITION_KEY = "colonistAssistantPosition";
export const RESET_NONCE_KEY = "colonistAssistantResetNonce";
const LEGACY_SETTINGS_KEY = "harborLedgerSettings";
const LEGACY_POSITION_KEY = "harborLedgerPosition";

export const readSettings = async (): Promise<AssistantSettings> => {
  const result = await chrome.storage.sync.get([
    SETTINGS_KEY,
    LEGACY_SETTINGS_KEY,
  ]);
  const settings = {
    ...DEFAULT_SETTINGS,
    ...((result[SETTINGS_KEY] ?? result[LEGACY_SETTINGS_KEY]) as
      | Partial<AssistantSettings>
      | undefined),
  };
  const normalizedEngine = normalizeDecisionEngine(settings.engine);
  const normalizedDelay = normalizeAutopilotDelaySeconds(
    settings.autopilotDelaySeconds,
  );
  const needsNormalization =
    settings.engine !== normalizedEngine ||
    settings.autopilotDelaySeconds !== normalizedDelay;
  settings.engine = normalizedEngine;
  settings.autopilotDelaySeconds = normalizedDelay;
  if (needsNormalization) {
    await chrome.storage.sync.set({ [SETTINGS_KEY]: settings });
  }
  return settings;
};

export const saveSettings = async (settings: AssistantSettings): Promise<void> => {
  try {
    await chrome.storage.sync.set({
      [SETTINGS_KEY]: {
        ...settings,
        engine: normalizeDecisionEngine(settings.engine),
        autopilotDelaySeconds: normalizeAutopilotDelaySeconds(
          settings.autopilotDelaySeconds,
        ),
      },
    });
  } catch (error) {
    if (!isExtensionContextInvalidatedError(error)) throw error;
  }
};

export const readPosition = async (): Promise<OverlayPosition> => {
  try {
    const result = await chrome.storage.local.get([
      POSITION_KEY,
      LEGACY_POSITION_KEY,
    ]);
    return ((result[POSITION_KEY] ?? result[LEGACY_POSITION_KEY]) as
      | OverlayPosition
      | undefined) ?? {};
  } catch (error) {
    if (isExtensionContextInvalidatedError(error)) return {};
    throw error;
  }
};

export const savePosition = async (position: OverlayPosition): Promise<void> => {
  try {
    await chrome.storage.local.set({ [POSITION_KEY]: position });
  } catch (error) {
    if (!isExtensionContextInvalidatedError(error)) throw error;
  }
};
