import { describe, expect, it } from "vitest";

import type { DecisionAnalysis } from "../src/core/engine";
import type { BoardSnapshot } from "../src/core/placement";
import { WinPredictionStabilizer } from "../src/core/win-prediction";

const analysis = (probabilities: number[]): DecisionAnalysis => ({
  engine: "deep-search",
  players: probabilities.map((probability, index) => ({
    player: `P${index + 1}`,
    probability,
    etaTurns: 12,
    samples: 100,
    confidence: "medium",
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

const board = (gameKey: string, leaderPoints = 2): BoardSnapshot => ({
  gameKey,
  victoryTarget: 10,
  hexes: [],
  vertices: [],
  edges: [],
  players: Object.fromEntries(
    [1, 2, 3, 4].map((player) => [
      `P${player}`,
      {
        handSize: 0,
        tradeRatios: {
          lumber: 4,
          brick: 4,
          wool: 4,
          grain: 4,
          ore: 4,
        },
        cardDiscardLimit: 7,
        visiblePoints: player === 1 ? leaderPoints : 2,
      },
    ]),
  ),
});

describe("win prediction stabilization", () => {
  it("shrinks unsupported extremes and rate-limits non-material reversals", () => {
    const stabilizer = new WinPredictionStabilizer();
    const first = stabilizer.update(
      analysis([0.01, 0.4, 0.3, 0.29]),
      board("game"),
    )!;
    const second = stabilizer.update(
      analysis([0.4, 0.01, 0.3, 0.29]),
      board("game"),
    )!;

    expect(first.players[0]!.probability).toBeGreaterThan(0.1);
    for (const estimate of second.players) {
      const previous = first.players.find(
        (candidate) => candidate.player === estimate.player,
      )!;
      expect(
        Math.abs(estimate.probability - previous.probability),
      ).toBeLessThanOrEqual(0.025001);
    }
    expect(
      second.players.reduce(
        (sum, estimate) => sum + estimate.probability,
        0,
      ),
    ).toBeCloseTo(1, 8);
  });

  it("allows a larger but bounded move after public victory progress", () => {
    const stabilizer = new WinPredictionStabilizer();
    const first = stabilizer.update(
      analysis([0.25, 0.25, 0.25, 0.25]),
      board("game"),
    )!;
    const second = stabilizer.update(
      analysis([0.8, 0.07, 0.07, 0.06]),
      board("game", 3),
    )!;

    expect(
      second.players[0]!.probability -
        first.players[0]!.probability,
    ).toBeGreaterThan(0.025);
    expect(
      second.players[0]!.probability -
        first.players[0]!.probability,
    ).toBeLessThanOrEqual(0.110001);
  });
});
