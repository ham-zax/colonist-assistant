import { describe, expect, it } from "vitest";

import { emptyResources } from "../src/core/resources";
import type { StoredEvent, TrackerEvent } from "../src/core/types";
import { canonicalizeBoardPlayerAliases } from "../src/content/board";
import { canonicalizeEvent } from "../src/content/session";

const stored = (
  event: TrackerEvent,
): StoredEvent =>
  ({
    ...event,
    id: "event",
    timestamp: 1,
    raw: "fixture",
  }) as StoredEvent;

describe("local player alias canonicalization", () => {
  it("merges Colonist's You label into the actual username", () => {
    const transfer = canonicalizeEvent(
      stored({
        type: "transfer",
        from: "Debby",
        to: "You",
        cards: emptyResources(),
        reason: "robbery",
      }),
      "rodrgds",
    );
    const trade = canonicalizeEvent(
      stored({
        type: "trade",
        player: "You",
        acceptingPlayer: "Prager",
        given: emptyResources(),
        received: emptyResources(),
        bank: false,
      }),
      "rodrgds",
    );

    expect(transfer).toMatchObject({ from: "Debby", to: "rodrgds" });
    expect(trade).toMatchObject({
      player: "rodrgds",
      acceptingPlayer: "Prager",
    });
  });

  it("canonicalizes every public-board reference to the account username", () => {
    const hand = emptyResources();
    const board = canonicalizeBoardPlayerAliases({
      hexes: [],
      diceMode: "unknown",
      vertices: [
        {
          id: "v0",
          adjacentHexes: [],
          adjacentVertices: [],
          building: { player: "You", kind: "settlement" },
        },
      ],
      edges: [{ id: "e0", vertices: ["v0", "v0"], player: "You" }],
      myPlayer: "rodrgds",
      playerOrder: ["You", "Debby", "rodrgds"],
      currentPlayer: "You",
      players: {
        You: {
          handSize: 4,
          tradeRatios: { ...hand },
          cardDiscardLimit: 7,
        },
        Debby: {
          handSize: 3,
          tradeRatios: { ...hand },
          cardDiscardLimit: 7,
        },
      },
      activeTrades: [
        {
          id: "t1",
          creator: "You",
          tradeExecutor: "You",
          creatorGive: { ...hand },
          creatorReceive: { ...hand },
          incoming: false,
          counterOffer: false,
          canAccept: false,
          acceptedPlayers: ["Debby"],
          rejectedPlayers: ["You"],
        },
      ],
      robberVictimPlayers: ["You", "Debby"],
    });

    expect(board.playerOrder).toEqual(["rodrgds", "Debby"]);
    expect(board.currentPlayer).toBe("rodrgds");
    expect(board.vertices[0]?.building?.player).toBe("rodrgds");
    expect(board.edges[0]?.player).toBe("rodrgds");
    expect(Object.keys(board.players ?? {})).toEqual(["rodrgds", "Debby"]);
    expect(board.activeTrades?.[0]).toMatchObject({
      creator: "rodrgds",
      tradeExecutor: "rodrgds",
      rejectedPlayers: ["rodrgds"],
    });
    expect(board.robberVictimPlayers).toEqual(["rodrgds", "Debby"]);
  });
});
