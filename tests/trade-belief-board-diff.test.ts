import { describe, expect, it } from "vitest";
import {
  snapshotActiveTrades,
  tradeBeliefEventsFromDiff,
} from "../src/core/trade-beliefs";
import { emptyResources } from "../src/core/resources";
import { createTrackerState, reduceTracker } from "../src/core/tracker";
import type { ActiveTradeOffer } from "../src/core/placement";

const offer = (
  overrides: Partial<ActiveTradeOffer> & Pick<ActiveTradeOffer, "id" | "creator">,
): ActiveTradeOffer => ({
  tradeExecutor: overrides.creator,
  creatorGive: { ...emptyResources(), lumber: 1 },
  creatorReceive: { ...emptyResources(), grain: 1 },
  incoming: false,
  counterOffer: false,
  canAccept: true,
  pendingPlayers: ["Bob"],
  ...overrides,
});

describe("trade belief board diffs", () => {
  it("emits offer, reject, and expire events from active trade panels", () => {
    const first = snapshotActiveTrades([
      offer({ id: "t1", creator: "Alice", pendingPlayers: ["Bob"] }),
    ]);
    expect(tradeBeliefEventsFromDiff(new Map(), first)).toEqual([
      expect.objectContaining({
        type: "trade-offered",
        player: "Alice",
        recipients: ["Bob"],
      }),
    ]);

    const rejected = snapshotActiveTrades([
      offer({
        id: "t1",
        creator: "Alice",
        pendingPlayers: [],
        rejectedPlayers: ["Bob"],
      }),
    ]);
    expect(tradeBeliefEventsFromDiff(first, rejected)).toEqual([
      expect.objectContaining({
        type: "trade-rejected",
        player: "Bob",
        creator: "Alice",
      }),
    ]);

    expect(tradeBeliefEventsFromDiff(rejected, new Map())).toEqual([
      expect.objectContaining({
        type: "trade-expired",
        player: "Alice",
      }),
    ]);
  });

  it("conditions an incoming offer on the creator's offered resources exactly once", () => {
    let state = createTrackerState();
    for (const player of ["You", "Rival"]) {
      state = reduceTracker(state, { type: "discover", player });
    }
    state.worlds = [
      {
        hands: {
          You: { ...emptyResources(), brick: 1 },
          Rival: { ...emptyResources(), lumber: 1 },
        },
        weight: 0.5,
      },
      {
        hands: {
          You: { ...emptyResources(), brick: 1 },
          Rival: { ...emptyResources(), brick: 1 },
        },
        weight: 0.5,
      },
    ];
    const incoming = snapshotActiveTrades([
      offer({
        id: "incoming-orientation",
        creator: "Rival",
        incoming: true,
        pendingPlayers: ["You"],
        creatorGive: { ...emptyResources(), lumber: 1 },
        creatorReceive: { ...emptyResources(), brick: 1 },
      }),
    ]);
    const events = tradeBeliefEventsFromDiff(new Map(), incoming);
    for (const event of events) state = reduceTracker(state, event);

    const lumberMass = state.worlds
      .filter((world) => (world.hands.Rival?.lumber ?? 0) >= 1)
      .reduce((sum, world) => sum + world.weight, 0);
    const brickMass = state.worlds
      .filter((world) => (world.hands.Rival?.brick ?? 0) >= 1)
      .reduce((sum, world) => sum + world.weight, 0);

    expect(lumberMass).toBeGreaterThan(0.98);
    expect(brickMass).toBeLessThan(0.02);
    expect(tradeBeliefEventsFromDiff(incoming, incoming)).toEqual([]);
  });

  it("emits exactly one counter event when Colonist replaces the open offer", () => {
    const open = snapshotActiveTrades([
      offer({ id: "t-counter", creator: "Alice", pendingPlayers: ["Bob"] }),
    ]);
    const countered = snapshotActiveTrades([
      offer({
        id: "t-counter-response",
        creator: "Bob",
        counterOffer: true,
        counterOfferInResponseToTradeId: "t-counter",
        pendingPlayers: ["Alice"],
        creatorGive: { ...emptyResources(), ore: 1 },
        creatorReceive: { ...emptyResources(), grain: 1 },
      }),
    ]);

    expect(tradeBeliefEventsFromDiff(open, countered)).toEqual([
      expect.objectContaining({
        type: "trade-countered",
        player: "Bob",
        creator: "Alice",
        give: open.get("t-counter")!.creatorGive,
        receive: open.get("t-counter")!.creatorReceive,
        counterGive: countered.get("t-counter-response")!.creatorGive,
        counterReceive: countered.get("t-counter-response")!.creatorReceive,
      }),
    ]);
  });

  it("records accepts without treating completion as an expiry", () => {
    const open = snapshotActiveTrades([
      offer({ id: "t2", creator: "Alice", pendingPlayers: ["Bob"] }),
    ]);
    const accepted = snapshotActiveTrades([
      offer({
        id: "t2",
        creator: "Alice",
        pendingPlayers: [],
        acceptedPlayers: ["Bob"],
        responsesComplete: true,
      }),
    ]);
    expect(tradeBeliefEventsFromDiff(open, accepted)).toEqual([
      expect.objectContaining({
        type: "trade-accepted",
        player: "Bob",
        creator: "Alice",
      }),
    ]);
    expect(tradeBeliefEventsFromDiff(accepted, new Map())).toEqual([]);
  });
});
