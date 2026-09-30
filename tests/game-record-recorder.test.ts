// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";

import { GameRecordRecorder, type GameRecordCapture } from "../src/core/game-record";

const storage = new Map<string, unknown>();

afterEach(() => {
  storage.clear();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

const installStorage = (): void => {
  vi.stubGlobal("chrome", {
    storage: {
      local: {
        get: vi.fn(async (keys: string[]) =>
          Object.fromEntries(keys.map((key) => [key, structuredClone(storage.get(key))])),
        ),
        set: vi.fn(async (values: Record<string, unknown>) => {
          for (const [key, value] of Object.entries(values)) {
            storage.set(key, structuredClone(value));
          }
        }),
        remove: vi.fn(async (keys: string | string[]) => {
          for (const key of [keys].flat()) storage.delete(key);
        }),
      },
    },
  });
};

const capture = (): GameRecordCapture =>
  ({
    scope: "/|game-a|1",
    sessionId: "session-a",
    startedAt: 1,
    partialHistory: false,
    unmatchedCount: 0,
    assistant: {} as GameRecordCapture["assistant"],
    events: [],
    decisions: [],
  }) as GameRecordCapture;

describe("GameRecordRecorder", () => {
  it("exposes the live record before it is persisted", async () => {
    installStorage();
    const recorder = new GameRecordRecorder();
    recorder.capture(capture());
    await vi.waitFor(() => expect(recorder.current()?.scope).toBe("/|game-a|1"));
    await recorder.flush();
  });

  it("does not resurrect a pre-reset capture after reset", async () => {
    installStorage();
    const recorder = new GameRecordRecorder();
    recorder.capture(capture());
    await recorder.reset();
    await recorder.flush();
    expect(recorder.current()).toBeUndefined();
    expect([...storage.keys()]).toEqual([]);
    expect(recorder.lastPersistError()).toBeUndefined();
  });
});
