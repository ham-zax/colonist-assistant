import { describe, expect, it } from "vitest";

import type { BoardSnapshot } from "../src/core/placement";
import { shouldFastTrackRoll } from "../src/core/forced-action";

const board = (): BoardSnapshot => ({
  hexes: [],
  vertices: [],
  edges: [],
  isMyTurn: true,
  action: "none",
});

describe("forced action fast lane", () => {
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
});
