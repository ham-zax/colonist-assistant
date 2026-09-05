import type { DecisionEngine } from "../core/engine";
import { isExtensionContextInvalidatedError } from "./extension-context";

export type AutopilotDelaySeconds = 0 | 1 | 3 | 5;
export type InterfaceScale = 1 | 1.15 | 1.3;

export const AUTOPILOT_DELAY_OPTIONS: readonly AutopilotDelaySeconds[] = [
  0, 1, 3, 5,
];

export const INTERFACE_SCALE_OPTIONS: readonly InterfaceScale[] = [
  1, 1.15, 1.3,
];

export const normalizeAutopilotDelaySeconds = (
  value: unknown,
): AutopilotDelaySeconds =>
  AUTOPILOT_DELAY_OPTIONS.includes(value as AutopilotDelaySeconds)
    ? (value as AutopilotDelaySeconds)
    : 0;

export const normalizeInterfaceScale = (value: unknown): InterfaceScale =>
  INTERFACE_SCALE_OPTIONS.includes(value as InterfaceScale)
    ? (value as InterfaceScale)
    : 1.15;

export const normalizeDecisionEngine = (_value: unknown): DecisionEngine =>
  "deep-search";

export interface AssistantSettings {
  enabled: boolean;
  startCollapsed: boolean;
  engine: DecisionEngine;
  highlightNextAction: boolean;
  disablePlayerTrades: boolean;
  recordGame: boolean;
  investigationLog: boolean;
  /** Legacy key name; enables autopilot in any Colonist game. */
  autonomousPrivateGames: boolean;
  autopilotDelaySeconds: AutopilotDelaySeconds;
  interfaceScale: InterfaceScale;
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
  investigationLog: false,
  autonomousPrivateGames: false,
  autopilotDelaySeconds: 0,
  interfaceScale: 1.15,
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
  const normalizedInterfaceScale = normalizeInterfaceScale(
    settings.interfaceScale,
  );
  const needsNormalization =
    settings.engine !== normalizedEngine ||
    settings.autopilotDelaySeconds !== normalizedDelay ||
    settings.interfaceScale !== normalizedInterfaceScale;
  settings.engine = normalizedEngine;
  settings.autopilotDelaySeconds = normalizedDelay;
  settings.interfaceScale = normalizedInterfaceScale;
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
        interfaceScale: normalizeInterfaceScale(settings.interfaceScale),
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
