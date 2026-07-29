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
  it("migrates existing installs to the validated Deep MaxN default once", async () => {
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
        colonistAssistantMaxNDefaultV1: true,
      }),
    );
  });

  it("preserves an explicit engine choice after the default migration", async () => {
    const set = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("chrome", {
      storage: {
        sync: {
          get: vi.fn().mockResolvedValue({
            [SETTINGS_KEY]: {
              ...DEFAULT_SETTINGS,
              engine: "deep-puct",
            },
            colonistAssistantMaxNDefaultV1: true,
          }),
          set,
        },
      },
    });

    const settings = await readSettings();

    expect(settings.engine).toBe("deep-puct");
    expect(set).not.toHaveBeenCalled();
  });

  it("preserves the selectable AlphaBeta engine", async () => {
    const set = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("chrome", {
      storage: {
        sync: {
          get: vi.fn().mockResolvedValue({
            [SETTINGS_KEY]: {
              ...DEFAULT_SETTINGS,
              engine: "deep-alpha-beta",
            },
            colonistAssistantMaxNDefaultV1: true,
          }),
          set,
        },
      },
    });

    const settings = await readSettings();

    expect(settings.engine).toBe("deep-alpha-beta");
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
