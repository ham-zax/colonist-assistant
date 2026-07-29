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
const ENGINE_DEFAULT_MIGRATION_KEY = "colonistAssistantMaxNDefaultV1";

const LEGACY_SETTINGS_KEY = "harborLedgerSettings";
const LEGACY_POSITION_KEY = "harborLedgerPosition";

export const readSettings = async (): Promise<AssistantSettings> => {
  const result = await chrome.storage.sync.get([
    SETTINGS_KEY,
    LEGACY_SETTINGS_KEY,
    ENGINE_DEFAULT_MIGRATION_KEY,
  ]);
  const settings = {
    ...DEFAULT_SETTINGS,
    ...((result[SETTINGS_KEY] ?? result[LEGACY_SETTINGS_KEY]) as
      | Partial<AssistantSettings>
      | undefined),
  };
  // Belief PUCT shipped as the experimental default before its strength gate
  // was complete. Move existing installs to the stronger validated MaxN
  // engine once; an explicit choice made after this migration is preserved.
  if (result[ENGINE_DEFAULT_MIGRATION_KEY] !== true) {
    settings.engine = "deep-search";
    await chrome.storage.sync.set({
      [SETTINGS_KEY]: settings,
      [ENGINE_DEFAULT_MIGRATION_KEY]: true,
    });
  }
  return settings;
};

export const saveSettings = async (settings: AssistantSettings): Promise<void> => {
  try {
    await chrome.storage.sync.set({ [SETTINGS_KEY]: settings });
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
