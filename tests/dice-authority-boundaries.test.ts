// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GameSession } from "../src/content/session";
import { buildLiveDecisionStochasticInput } from "../src/core/dice-history";

const storage = new Map<string, unknown>();
const sessions: GameSession[] = [];

beforeEach(() => {
  storage.clear();
  vi.stubGlobal("chrome", {
    storage: {
      local: {
        get: vi.fn(async (key: string | null) => key === null
          ? Object.fromEntries(storage)
          : { [key]: storage.get(key) }),
        set: vi.fn(async (values: Record<string, unknown>) => {
          for (const [key, value] of Object.entries(values)) {
            storage.set(key, structuredClone(value));
          }
        }),
        remove: vi.fn(async (keys: string | string[]) => {
          for (const key of typeof keys === "string" ? [keys] : keys) storage.delete(key);
        }),
      },
    },
  });
});

afterEach(async () => {
  for (const session of sessions.splice(0)) session.stop();
  // Drain queued storage promises before removing the Chrome test boundary.
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  document.body.replaceChildren();
  vi.unstubAllGlobals();
});

const message = (index: number | undefined, text: string): HTMLDivElement => {
  const element = document.createElement("div");
  if (index !== undefined) element.dataset.index = String(index);
  element.textContent = text;
  return element;
};

const roll = (index: number, actor: string, left: number, right: number): HTMLDivElement => {
  const element = message(index, `${actor} rolled`);
  for (const value of [left, right]) {
    const image = document.createElement("img");
    image.alt = `dice_white${value}`;
    element.append(image);
  }
  return element;
};

const createSession = (root: HTMLDivElement, update = vi.fn()): GameSession => {
  document.body.append(root);
  const session = new GameSession(root, update, "r6-authority-boundary-game");
  sessions.push(session);
  return session;
};

const construct = (session: GameSession) =>
  buildLiveDecisionStochasticInput("balanced", session.diceHistory, ["Alice", "Bob"]);

const seedLegacy = (session: GameSession, schema: 3 | 4) => ({
  schema,
  id: session.id,
  gameKey: session.gameKey,
  page: `${location.origin}${location.pathname}${location.search}`,
  startedAt: 1,
  updatedAt: 2,
  events: [{
    type: "roll", player: "Alice", dice: [3, 5],
    id: "legacy-roll-0", index: 0, timestamp: 2,
    raw: "Alice rolled :die-3: :die-5:",
  }],
  seenIds: ["legacy-roll-0"],
  partialHistory: false,
  ...(schema === 4 ? {
    diceHistory: {
      rolls: [{ actor: "Alice", total: 8, dice: [3, 5], eventId: "legacy-roll-0", logIndex: 0 }],
      coverage: { ranges: [[0, 1]] },
      provenance: "complete-from-first-gameplay-roll",
      hasUnknownRollGap: false,
      gaps: [],
    },
  } : {}),
  unmatchedCount: 1,
  unmatchedIntegrityCount: 1,
  unmatchedSamples: [{
    signature: "legacy-miss", count: 1, firstSeenAt: 2, lastSeenAt: 2,
    firstLogIndex: 1, lastLogIndex: 1,
    reason: "unrecognized-log-format", affectsIntegrity: true,
    sample: "Alice cast the dice in an unknown format",
  }],
});

describe("R6 stochastic authority lifecycle boundaries", () => {
  it.each([false, true])("retains log-zero history across rerenders (conflict=%s)", async (conflict) => {
    const root = document.createElement("div");
    const first = roll(0, "Alice", 3, 5);
    root.append(first, roll(1, "Bob", 6, 1));
    const update = vi.fn();
    const session = createSession(root, update);
    await session.start();
    update.mockClear();
    const replacement = roll(0, "Alice", conflict ? 4 : 3, 5);
    replacement.firstChild!.nodeValue = "Alice rolled the dice";
    first.replaceWith(replacement);
    if (conflict) {
      await vi.waitFor(() => expect(session.diceHistory.ambiguousLogIndices).toEqual([0]));
      expect(update).toHaveBeenCalled();
    } else {
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
      expect(update).not.toHaveBeenCalled();
    }

    expect(session.diceHistory.rolls.map((item) => item.total)).toEqual([8, 7]);
    if (conflict) {
      expect(session.diceHistory.ambiguousLogIndices).toEqual([0]);
      expect(() => construct(session)).toThrow(/usable public reference-dice history/);
    } else {
      expect(construct(session).rolls).toEqual([
        { ordinal: 0, actor: 0, total: 8 },
        { ordinal: 1, actor: 1, total: 7 },
      ]);
    }
  });

  it("does not clear a conflict when the informational log-zero row rerenders", async () => {
    const root = document.createElement("div");
    const header = message(0, "Happy settling!");
    const first = roll(1, "Alice", 3, 5);
    root.append(header, first);
    const update = vi.fn();
    const session = createSession(root, update);
    await session.start();
    first.replaceWith(roll(1, "Alice", 4, 5));
    await vi.waitFor(() => expect(session.diceHistory.ambiguousLogIndices).toEqual([1]));
    update.mockClear();
    header.replaceWith(message(0, "Happy settling! Enjoy the game."));
    await vi.waitFor(() => expect(update).toHaveBeenCalled());
    expect(session.diceHistory.rolls.map((item) => item.total)).toEqual([8]);
    expect(() => construct(session)).toThrow(/usable public reference-dice history/);
  });

  it.each(["parent", "first-repair", "second-repair"])("restores legacy schema-4 uncertainty written by %s", async (writer) => {
    const session = createSession(document.createElement("div"));
    const record = seedLegacy(session, 4);
    storage.set(`colonistAssistantSession:${session.id}`, {
      ...record,
      diceHistory: {
        ...record.diceHistory,
        ...(writer !== "parent" ? { ambiguousLogIndices: [] } : {}),
        ...(writer === "second-repair" ? { hasUnlocatedRollAmbiguity: false } : {}),
      },
    });
    await session.start();
    expect(session.diceHistory.ambiguousLogIndices).toEqual([1]);
    expect(() => construct(session)).toThrow(/usable public reference-dice history/);
  });

  it.each([
    { integrityVersion: undefined, player: "Bob", dice: [3, 5] },
    { integrityVersion: undefined, player: "Alice", dice: [4, 5] },
    { integrityVersion: 1, player: "Bob", dice: [3, 5] },
    { integrityVersion: 1, player: "Alice", dice: [4, 5] },
    { integrityVersion: 1, player: "Alice", dice: [2, 6] },
  ])("reconciles retained conflicting rolls even with integrityVersion=$integrityVersion, $player $dice", async (conflict) => {
    const session = createSession(document.createElement("div"));
    const record = seedLegacy(session, 4);
    const key = `colonistAssistantSession:${session.id}`;
    storage.set(key, {
      ...record,
      events: [...record.events, {
        ...record.events[0], id: "conflicting-render", player: conflict.player, dice: conflict.dice,
      }],
      unmatchedCount: 0, unmatchedIntegrityCount: 0, unmatchedSamples: [],
      diceHistory: {
        ...record.diceHistory, integrityVersion: conflict.integrityVersion,
        ambiguousLogIndices: [], hasUnlocatedRollAmbiguity: false,
      },
    });
    await session.start();
    expect(session.diceHistory.rolls).toHaveLength(1);
    expect(session.diceHistory.ambiguousLogIndices).toEqual([0]);
    expect(() => construct(session)).toThrow(/usable public reference-dice history/);
    await vi.waitFor(() => expect(storage.get(key)).toMatchObject({
      diceHistory: { ambiguousLogIndices: [0] },
    }));
    session.stop();
    const restored = createSession(document.createElement("div"));
    await restored.start();
    expect(() => construct(restored)).toThrow(/usable public reference-dice history/);
  });

  it("compares a retained roll against the independent ledger without rebuilding a truncated history", async () => {
    const session = createSession(document.createElement("div"));
    const record = seedLegacy(session, 4);
    storage.set(`colonistAssistantSession:${session.id}`, {
      ...record,
      events: [{ ...record.events[0], id: "only-retained-render", dice: [4, 5] }],
      partialHistory: true,
      unmatchedCount: 0, unmatchedIntegrityCount: 0, unmatchedSamples: [],
      diceHistory: {
        ...record.diceHistory, integrityVersion: 1,
        ambiguousLogIndices: [], hasUnlocatedRollAmbiguity: false,
      },
    });
    await session.start();
    expect(session.diceHistory.rolls.map((item) => item.total)).toEqual([8]);
    expect(session.diceHistory.ambiguousLogIndices).toEqual([0]);
    expect(() => construct(session)).toThrow(/usable public reference-dice history/);
  });

  it("keeps matching historical rerenders and truncated generic history compatible with the ledger", async () => {
    const session = createSession(document.createElement("div"));
    const record = seedLegacy(session, 4);
    storage.set(`colonistAssistantSession:${session.id}`, {
      ...record,
      events: [record.events[0], { ...record.events[0], id: "matching-render" }],
      partialHistory: true,
      unmatchedCount: 0, unmatchedIntegrityCount: 0, unmatchedSamples: [],
      diceHistory: {
        ...record.diceHistory, integrityVersion: 1,
        ambiguousLogIndices: [], hasUnlocatedRollAmbiguity: false,
        rolls: [...record.diceHistory!.rolls, {
          actor: "Bob", total: 7, dice: [3, 4], eventId: "ledger-only", logIndex: 1,
        }],
      },
    });
    await session.start();
    expect(session.diceHistory.rolls.map((item) => item.total)).toEqual([8, 7]);
    expect(construct(session).model).toBe("mref-colonist-linked-2024-v1");
  });

  it.each(["scan", "reload"])("keeps a partial schema-3 prefix uncertain after %s", async (continuation) => {
    const root = document.createElement("div");
    const session = createSession(root);
    const key = `colonistAssistantSession:${session.id}`;
    storage.set(key, {
      ...seedLegacy(session, 3),
      partialHistory: true,
      unmatchedCount: 0,
      unmatchedIntegrityCount: 0,
      unmatchedSamples: [],
    });
    await session.start();
    expect(() => construct(session)).toThrow(/usable public reference-dice history/);
    if (continuation === "scan") {
      root.append(roll(1, "Bob", 6, 1));
      await vi.waitFor(() => expect(session.diceHistory.rolls).toHaveLength(2));
      expect(() => construct(session)).toThrow(/usable public reference-dice history/);
    } else {
      await vi.waitFor(() => expect(storage.get(key)).toMatchObject({ schema: 4 }));
      session.stop();
      const restored = createSession(document.createElement("div"));
      await restored.start();
      expect(() => construct(restored)).toThrow(/usable public reference-dice history/);
    }
  });

  it.each([false, true])("distinguishes validated dice authority from old partial-history resaves (validated=%s)", async (validated) => {
    const session = createSession(document.createElement("div"));
    const record = seedLegacy(session, 4);
    storage.set(`colonistAssistantSession:${session.id}`, {
      ...record,
      partialHistory: true,
      unmatchedCount: 0,
      unmatchedIntegrityCount: 0,
      unmatchedSamples: [],
      diceHistory: {
        ...record.diceHistory,
        ambiguousLogIndices: [],
        hasUnlocatedRollAmbiguity: false,
        ...(validated ? { integrityVersion: 1 } : {}),
      },
    });
    await session.start();
    if (validated) {
      // Generic event truncation does not weaken independently established
      // dice completeness from a current, validated writer.
      expect(construct(session).model).toBe("mref-colonist-linked-2024-v1");
    } else {
      expect(() => construct(session)).toThrow(/usable public reference-dice history/);
    }
  });

  it("does not assume missing legacy unmatched diagnostics were harmless", async () => {
    const session = createSession(document.createElement("div"));
    const record = seedLegacy(session, 3);
    const { unmatchedIntegrityCount: _count, unmatchedSamples: _samples, ...oldRecord } = record;
    storage.set(`colonistAssistantSession:${session.id}`, oldRecord);
    await session.start();
    expect(() => construct(session)).toThrow(/usable public reference-dice history/);
  });

  it.each([false, true])("handles trailing unindexed messages without losing uncertainty (roll-capable=%s)", async (rollCapable) => {
    const root = document.createElement("div");
    root.append(roll(0, "Alice", 3, 5));
    const session = createSession(root);
    await session.start();
    const miss = message(undefined, rollCapable
      ? "Alice cast the dice in an unknown format"
      : "Happy settling!");
    // Exercise the existing unindexed-log fallback after the indexed prefix
    // has left the virtualized DOM.
    root.replaceChildren(miss);
    await vi.waitFor(() => expect(session.unmatchedCount).toBe(1));
    if (rollCapable) {
      expect(() => construct(session)).toThrow(/usable public reference-dice history/);
    } else {
      expect(construct(session).model).toBe("mref-colonist-linked-2024-v1");
    }
  });
});
