import { describe, expect, it } from "vitest";
import { MaritimeCycleMemory } from "../src/core/maritime-cycle";
import { maritimeCycleObservation, maritimeCycleRootExclusions } from "../src/core/maritime-cycle-board";
import { makeBoard, publicPlayer, resources } from "./helpers/deep-search-fixtures";

const board = (hand = resources({ lumber: 3, grain: 3 })) => makeBoard(["You", "Rival"], {
  gameKey: "sea8653",
  ownHand: hand,
  players: {
    You: publicPlayer({ tradeRatios: resources({ lumber: 3, brick: 3, wool: 3, grain: 3, ore: 3 }), cardDiscardLimit: 9 }),
    Rival: publicPlayer({ handSize: 4 }),
  },
  buildableRoadIds: [],
  buildableSettlementIds: [],
  buildableCityIds: [],
  localSeatDiagnostics: {
    seatSource: "gameController.myColor+currentUserId+gameUserStates",
    localActionState: 0,
    identity: { status: "resolved", reason: "cross-checked", source: "controller+account-user-id+store-roster", currentUserIdAvailable: true, currentUserMatchColors: [1] },
  },
});

describe("bank cycle board integration", () => {
  it("emits a vector root exclusion for a confirmed dominated reversal", () => {
    const memory = new MaritimeCycleMemory();
    const before = board();
    const after = board(resources({ lumber: 4 }));
    memory.observe(maritimeCycleObservation(before)!);
    memory.observe(maritimeCycleObservation(after)!);
    expect(maritimeCycleRootExclusions(memory, after)).toContainEqual({
      kind: "maritime-trade",
      give: resources({ lumber: 3 }),
      receive: resources({ grain: 1 }),
    });
  });

  it("preserves the recorded reverse leg when it restores a development purchase", () => {
    const memory = new MaritimeCycleMemory();
    const before = board(resources({ lumber: 2, wool: 1, grain: 3, ore: 1 }));
    const after = board(resources({ lumber: 3, wool: 1, ore: 1 }));
    memory.observe(maritimeCycleObservation(before)!);
    memory.observe(maritimeCycleObservation(after)!);
    expect(maritimeCycleRootExclusions(memory, after)).toEqual([]);
  });

  it("rejects domestic-shaped transfers when the rival hand total changes", () => {
    const memory = new MaritimeCycleMemory();
    const before = board();
    const after = board(resources({ lumber: 4 }));
    after.players!.Rival!.handSize = 6;
    memory.observe(maritimeCycleObservation(before)!);
    memory.observe(maritimeCycleObservation(after)!);
    expect(maritimeCycleRootExclusions(memory, after)).toEqual([]);
  });

  it("requires resolved identity and known build target sets", () => {
    const memory = new MaritimeCycleMemory();
    const before = board();
    const after = board(resources({ lumber: 4 }));
    memory.observe(maritimeCycleObservation(before)!);
    memory.observe(maritimeCycleObservation(after)!);
    expect(maritimeCycleRootExclusions(memory, { ...after, buildableRoadIds: undefined })).toEqual([]);
    expect(maritimeCycleObservation({ ...after, localSeatDiagnostics: undefined })).toBeUndefined();
  });
});
