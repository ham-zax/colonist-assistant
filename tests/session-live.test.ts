// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { GameSession } from "../src/content/session";
import { buildLiveDecisionStochasticInput } from "../src/core/dice-history";
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

afterEach(async () => {
  // stop() queues the final save; retain the Chrome mock until that queue drains.
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  document.body.replaceChildren();
  vi.unstubAllGlobals();
});

const message = (index: number, text: string): HTMLDivElement => {
  const element = document.createElement("div");
  element.dataset.index = String(index);
  element.textContent = text;
  return element;
};

const diceMessage = (
  index: number,
  player: string,
  left: number,
  right: number,
): HTMLDivElement => {
  const element = message(index, `${player} rolled`);
  for (const die of [left, right]) {
    const image = document.createElement("img");
    image.alt = `dice_white${die}`;
    element.append(image);
  }
  return element;
};

const rollPlayers = (session: GameSession): string[] =>
  session.events.flatMap((event) =>
    event.type === "roll" ? [event.player] : [],
  );

describe("live log session scanning", () => {
  it("owns ordered public dice history and complete indexed provenance", async () => {
    const root = document.createElement("div");
    root.append(diceMessage(0, "Alice", 3, 5), diceMessage(1, "Bob", 6, 1));
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "dice-history-game");

    await session.start();

    expect(session.diceHistory.provenance).toBe("complete-from-first-gameplay-roll");
    expect(session.diceHistory.rolls).toEqual([
      expect.objectContaining({ actor: "Alice", total: 8, dice: [3, 5], logIndex: 0 }),
      expect.objectContaining({ actor: "Bob", total: 7, dice: [6, 1], logIndex: 1 }),
    ]);
    session.stop();
  });

  it.each([0, 1])("keeps one stochastic roll when log index %i rerenders with a new presentation fingerprint", async (logIndex) => {
    const root = document.createElement("div");
    const entry = diceMessage(logIndex, "Alice", 3, 5);
    if (logIndex > 0) root.append(message(0, "Happy settling!"));
    root.append(entry);
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "dice-rerender-game");

    await session.start();
    const firstEventId = session.diceHistory.rolls[0]?.eventId;
    expect(session.diceHistory.rolls).toHaveLength(1);

    const rerendered = diceMessage(logIndex, "Alice", 3, 5);
    rerendered.firstChild!.nodeValue = "Alice rolled the dice";
    entry.replaceWith(rerendered);
    await vi.waitFor(() => {
      expect(session.events.filter((event) => event.type === "roll")).toHaveLength(1);
    });

    expect(session.state.currentTurn.sequence).toBe(1);
    expect(session.diceHistory.rolls).toHaveLength(1);
    expect(session.diceHistory.rolls[0]).toMatchObject({
      eventId: firstEventId,
      actor: "Alice",
      total: 8,
      logIndex,
    });
    expect(
      buildLiveDecisionStochasticInput("balanced", session.diceHistory, ["Alice"]),
    ).toMatchObject({ model: "mref-colonist-linked-2024-v1" });
    session.stop();
  });

  it("resolves an indexed roll when Colonist hydrates die attributes in place", async () => {
    const root = document.createElement("div");
    const entry = message(0, "Alice rolled");
    const left = document.createElement("img");
    const right = document.createElement("img");
    left.alt = "loading-die";
    right.alt = "loading-die";
    entry.append(left, right);
    root.append(entry);
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "dice-attribute-hydration-game");

    await session.start();
    expect(session.diceHistory.rolls).toEqual([]);
    expect(session.diceHistory.ambiguousLogIndices).toEqual([0]);
    expect(session.events).toEqual([
      expect.objectContaining({ type: "roll", player: "Alice", index: 0 }),
    ]);

    left.alt = "dice_white3";
    right.alt = "dice_white5";
    await vi.waitFor(() => {
      expect(session.diceHistory.rolls).toEqual([
        expect.objectContaining({ actor: "Alice", total: 8, dice: [3, 5], logIndex: 0 }),
      ]);
    });

    expect(session.diceHistory.ambiguousLogIndices).toEqual([]);
    expect(session.diceHistory.provenance).toBe("complete-from-first-gameplay-roll");
    expect(session.events).toEqual([
      expect.objectContaining({ type: "roll", player: "Alice", dice: [3, 5], index: 0 }),
    ]);
    expect(session.state.currentTurn.sequence).toBe(1);
    session.stop();
  });

  it("replays a late virtualized log backfill in authoritative index order", async () => {
    const root = document.createElement("div");
    root.append(diceMessage(0, "Alice", 3, 5), diceMessage(2, "Bob", 6, 1));
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "late-index-backfill-game");

    await session.start();
    expect(session.state.currentTurn).toMatchObject({ player: "Bob", sequence: 2 });

    root.append(diceMessage(1, "Carol", 2, 2));
    await vi.waitFor(() => {
      expect(session.events.flatMap((event) => event.index ?? [])).toEqual([0, 1, 2]);
    });

    expect(rollPlayers(session)).toEqual(["Alice", "Carol", "Bob"]);
    expect(session.state.currentTurn).toMatchObject({ player: "Bob", sequence: 3 });
    session.stop();
  });

  it("preserves a non-roll indexed conflict through rerender and persistence", async () => {
    const root = document.createElement("div");
    const entry = message(0, "Alice placed a Road");
    root.append(entry);
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "non-roll-slot-conflict");
    await session.start();
    entry.textContent = "Bob placed a Road";
    await vi.waitFor(() => expect(session.diceHistory.conflictingLogIndices).toEqual([0]));
    entry.textContent = "Happy settling!";
    await vi.waitFor(() => expect(session.unmatchedCount).toBeGreaterThan(1));
    expect(() => buildLiveDecisionStochasticInput("balanced", session.diceHistory, ["Alice"])).toThrow();
    session.stop();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    const restored = new GameSession(root, vi.fn(), "non-roll-slot-conflict");
    await restored.start();
    expect(restored.diceHistory.conflictingLogIndices).toEqual([0]);
    expect(() => buildLiveDecisionStochasticInput("balanced", restored.diceHistory, ["Alice"])).toThrow();
    restored.stop();
  });

  it("keeps public awards and rendered counteroffers out of dice uncertainty", async () => {
    const longestRoad = "hamzax received Longest Road :road: (+2 VPs)";
    const counter =
      "Grandetorino proposed counter offer to NJDgaming, offering :wool: for :grain:";
    const root = document.createElement("div");
    root.append(
      diceMessage(0, "Alice", 3, 5),
      message(1, longestRoad),
      message(2, counter),
      diceMessage(3, "Bob", 6, 1),
    );
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "harmless-public-log-game");

    await session.start();

    expect(session.unmatchedIntegrityCount).toBe(0);
    expect(session.unmatchedSamples).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          reason: "known-redundant-award",
          affectsIntegrity: false,
          sample: longestRoad,
        }),
        expect.objectContaining({
          reason: "known-redundant-trade-offer",
          affectsIntegrity: false,
          sample: counter,
        }),
      ]),
    );
    expect(session.diceHistory.ambiguousLogIndices).toEqual([]);
    expect(session.diceHistory.provenance).toBe("complete-from-first-gameplay-roll");
    expect(() =>
      buildLiveDecisionStochasticInput("balanced", session.diceHistory, ["Alice", "Bob"]),
    ).not.toThrow();
    session.stop();
  });

  it.each([0, 1])("keeps contradictory dice at log index %i out of Mref authority across later matching rerenders", async (logIndex) => {
    const root = document.createElement("div");
    const entry = diceMessage(logIndex, "Alice", 3, 5);
    if (logIndex > 0) root.append(message(0, "Happy settling!"));
    root.append(entry);
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "dice-conflict-rerender-game");

    await session.start();
    expect(
      buildLiveDecisionStochasticInput("balanced", session.diceHistory, ["Alice"]),
    ).toMatchObject({ model: "mref-colonist-linked-2024-v1" });

    const conflicting = diceMessage(logIndex, "Alice", 4, 5);
    conflicting.firstChild!.nodeValue = "Alice rolled differently";
    entry.replaceWith(conflicting);
    await vi.waitFor(() => {
      expect(session.diceHistory.ambiguousLogIndices).toEqual([logIndex]);
    });

    expect(session.diceHistory.rolls).toHaveLength(1);
    expect(session.diceHistory.rolls[0]).toMatchObject({
      actor: "Alice",
      total: 8,
      dice: [3, 5],
      logIndex,
    });
    expect(session.diceHistory.provenance).toBe("gapped");
    expect(session.diceHistory.hasUnknownRollGap).toBe(true);
    expect(() =>
      buildLiveDecisionStochasticInput("balanced", session.diceHistory, ["Alice"]),
    ).toThrow(/usable public reference-dice history/);
    await vi.waitFor(() => {
      expect(localStorage.get(`colonistAssistantSession:${session.id}`)).toMatchObject({
        diceHistory: {
          ambiguousLogIndices: [logIndex],
          provenance: "gapped",
        },
      });
    });

    const matching = diceMessage(logIndex, "Alice", 3, 5);
    matching.firstChild!.nodeValue = "Alice rolled again";
    conflicting.replaceWith(matching);
    await vi.waitFor(() => {
      expect(session.events.filter((event) => event.type === "roll")).toHaveLength(1);
    });

    expect(session.diceHistory.rolls).toHaveLength(1);
    expect(session.diceHistory.ambiguousLogIndices).toEqual([logIndex]);
    expect(session.diceHistory.provenance).toBe("gapped");
    expect(() =>
      buildLiveDecisionStochasticInput("balanced", session.diceHistory, ["Alice"]),
    ).toThrow(/usable public reference-dice history/);
    session.stop();
  });

  it.each([false, true])("preserves log-zero dice authority across a rerender (conflict=%s)", async (conflict) => {
    const root = document.createElement("div");
    const first = diceMessage(0, "Alice", 3, 5);
    root.append(first, diceMessage(1, "Bob", 6, 1));
    document.body.append(root);
    const onUpdate = vi.fn();
    const session = new GameSession(root, onUpdate, "dice-log-zero-game");
    await session.start();
    try {
      onUpdate.mockClear();
      const rerendered = diceMessage(0, "Alice", conflict ? 4 : 3, 5);
      rerendered.firstChild!.nodeValue = "Alice rolled the dice";
      first.replaceWith(rerendered);
      if (conflict) {
        await vi.waitFor(() => expect(session.diceHistory.ambiguousLogIndices).toEqual([0]));
        expect(onUpdate).toHaveBeenCalled();
      } else {
        await new Promise<void>((resolve) => setTimeout(resolve, 0));
        expect(onUpdate).not.toHaveBeenCalled();
      }

      expect(session.diceHistory.rolls.map((roll) => roll.total)).toEqual([8, 7]);
      const construct = () => buildLiveDecisionStochasticInput(
        "balanced", session.diceHistory, ["Alice", "Bob"],
      );
      if (conflict) {
        expect(session.diceHistory.ambiguousLogIndices).toEqual([0]);
        expect(construct).toThrow(/usable public reference-dice history/);
      } else {
        expect(session.diceHistory.ambiguousLogIndices).toEqual([]);
        expect(construct()).toMatchObject({ model: "mref-colonist-linked-2024-v1" });
      }
    } finally {
      session.stop();
    }
  });

  it("does not let a harmless log-zero rerender erase an existing dice conflict", async () => {
    const root = document.createElement("div");
    const header = message(0, "Happy settling!");
    const roll = diceMessage(1, "Alice", 3, 5);
    root.append(header, roll);
    document.body.append(root);
    const onUpdate = vi.fn();
    const session = new GameSession(root, onUpdate, "dice-conflict-header-game");
    await session.start();
    try {
      roll.replaceWith(diceMessage(1, "Alice", 4, 5));
      await vi.waitFor(() => expect(session.diceHistory.ambiguousLogIndices).toEqual([1]));
      onUpdate.mockClear();
      header.replaceWith(message(0, "Happy settling! Enjoy the game."));
      await vi.waitFor(() => expect(onUpdate).toHaveBeenCalled());
      expect(session.diceHistory.rolls.map((entry) => entry.total)).toEqual([8]);
      expect(session.diceHistory.ambiguousLogIndices).toEqual([1]);
      expect(() => buildLiveDecisionStochasticInput(
        "balanced", session.diceHistory, ["Alice"],
      )).toThrow(/usable public reference-dice history/);
    } finally {
      session.stop();
    }
  });

  it("does not claim complete dice provenance across an integrity-relevant parser miss", async () => {
    const root = document.createElement("div");
    root.append(
      diceMessage(0, "Alice", 3, 5),
      message(1, "Alice cast the dice and the table reported eight"),
      diceMessage(2, "Bob", 6, 1),
    );
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "dice-parser-miss-game");

    await session.start();

    expect(session.unmatchedIntegrityCount).toBe(1);
    expect(session.diceHistory.coverage.ranges).toEqual([[0, 0], [2, 2]]);
    expect(session.diceHistory.ambiguousLogIndices).toEqual([1]);
    expect(session.diceHistory.provenance).toBe("gapped");
    expect(session.diceHistory.hasUnknownRollGap).toBe(true);
    session.stop();
  });

  it("weakens dice provenance for a trailing roll-capable parser miss", async () => {
    const root = document.createElement("div");
    root.append(
      diceMessage(0, "Alice", 3, 5),
      message(1, "Alice cast the dice and the table reported eight"),
    );
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "dice-trailing-parser-miss-game");

    await session.start();

    expect(session.unmatchedIntegrityCount).toBe(1);
    expect(session.diceHistory.coverage.ranges).toEqual([[0, 0]]);
    expect(session.diceHistory.ambiguousLogIndices).toEqual([1]);
    expect(session.diceHistory.provenance).toBe("gapped");
    expect(session.diceHistory.hasUnknownRollGap).toBe(true);
    session.stop();
  });

  it("keeps independently complete dice provenance across known harmless unmatched messages", async () => {
    const root = document.createElement("div");
    root.append(
      diceMessage(0, "Alice", 3, 5),
      message(1, "Happy settling!"),
      diceMessage(2, "Bob", 6, 1),
    );
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "dice-harmless-unmatched-game");

    await session.start();

    expect(session.unmatchedIntegrityCount).toBe(0);
    expect(session.diceHistory.coverage.ranges).toEqual([[0, 2]]);
    expect(session.diceHistory.ambiguousLogIndices).toEqual([]);
    expect(session.diceHistory.provenance).toBe("complete-from-first-gameplay-roll");
    session.stop();
  });

  it("persists schema 4 dice history independently and reconnects a contiguous suffix", async () => {
    const firstRoot = document.createElement("div");
    firstRoot.append(diceMessage(0, "Alice", 4, 4), diceMessage(1, "Bob", 3, 3));
    document.body.append(firstRoot);
    const first = new GameSession(firstRoot, vi.fn(), "dice-reconnect-game");
    await first.start();
    await vi.waitFor(() => {
      expect(localStorage.get(`colonistAssistantSession:${first.id}`)).toMatchObject({
        schema: 4,
        diceHistory: {
          provenance: "complete-from-first-gameplay-roll",
        },
      });
    });
    first.stop();
    firstRoot.remove();

    const secondRoot = document.createElement("div");
    secondRoot.append(diceMessage(2, "Alice", 2, 5), diceMessage(3, "Bob", 5, 4));
    document.body.append(secondRoot);
    const second = new GameSession(secondRoot, vi.fn(), "dice-reconnect-game");
    await second.start();

    expect(second.diceHistory.provenance).toBe("complete-from-first-gameplay-roll");
    expect(second.diceHistory.coverage.ranges).toEqual([[0, 3]]);
    expect(second.diceHistory.rolls.map((roll) => roll.total)).toEqual([8, 6, 7, 9]);
    second.stop();
  });

  it.each([false, true])("reconstructs legacy schema 4 ambiguity, including first-repair resaves (marker present: %s)", async (resavedByFirstRepair) => {
    const root = document.createElement("div");
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "legacy-schema4-indexed-ambiguity");
    localStorage.set(`colonistAssistantSession:${session.id}`, {
      schema: 4,
      id: session.id,
      page: `${location.origin}${location.pathname}${location.search}`,
      gameKey: "legacy-schema4-indexed-ambiguity",
      startedAt: 1,
      updatedAt: 2,
      events: [
        {
          type: "roll",
          player: "Alice",
          dice: [3, 5],
          id: "legacy-roll-0",
          index: 0,
          timestamp: 2,
          raw: "Alice rolled :die-3: :die-5:",
        },
      ],
      seenIds: ["legacy-roll-0"],
      partialHistory: false,
      diceHistory: {
        rolls: [
          {
            actor: "Alice",
            total: 8,
            dice: [3, 5],
            eventId: "legacy-roll-0",
            logIndex: 0,
          },
        ],
        provenance: "complete-from-first-gameplay-roll",
        coverage: { ranges: [[0, 1]] },
        ...(resavedByFirstRepair ? { ambiguousLogIndices: [] } : {}),
        gaps: [],
        hasUnknownRollGap: false,
      },
      unmatchedCount: 1,
      unmatchedIntegrityCount: 1,
      unmatchedSamples: [
        {
          signature: "legacy-unknown-1",
          count: 1,
          firstSeenAt: 2,
          lastSeenAt: 2,
          firstLogIndex: 1,
          lastLogIndex: 1,
          reason: "unrecognized-log-format",
          affectsIntegrity: true,
          sample: "Alice cast the dice and the table reported eight",
        },
      ],
    });

    await session.start();

    expect(session.diceHistory.ambiguousLogIndices).toEqual([1]);
    expect(session.diceHistory.hasUnlocatedRollAmbiguity).toBe(false);
    expect(session.diceHistory.provenance).toBe("gapped");
    expect(() =>
      buildLiveDecisionStochasticInput("balanced", session.diceHistory, ["Alice"]),
    ).toThrow(/usable public reference-dice history/);
    session.stop();
  });

  it("keeps legacy integrity evidence unavailable when retained samples cannot locate every ambiguous index", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "legacy-schema4-unlocated-ambiguity");
    localStorage.set(`colonistAssistantSession:${session.id}`, {
      schema: 4,
      id: session.id,
      page: `${location.origin}${location.pathname}${location.search}`,
      gameKey: "legacy-schema4-unlocated-ambiguity",
      startedAt: 1,
      updatedAt: 2,
      events: [
        {
          type: "roll",
          player: "Alice",
          dice: [3, 5],
          id: "legacy-roll-0",
          index: 0,
          timestamp: 2,
          raw: "Alice rolled :die-3: :die-5:",
        },
      ],
      seenIds: ["legacy-roll-0"],
      partialHistory: false,
      diceHistory: {
        rolls: [
          {
            actor: "Alice",
            total: 8,
            dice: [3, 5],
            eventId: "legacy-roll-0",
            logIndex: 0,
          },
        ],
        provenance: "complete-from-first-gameplay-roll",
        coverage: { ranges: [[0, 3]] },
        gaps: [],
        hasUnknownRollGap: false,
      },
      unmatchedCount: 3,
      unmatchedIntegrityCount: 3,
      unmatchedSamples: [
        {
          signature: "legacy-repeated-unknown",
          count: 3,
          firstSeenAt: 2,
          lastSeenAt: 4,
          firstLogIndex: 1,
          lastLogIndex: 3,
          reason: "unrecognized-log-format",
          affectsIntegrity: true,
          sample: "Alice cast the dice in an unknown format",
        },
      ],
    });

    await session.start();

    expect(session.diceHistory.ambiguousLogIndices).toEqual([1, 3]);
    expect(session.diceHistory.hasUnlocatedRollAmbiguity).toBe(true);
    expect(session.diceHistory.provenance).toBe("gapped");
    expect(() =>
      buildLiveDecisionStochasticInput("balanced", session.diceHistory, ["Alice"]),
    ).toThrow(/usable public reference-dice history/);
    session.stop();
  });

  it("migrates schema 3 roll evidence conservatively without inferring completeness", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "legacy-dice-game");
    localStorage.set(`colonistAssistantSession:${session.id}`, {
      schema: 3,
      id: session.id,
      page: `${location.origin}${location.pathname}${location.search}`,
      gameKey: "legacy-dice-game",
      startedAt: 1,
      updatedAt: 2,
      events: [
        {
          type: "roll",
          player: "Alice",
          dice: [3, 4],
          id: "legacy-roll",
          index: 9,
          timestamp: 2,
          raw: "Alice rolled :die-3: :die-4:",
        },
      ],
      seenIds: ["legacy-roll"],
      partialHistory: false,
      unmatchedCount: 0,
    });

    await session.start();

    expect(session.diceHistory.rolls.map((roll) => roll.total)).toEqual([7]);
    expect(session.diceHistory.provenance).toBe("gap-free-suffix");
    expect(session.diceHistory.missingPrefixRolls).toBeUndefined();
    session.stop();
  });

  it.each(["next indexed message", "save and restore"])(
    "keeps a partial schema 3 prefix untrusted after %s",
    async (continuation) => {
      const root = document.createElement("div");
      document.body.append(root);
      const gameKey = "legacy-partial-prefix-game";
      const session = new GameSession(root, vi.fn(), gameKey);
      const key = `colonistAssistantSession:${session.id}`;
      localStorage.set(key, {
        schema: 3,
        id: session.id,
        page: `${location.origin}${location.pathname}${location.search}`,
        gameKey,
        startedAt: 1,
        updatedAt: 2,
        events: [{
          type: "roll", player: "Alice", dice: [3, 5],
          id: "legacy-roll-0", index: 0, timestamp: 2,
          raw: "Alice rolled :die-3: :die-5:",
        }],
        seenIds: ["legacy-roll-0"],
        partialHistory: true,
        unmatchedCount: 0,
      });
      await session.start();
      const construct = (target: GameSession) => () => buildLiveDecisionStochasticInput(
        "balanced", target.diceHistory, ["Alice", "Bob"],
      );
      let restored: GameSession | undefined;
      try {
        expect(construct(session)).toThrow(/usable public reference-dice history/);
        if (continuation === "next indexed message") {
          root.append(diceMessage(1, "Bob", 6, 1));
          await vi.waitFor(() => expect(session.diceHistory.rolls).toHaveLength(2));
          expect(construct(session)).toThrow(/usable public reference-dice history/);
        } else {
          await vi.waitFor(() => expect(localStorage.get(key)).toMatchObject({ schema: 4 }));
          session.stop();
          restored = new GameSession(root, vi.fn(), gameKey);
          await restored.start();
          expect(construct(restored)).toThrow(/usable public reference-dice history/);
        }
      } finally {
        session.stop();
        restored?.stop();
      }
    },
  );

  it("carries trailing schema 3 integrity ambiguity into dice authority", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "legacy-schema3-trailing-ambiguity");
    localStorage.set(`colonistAssistantSession:${session.id}`, {
      schema: 3,
      id: session.id,
      page: `${location.origin}${location.pathname}${location.search}`,
      gameKey: "legacy-schema3-trailing-ambiguity",
      startedAt: 1,
      updatedAt: 2,
      events: [
        {
          type: "roll",
          player: "Alice",
          dice: [3, 5],
          id: "legacy-roll-0",
          index: 0,
          timestamp: 2,
          raw: "Alice rolled :die-3: :die-5:",
        },
      ],
      seenIds: ["legacy-roll-0"],
      partialHistory: false,
      unmatchedCount: 1,
      unmatchedIntegrityCount: 1,
      unmatchedSamples: [
        {
          signature: "legacy-schema3-unknown-1",
          count: 1,
          firstSeenAt: 3,
          lastSeenAt: 3,
          firstLogIndex: 1,
          lastLogIndex: 1,
          reason: "unrecognized-log-format",
          affectsIntegrity: true,
          sample: "Alice cast the dice in an unknown format",
        },
      ],
    });

    await session.start();

    expect(session.diceHistory.ambiguousLogIndices).toEqual([1]);
    expect(session.diceHistory.hasUnlocatedRollAmbiguity).toBe(false);
    expect(session.diceHistory.provenance).toBe("gapped");
    expect(() =>
      buildLiveDecisionStochasticInput("balanced", session.diceHistory, ["Alice"]),
    ).toThrow(/usable public reference-dice history/);
    session.stop();
  });

  it("does not poison parent schema 4 dice completeness from known harmless unmatched messages", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "legacy-schema4-harmless-unmatched");
    localStorage.set(`colonistAssistantSession:${session.id}`, {
      schema: 4,
      id: session.id,
      page: `${location.origin}${location.pathname}${location.search}`,
      gameKey: "legacy-schema4-harmless-unmatched",
      startedAt: 1,
      updatedAt: 2,
      events: [
        {
          type: "roll",
          player: "Alice",
          dice: [3, 5],
          id: "legacy-roll-0",
          index: 0,
          timestamp: 2,
          raw: "Alice rolled :die-3: :die-5:",
        },
      ],
      seenIds: ["legacy-roll-0"],
      partialHistory: false,
      diceHistory: {
        rolls: [
          {
            actor: "Alice",
            total: 8,
            dice: [3, 5],
            eventId: "legacy-roll-0",
            logIndex: 0,
          },
        ],
        provenance: "complete-from-first-gameplay-roll",
        coverage: { ranges: [[0, 1]] },
        gaps: [],
        hasUnknownRollGap: false,
      },
      unmatchedCount: 1,
      unmatchedIntegrityCount: 0,
      unmatchedSamples: [
        {
          signature: "legacy-happy-settling",
          count: 1,
          firstSeenAt: 3,
          lastSeenAt: 3,
          firstLogIndex: 1,
          lastLogIndex: 1,
          reason: "known-ignored-system-message",
          affectsIntegrity: false,
          sample: "Happy settling!",
        },
      ],
    });

    await session.start();

    expect(session.diceHistory.ambiguousLogIndices).toEqual([]);
    expect(session.diceHistory.hasUnlocatedRollAmbiguity).toBe(false);
    expect(session.diceHistory.provenance).toBe("complete-from-first-gameplay-roll");
    expect(
      buildLiveDecisionStochasticInput("balanced", session.diceHistory, ["Alice"]),
    ).toMatchObject({ model: "mref-colonist-linked-2024-v1" });
    session.stop();
  });

  it("trusts current schema 4 explicit ambiguity markers instead of replaying historical unmatched counters", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "current-schema4-explicit-ambiguity-marker");
    localStorage.set(`colonistAssistantSession:${session.id}`, {
      schema: 4,
      id: session.id,
      page: `${location.origin}${location.pathname}${location.search}`,
      gameKey: "current-schema4-explicit-ambiguity-marker",
      startedAt: 1,
      updatedAt: 2,
      events: [
        {
          type: "roll",
          player: "Alice",
          dice: [3, 5],
          id: "current-roll-0",
          index: 0,
          timestamp: 2,
          raw: "Alice rolled :die-3: :die-5:",
        },
      ],
      seenIds: ["current-roll-0"],
      partialHistory: false,
      diceHistory: {
        rolls: [
          {
            actor: "Alice",
            total: 8,
            dice: [3, 5],
            eventId: "current-roll-0",
            logIndex: 0,
          },
        ],
        provenance: "complete-from-first-gameplay-roll",
        coverage: { ranges: [[0, 1]] },
        integrityVersion: 1,
        ambiguousLogIndices: [],
        hasUnlocatedRollAmbiguity: false,
        gaps: [],
        hasUnknownRollGap: false,
      },
      unmatchedCount: 1,
      unmatchedIntegrityCount: 1,
      unmatchedSamples: [
        {
          signature: "resolved-current-unknown",
          count: 1,
          firstSeenAt: 2,
          lastSeenAt: 2,
          firstLogIndex: 1,
          lastLogIndex: 1,
          reason: "unrecognized-log-format",
          affectsIntegrity: true,
          sample: "Previously unresolved rendering at index one",
        },
      ],
    });

    await session.start();

    expect(session.diceHistory.ambiguousLogIndices).toEqual([]);
    expect(session.diceHistory.hasUnlocatedRollAmbiguity).toBe(false);
    expect(session.diceHistory.provenance).toBe("complete-from-first-gameplay-roll");
    expect(
      buildLiveDecisionStochasticInput("balanced", session.diceHistory, ["Alice"]),
    ).toMatchObject({ model: "mref-colonist-linked-2024-v1" });
    session.stop();
  });

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

  it("keeps production bot/system status rows out of dice ambiguity", async () => {
    const notices = [
      "Bot is selecting where to place Robber for hamzax",
      "hamzax has disconnected. A bot will take over next turn unless hamzax reconnects.",
      "You are the last player remaining. You will be awarded the win in 100 seconds if your opponent does not reconnect.",
      "hamzax has reconnected",
    ];
    const root = document.createElement("div");
    notices.forEach((text, index) => root.append(message(43 + index, text)));
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "production-status-game");

    await session.start();

    expect(session.events).toEqual([]);
    expect(session.unmatchedIntegrityCount).toBe(0);
    expect(session.diceHistory.ambiguousLogIndices).toEqual([]);
    expect(session.unmatchedSamples).toEqual(
      expect.arrayContaining(
        notices.map((sample) =>
          expect.objectContaining({ affectsIntegrity: false, sample }),
        ),
      ),
    );
    session.stop();
  });

  it("does not turn an unknown non-roll status into dice ambiguity", async () => {
    const root = document.createElement("div");
    root.append(message(43, "Colonist emitted a new harmless-looking status"));
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "unknown-status-game");

    await session.start();

    expect(session.unmatchedIntegrityCount).toBe(1);
    expect(session.diceHistory.ambiguousLogIndices).toEqual([]);
    session.stop();
  });

  it("keeps a roll-shaped unknown fail-closed for Balanced Dice", async () => {
    const root = document.createElement("div");
    root.append(message(43, "Dice result for Alice :die-3: :die-5:"));
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "unknown-roll-game");

    await session.start();

    expect(session.unmatchedIntegrityCount).toBe(1);
    expect(session.diceHistory.ambiguousLogIndices).toEqual([43]);
    session.stop();
  });

  it("recognizes Friendly Robber status messages without failing integrity", async () => {
    const notices = [
      "Friendly Robber is active, tiles available to block are limited",
      "Friendly Robber is active, no available tiles to block",
    ];
    const root = document.createElement("div");
    notices.forEach((text, index) => root.append(message(index, text)));
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "friendly-robber-game");

    await session.start();

    expect(session.events).toEqual([]);
    expect(session.unmatchedCount).toBe(2);
    expect(session.unmatchedIntegrityCount).toBe(0);
    expect(session.unmatchedSamples).toHaveLength(2);
    expect(session.unmatchedSamples).toEqual(
      expect.arrayContaining(
        notices.map((sample) =>
          expect.objectContaining({
            reason: "known-ignored-friendly-robber-status",
            affectsIntegrity: false,
            sample,
          }),
        ),
      ),
    );
    session.stop();
  });

  it("treats Colonist's empty robber victim notice as harmless public evidence", async () => {
    const root = document.createElement("div");
    root.append(message(47, "No player to steal from"));
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "empty-robbery-game");

    await session.start();

    expect(session.events).toEqual([]);
    expect(session.unmatchedIntegrityCount).toBe(0);
    expect(session.unmatchedSamples).toEqual([
      expect.objectContaining({
        reason: "known-ignored-empty-robbery",
        affectsIntegrity: false,
        sample: "No player to steal from",
      }),
    ]);
    expect(session.diceHistory.ambiguousLogIndices).toEqual([]);
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

  it("waits for the previous root's final save before restoring a replacement session", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    const gameKey = "atomic-session-handoff";
    const session = new GameSession(root, vi.fn(), gameKey);

    await session.start();
    await vi.waitFor(() =>
      expect(localStorage.get(`colonistAssistantSession:${session.id}`)).toBeDefined(),
    );
    session.setInitialPlacement(true, gameKey);
    session.setInitialPlacement(false, gameKey);
    expect(
      session.observeBoardDiceSnapshot({
        gameKey,
        botOnlyGame: true,
        initialPlacement: false,
        hasRolled: true,
        lastRoll: 8,
        currentPlayer: "Alice",
        turn: 0,
        gameplayRollCount: 1,
      }),
    ).toBe(true);

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

    session.stop();
    await saveStarted;

    const restored = new GameSession(root, vi.fn(), gameKey);
    let restoreFinished = false;
    const restoring = restored.start().then(() => {
      restoreFinished = true;
    });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(restoreFinished).toBe(false);

    releaseSave?.();
    await restoring;

    expect(restored.diceHistory.rolls).toEqual([
      expect.objectContaining({ actor: "Alice", total: 8 }),
    ]);
    expect(restored.diceHistory.provenance).toBe("complete-from-first-gameplay-roll");
    restored.stop();
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
