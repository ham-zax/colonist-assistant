import { describe, expect, it } from "vitest";

import {
  createTrackerState,
  effectiveParticleCount,
  reduceTracker,
  reweightTradeEvidence,
} from "../src/core/tracker";
import { emptyResources } from "../src/core/resources";
import { selectRepresentativeWorlds } from "../src/worker/deep-search";

const resources = (
  lumber: number,
  brick: number,
  wool: number,
  grain: number,
  ore: number,
) => ({ lumber, brick, wool, grain, ore });

describe("weighted hidden-state filter", () => {
  it("weights an unknown steal by card multiplicity", () => {
    let state = createTrackerState();
    state = reduceTracker(state, { type: "discover", player: "Victim" });
    state = reduceTracker(state, { type: "discover", player: "Thief" });
    state.worlds = [
      {
        hands: {
          Victim: resources(0, 0, 0, 3, 1),
          Thief: emptyResources(),
        },
        weight: 1,
      },
    ];
    state = reduceTracker(state, {
      type: "unknown-transfer",
      from: "Victim",
      to: "Thief",
      count: 1,
    });

    const grain = state.worlds.find(
      (world) => world.hands.Thief?.grain === 1,
    );
    const ore = state.worlds.find(
      (world) => world.hands.Thief?.ore === 1,
    );
    expect(grain?.weight).toBeCloseTo(0.75, 6);
    expect(ore?.weight).toBeCloseTo(0.25, 6);
  });

  it("uses trade responses as Bayesian evidence and reports ESS", () => {
    let state = createTrackerState();
    for (const player of ["Trader", "Responder"]) {
      state = reduceTracker(state, { type: "discover", player });
    }
    state.worlds = [
      {
        hands: {
          Trader: resources(1, 0, 0, 0, 0),
          Responder: resources(0, 1, 0, 0, 0),
        },
        weight: 0.5,
      },
      {
        hands: {
          Trader: resources(1, 0, 0, 0, 0),
          Responder: resources(0, 0, 1, 0, 0),
        },
        weight: 0.5,
      },
    ];
    const before = effectiveParticleCount(state);
    state = reweightTradeEvidence(state, [
      {
        id: "trade",
        creator: "Trader",
        give: resources(1, 0, 0, 0, 0),
        receive: resources(0, 1, 0, 0, 0),
        acceptedPlayers: ["Responder"],
      },
    ]);

    expect(before).toBeCloseTo(2, 6);
    expect(state.worlds[0]?.hands.Responder?.brick).toBe(1);
    expect(state.worlds[0]?.weight).toBeGreaterThan(0.99);
    expect(effectiveParticleCount(state)).toBeLessThan(1.02);
  });

  it("treats an offer as soft surplus and bottleneck evidence", () => {
    let state = createTrackerState();
    for (const player of ["Trader", "Responder"]) {
      state = reduceTracker(state, { type: "discover", player });
    }
    state.worlds = [
      {
        hands: {
          Trader: resources(0, 0, 5, 0, 0),
          Responder: resources(0, 0, 0, 0, 1),
        },
        weight: 0.5,
      },
      {
        hands: {
          Trader: resources(0, 0, 2, 0, 4),
          Responder: resources(0, 0, 0, 0, 1),
        },
        weight: 0.5,
      },
    ];
    state = reweightTradeEvidence(state, [
      {
        id: "surplus-offer",
        creator: "Trader",
        give: resources(0, 0, 2, 0, 0),
        receive: resources(0, 0, 0, 0, 1),
      },
    ]);

    const woolSurplus = state.worlds.find(
      (world) => world.hands.Trader?.wool === 5,
    );
    const oreSurplus = state.worlds.find(
      (world) => world.hands.Trader?.ore === 4,
    );
    expect(woolSurplus?.weight).toBeGreaterThan(oreSurplus?.weight ?? 1);
    expect(woolSurplus?.weight).toBeLessThan(0.7);
  });

  it("retains posterior mass and strategic hand diversity when stratifying", () => {
    const worlds = Array.from({ length: 160 }, (_, index) => ({
      hands: {
        You: resources(1, 1, 1, 1, 0),
        Rival: resources(
          index % 5,
          (index * 2) % 5,
          (index * 3) % 4,
          (index * 5) % 6,
          (index * 7) % 5,
        ),
      },
      weight: index + 1,
    }));
    const selected = selectRepresentativeWorlds(
      worlds,
      ["You", "Rival"],
      48,
    );

    expect(selected.length).toBeLessThanOrEqual(48);
    expect(
      selected.reduce((sum, world) => sum + world.weight, 0),
    ).toBeCloseTo(1, 6);
    expect(
      new Set(
        selected.map((world) => {
          const hand = world.hands.Rival!;
          return [
            Number(hand.lumber >= 1 && hand.brick >= 1),
            Number(
              hand.lumber >= 1 &&
                hand.brick >= 1 &&
                hand.wool >= 1 &&
                hand.grain >= 1,
            ),
            Number(hand.grain >= 2 && hand.ore >= 3),
          ].join("");
        }),
    ).size,
    ).toBeGreaterThan(2);
  });

  it("rejuvenates collapsed beliefs only from already-valid particle support", () => {
    let state = createTrackerState();
    for (const player of ["Trader", "Responder"]) {
      state = reduceTracker(state, { type: "discover", player });
    }
    state.worlds = Array.from({ length: 12 }, (_, index) => ({
      hands: {
        Trader: resources(1, 0, 0, 0, 0),
        Responder:
          index === 0
            ? resources(0, 1, 0, 0, 0)
            : resources(0, 0, index, 0, 0),
      },
      weight: 1 / 12,
    }));
    const supported = new Set(
      state.worlds.map((world) => JSON.stringify(world.hands)),
    );
    state = reweightTradeEvidence(state, [
      {
        id: "near-collapse",
        creator: "Trader",
        give: resources(1, 0, 0, 0, 0),
        receive: resources(0, 1, 0, 0, 0),
        acceptedPlayers: ["Responder"],
      },
    ]);

    expect(
      state.worlds.every((world) =>
        supported.has(JSON.stringify(world.hands)),
      ),
    ).toBe(true);
    expect(
      new Set(state.worlds.map((world) => JSON.stringify(world.hands))).size,
    ).toBeGreaterThan(1);
  });
});
