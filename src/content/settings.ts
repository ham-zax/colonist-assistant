import type { DecisionEngine } from "../core/engine";
import { isExtensionContextInvalidatedError } from "./extension-context";

export interface AssistantSettings {
  enabled: boolean;
  startCollapsed: boolean;
  engine: DecisionEngine;
  highlightNextAction: boolean;
  autonomousPrivateGames: boolean;
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
  autonomousPrivateGames: false,
};

export const SETTINGS_KEY = "colonistAssistantSettings";
export const POSITION_KEY = "colonistAssistantPosition";
export const RESET_NONCE_KEY = "colonistAssistantResetNonce";
const STRATEGIST_MIGRATION_KEY = "colonistAssistantStrategistDefaultV1";

const LEGACY_SETTINGS_KEY = "harborLedgerSettings";
const LEGACY_POSITION_KEY = "harborLedgerPosition";

export const readSettings = async (): Promise<AssistantSettings> => {
  const result = await chrome.storage.sync.get([
    SETTINGS_KEY,
    LEGACY_SETTINGS_KEY,
    STRATEGIST_MIGRATION_KEY,
  ]);
  const settings = {
    ...DEFAULT_SETTINGS,
    ...((result[SETTINGS_KEY] ?? result[LEGACY_SETTINGS_KEY]) as
      | Partial<AssistantSettings>
      | undefined),
  };
  // Strategy selection is intentionally no longer a product setting. Migrate
  // every historical or invalid value to the one observation-safe authority.
  const needsMigration =
    result[STRATEGIST_MIGRATION_KEY] !== true ||
    settings.engine !== "deep-search";
  settings.engine = "deep-search";
  if (needsMigration) {
    await chrome.storage.sync.set({
      [SETTINGS_KEY]: settings,
      [STRATEGIST_MIGRATION_KEY]: true,
    });
  }
  return settings;
};

export const saveSettings = async (settings: AssistantSettings): Promise<void> => {
  try {
    await chrome.storage.sync.set({
      [SETTINGS_KEY]: { ...settings, engine: "deep-search" },
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
