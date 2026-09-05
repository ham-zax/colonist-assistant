// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GameSession } from "../src/content/session";
import { buildLiveDecisionStochasticInput, noteMissingPublicRoll } from "../src/core/dice-history";

const storage = new Map<string, unknown>();
const sessions: GameSession[] = [];
const gameKey = "balanced-setup-regression";
beforeEach(() => {
  storage.clear();
  vi.stubGlobal("chrome", { storage: { local: {
    get: vi.fn(async (key: string | null) => key === null ? Object.fromEntries(storage) : { [key]: structuredClone(storage.get(key)) }),
    set: vi.fn(async (values: Record<string, unknown>) => { for (const [key, value] of Object.entries(values)) storage.set(key, structuredClone(value)); }),
    remove: vi.fn(async (keys: string | string[]) => { for (const key of typeof keys === "string" ? [keys] : keys) storage.delete(key); }),
  } } });
});
afterEach(async () => {
  for (const session of sessions.splice(0)) session.stop();
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  document.body.replaceChildren();
  vi.unstubAllGlobals();
});
const message = (index: number, text: string): HTMLDivElement => {
  const row = document.createElement("div");
  row.dataset.index = String(index);
  row.textContent = text;
  return row;
};
const roll = (index: number, actor: string, left: number, right: number): HTMLDivElement => {
  const row = message(index, `${actor} rolled`);
  for (const value of [left, right]) {
    const image = document.createElement("img");
    image.alt = `dice_white${value}`;
    row.append(image);
  }
  return row;
};
const sessionFor = (root: HTMLDivElement, setupKey: string | undefined = gameKey): GameSession => {
  document.body.append(root);
  const session = new GameSession(root, vi.fn(), gameKey);
  session.setInitialPlacement(true, setupKey);
  sessions.push(session);
  return session;
};
const construct = (session: GameSession) => buildLiveDecisionStochasticInput("balanced", session.diceHistory, ["Alice", "Bob"]);

describe("Balanced setup dice authority", () => {
  it("uses an empty reference controller during confirmed setup without inventing a log index", async () => {
    const session = sessionFor(document.createElement("div"));
    await session.start();
    expect(construct(session)).toMatchObject({ model: "mref-colonist-linked-2024-v1", rolls: [], provenance: "complete-from-first-gameplay-roll" });
    expect(session.diceHistory.coverage.ranges).toEqual([]);
  });

  it("ignores blank setup positions but continues to reject gaps after gameplay starts", async () => {
    const root = document.createElement("div");
    root.append(message(0, "Happy settling!"), message(2, "Alice placed a Settlement"), message(3, "Alice placed a Road"), message(5, "Bob placed a Settlement"), message(6, "Bob placed a Road"), message(12, "Bot is placing a road for Alice"), message(13, "Alice placed a Road"));
    const session = sessionFor(root);
    await session.start();
    expect(session.unmatchedIntegrityCount).toBe(0);
    expect(construct(session).rolls).toEqual([]);
    expect(session.diceHistory.coverage.ranges).toEqual([[0, 13]]);
    session.setInitialPlacement(false, gameKey);
    root.append(roll(14, "Alice", 3, 5));
    await vi.waitFor(() => expect(session.diceHistory.rolls).toHaveLength(1));
    expect(construct(session).rolls).toEqual([{ ordinal: 0, actor: 0, total: 8 }]);
    root.append(roll(16, "Bob", 4, 5));
    await vi.waitFor(() => expect(session.diceHistory.rolls).toHaveLength(2));
    expect(() => construct(session)).toThrow(/usable public reference-dice history/);
  });

  it("requires matching game identity before setup can establish the empty prefix", async () => {
    const session = sessionFor(document.createElement("div"), "other-game");
    await session.start();
    expect(() => construct(session)).toThrow(/usable public reference-dice history/);
  });

  it("does not make a late setup-log prefix permanently partial after index zero backfills", async () => {
    const root = document.createElement("div");
    root.append(message(2, "Alice placed a Settlement"));
    const session = sessionFor(root);
    await session.start();
    expect(session.partialHistory).toBe(false);

    root.prepend(
      message(0, "Happy settling!"),
      message(1, "Bot is placing a settlement for Alice"),
    );
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    session.setInitialPlacement(false, gameKey);

    expect(session.partialHistory).toBe(false);
  });

  it("marks setup history partial if the deferred log prefix never backfills before gameplay", async () => {
    const root = document.createElement("div");
    root.append(message(2, "Alice placed a Settlement"));
    const session = sessionFor(root);
    await session.start();
    expect(session.partialHistory).toBe(false);

    session.setInitialPlacement(false, gameKey);

    expect(session.partialHistory).toBe(true);
  });

  it("does not erase a roll-capable parser miss or observed conflict because setup is reported", async () => {
    const root = document.createElement("div");
    root.append(message(0, "Happy settling!"), message(1, "Alice cast the dice in an unknown format"));
    const session = sessionFor(root);
    await session.start();
    expect(() => construct(session)).toThrow(/usable public reference-dice history/);
    root.replaceChildren(roll(1, "Alice", 3, 5));
    await vi.waitFor(() => expect(session.diceHistory.rolls).toHaveLength(1));
    root.replaceChildren(roll(1, "Alice", 4, 5));
    await vi.waitFor(() => expect(session.diceHistory.ambiguousLogIndices).toEqual([1]));
    session.setInitialPlacement(true, gameKey);
    expect(() => construct(session)).toThrow(/usable public reference-dice history/);
  });

  it("does not erase a known missing gameplay roll under a stale setup snapshot", async () => {
    const session = sessionFor(document.createElement("div"));
    await session.start();
    noteMissingPublicRoll(session.diceHistory);
    expect(session.diceHistory.missingPrefixRolls).toBe(1);
    session.setInitialPlacement(true, gameKey);
    expect(session.diceHistory.missingPrefixRolls).toBe(1);
    expect(session.diceHistory.provenance).not.toBe("complete-from-first-gameplay-roll");
  });

  it("keeps Balanced authority from public board evidence when the game log is absent", async () => {
    const session = sessionFor(document.createElement("div"));
    await session.start();
    expect(construct(session)).toMatchObject({
      model: "mref-colonist-linked-2024-v1",
      rolls: [],
      provenance: "complete-from-first-gameplay-roll",
    });
    session.setInitialPlacement(false, gameKey);
    expect(session.observeBoardDiceSnapshot({
      gameKey,
      botOnlyGame: false,
      initialPlacement: false,
      hasRolled: true,
      lastRoll: 8,
      currentPlayer: "Alice",
      turn: 4,
      gameplayRollCount: 1,
    })).toBe(true);
    expect(session.observeBoardDiceSnapshot({
      gameKey,
      botOnlyGame: false,
      initialPlacement: false,
      hasRolled: true,
      lastRoll: 8,
      currentPlayer: "Alice",
      turn: 4,
      gameplayRollCount: 1,
    })).toBe(false);
    expect(construct(session)).toMatchObject({
      model: "mref-colonist-linked-2024-v1",
      rolls: [{ ordinal: 0, actor: 0, total: 8 }],
    });
  });

  it("uses bot board rolls immediately while a mounted virtualized log is still hydrating", async () => {
    const root = document.createElement("div");
    for (let index = 0; index <= 16; index += 1) {
      root.append(message(index, "Happy settling!"));
    }
    const session = sessionFor(root);
    await session.start();
    session.setInitialPlacement(false, gameKey);

    expect(session.observeBoardDiceSnapshot({
      gameKey,
      botOnlyGame: true,
      initialPlacement: false,
      hasRolled: true,
      lastRoll: 12,
      currentPlayer: "Alice",
      turn: 8,
      gameplayRollCount: 1,
    })).toBe(true);
    expect(buildLiveDecisionStochasticInput(
      "balanced",
      session.diceHistory,
      ["Alice", "Bob", "Cas", "Warp"],
      1,
    )).toMatchObject({
      provenance: "complete-from-first-gameplay-roll",
      rolls: [{ ordinal: 0, actor: 0, total: 12 }],
    });

    root.append(
      message(18, "Happy settling!"),
      message(20, "Happy settling!"),
      roll(30, "Alice", 6, 6),
    );
    await vi.waitFor(() =>
      expect(session.diceHistory.rolls.some((entry) => entry.logIndex === 30)).toBe(true));

    expect(buildLiveDecisionStochasticInput(
      "balanced",
      session.diceHistory,
      ["Alice", "Bob", "Cas", "Warp"],
      1,
    )).toMatchObject({
      provenance: "complete-from-first-gameplay-roll",
      rolls: [{ ordinal: 0, actor: 0, total: 12 }],
    });
  });

  it("keeps a known missed bot turn explicit in the Mref posterior", async () => {
    const session = sessionFor(document.createElement("div"));
    await session.start();
    session.setInitialPlacement(false, gameKey);
    session.observeBoardDiceSnapshot({
      gameKey, botOnlyGame: true, initialPlacement: false,
      hasRolled: true, lastRoll: 8, currentPlayer: "Alice", turn: 4,
      gameplayRollCount: 1,
    });
    session.observeBoardDiceSnapshot({
      gameKey, botOnlyGame: true, initialPlacement: false,
      hasRolled: true, lastRoll: 9, currentPlayer: "Alice", turn: 6,
      gameplayRollCount: 3,
    });
    expect(session.diceHistory.gaps).toEqual([{ afterOrdinal: 0, missingRolls: 1 }]);
    expect(construct(session)).toMatchObject({
      model: "mref-colonist-linked-2024-v1",
      rolls: [
        { ordinal: 0, actor: 0, total: 8 },
        { ordinal: 2, actor: 0, total: 9 },
      ],
      gaps: [{ afterOrdinal: 0, missingRolls: 1 }],
    });
  });

  it("does not duplicate a log-observed roll when the bot log unmounts midgame", async () => {
    const root = document.createElement("div");
    root.append(roll(0, "Alice", 3, 5));
    const session = sessionFor(root);
    await session.start();
    session.setInitialPlacement(false, gameKey);
    expect(session.diceHistory.rolls).toHaveLength(1);
    expect(session.observeBoardDiceSnapshot({
      gameKey, botOnlyGame: true, initialPlacement: false,
      hasRolled: true, lastRoll: 8, currentPlayer: "Alice", turn: 4,
      gameplayRollCount: 1,
    })).toBe(false);
    expect(session.diceHistory.rolls).toHaveLength(1);
    expect(construct(session).rolls).toEqual([{ ordinal: 0, actor: 0, total: 8 }]);
  });

  it.each([false, true])("reconciles a returning bot log by ordinal (conflict=%s)", async (conflict) => {
    const root = document.createElement("div");
    const session = sessionFor(root);
    await session.start();
    session.setInitialPlacement(false, gameKey);
    session.observeBoardDiceSnapshot({
      gameKey, botOnlyGame: true, hasRolled: true,
      lastRoll: 8, currentPlayer: "Alice", turn: 0,
    });
    session.observeBoardDiceSnapshot({
      gameKey, botOnlyGame: true, hasRolled: true,
      lastRoll: 9, currentPlayer: "Bob", turn: 1,
    });
    root.append(roll(0, "Alice", conflict ? 4 : 3, 5), roll(1, "Bob", 4, 5));
    await vi.waitFor(() => expect(session.events).toHaveLength(2));
    if (conflict) {
      expect(() => construct(session)).toThrow();
    } else {
      expect(session.diceHistory.rolls).toHaveLength(2);
      expect(construct(session).rolls).toEqual([
        { ordinal: 0, actor: 0, total: 8 }, { ordinal: 1, actor: 1, total: 9 },
      ]);
    }
  });

  it("preserves missed turns when a complete log hands off to board capture", async () => {
    const root = document.createElement("div");
    root.append(roll(0, "Alice", 3, 5));
    const session = sessionFor(root);
    await session.start();
    session.setInitialPlacement(false, gameKey);
    session.observeBoardDiceSnapshot({
      gameKey, botOnlyGame: true, hasRolled: true,
      lastRoll: 9, currentPlayer: "Alice", turn: 2,
    });
    expect(session.diceHistory.gaps).toEqual([{ afterOrdinal: 0, missingRolls: 1 }]);
    expect(construct(session).rolls?.map((event) => event.ordinal)).toEqual([0, 2]);
  });

  it("rejects a different actor for an already observed board turn after a gap", async () => {
    const session = sessionFor(document.createElement("div"));
    await session.start();
    session.setInitialPlacement(false, gameKey);
    for (const [turn, currentPlayer] of [[0, "Alice"], [2, "Alice"], [2, "Bob"]] as const) {
      session.observeBoardDiceSnapshot({
        gameKey, botOnlyGame: true, hasRolled: true, lastRoll: 8, currentPlayer, turn,
      });
    }
    expect(session.diceHistory.rolls).toHaveLength(2);
    expect(() => construct(session)).toThrow();
  });

  it("recovers the persisted harmless bot-placement miss captured in the live setup", async () => {
    const session = sessionFor(document.createElement("div"));
    storage.set(`colonistAssistantSession:${session.id}`, {
      schema: 4, id: session.id, gameKey,
      page: `${location.origin}${location.pathname}${location.search}`,
      startedAt: 1, updatedAt: 2, events: [], seenIds: [], partialHistory: false,
      unmatchedCount: 1, unmatchedIntegrityCount: 1,
      unmatchedSamples: [{ signature: "bot-placement", count: 1, firstSeenAt: 1, lastSeenAt: 1, firstLogIndex: 12, lastLogIndex: 12, reason: "unrecognized-log-format", affectsIntegrity: true, sample: "Bot is placing a road for Alice" }],
      diceHistory: { integrityVersion: 1, rolls: [], coverage: { ranges: [[0, 0], [2, 3], [5, 6], [13, 13]] }, ambiguousLogIndices: [12], hasUnlocatedRollAmbiguity: false, gaps: [], hasUnknownRollGap: true, provenance: "gapped" },
    });
    await session.start();
    expect(session.unmatchedIntegrityCount).toBe(0);
    expect(session.diceHistory.ambiguousLogIndices).toEqual([]);
    expect(construct(session)).toMatchObject({ model: "mref-colonist-linked-2024-v1", rolls: [], provenance: "complete-from-first-gameplay-roll" });
    await vi.waitFor(() => expect(storage.get(`colonistAssistantSession:${session.id}`)).toMatchObject({ diceHistory: { missingPrefixRolls: 0, hasUnknownRollGap: false } }));
  });
});
