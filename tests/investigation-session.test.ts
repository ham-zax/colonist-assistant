// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { GameSession } from "../src/content/session";
import { investigationRecorder } from "../src/content/investigation-recorder";

const storage = new Map<string, unknown>();
const sessions: GameSession[] = [];

beforeEach(() => {
  storage.clear();
  vi.stubGlobal("chrome", {
    storage: {
      local: {
        get: vi.fn(async (keys: string | string[] | null) => {
          if (keys === null) return Object.fromEntries(storage);
          const requested = Array.isArray(keys) ? keys : [keys];
          return Object.fromEntries(
            requested.flatMap((key) => storage.has(key) ? [[key, storage.get(key)]] : []),
          );
        }),
        set: vi.fn(async (values: Record<string, unknown>) => {
          for (const [key, value] of Object.entries(values)) {
            storage.set(key, structuredClone(value));
          }
        }),
        remove: vi.fn(async (keys: string | string[]) => {
          for (const key of Array.isArray(keys) ? keys : [keys]) storage.delete(key);
        }),
      },
    },
  });
});

afterEach(async () => {
  for (const session of sessions.splice(0)) session.stop();
  investigationRecorder.setEnabled(false);
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  document.body.replaceChildren();
  vi.unstubAllGlobals();
});

describe("investigation session capture", () => {
  it("records a diceless roll render and its later dice-icon hydration as one causal sequence", async () => {
    const gameKey = "investigation-hydration-game";
    await investigationRecorder.initialize(true, gameKey);

    const root = document.createElement("div");
    const row = document.createElement("div");
    row.dataset.index = "83";
    row.append("Alice rolled");
    const left = document.createElement("img");
    const right = document.createElement("img");
    left.alt = "loading-die";
    right.alt = "loading-die";
    row.append(left, right);
    root.append(row);
    document.body.append(root);

    const session = new GameSession(root, vi.fn(), gameKey);
    sessions.push(session);
    await session.start();

    expect(investigationRecorder.snapshot().entries).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          kind: "dice",
          data: expect.objectContaining({
            action: "indexed-roll-awaiting-dice-hydration",
            logIndex: 83,
          }),
        }),
      ]),
    );

    left.alt = "dice_white3";
    right.alt = "dice_white5";

    await vi.waitFor(() => {
      const actions = investigationRecorder.snapshot().entries
        .filter((entry) => entry.kind === "dice")
        .map((entry) => entry.data.action);
      expect(actions).toContain("indexed-ambiguity-cleared-by-rerender");
      expect(actions).toContain("indexed-roll-appended");
    });

    const relevant = investigationRecorder.snapshot().entries.filter(
      (entry) => entry.kind === "dice" && entry.data.logIndex === 83,
    );
    expect(relevant.map((entry) => entry.data.action)).toEqual([
      "indexed-roll-awaiting-dice-hydration",
      "indexed-ambiguity-cleared-by-rerender",
      "indexed-roll-appended",
    ]);
  });
});
