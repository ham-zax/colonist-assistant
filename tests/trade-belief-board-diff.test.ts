import { describe, expect, it } from "vitest";
import {
  snapshotActiveTrades,
  tradeBeliefEventsFromDiff,
} from "../src/core/trade-beliefs";
import { emptyResources } from "../src/core/resources";
import type { ActiveTradeOffer } from "../src/core/placement";

const offer = (
  overrides: Partial<ActiveTradeOffer> & Pick<ActiveTradeOffer, "id" | "creator">,
): ActiveTradeOffer => ({
  tradeExecutor: overrides.creator,
  give: { ...emptyResources(), lumber: 1 },
  receive: { ...emptyResources(), grain: 1 },
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
