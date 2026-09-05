// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";

import { InvestigationRecorder } from "../src/content/investigation-recorder";

const storage = new Map<string, unknown>();

afterEach(() => {
  storage.clear();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const installStorage = (): void => {
  vi.stubGlobal("chrome", {
    storage: {
      local: {
        get: vi.fn(async (key: string) => ({ [key]: structuredClone(storage.get(key)) })),
        set: vi.fn(async (values: Record<string, unknown>) => {
          for (const [key, value] of Object.entries(values)) {
            storage.set(key, structuredClone(value));
          }
        }),
        remove: vi.fn(async (key: string) => {
          storage.delete(key);
        }),
      },
    },
  });
};

describe("InvestigationRecorder", () => {
  it("persists a bounded game log across extension reloads", async () => {
    installStorage();
    const first = new InvestigationRecorder(4);
    await first.initialize(true, "game-a");
    first.record("board", { gameplayRollCount: 1 });
    first.record("dom", { logIndex: 12 });
    await first.flush();

    const restored = new InvestigationRecorder(4);
    await restored.initialize(true, "game-a");

    expect(restored.snapshot()).toMatchObject({
      gameKey: "game-a",
      entries: [
        { kind: "board", data: { gameplayRollCount: 1 } },
        { kind: "dom", data: { logIndex: 12 } },
      ],
    });
  });

  it("records only while enabled and starts a fresh stream for a new game", async () => {
    installStorage();
    const recorder = new InvestigationRecorder(4);
    await recorder.initialize(false, "game-a");
    recorder.record("board", { gameplayRollCount: 1 });
    expect(recorder.snapshot().entries).toEqual([]);

    recorder.setEnabled(true);
    recorder.record("board", { gameplayRollCount: 2 });
    expect(recorder.snapshot().entries).toHaveLength(1);

    recorder.setGame("game-b");
    expect(recorder.snapshot()).toMatchObject({ gameKey: "game-b", entries: [] });
  });

});
