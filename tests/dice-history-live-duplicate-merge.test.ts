import { describe, expect, it } from "vitest";

import {
  buildLiveDecisionStochasticInput,
  restoreDiceHistoryState,
  type StoredDiceHistoryState,
} from "../src/core/dice-history";

// Live record grid7603 (bot game, Balanced Dice): every gameplay roll was
// captured twice — once from board turns (`board-roll:N`, no logIndex) and
// once from the DOM log (`index:M`, with logIndex) — while log coverage stayed
// gapped, so the sources never merged.
const PLAYERS = ["Autumn", "Uzzia", "Frayne", "hamzax"];
const TOTALS = [7, 11, 5, 12, 8, 4, 3, 10];
const LOG_INDEXES = [32, 37, 42, 47, 50, 55, 60, 66];

const stored = (): StoredDiceHistoryState => ({
  integrityVersion: 1,
  provenance: "gapped",
  coverage: {
    ranges: [
      [0, 30], [32, 35], [37, 40], [42, 45], [47, 48],
      [50, 53], [55, 58], [60, 64], [66, 69],
    ],
  },
  ambiguousLogIndices: [],
  hasUnlocatedRollAmbiguity: false,
  hasUnreconciledSources: true,
  missingPrefixRolls: 0,
  gaps: [],
  hasUnknownRollGap: true,
  rolls: [
    ...TOTALS.map((total, i) => ({
      actor: PLAYERS[i % PLAYERS.length]!,
      dice: [[1, 6], [6, 5], [4, 1], [6, 6], [5, 3], [1, 3], [2, 1], [6, 4]][i] as [number, number],
      eventId: `index:${LOG_INDEXES[i]}:rec${i}`,
      logIndex: LOG_INDEXES[i]!,
      total,
    })),
    ...TOTALS.map((total, i) => ({
      actor: PLAYERS[i % PLAYERS.length]!,
      eventId: `board-roll:${i}:${PLAYERS[i % PLAYERS.length]}`,
      total,
    })),
  ],
});

describe("live duplicate-source dice history (grid7603)", () => {
  it("keeps both sources unmerged under gapped coverage", () => {
    const state = restoreDiceHistoryState(stored());
    expect(state.rolls).toHaveLength(16);
    expect(state.hasUnreconciledSources).toBe(true);
  });

  it("still reconciles 8 board ordinals against 8 matching indexed rolls", () => {
    const state = restoreDiceHistoryState(stored());
    const input = buildLiveDecisionStochasticInput("balanced", state, PLAYERS, 8);
    expect(input.model).toBe("mref-colonist-linked-2024-v1");
    expect(input.rolls).toHaveLength(8);
  });

  it("fails closed when unresolved ambiguity joins the duplicated sources", () => {
    const withAmbiguity = stored();
    withAmbiguity.ambiguousLogIndices = [83, 84];
    const state = restoreDiceHistoryState(withAmbiguity);
    expect(() =>
      buildLiveDecisionStochasticInput("balanced", state, PLAYERS, 12),
    ).toThrow(
      "Balanced Dice public roll sequence does not reconcile with public turn progress",
    );
  });
});
