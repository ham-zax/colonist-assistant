import { describe, expect, it } from "vitest";

import { emptyResources } from "../src/core/resources";
import {
  outgoingTradeDisposition,
  tradeMemoryScopeChanged,
  unansweredIncomingTrades,
} from "../src/core/trade-guard";
import { localTradeBundles } from "../src/core/trades";

describe("live trade guard", () => {
  it("derives local trade bundles from creator-relative storage", () => {
    const creatorGive = { ...emptyResources(), lumber: 1 };
    const creatorReceive = { ...emptyResources(), brick: 1 };

    expect(
      localTradeBundles({ creatorGive, creatorReceive, incoming: true }),
    ).toEqual({ give: creatorReceive, receive: creatorGive });
    expect(
      localTradeBundles({ creatorGive, creatorReceive, incoming: false }),
    ).toEqual({ give: creatorGive, receive: creatorReceive });
  });

  it("closes rejected offers immediately and unanswered offers after the watchdog", () => {
    expect(outgoingTradeDisposition(true, 1_000, 1_001)).toBe("cancel");
    expect(outgoingTradeDisposition(false, 1_000, 18_999)).toBe("wait");
    expect(outgoingTradeDisposition(false, 1_000, 19_000)).toBe("cancel");
  });

  it("keeps rejection memory through transient trade UI turn flags", () => {
    expect(
      tradeMemoryScopeChanged(
        { gameKey: "g", currentPlayer: "You", isMyTurn: true },
        { gameKey: "g", currentPlayer: "You", isMyTurn: false },
      ),
    ).toBe(false);
    expect(
      tradeMemoryScopeChanged(
        { gameKey: "g", currentPlayer: "You", isMyTurn: false },
        { gameKey: "g", currentPlayer: "Bot", isMyTurn: false },
      ),
    ).toBe(true);
  });

  it("resets rejection memory on a new turn without a current-player label", () => {
    expect(
      tradeMemoryScopeChanged(
        { gameKey: "g", turn: 8, isMyTurn: false },
        { gameKey: "g", turn: 9, isMyTurn: false },
      ),
    ).toBe(true);
    expect(
      tradeMemoryScopeChanged(
        { gameKey: "g", isMyTurn: false },
        { gameKey: "g", isMyTurn: true },
      ),
    ).toBe(true);
  });

  it("does not re-analyse an incoming offer after its response workflow completed", () => {
    const incoming = {
      id: "incoming-1",
      creator: "Bot",
      tradeExecutor: "Bot",
      creatorGive: emptyResources(),
      creatorReceive: emptyResources(),
      incoming: true,
      counterOffer: false,
      canAccept: true,
      myResponse: "pending" as const,
    };

    expect(unansweredIncomingTrades([incoming], new Set())).toEqual([
      incoming,
    ]);
    expect(
      unansweredIncomingTrades([incoming], new Set(["incoming-1"])),
    ).toEqual([]);
  });
});
