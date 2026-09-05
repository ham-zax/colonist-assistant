import { describe, expect, it } from "vitest";

import {
  appendPublicDiceRoll,
  buildLiveDecisionStochasticInput,
  cloneDiceHistoryState,
  createDiceHistoryState,
  diceHistoryDigest,
  noteMissingPublicRoll,
  noteRollCapableLogAmbiguity,
  observeLogCoverage,
  publicRollObservations,
  restoreDiceHistoryState,
  serializeDiceHistoryState,
  type PublicDiceRoll,
} from "../src/core/dice-history";

const roll = (
  eventId: string,
  actor: string,
  total: number,
  logIndex: number,
): PublicDiceRoll => ({ eventId, actor, total, logIndex });

describe("public dice history", () => {
  it("keeps captured rolls in canonical log order", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [0, 1, 2, 3]);
    appendPublicDiceRoll(state, roll("r3", "P1", 8, 3));
    appendPublicDiceRoll(state, roll("r1", "P0", 6, 1));
    appendPublicDiceRoll(state, roll("r2", "P1", 7, 2));

    expect(state.rolls.map(({ eventId }) => eventId)).toEqual(["r1", "r2", "r3"]);
    expect(state.rolls.map(({ total }) => total)).toEqual([6, 7, 8]);
  });

  it("treats a stable Colonist log index as dice-event identity across presentation changes", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [4]);
    appendPublicDiceRoll(state, {
      ...roll("presentation-a", "P0", 8, 4),
      dice: [3, 5],
    });
    appendPublicDiceRoll(state, {
      ...roll("presentation-b", "P0", 8, 4),
      dice: [3, 5],
    });

    expect(state.rolls).toHaveLength(1);
    expect(state.rolls[0]).toMatchObject({
      eventId: "presentation-a",
      actor: "P0",
      total: 8,
      dice: [3, 5],
      logIndex: 4,
    });
  });

  it("keeps conflicting same-index dice evidence sticky and unavailable to Balanced decisions", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [0]);
    appendPublicDiceRoll(state, {
      ...roll("presentation-a", "P0", 8, 0),
      dice: [3, 5],
    });
    expect(
      buildLiveDecisionStochasticInput("balanced", state, ["P0"]),
    ).toMatchObject({ model: "mref-colonist-linked-2024-v1" });

    expect(() =>
      appendPublicDiceRoll(state, {
        ...roll("presentation-b", "P0", 9, 0),
        dice: [4, 5],
      }),
    ).toThrow(/log index 0/);
    expect(state.rolls).toHaveLength(1);
    expect(state.rolls[0]?.total).toBe(8);
    expect(state.ambiguousLogIndices).toEqual([0]);
    expect(state.provenance).toBe("gapped");
    expect(state.hasUnknownRollGap).toBe(true);
    expect(() =>
      buildLiveDecisionStochasticInput("balanced", state, ["P0"]),
    ).toThrow(/usable public reference-dice history/);

    observeLogCoverage(state, [0]);
    appendPublicDiceRoll(state, {
      ...roll("presentation-c", "P0", 8, 0),
      dice: [3, 5],
    });
    expect(state.ambiguousLogIndices).toEqual([0]);
    expect(state.provenance).toBe("gapped");
  });

  it("marks coverage complete when indexed observation is continuous from zero", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [0, 1, 2, 3]);
    appendPublicDiceRoll(state, roll("r1", "P0", 6, 1));
    appendPublicDiceRoll(state, roll("r2", "P1", 7, 3));

    expect(state.provenance).toBe("complete-from-first-gameplay-roll");
  });

  it("marks an indexed mid-log capture as a gap-free suffix without inventing a missing roll count", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [8, 9, 10]);
    appendPublicDiceRoll(state, roll("r9", "P0", 5, 9));

    expect(state.provenance).toBe("gap-free-suffix");
    expect(state.missingPrefixRolls).toBeUndefined();
  });

  it("detects log coverage gaps independently of captured event truncation", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [0, 1, 3, 4]);
    appendPublicDiceRoll(state, roll("r4", "P1", 11, 4));

    expect(state.provenance).toBe("gapped");
    expect(state.coverage.ranges).toEqual([[0, 1], [3, 4]]);
    expect(state.hasUnknownRollGap).toBe(true);
  });

  it("uses public gameplay-roll count to prove sparse generic log indexes did not lose a roll", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [
      ...Array.from({ length: 29 }, (_, index) => index),
      30,
    ]);
    appendPublicDiceRoll(state, roll("r30", "P0", 8, 30));

    expect(state.provenance).toBe("gapped");
    expect(state.ambiguousLogIndices).toEqual([]);
    expect(
      buildLiveDecisionStochasticInput(
        "balanced",
        state,
        ["P0", "P1", "P2", "P3"],
        1,
      ),
    ).toMatchObject({
      model: "mref-colonist-linked-2024-v1",
      provenance: "complete-from-first-gameplay-roll",
      rolls: [{ ordinal: 0, actor: 0, total: 8 }],
    });
  });

  it("keeps complete bot board authority despite parser-only DOM ambiguity", () => {
    const state = createDiceHistoryState();
    const players = ["P0", "P1"];
    const logIndexes = [0, 2, 4, 7, 9, 12, 15, 18, 21, 24, 27, 30];
    observeLogCoverage(state, [
      ...Array.from({ length: 29 }, (_, index) => index),
      30,
      31,
      32,
      34,
      35,
      36,
      37,
      38,
      40,
      41,
      42,
      43,
    ]);
    for (let ordinal = 0; ordinal < 12; ordinal += 1) {
      const actor = players[ordinal % players.length]!;
      const total = 2 + (ordinal % 11);
      appendPublicDiceRoll(state, {
        eventId: `board-roll:${ordinal}:${actor}`,
        actor,
        total,
      });
      appendPublicDiceRoll(
        state,
        roll(`log-roll-${ordinal}`, actor, total, logIndexes[ordinal]!),
      );
    }
    noteRollCapableLogAmbiguity(state, 83);
    noteRollCapableLogAmbiguity(state, 84);

    expect(state.rolls).toHaveLength(24);
    expect(state.provenance).toBe("gapped");
    expect(
      buildLiveDecisionStochasticInput("balanced", state, players, 12),
    ).toMatchObject({
      model: "mref-colonist-linked-2024-v1",
      provenance: "complete-from-first-gameplay-roll",
      rolls: Array.from({ length: 12 }, (_, ordinal) => ({
        ordinal,
        actor: ordinal % 2,
        total: 2 + (ordinal % 11),
      })),
    });
  });

  it("still fails closed on a concrete indexed contradiction with complete board authority", () => {
    const state = createDiceHistoryState();
    appendPublicDiceRoll(state, {
      eventId: "board-roll:0:P0",
      actor: "P0",
      total: 8,
    });
    appendPublicDiceRoll(state, roll("log-roll", "P0", 8, 3));
    expect(() =>
      appendPublicDiceRoll(state, roll("contradictory-rerender", "P0", 9, 3)),
    ).toThrow(/log index 3/);

    expect(() =>
      buildLiveDecisionStochasticInput("balanced", state, ["P0"], 1),
    ).toThrow(/does not reconcile with public turn progress/);
  });

  it("reconstructs a uniquely located internal roll miss from public turn progress", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [0, 2]);
    appendPublicDiceRoll(state, roll("r0", "P0", 8, 0));
    appendPublicDiceRoll(state, roll("r2", "P2", 6, 2));

    expect(
      buildLiveDecisionStochasticInput(
        "balanced",
        state,
        ["P0", "P1", "P2", "P3"],
        3,
      ),
    ).toMatchObject({
      provenance: "gapped",
      gaps: [{ afterOrdinal: 0, missingRolls: 1 }],
      rolls: [
        { ordinal: 0, actor: 0, total: 8 },
        { ordinal: 2, actor: 2, total: 6 },
      ],
    });
  });

  it("fails closed when board progress cannot uniquely place a sparse roll across a full seat cycle", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [0, 2]);
    appendPublicDiceRoll(state, roll("r0-or-r4", "P0", 8, 2));

    expect(() =>
      buildLiveDecisionStochasticInput(
        "balanced",
        state,
        ["P0", "P1", "P2", "P3"],
        5,
      ),
    ).toThrow(/does not reconcile with public turn progress/);
  });

  it("preserves complete provenance when a persisted prefix reconnects to a contiguous suffix", () => {
    const first = createDiceHistoryState();
    observeLogCoverage(first, [0, 1, 2]);
    appendPublicDiceRoll(first, roll("r1", "P0", 8, 1));
    const restored = restoreDiceHistoryState(serializeDiceHistoryState(first));

    observeLogCoverage(restored, [3, 4, 5]);
    appendPublicDiceRoll(restored, roll("r5", "P1", 9, 5));

    expect(restored.provenance).toBe("complete-from-first-gameplay-roll");
    expect(restored.rolls.map(({ total }) => total)).toEqual([8, 9]);
    expect(restored.coverage.ranges).toEqual([[0, 5]]);
  });

  it("produces a deterministic digest and changes it when public stochastic evidence changes", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [0, 1, 2]);
    appendPublicDiceRoll(state, roll("r1", "P0", 8, 1));
    appendPublicDiceRoll(state, roll("r2", "P1", 7, 2));

    const identical = cloneDiceHistoryState(state);
    const changed = cloneDiceHistoryState(state);
    changed.rolls[1] = { ...changed.rolls[1]!, total: 6 };

    expect(diceHistoryDigest(state)).toBe(diceHistoryDigest(identical));
    expect(diceHistoryDigest(state)).not.toBe(diceHistoryDigest(changed));
  });

  it("does not let generic partial-history state downgrade independently complete dice coverage", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [0, 1, 2]);
    appendPublicDiceRoll(state, roll("r2", "P0", 10, 2));
    const genericPartialHistory = true;

    expect(genericPartialHistory).toBe(true);
    expect(state.provenance).toBe("complete-from-first-gameplay-roll");
  });

  it("persists roll-capable parser ambiguity until that exact log index is semantically resolved", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [0]);
    appendPublicDiceRoll(state, roll("r0", "P0", 8, 0));
    noteRollCapableLogAmbiguity(state, 1);

    expect(state.ambiguousLogIndices).toEqual([1]);
    expect(state.provenance).toBe("gapped");
    expect(state.hasUnknownRollGap).toBe(true);

    const restored = restoreDiceHistoryState(serializeDiceHistoryState(state));
    expect(restored.ambiguousLogIndices).toEqual([1]);
    expect(restored.hasUnlocatedRollAmbiguity).toBe(false);
    expect(restored.provenance).toBe("gapped");

    observeLogCoverage(restored, [1]);
    expect(restored.ambiguousLogIndices).toEqual([]);
    expect(restored.coverage.ranges).toEqual([[0, 1]]);
    expect(restored.provenance).toBe("complete-from-first-gameplay-roll");
  });

  it("represents missing-prefix and known middle rolls in stochastic ordinals", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [0, 1, 2, 3]);
    noteMissingPublicRoll(state);
    appendPublicDiceRoll(state, roll("r1", "P0", 8, 1));
    noteMissingPublicRoll(state);
    appendPublicDiceRoll(state, roll("r3", "P1", 6, 3));

    expect(state.provenance).toBe("gapped");
    expect(state.missingPrefixRolls).toBe(1);
    expect(state.gaps).toEqual([{ afterOrdinal: 1, missingRolls: 1 }]);
    expect(publicRollObservations(state, ["P0", "P1"])).toEqual([
      { ordinal: 1, actor: 0, total: 8 },
      { ordinal: 3, actor: 1, total: 6 },
    ]);
  });

  it("selects M_ref only for Balanced live decisions with usable canonical evidence", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [0]);
    appendPublicDiceRoll(state, roll("r0", "P0", 8, 0));

    expect(
      buildLiveDecisionStochasticInput("balanced", state, ["P0", "P1"]),
    ).toMatchObject({
      model: "mref-colonist-linked-2024-v1",
      playerMapping: ["P0", "P1"],
      rolls: [{ ordinal: 0, actor: 0, total: 8 }],
      provenance: "complete-from-first-gameplay-roll",
    });
    expect(
      buildLiveDecisionStochasticInput("random", undefined, undefined),
    ).toEqual({ model: "m0-fair-iid-2d6-v1" });
    expect(
      buildLiveDecisionStochasticInput("unknown", undefined, undefined),
    ).toEqual({ model: "m0-fair-iid-2d6-v1" });
  });

  it("fails closed instead of downgrading Balanced live decisions without usable M_ref authority", () => {
    const unavailable = createDiceHistoryState();
    expect(() =>
      buildLiveDecisionStochasticInput("balanced", unavailable, ["P0", "P1"]),
    ).toThrow(/usable public reference-dice history/);

    observeLogCoverage(unavailable, [0]);
    expect(() =>
      buildLiveDecisionStochasticInput("balanced", unavailable, undefined),
    ).toThrow(/canonical engine player ordering/);
  });

  it("rejects totals outside the public 2..12 domain", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [0]);
    expect(() => appendPublicDiceRoll(state, roll("bad", "P0", 13, 0))).toThrow(
      /2\.\.12/,
    );
  });
});
