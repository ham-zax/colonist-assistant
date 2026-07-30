import { describe, expect, it } from "vitest";

import type { BoardSnapshot } from "../src/core/placement";
import {
  resolveLocalBoardAction,
  shouldFastTrackEndTurn,
  shouldFastTrackRoll,
} from "../src/core/forced-action";

const board = (): BoardSnapshot => ({
  hexes: [],
  vertices: [],
  edges: [],
  isMyTurn: true,
  action: "none",
});

describe("forced action fast lane", () => {
  it("does not claim another player's discard phase as a local action", () => {
    expect(resolveLocalBoardAction("discard", false)).toBe("none");
    expect(resolveLocalBoardAction("none", true)).toBe("discard");
    expect(resolveLocalBoardAction("robber", false)).toBe("robber");
  });

  it("rolls immediately when no pre-roll decision exists", () => {
    expect(shouldFastTrackRoll(board(), "roll")).toBe(true);
  });

  it("waits when a playable development card creates a real choice", () => {
    const position = board();
    position.ownDevelopmentCards = {
      cards: {
        knight: 1,
        monopoly: 0,
        "road-building": 0,
        "year-of-plenty": 0,
        "victory-point": 0,
      },
      playable: {
        knight: 1,
        monopoly: 0,
        "road-building": 0,
        "year-of-plenty": 0,
        "victory-point": 0,
      },
      boughtThisTurn: {
        knight: 0,
        monopoly: 0,
        "road-building": 0,
        "year-of-plenty": 0,
        "victory-point": 0,
      },
      hasPlayedThisTurn: false,
    };

    expect(shouldFastTrackRoll(position, "roll")).toBe(false);
  });

  it("never fast-tracks a stale dice control over a mandatory board action", () => {
    for (const action of ["robber", "road", "settlement", "city"] as const) {
      const position = board();
      position.action = action;
      expect(shouldFastTrackRoll(position, "roll")).toBe(false);
    }
  });

  it("ends immediately when the roll is done and no conversion remains", () => {
    const position = board();
    position.hasRolled = true;
    position.domesticTradeUsed = true;
    position.myPlayer = "You";
    position.ownHand = {
      lumber: 0,
      brick: 0,
      wool: 1,
      grain: 0,
      ore: 0,
    };
    position.players = {
      You: {
        handSize: 1,
        tradeRatios: {
          lumber: 3,
          brick: 3,
          wool: 3,
          grain: 3,
          ore: 3,
        },
        cardDiscardLimit: 7,
      },
    };

    expect(shouldFastTrackEndTurn(position, "end")).toBe(true);
  });

  it("does not skip a player trade, bank conversion, or affordable build", () => {
    const position = board();
    position.hasRolled = true;
    position.myPlayer = "You";
    position.ownHand = {
      lumber: 0,
      brick: 0,
      wool: 1,
      grain: 0,
      ore: 0,
    };
    position.players = {
      You: {
        handSize: 1,
        tradeRatios: {
          lumber: 4,
          brick: 4,
          wool: 2,
          grain: 4,
          ore: 4,
        },
        cardDiscardLimit: 7,
      },
    };

    expect(shouldFastTrackEndTurn(position, "end")).toBe(false);

    position.domesticTradeUsed = true;
    position.ownHand.wool = 2;
    expect(shouldFastTrackEndTurn(position, "end")).toBe(false);

    position.ownHand = {
      lumber: 1,
      brick: 1,
      wool: 0,
      grain: 0,
      ore: 0,
    };
    expect(shouldFastTrackEndTurn(position, "end")).toBe(false);
  });
});
