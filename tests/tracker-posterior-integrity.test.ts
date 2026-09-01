import { describe, expect, it } from "vitest";

import {
  createTrackerState,
  effectiveParticleCount,
  reconcilePublicResourceEvidence,
  reduceTracker,
  reweightTradeEvidence,
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
