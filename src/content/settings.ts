import type { DecisionEngine, EngineTuning, FallbackEngine } from "../core/engine";
import { isExtensionContextInvalidatedError } from "./extension-context";

export type AutopilotDelaySeconds = 0 | 1 | 3 | 5;
export type TuningProfile = "extra-high" | "high" | "medium" | "fast" | "max" | "custom";

export const AUTOPILOT_DELAY_OPTIONS: readonly AutopilotDelaySeconds[] = [
  0, 1, 3, 5,
];

export const normalizeAutopilotDelaySeconds = (
  value: unknown,
): AutopilotDelaySeconds =>
  value === 0 || value === 1 || value === 3 || value === 5 ? value : 0;

export const normalizeFallbackEngine = (value: unknown): FallbackEngine =>
  value === "weighted" || value === "alpha-beta-fast" ? value : "none";

export interface AssistantSettings {
  enabled: boolean;
  startCollapsed: boolean;
  engine: DecisionEngine;
  highlightNextAction: boolean;
  /** Legacy key name; enables autopilot in any Colonist game. */
  autonomousPrivateGames: boolean;
  autopilotDelaySeconds: AutopilotDelaySeconds;
  fallbackEngine: FallbackEngine;
  engineTuning: Partial<Record<DecisionEngine, EngineTuning>>;
}

export const TUNING_PROFILE_LABELS: Record<Exclude<TuningProfile, "custom">, string> = {
  "extra-high": "Extra High",
  high: "High",
  medium: "Medium",
  fast: "Fast",
  max: "MAX",
};

export const getTuningProfile = (
  engine: DecisionEngine,
  profile: Exclude<TuningProfile, "custom">,
): EngineTuning => {
  const defaults = ENGINE_TUNING_DEFAULTS[engine];
  const overrides: Partial<EngineTuning> = {
    "extra-high": {
      maxDepth: 5, branchCap: 22, maxNodes: 64_000, beliefParticles: 112,
      strategicParticleLimit: 56, iterations: 1_600, rolloutActions: 260,
      maxThinkingTimeMs: 4_000,
    },
    high: {
      maxDepth: 3, branchCap: 16, maxNodes: 24_000, beliefParticles: 64,
      strategicParticleLimit: 32, iterations: 768, rolloutActions: 180,
      maxThinkingTimeMs: 2_000,
    },
    medium: {},
    fast: {
      maxDepth: 2, branchCap: 8, maxNodes: 4_000, beliefParticles: 8,
      strategicParticleLimit: 4, iterations: 128, rolloutActions: 48,
      maxThinkingTimeMs: 500,
    },
    max: {
      maxDepth: 6,
      branchCap: 24,
      maxNodes: 100_000,
      beliefParticles: 128,
      strategicParticleLimit: 64,
      iterations: 2_000,
      rolloutActions: 300,
      maxThinkingTimeMs: 5_000,
    },
  }[profile];
  return normalizeEngineTuning(engine, { ...defaults, ...overrides });
};

export const ENGINE_TUNING_DEFAULTS: Record<DecisionEngine, EngineTuning> = {
  "deep-search": {
    maxDepth: 3,
    branchCap: 12,
    maxNodes: 16_000,
    beliefParticles: 48,
    strategicParticleLimit: 24,
    iterations: 384,
    rolloutActions: 108,
    maxThinkingTimeMs: 1_000,
  },
  "deep-alpha-beta": {
    maxDepth: 2,
    branchCap: 12,
    maxNodes: 12_000,
    beliefParticles: 32,
    strategicParticleLimit: 16,
    iterations: 256,
    rolloutActions: 96,
    maxThinkingTimeMs: 1_000,
  },
  "deep-puct": {
    maxDepth: 2,
    branchCap: 12,
    maxNodes: 12_000,
    beliefParticles: 32,
    strategicParticleLimit: 16,
    iterations: 512,
    rolloutActions: 128,
    maxThinkingTimeMs: 1_000,
  },
  hybrid: {
    maxDepth: 0,
    branchCap: 0,
    maxNodes: 0,
    beliefParticles: 0,
    strategicParticleLimit: 0,
    iterations: 0,
    rolloutActions: 0,
    maxThinkingTimeMs: 500,
  },
  "race-eta": {
    maxDepth: 0,
    branchCap: 0,
    maxNodes: 0,
    beliefParticles: 0,
    strategicParticleLimit: 0,
    iterations: 0,
    rolloutActions: 0,
    maxThinkingTimeMs: 250,
  },
  "vector-mcts": {
    maxDepth: 0,
    branchCap: 0,
    maxNodes: 0,
    beliefParticles: 0,
    strategicParticleLimit: 0,
    iterations: 384,
    rolloutActions: 108,
    maxThinkingTimeMs: 1_000,
  },
};

export const getEngineTuning = (
  settings: Pick<AssistantSettings, "engine" | "engineTuning">,
): EngineTuning => ({
  ...ENGINE_TUNING_DEFAULTS[settings.engine],
  ...(settings.engineTuning?.[settings.engine] ?? {}),
});

export const normalizeEngineTuning = (
  engine: DecisionEngine,
  tuning: Partial<EngineTuning>,
): EngineTuning => {
  const defaults = ENGINE_TUNING_DEFAULTS[engine];
  const integer = (value: unknown, min: number, max: number, fallback: number) =>
    Number.isFinite(Number(value))
      ? Math.round(Math.max(min, Math.min(max, Number(value))))
      : fallback;
  return {
    maxDepth: integer(tuning.maxDepth, 1, 6, defaults.maxDepth),
    branchCap: integer(tuning.branchCap, 4, 24, defaults.branchCap),
    maxNodes: integer(tuning.maxNodes, 2_000, 100_000, defaults.maxNodes),
    beliefParticles: integer(tuning.beliefParticles, 1, 128, defaults.beliefParticles),
    strategicParticleLimit: integer(tuning.strategicParticleLimit, 1, 64, defaults.strategicParticleLimit),
    iterations: integer(tuning.iterations, 16, 2_000, defaults.iterations),
    rolloutActions: integer(tuning.rolloutActions, 16, 300, defaults.rolloutActions),
    maxThinkingTimeMs: integer(tuning.maxThinkingTimeMs, 100, 5_000, defaults.maxThinkingTimeMs),
  };
};

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
  autopilotDelaySeconds: 0,
  fallbackEngine: "none",
  engineTuning: {},
};

export const SETTINGS_KEY = "colonistAssistantSettings";
export const POSITION_KEY = "colonistAssistantPosition";
export const RESET_NONCE_KEY = "colonistAssistantResetNonce";
const STRATEGIST_MIGRATION_KEY = "colonistAssistantStrategistDefaultV1";

const isDecisionEngine = (value: unknown): value is DecisionEngine =>
  value === "deep-search" ||
  value === "deep-alpha-beta" ||
  value === "deep-puct" ||
  value === "hybrid" ||
  value === "race-eta" ||
  value === "vector-mcts";

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
  settings.engineTuning = Object.fromEntries(
    Object.entries(settings.engineTuning ?? {}).map(([engine, tuning]) => [
      engine,
      normalizeEngineTuning(engine as DecisionEngine, tuning as Partial<EngineTuning>),
    ]),
  );
  // Preserve the historical selector while repairing corrupt values.
  const normalizedDelay = normalizeAutopilotDelaySeconds(
    settings.autopilotDelaySeconds,
  );
  const normalizedFallback = normalizeFallbackEngine(settings.fallbackEngine);
  const needsMigration =
    result[STRATEGIST_MIGRATION_KEY] !== true ||
    !isDecisionEngine(settings.engine) ||
    settings.autopilotDelaySeconds !== normalizedDelay ||
    settings.fallbackEngine !== normalizedFallback;
  if (!isDecisionEngine(settings.engine)) settings.engine = "deep-search";
  settings.autopilotDelaySeconds = normalizedDelay;
  settings.fallbackEngine = normalizedFallback;
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
      [SETTINGS_KEY]: {
        ...settings,
        engine: isDecisionEngine(settings.engine) ? settings.engine : "deep-search",
        engineTuning: Object.fromEntries(
          Object.entries(settings.engineTuning ?? {}).map(([engine, tuning]) => [
            engine,
            isDecisionEngine(engine)
              ? normalizeEngineTuning(engine, tuning as Partial<EngineTuning>)
              : tuning,
          ]),
        ),
        autopilotDelaySeconds: normalizeAutopilotDelaySeconds(
          settings.autopilotDelaySeconds,
        ),
        fallbackEngine: normalizeFallbackEngine(settings.fallbackEngine),
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
