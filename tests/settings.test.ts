// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";

import {
  DEFAULT_SETTINGS,
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
  it("migrates every retired engine to Strategist", async () => {
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

    expect(settings.engine).toBe("deep-search");
    expect(set).toHaveBeenCalledWith(
      expect.objectContaining({
        [SETTINGS_KEY]: expect.objectContaining({
          engine: "deep-search",
        }),
        colonistAssistantStrategistDefaultV1: true,
      }),
    );
  });

  it("does not preserve a retired engine after migration", async () => {
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

    expect(settings.engine).toBe("deep-search");
    expect(set).toHaveBeenCalledWith(
      expect.objectContaining({
        [SETTINGS_KEY]: expect.objectContaining({
          engine: "deep-search",
        }),
      }),
    );
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
