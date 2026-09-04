// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { GameSession } from "../src/content/session";
import { parseBankShortageNotice } from "../src/core/parser";

const localStorage = new Map<string, unknown>();
let beforeNextSet: (() => Promise<void>) | undefined;

beforeEach(() => {
  localStorage.clear();
  beforeNextSet = undefined;
  vi.stubGlobal("chrome", {
    storage: {
      local: {
        get: vi.fn(
          async (
            keys: string | string[] | Record<string, unknown> | null,
          ) => {
            if (keys === null) return Object.fromEntries(localStorage);
            const requested = Array.isArray(keys)
              ? keys
              : typeof keys === "string"
                ? [keys]
                : Object.keys(keys);
            return Object.fromEntries(
              requested.flatMap((key) =>
                localStorage.has(key)
                  ? [[key, localStorage.get(key)]]
                  : [],
              ),
            );
          },
        ),
        set: vi.fn(async (values: Record<string, unknown>) => {
          const beforeSet = beforeNextSet;
          beforeNextSet = undefined;
          await beforeSet?.();
          for (const [key, value] of Object.entries(values)) {
            localStorage.set(key, value);
          }
        }),
        remove: vi.fn(async (keys: string | string[]) => {
          for (const key of Array.isArray(keys) ? keys : [keys]) {
            localStorage.delete(key);
          }
        }),
      },
    },
  });
});

afterEach(() => {
  document.body.replaceChildren();
  vi.unstubAllGlobals();
});

const message = (index: number, text: string): HTMLDivElement => {
  const element = document.createElement("div");
  element.dataset.index = String(index);
  element.textContent = text;
  return element;
};

const rollPlayers = (session: GameSession): string[] =>
  session.events.flatMap((event) =>
    event.type === "roll" ? [event.player] : [],
  );

describe("live log session scanning", () => {
  it("recognizes a Colonist bank-shortage notice without failing integrity", async () => {
    const text =
      "Not enough wool for all players. 2 are left, and 3 were needed.";
    expect(
      parseBankShortageNotice({
        visibleText: text,
        serialText: text,
        language: "en",
      }),
    ).toEqual({
      type: "bank-shortage",
      resource: "wool",
      bankCount: 2,
      requiredCount: 3,
    });

    const root = document.createElement("div");
    root.append(message(0, text));
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "bank-shortage-game");

    await session.start();

    expect(session.events).toEqual([]);
    expect(session.unmatchedCount).toBe(1);
    expect(session.unmatchedIntegrityCount).toBe(0);
    expect(session.unmatchedSamples).toEqual([
      expect.objectContaining({
        reason: "known-bank-shortage-notice",
        affectsIntegrity: false,
        sample: text,
      }),
    ]);
    session.stop();
  });

  it("ingests a new message when a virtualized element is reused", async () => {
    const root = document.createElement("div");
    const entry = message(0, "Alice rolled");
    root.append(entry);
    document.body.append(root);
    const updated = vi.fn();
    const session = new GameSession(root, updated, "game-1");

    await session.start();
    expect(rollPlayers(session)).toEqual(["Alice"]);

    entry.dataset.index = "1";
    entry.textContent = "Bob rolled";

    await vi.waitFor(() => {
      expect(rollPlayers(session)).toEqual(["Alice", "Bob"]);
    });
    session.stop();
  });

  it("keeps the mounted old log suppressed but accepts reused nodes after a game-key reset", async () => {
    const root = document.createElement("div");
    const entry = message(0, "Alice rolled");
    root.append(entry);
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "game-1");

    await session.start();
    expect(session.events).toHaveLength(1);
    await vi.waitFor(() => {
      expect(
        localStorage.get(`colonistAssistantSession:${session.id}`),
      ).toMatchObject({ gameKey: "game-1" });
    });
    localStorage.set("colonistAssistantSession:older-game", {
      gameKey: "older-game",
    });

    session.setGameKey("game-2");
    expect(session.events).toHaveLength(0);
    await vi.waitFor(() => {
      expect(
        localStorage.get(`colonistAssistantSession:${session.id}`),
      ).toMatchObject({ gameKey: "game-2" });
      expect(
        [...localStorage.keys()].filter((key) =>
          key.startsWith("colonistAssistantSession:"),
        ),
      ).toEqual([`colonistAssistantSession:${session.id}`]);
    });

    // An unrelated mutation of the still-mounted previous log must not ingest
    // it into the new game.
    entry.style.padding = "1px";
    await new Promise((resolve) => window.setTimeout(resolve, 0));
    expect(session.events).toHaveLength(0);

    entry.dataset.index = "1";
    entry.textContent = "Carol rolled";

    await vi.waitFor(() => {
      expect(rollPlayers(session)).toEqual(["Carol"]);
    });
    session.stop();
  });

  it("deletes persisted game evidence without re-ingesting the mounted log", async () => {
    const root = document.createElement("div");
    const entry = message(0, "Alice rolled");
    root.append(entry);
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "game-private");

    await session.start();
    await vi.waitFor(() => {
      expect(
        localStorage.get("colonistAssistantLatestSummary"),
      ).toBeDefined();
    });
    localStorage.set("colonist-assistant-decision-traces-v1", [
      { replayBoard: { myPlayer: "Alice" } },
    ]);
    localStorage.set("colonistAssistantSession:orphaned-game-1", {
      events: ["private"],
    });
    localStorage.set("colonistAssistantSession:orphaned-game-2", {
      events: ["private"],
    });
    localStorage.set("colonistAssistantOverlayPosition", { x: 12, y: 24 });

    await session.clearStoredData();

    expect(session.events).toEqual([]);
    expect(
      [...localStorage.keys()].filter(
        (key) =>
          key.startsWith("colonistAssistantSession:") ||
          key === "colonistAssistantActiveSession" ||
          key === "colonistAssistantLatestSummary" ||
          key === "colonist-assistant-decision-traces-v1",
      ),
    ).toEqual([]);
    expect(localStorage.get("colonistAssistantOverlayPosition")).toEqual({
      x: 12,
      y: 24,
    });

    entry.style.padding = "1px";
    await new Promise((resolve) => window.setTimeout(resolve, 0));
    expect(session.events).toEqual([]);
    expect(localStorage.has("colonistAssistantLatestSummary")).toBe(false);

    entry.dataset.index = "1";
    entry.textContent = "Bob rolled";
    await vi.waitFor(() => {
      expect(rollPlayers(session)).toEqual(["Bob"]);
    });
    session.stop();
  });

  it("prunes older session records when a newly active session first saves", async () => {
    localStorage.set("colonistAssistantSession:previous-game-1", {
      events: ["private"],
    });
    localStorage.set("colonistAssistantSession:previous-game-2", {
      events: ["private"],
    });
    const root = document.createElement("div");
    root.append(message(0, "Alice rolled"));
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "current-game");

    await session.start();

    await vi.waitFor(() => {
      expect(
        [...localStorage.keys()].filter((key) =>
          key.startsWith("colonistAssistantSession:"),
        ),
      ).toEqual([`colonistAssistantSession:${session.id}`]);
      expect(
        localStorage.get(`colonistAssistantSession:${session.id}`),
      ).toMatchObject({ gameKey: "current-game" });
    });
    session.stop();
  });

  it("does not let an in-flight save restore session evidence after reset", async () => {
    const root = document.createElement("div");
    const entry = message(0, "Alice rolled");
    root.append(entry);
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "game-private");

    await session.start();
    await vi.waitFor(() => {
      expect(
        localStorage.get("colonistAssistantLatestSummary"),
      ).toBeDefined();
    });

    let releaseSave: (() => void) | undefined;
    let markSaveStarted: (() => void) | undefined;
    const saveStarted = new Promise<void>((resolve) => {
      markSaveStarted = resolve;
    });
    beforeNextSet = () =>
      new Promise<void>((resolve) => {
        releaseSave = resolve;
        markSaveStarted?.();
      });

    entry.dataset.index = "1";
    entry.textContent = "Bob rolled";
    await saveStarted;

    const clearing = session.clearStoredData();
    releaseSave?.();
    await clearing;

    expect(
      [...localStorage.keys()].filter(
        (key) =>
          key.startsWith("colonistAssistantSession:") ||
          key === "colonistAssistantActiveSession" ||
          key === "colonistAssistantLatestSummary",
      ),
    ).toEqual([]);
    session.stop();
  });
});
