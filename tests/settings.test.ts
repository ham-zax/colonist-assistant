// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";

import {
  DEFAULT_SETTINGS,
  AUTOPILOT_DELAY_OPTIONS,
  ENGINE_TUNING_DEFAULTS,
  getEngineTuning,
  getTuningProfile,
  normalizeFallbackEngine,
  normalizeEngineTuning,
  normalizeAutopilotDelaySeconds,
  readPosition,
  readSettings,
  savePosition,
  saveSettings,
  SETTINGS_KEY,
} from "../src/content/settings";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("assistant settings", () => {
  it("uses stronger per-engine defaults and clamps custom budgets", () => {
    expect(ENGINE_TUNING_DEFAULTS["deep-search"].maxDepth).toBe(3);
    expect(ENGINE_TUNING_DEFAULTS["deep-search"].maxNodes).toBe(16_000);
    expect(ENGINE_TUNING_DEFAULTS["deep-search"].beliefParticles).toBe(48);
    expect(ENGINE_TUNING_DEFAULTS["deep-alpha-beta"].maxDepth).toBe(2);
    expect(normalizeEngineTuning("deep-alpha-beta", {
      maxDepth: 99, maxNodes: 1, beliefParticles: 999, maxThinkingTimeMs: 0,
    })).toMatchObject({
      maxDepth: 6, maxNodes: 2_000, beliefParticles: 128, maxThinkingTimeMs: 100,
    });
    expect(getEngineTuning({ ...DEFAULT_SETTINGS, engineTuning: {} }).maxDepth).toBe(3);
    expect(getTuningProfile("deep-search", "fast")).toMatchObject({
      maxNodes: 4_000,
      beliefParticles: 8,
      maxThinkingTimeMs: 500,
    });
    expect(getTuningProfile("deep-search", "max")).toEqual({
      maxDepth: 6,
      branchCap: 24,
      maxNodes: 100_000,
      beliefParticles: 128,
      strategicParticleLimit: 64,
      iterations: 2_000,
      rolloutActions: 300,
      maxThinkingTimeMs: 5_000,
    });
    expect(normalizeFallbackEngine("alpha-beta-fast")).toBe("alpha-beta-fast");
    expect(normalizeFallbackEngine("unknown")).toBe("none");
  });

  it("round-trips every displayed autopilot delay option", () => {
    expect(
      AUTOPILOT_DELAY_OPTIONS.map(normalizeAutopilotDelaySeconds),
    ).toEqual(AUTOPILOT_DELAY_OPTIONS);
  });

  it("preserves every supported engine selection", async () => {
    const set = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("chrome", {
      storage: {
        sync: {
          get: vi.fn().mockResolvedValue({
            [SETTINGS_KEY]: {
              ...DEFAULT_SETTINGS,
              engine: "deep-puct",
            },
          }),
          set,
        },
      },
    });

    const settings = await readSettings();

    expect(settings.engine).toBe("deep-puct");
    expect(set).toHaveBeenCalledWith(
      expect.objectContaining({
        [SETTINGS_KEY]: expect.objectContaining({
          engine: "deep-puct",
        }),
        colonistAssistantStrategistDefaultV1: true,
      }),
    );
  });

  it("preserves a supported engine after migration", async () => {
    const set = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("chrome", {
      storage: {
        sync: {
          get: vi.fn().mockResolvedValue({
            [SETTINGS_KEY]: {
              ...DEFAULT_SETTINGS,
              engine: "deep-puct",
            },
            colonistAssistantStrategistDefaultV1: true,
          }),
          set,
        },
      },
    });

    const settings = await readSettings();

    expect(settings.engine).toBe("deep-puct");
    expect(set).not.toHaveBeenCalled();
  });

  it("sanitizes invalid stored engine strings", async () => {
    const set = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("chrome", {
      storage: {
        sync: {
          get: vi.fn().mockResolvedValue({
            [SETTINGS_KEY]: {
              ...DEFAULT_SETTINGS,
              engine: "corrupt-engine",
            },
            colonistAssistantStrategistDefaultV1: true,
          }),
          set,
        },
      },
    });

    const settings = await readSettings();

    expect(settings.engine).toBe("deep-search");
    expect(set).toHaveBeenCalled();
  });

  it("normalizes autopilot delay to 0, 1, 3, or 5 seconds", async () => {
    const set = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("chrome", {
      storage: {
        sync: {
          get: vi.fn().mockResolvedValue({
            [SETTINGS_KEY]: {
              ...DEFAULT_SETTINGS,
              autopilotDelaySeconds: 4,
            },
            colonistAssistantStrategistDefaultV1: true,
          }),
          set,
        },
      },
    });

    const settings = await readSettings();

    expect(settings.autopilotDelaySeconds).toBe(0);
    expect(set).toHaveBeenCalledWith(
      expect.objectContaining({
        [SETTINGS_KEY]: expect.objectContaining({
          autopilotDelaySeconds: 0,
        }),
      }),
    );
  });

  it("preserves a valid zero-second autopilot delay", async () => {
    const set = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("chrome", {
      storage: {
        sync: {
          get: vi.fn().mockResolvedValue({
            [SETTINGS_KEY]: {
              ...DEFAULT_SETTINGS,
              autopilotDelaySeconds: 0,
            },
            colonistAssistantStrategistDefaultV1: true,
          }),
          set,
        },
      },
    });

    const settings = await readSettings();

    expect(settings.autopilotDelaySeconds).toBe(0);
    expect(set).not.toHaveBeenCalled();
  });

  it("preserves a valid three-second autopilot delay", async () => {
    const set = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("chrome", {
      storage: {
        sync: {
          get: vi.fn().mockResolvedValue({
            [SETTINGS_KEY]: {
              ...DEFAULT_SETTINGS,
              autopilotDelaySeconds: 3,
            },
            colonistAssistantStrategistDefaultV1: true,
          }),
          set,
        },
      },
    });

    const settings = await readSettings();

    expect(settings.autopilotDelaySeconds).toBe(3);
    expect(set).not.toHaveBeenCalled();
  });

  it("preserves the one-second autopilot delay regression", async () => {
    const set = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("chrome", {
      storage: {
        sync: {
          get: vi.fn().mockResolvedValue({
            [SETTINGS_KEY]: {
              ...DEFAULT_SETTINGS,
              autopilotDelaySeconds: 1,
            },
            colonistAssistantStrategistDefaultV1: true,
          }),
          set,
        },
      },
    });

    const settings = await readSettings();

    expect(settings.autopilotDelaySeconds).toBe(1);
    expect(set).not.toHaveBeenCalled();
  });

  it("silences storage cleanup after the extension context is replaced", async () => {
    const invalidated = () => {
      throw new Error("Extension context invalidated.");
    };
    vi.stubGlobal("chrome", {
      storage: {
        local: {
          get: vi.fn(invalidated),
          set: vi.fn(invalidated),
        },
        sync: {
          set: vi.fn(invalidated),
        },
      },
    });

    await expect(readPosition()).resolves.toEqual({});
    await expect(savePosition({ left: 10, top: 20 })).resolves.toBeUndefined();
    await expect(
      saveSettings(DEFAULT_SETTINGS),
    ).resolves.toBeUndefined();
  });
});
