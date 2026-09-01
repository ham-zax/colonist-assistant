import { describe, expect, it } from "vitest";

import {
  snapshotActiveTrades,
  tradeBeliefEventsFromDiff,
} from "../src/core/trade-beliefs";
import type { ActiveTradeOffer } from "../src/core/placement";
import {
  createTrackerState,
  effectiveParticleCount,
  reconcilePublicResourceEvidence,
  reduceTracker,
  reweightTradeEvidence,
  seedPublicResourceWorlds,
} from "../src/core/tracker";

const resources = (
  lumber: number,
  brick: number,
  wool: number,
  grain: number,
  ore: number,
) => ({ lumber, brick, wool, grain, ore });

const acceptedTradePosterior = () => {
  let state = createTrackerState();
  for (const player of ["Trader", "Responder"]) {
    state = reduceTracker(state, { type: "discover", player });
  }
  const unsupportedHands = [
    resources(0, 0, 5, 0, 0),
    resources(0, 0, 4, 1, 0),
    resources(0, 0, 4, 0, 1),
    resources(0, 0, 3, 2, 0),
    resources(0, 0, 3, 1, 1),
    resources(0, 0, 3, 0, 2),
    resources(0, 0, 2, 3, 0),
    resources(0, 0, 2, 2, 1),
    resources(0, 0, 2, 1, 2),
    resources(0, 0, 2, 0, 3),
    resources(0, 0, 1, 4, 0),
  ];
  state.worlds = [
    {
      hands: {
        Trader: resources(1, 0, 0, 0, 0),
        Responder: resources(0, 1, 4, 0, 0),
      },
      weight: 1 / 12,
    },
    ...unsupportedHands.map((hand) => ({
      hands: {
        Trader: resources(1, 0, 0, 0, 0),
        Responder: hand,
      },
      weight: 1 / 12,
    })),
  ];
  return reweightTradeEvidence(state, [
    {
      id: "accepted-hard-evidence",
      creator: "Trader",
      give: resources(1, 0, 0, 0, 0),
      receive: resources(0, 1, 0, 0, 0),
      acceptedPlayers: ["Responder"],
    },
  ]);
};

describe("tracker posterior integrity", () => {
  it("keeps accepted-trade tails at their Bayesian mass instead of rejuvenating them", () => {
    const state = acceptedTradePosterior();
    const supported = state.worlds.filter(
      (world) => (world.hands.Responder?.brick ?? 0) >= 1,
    );
    const unsupported = state.worlds.filter(
      (world) => (world.hands.Responder?.brick ?? 0) === 0,
    );
    const supportedMass = supported.reduce(
      (sum, world) => sum + world.weight,
      0,
    );
    const unsupportedMass = unsupported.reduce(
      (sum, world) => sum + world.weight,
      0,
    );
    const maximumUnsupportedWeight = Math.max(
      0,
      ...unsupported.map((world) => world.weight),
    );

    expect(supportedMass).toBeCloseTo(0.9889001, 6);
    expect(unsupportedMass).toBeCloseTo(0.0110999, 6);
    expect(maximumUnsupportedWeight).toBeCloseTo(0.00100908, 6);
    expect(effectiveParticleCount(state)).toBeLessThan(1.03);
    expect(
      state.worlds.reduce((sum, world) => sum + world.weight, 0),
    ).toBeCloseTo(1, 8);
  });

  it("is probability-idempotent when normalization sees no new evidence", () => {
    const once = reconcilePublicResourceEvidence(acceptedTradePosterior(), {});
    const twice = reconcilePublicResourceEvidence(once, {});

    expect(twice.worlds.map((world) => world.weight)).toEqual(
      once.worlds.map((world) => world.weight),
    );
    expect(effectiveParticleCount(twice)).toBeCloseTo(
      effectiveParticleCount(once),
      10,
    );
  });

  it("does not condition the posterior twice for an unchanged active trade", () => {
    let state = createTrackerState();
    for (const player of ["You", "Rival"]) {
      state = reduceTracker(state, { type: "discover", player });
    }
    state.worlds = [
      {
        hands: {
          You: resources(0, 1, 0, 0, 0),
          Rival: resources(1, 0, 0, 0, 0),
        },
        weight: 0.5,
      },
      {
        hands: {
          You: resources(0, 1, 0, 0, 0),
          Rival: resources(0, 1, 0, 0, 0),
        },
        weight: 0.5,
      },
    ];
    const trade: ActiveTradeOffer = {
      id: "incoming-idempotent",
      creator: "Rival",
      tradeExecutor: "Rival",
      creatorGive: resources(1, 0, 0, 0, 0),
      creatorReceive: resources(0, 1, 0, 0, 0),
      incoming: true,
      counterOffer: false,
      canAccept: true,
      pendingPlayers: ["You"],
    };
    const snapshot = snapshotActiveTrades([trade]);
    for (const event of tradeBeliefEventsFromDiff(new Map(), snapshot)) {
      state = reduceTracker(state, event);
    }
    const once = state.worlds.map((world) => world.weight);
    const duplicateEvents = tradeBeliefEventsFromDiff(snapshot, snapshot);
    for (const event of duplicateEvents) state = reduceTracker(state, event);

    expect(duplicateEvents).toEqual([]);
    expect(state.worlds.map((world) => world.weight)).toEqual(once);
  });

  it("seeds deterministic midgame public-resource worlds without replacement", () => {
    const playerOrder = ["You", "Rival A", "Rival B", "Rival C"];
    const handSizes = {
      You: 9,
      "Rival A": 8,
      "Rival B": 8,
      "Rival C": 7,
    };
    const ownHand = resources(2, 2, 1, 2, 2);
    const input = {
      playerOrder,
      ownPlayer: "You",
      exactOwnHand: ownHand,
      handSizes,
      resourceSupply: 19,
      seed: 54,
      sampleCount: 24,
    };

    const first = seedPublicResourceWorlds(input);
    const second = seedPublicResourceWorlds(input);

    expect(first).toEqual(second);
    expect(first.length).toBeGreaterThan(0);
    expect(first.length).toBeLessThanOrEqual(24);
    expect(first.reduce((sum, world) => sum + world.weight, 0)).toBeCloseTo(1, 10);
    for (const world of first) {
      expect(world.hands.You).toEqual(ownHand);
      for (const player of playerOrder) {
        expect(
          Object.values(world.hands[player] ?? {}).reduce(
            (sum, count) => sum + count,
            0,
          ),
        ).toBe(handSizes[player as keyof typeof handSizes]);
      }
      for (const resource of ["lumber", "brick", "wool", "grain", "ore"] as const) {
        const held = playerOrder.reduce(
          (sum, player) => sum + (world.hands[player]?.[resource] ?? 0),
          0,
        );
        expect(held).toBeLessThanOrEqual(19);
      }
    }
  });

  it("matches the enumerable public-bank conditional within the sample budget", () => {
    const worlds = seedPublicResourceWorlds({
      playerOrder: ["You", "A", "B"],
      ownPlayer: "You",
      exactOwnHand: resources(1, 0, 0, 0, 0),
      handSizes: { You: 1, A: 1, B: 1 },
      bank: resources(0, 0, 1, 0, 1),
      resourceSupply: 1,
      seed: 17,
      sampleCount: 200,
    });
    const aHasBrick = worlds
      .filter((world) => (world.hands.A?.brick ?? 0) === 1)
      .reduce((sum, world) => sum + world.weight, 0);
    const aHasGrain = worlds
      .filter((world) => (world.hands.A?.grain ?? 0) === 1)
      .reduce((sum, world) => sum + world.weight, 0);

    expect(aHasBrick).toBeCloseTo(0.5, 2);
    expect(aHasGrain).toBeCloseTo(0.5, 2);
    expect(worlds.reduce((sum, world) => sum + world.weight, 0)).toBeCloseTo(1, 10);
  });

  it("rejects fallback snapshots whose public slots exceed physical supply", () => {
    expect(() =>
      seedPublicResourceWorlds({
        playerOrder: ["You", "Rival"],
        ownPlayer: "You",
        exactOwnHand: resources(1, 0, 0, 0, 0),
        handSizes: { You: 1, Rival: 5 },
        bank: resources(0, 0, 0, 0, 0),
        resourceSupply: 1,
        seed: 1,
        sampleCount: 16,
      }),
    ).toThrow(/physical resource supply/i);
  });

  it("systematically compacts more than MAX_WORLDS without injecting tail support", () => {
    let state = createTrackerState();
    for (const player of ["Trader", "Responder", "Observer"]) {
      state = reduceTracker(state, { type: "discover", player });
    }
    const tailWeight = 0.01 / 4_999;
    state.worlds = Array.from({ length: 5_000 }, (_, index) => {
      let value = index;
      const observer = resources(0, 0, 0, 0, 0);
      for (const resource of ["lumber", "brick", "wool", "grain", "ore"] as const) {
        observer[resource] = value % 9;
        value = Math.floor(value / 9);
      }
      return {
        hands: {
          Trader: resources(1, 0, 0, 0, 0),
          Responder: resources(0, 0, 0, 0, 0),
          Observer: observer,
        },
        weight: index === 0 ? 0.99 : tailWeight,
      };
    });

    state = reduceTracker(state, {
      type: "unknown-transfer",
      from: "Trader",
      to: "Responder",
      count: 1,
    });

    const dominant = state.worlds.find((world) =>
      Object.values(world.hands.Observer ?? {}).every((count) => count === 0),
    );
    const quantum = 1 / 4_096;

    expect(state.possibilitiesTruncated).toBe(true);
    expect(state.worlds.length).toBeLessThanOrEqual(4_096);
    expect(dominant?.weight).toBeCloseTo(0.99, 3);
    expect(Math.abs((dominant?.weight ?? 0) - 0.99)).toBeLessThanOrEqual(quantum);
    expect(
      state.worlds.reduce((sum, world) => sum + world.weight, 0),
    ).toBeCloseTo(1, 8);
  });
});
