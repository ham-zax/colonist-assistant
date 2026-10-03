import { describe, expect, it } from "vitest";

import { analyzePublicEstimate } from "../src/core/engine";
import type { BoardSnapshot } from "../src/core/placement";
import { emptyResources } from "../src/core/resources";
import { createTrackerState } from "../src/core/tracker";
import type { TrackerState } from "../src/core/types";
import { WinPredictionStabilizer } from "../src/core/win-prediction";

const state = (players: string[]): TrackerState => {
  const tracker = createTrackerState();
  tracker.playerOrder = players;
  return tracker;
};

const board = (
  gameKey: string,
  ownVictoryPoints: number,
  myPlayer = "hamzax",
  extra: {
    winner?: string;
    gameOver?: boolean;
    victoryTarget?: number;
  } = {},
): BoardSnapshot => ({
  gameKey,
  victoryTarget: extra.victoryTarget ?? 10,
  diceMode: "unknown",
  hexes: [],
  vertices: [],
  edges: [],
  myPlayer,
  ...(extra.winner ? { winner: extra.winner } : {}),
  ...(extra.gameOver !== undefined ? { gameOver: extra.gameOver } : {}),
  ownHand: emptyResources(),
  ownDevelopmentCards: {
    cards: {
      knight: 0,
      monopoly: 0,
      "road-building": 0,
      "year-of-plenty": 0,
      "victory-point": ownVictoryPoints,
    },
    playable: {
      knight: 0,
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
  },
  players: {
    hamzax: {
      handSize: 0,
      tradeRatios: {
        lumber: 4,
        brick: 4,
        wool: 4,
        grain: 4,
        ore: 4,
      },
      cardDiscardLimit: 7,
      visiblePoints: 6,
    },
    popdart: {
      handSize: 0,
      tradeRatios: {
        lumber: 4,
        brick: 4,
        wool: 4,
        grain: 4,
        ore: 4,
      },
      cardDiscardLimit: 7,
      visiblePoints: 6,
    },
  },
});

const probabilityOf = (
  gameKey: string,
  ownVictoryPoints: number,
  player: string,
  myPlayer = "hamzax",
  extra: { victoryTarget?: number } = {},
): { probability: number; etaTurns: number } => {
  const analysis = analyzePublicEstimate(
    state(["hamzax", "popdart"]),
    board(gameKey, ownVictoryPoints, myPlayer, extra),
    "hamzax",
  );
  const estimate = analysis.players.find(
    (candidate) => candidate.player === player,
  )!;
  return {
    probability: estimate.probability,
    etaTurns: estimate.etaTurns,
  };
};

describe("display estimate with exact local victory points", () => {
  it("credits exact held VP against a matched no-VP baseline", () => {
    const baseline = probabilityOf("vp-ordering-baseline", 0, "hamzax");
    const withHidden = probabilityOf("vp-ordering-hidden", 3, "hamzax");

    expect(withHidden.etaTurns).toBeLessThan(baseline.etaTurns);
    expect(withHidden.probability).toBeGreaterThan(baseline.probability);
  });

  it("invalidates the display cache when only the held VP count changes", () => {
    const before = probabilityOf("vp-cache-key", 0, "hamzax");
    const after = probabilityOf("vp-cache-key", 3, "hamzax");

    expect(after.probability).not.toBe(before.probability);
    expect(after.etaTurns).not.toBe(before.etaTurns);
  });

  it("does not transfer exact local VP across a seat change with the same held count", () => {
    const asHamzax = probabilityOf("vp-seat-key", 3, "hamzax", "hamzax");
    const asPopdart = probabilityOf("vp-seat-key", 3, "hamzax", "popdart");

    expect(asPopdart.probability).not.toBe(asHamzax.probability);
  });

  it("recomputes when unknown local VP becomes an exact zero", () => {
    const snapshot = board("vp-exact-zero-key", 0);
    snapshot.players!.hamzax!.developmentCards = 6;
    snapshot.players!.hamzax!.visiblePoints = 8;
    const tracker = state(["hamzax", "popdart"]);
    const unknown = analyzePublicEstimate(
      tracker,
      { ...snapshot, ownDevelopmentCards: undefined },
      "hamzax",
    );
    const exact = analyzePublicEstimate(tracker, snapshot, "hamzax");

    expect(exact.players[0]!.probability).toBeLessThan(
      unknown.players[0]!.probability,
    );
  });

  it("recomputes the display estimate when the victory target changes", () => {
    const target10 = probabilityOf("vp-target-key", 3, "hamzax");
    const target12 = probabilityOf("vp-target-key", 3, "hamzax", "hamzax", {
      victoryTarget: 12,
    });

    expect(target12.etaTurns).not.toBe(target10.etaTurns);
  });
});

const stabilizerAnalysis = (probabilities: number[]) => ({
  engine: "deep-search" as const,
  players: probabilities.map((probability, index) => ({
    player: index === 0 ? "hamzax" : "popdart",
    probability,
    etaTurns: 12,
    samples: 100,
    confidence: "medium" as const,
    reasons: [],
  })),
  actionScores: {
    road: 0,
    settlement: 0,
    city: 0,
    development: 0,
  },
  simulations: 0,
  model: "fixture",
});

describe("display smoothing with exact local victory points", () => {
  it("treats a hidden-VP-only change as material instead of rate-limiting it", () => {
    const stabilizer = new WinPredictionStabilizer();
    const first = stabilizer.update(
      stabilizerAnalysis([0.5, 0.5]),
      board("vp-material", 0),
    )!;
    const second = stabilizer.update(
      stabilizerAnalysis([0.9, 0.1]),
      board("vp-material", 3),
    )!;
    const delta = Math.abs(
      second.players[0]!.probability - first.players[0]!.probability,
    );

    expect(delta).toBeGreaterThan(0.025);
    expect(delta).toBeLessThanOrEqual(0.110001);
  });

  it("still rate-limits a non-material reversal to the tight step", () => {
    const stabilizer = new WinPredictionStabilizer();
    const first = stabilizer.update(
      stabilizerAnalysis([0.5, 0.5]),
      board("vp-immaterial", 0),
    )!;
    const second = stabilizer.update(
      stabilizerAnalysis([0.9, 0.1]),
      board("vp-immaterial", 0),
    )!;

    expect(
      Math.abs(
        second.players[0]!.probability - first.players[0]!.probability,
      ),
    ).toBeLessThanOrEqual(0.025001);
  });

  it("keeps target-reaching effective points below 1 without a declared winner", () => {
    const stabilizer = new WinPredictionStabilizer();
    const result = stabilizer.update(
      stabilizerAnalysis([0.4, 0.6]),
      board("vp-no-declared-winner", 4),
    )!;

    expect(
      result.players.find((estimate) => estimate.player === "hamzax")
        ?.probability,
    ).toBeLessThan(1);
  });

  it("transitions to 1/0 on a confirmed winner with unchanged analysis and material", () => {
    const stabilizer = new WinPredictionStabilizer();
    const open = stabilizer.update(
      stabilizerAnalysis([0.4, 0.6]),
      board("vp-confirmed-winner", 4),
    )!;
    expect(
      open.players.find((estimate) => estimate.player === "hamzax")
        ?.probability,
    ).toBeLessThan(1);

    const decided = stabilizer.update(
      stabilizerAnalysis([0.4, 0.6]),
      board("vp-confirmed-winner", 4, "hamzax", {
        winner: "hamzax",
        gameOver: true,
      }),
    )!;

    expect(
      decided.players.find((estimate) => estimate.player === "hamzax")
        ?.probability,
    ).toBe(1);
    expect(
      decided.players.find((estimate) => estimate.player === "popdart")
        ?.probability,
    ).toBe(0);
  });

  it("does not serve a stale result when the confirmed winner changes", () => {
    const stabilizer = new WinPredictionStabilizer();
    stabilizer.update(
      stabilizerAnalysis([0.4, 0.6]),
      board("vp-winner-change", 4, "hamzax", {
        winner: "hamzax",
        gameOver: true,
      }),
    )!;
    const revised = stabilizer.update(
      stabilizerAnalysis([0.4, 0.6]),
      board("vp-winner-change", 4, "hamzax", {
        winner: "popdart",
        gameOver: true,
      }),
    )!;

    expect(
      revised.players.find((estimate) => estimate.player === "popdart")
        ?.probability,
    ).toBe(1);
    expect(
      revised.players.find((estimate) => estimate.player === "hamzax")
        ?.probability,
    ).toBe(0);
  });

  it("does not invent an opponent win from visible points alone below target", () => {
    const stabilizer = new WinPredictionStabilizer();
    const result = stabilizer.update(
      stabilizerAnalysis([0.4, 0.6]),
      board("vp-no-invented-win", 0),
    )!;

    expect(
      result.players.find((estimate) => estimate.player === "popdart")
        ?.probability,
    ).toBeLessThan(1);
  });
});
