import { describe, expect, it } from "vitest";
import { createTrackerState, reduceTracker } from "../src/core/tracker";
import { emptyResources } from "../src/core/resources";

describe("trade negotiation belief events", () => {
  it("records durable offer/reject evidence and opponent model counters", () => {
    let state = createTrackerState();
    state = reduceTracker(state, { type: "discover", player: "Alice" });
    state = reduceTracker(state, { type: "discover", player: "Bob" });
    state = reduceTracker(state, {
      type: "gain",
      player: "Alice",
      cards: { ...emptyResources(), lumber: 2, brick: 1 },
      reason: "starting",
    });
    state = reduceTracker(state, {
      type: "gain",
      player: "Bob",
      cards: { ...emptyResources(), grain: 2, ore: 1 },
      reason: "starting",
    });

    state = reduceTracker(state, {
      type: "trade-offered",
      player: "Alice",
      recipients: ["Bob"],
      give: { ...emptyResources(), lumber: 1 },
      receive: { ...emptyResources(), grain: 1 },
    });
    expect(state.players.Alice?.opponentModel.offersMade).toBe(1);

    state = reduceTracker(state, {
      type: "trade-rejected",
      player: "Bob",
      creator: "Alice",
      give: { ...emptyResources(), lumber: 1 },
      receive: { ...emptyResources(), grain: 1 },
    });
    expect(state.players.Bob?.opponentModel.tradeRejects).toBe(2);
    expect(state.players.Bob?.opponentModel.policyPosterior.tradeResistant).toBeGreaterThan(
      state.players.Bob?.opponentModel.policyPosterior.tradeFlexible ?? 0,
    );

    state = reduceTracker(state, {
      type: "trade-countered",
      player: "Bob",
      creator: "Alice",
      give: { ...emptyResources(), lumber: 1 },
      receive: { ...emptyResources(), grain: 1 },
      counterGive: { ...emptyResources(), ore: 1 },
      counterReceive: { ...emptyResources(), brick: 1 },
    });
    expect(state.players.Bob?.opponentModel.countersMade).toBe(1);
  });
});
