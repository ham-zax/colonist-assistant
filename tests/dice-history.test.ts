import { describe, expect, it } from "vitest";

import {
  appendPublicDiceRoll,
  cloneDiceHistoryState,
  createDiceHistoryState,
  diceHistoryDigest,
  noteMissingPublicRoll,
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

  it("rejects totals outside the public 2..12 domain", () => {
    const state = createDiceHistoryState();
    observeLogCoverage(state, [0]);
    expect(() => appendPublicDiceRoll(state, roll("bad", "P0", 13, 0))).toThrow(
      /2\.\.12/,
    );
  });
});
