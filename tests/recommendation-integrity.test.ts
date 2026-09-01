import { describe, expect, it } from "vitest";

import { AssistantOverlay } from "../src/content/overlay";
import { createCoachReport } from "../src/core/coach";
import { parseLogSnapshot } from "../src/core/parser";
import {
  scoreRoadPlacements,
  scoreRobberPlacements,
  type BoardSnapshot,
} from "../src/core/placement";
import { createTrackerState, reduceTracker } from "../src/core/tracker";
import type { TrackerState } from "../src/core/types";
import {
  buildDeepSearchRequest,
  selectRepresentativeWorlds,
} from "../src/worker/deep-search";
import {
  incomingTrade,
  makeBoard,
  makeState,
  publicPlayer,
  resources,
} from "./helpers/deep-search-fixtures";

const decisionSignature = (
  state: TrackerState,
  board: BoardSnapshot,
  player: string,
): string => {
  const method = (AssistantOverlay.prototype as unknown as {
    decisionSignature: (
      state: TrackerState,
      board: BoardSnapshot,
      player: string,
    ) => string;
  }).decisionSignature;
  return method.call(
    { settings: { engine: "deep-search" } },
    state,
    board,
    player,
  );
};

describe("recommendation state integrity", () => {
  it("fills every requested representative-world slot", () => {
    const worlds = [
      { weight: 0.6, hands: { a: resources({ lumber: 3 }) } },
      { weight: 0.2, hands: { a: resources({ lumber: 3, brick: 1 }) } },
      { weight: 0.15, hands: { a: resources() } },
      { weight: 0.03, hands: { a: resources({ brick: 1 }) } },
      { weight: 0.02, hands: { a: resources({ wool: 1 }) } },
    ];

    expect(selectRepresentativeWorlds(worlds, ["a"], 4)).toHaveLength(4);
  });

  it("rejects an unresolved root player instead of optimizing seat zero", () => {
    const names = ["a", "b"];
    expect(() =>
      buildDeepSearchRequest(makeState(names), makeBoard(names), "ghost"),
    ).toThrow(/root player|unknown player|not found/iu);
  });

  it("rejects an unresolved current player instead of advancing seat zero", () => {
    const names = ["a", "b"];
    const board = makeBoard(names, {
      isMyTurn: false,
      currentPlayer: "ghost",
    });

    expect(() => buildDeepSearchRequest(makeState(names), board, "a")).toThrow(
      /current player|unknown player|not found/iu,
    );
  });

  it("rejects games above four players instead of truncating them", () => {
    const names = ["a", "b", "c", "d", "e"];
    expect(() =>
      buildDeepSearchRequest(makeState(names), makeBoard(names), "a"),
    ).toThrow(/2.*4 player|supports.*4|unsupported/iu);
  });

  it("rejects unknown trade creators instead of aliasing them to seat zero", () => {
    const names = ["a", "b"];
    const board = makeBoard(names, {
      activeTrades: [incomingTrade("ghost")],
    });

    expect(() => buildDeepSearchRequest(makeState(names), board, "a")).toThrow(
      /trade creator|unknown player|not found/iu,
    );
  });

  it("rejects unknown trade response participants", () => {
    const names = ["a", "b"];
    const board = makeBoard(names, {
      activeTrades: [
        incomingTrade("a", {
          incoming: false,
          tradeExecutor: "a",
          acceptedPlayers: ["ghost"],
          pendingPlayers: [],
          rejectedPlayers: [],
          responsesComplete: true,
          myResponse: undefined,
        }),
      ],
    });

    expect(() => buildDeepSearchRequest(makeState(names), board, "a")).toThrow(
      /trade player|unknown player|not found/iu,
    );
  });

  it("rejects malformed topology instead of aliasing an unknown endpoint to vertex zero", () => {
    const names = ["a", "b"];
    const board = makeBoard(names, {
      vertices: [
        { id: "v0", adjacentHexes: ["h0"], adjacentVertices: ["v1"] },
        { id: "v1", adjacentHexes: ["h0"], adjacentVertices: ["v0"] },
      ],
      edges: [{ id: "e0", vertices: ["v0", "missing"] }],
    });

    expect(() => buildDeepSearchRequest(makeState(names), board, "a")).toThrow(
      /edge|vertex|endpoint|topology/iu,
    );
  });

  it("rejects unknown adjacent hex references", () => {
    const names = ["a", "b"];
    const board = makeBoard(names, {
      vertices: [
        {
          id: "v0",
          adjacentHexes: ["h0", "missing-hex"],
          adjacentVertices: [],
        },
      ],
    });

    expect(() => buildDeepSearchRequest(makeState(names), board, "a")).toThrow(
      /hex|vertex|adjacent|topology/iu,
    );
  });

  it("rejects unknown adjacent vertex references", () => {
    const names = ["a", "b"];
    const board = makeBoard(names, {
      vertices: [
        {
          id: "v0",
          adjacentHexes: ["h0"],
          adjacentVertices: ["missing-vertex"],
        },
      ],
    });

    expect(() => buildDeepSearchRequest(makeState(names), board, "a")).toThrow(
      /vertex|adjacent|topology/iu,
    );
  });

  it("rejects a missing robber location instead of assuming hex zero", () => {
    const names = ["a", "b"];
    const board = makeBoard(names, {
      hexes: [
        { id: "h0", resource: "lumber", number: 6 },
        { id: "h1", resource: "brick", number: 8 },
      ],
    });

    expect(() => buildDeepSearchRequest(makeState(names), board, "a")).toThrow(
      /robber|blocked hex|location/iu,
    );
  });

  it("rejects multiple robber locations instead of taking the first", () => {
    const names = ["a", "b"];
    const board = makeBoard(names, {
      hexes: [
        { id: "h0", resource: "lumber", number: 6, blocked: true },
        { id: "h1", resource: "brick", number: 8, blocked: true },
      ],
    });

    expect(() => buildDeepSearchRequest(makeState(names), board, "a")).toThrow(
      /robber|blocked hex|location/iu,
    );
  });

  it("passes live rule variants into the native search state", () => {
    const names = ["a", "b"];
    const board = makeBoard(names, {
      friendlyRobber: true,
      players: {
        a: publicPlayer({ cardDiscardLimit: 9 }),
        b: publicPlayer({ cardDiscardLimit: 9 }),
      },
    });

    const built = buildDeepSearchRequest(makeState(names), board, "a");
    expect(built.request.state.cardDiscardLimit).toBe(9);
    expect(built.request.state.friendlyRobber).toBe(true);
  });

  it("returns a pre-roll robber continuation to pre-roll", () => {
    const names = ["a", "b"];
    const built = buildDeepSearchRequest(
      makeState(names),
      makeBoard(names, { action: "robber", hasRolled: false }),
      "a",
    );

    expect(built.request.state.phase).toBe("move-robber");
    expect(built.request.state.robberReturnPhase).toBe("pre-roll");
  });

  it("encodes every public discard obligation", () => {
    const names = ["a", "b"];
    const board = makeBoard(names, {
      action: "discard",
      discardCount: 4,
      ownHand: resources({ lumber: 8 }),
      players: {
        a: publicPlayer({ handSize: 8 }),
        b: publicPlayer({ handSize: 10 }),
      },
    });

    const built = buildDeepSearchRequest(makeState(names), board, "a");
    expect(built.request.state.discardRemaining.slice(0, 2)).toEqual([4, 5]);
  });

  it("does not create resource worlds that exceed a visible bank supply", () => {
    const names = ["a", "b"];
    const board = makeBoard(names, {
      bank: resources({ lumber: 19, brick: 19, wool: 19, grain: 19, ore: 19 }),
      bankVisible: true,
      players: {
        a: publicPlayer({ handSize: 0 }),
        b: publicPlayer({ handSize: 1 }),
      },
    });

    const built = buildDeepSearchRequest(makeState(names), board, "a");
    for (const world of built.request.state.worlds) {
      for (let resource = 0; resource < 5; resource += 1) {
        const held = world.hands.reduce(
          (sum, hand) => sum + (hand[resource] ?? 0),
          0,
        );
        expect(held + (world.bank[resource] ?? 0)).toBeLessThanOrEqual(19);
      }
    }
  });

  it("changes the decision cache key when eventCount changes the search seed", () => {
    const names = ["a", "b"];
    const firstState = makeState(names);
    const secondState = makeState(names);
    secondState.eventCount += 1;
    const board = makeBoard(names);

    expect(buildDeepSearchRequest(firstState, board, "a").request.seed).not.toBe(
      buildDeepSearchRequest(secondState, board, "a").request.seed,
    );
    expect(decisionSignature(firstState, board, "a")).not.toBe(
      decisionSignature(secondState, board, "a"),
    );
  });

  it("changes the decision cache key when the robber moves", () => {
    const names = ["a", "b"];
    const state = makeState(names);
    const firstBoard = makeBoard(names, {
      hexes: [
        { id: "h0", resource: "lumber", number: 6, blocked: true },
        { id: "h1", resource: "brick", number: 8, blocked: false },
      ],
    });
    const secondBoard = makeBoard(names, {
      hexes: [
        { id: "h0", resource: "lumber", number: 6, blocked: false },
        { id: "h1", resource: "brick", number: 8, blocked: true },
      ],
    });

    expect(decisionSignature(state, firstBoard, "a")).not.toBe(
      decisionSignature(state, secondBoard, "a"),
    );
  });

  it("changes the decision cache key when opponent policy changes", () => {
    const names = ["a", "b"];
    const firstState = makeState(names);
    const secondState = makeState(names);
    secondState.players.b!.opponentModel.policyPosterior = {
      balanced: 0.02,
      expansion: 0.83,
      cityDevelopment: 0.05,
      tradeFlexible: 0.05,
      tradeResistant: 0.05,
    };
    const board = makeBoard(names);

    expect(decisionSignature(firstState, board, "a")).not.toBe(
      decisionSignature(secondState, board, "a"),
    );
  });

  it("changes the decision cache key when posterior world weights change", () => {
    const names = ["a", "b"];
    const firstState = makeState(names);
    const secondState = makeState(names);
    firstState.worlds = [
      {
        weight: 0.9,
        hands: { a: resources(), b: resources({ lumber: 1 }) },
      },
      {
        weight: 0.1,
        hands: { a: resources(), b: resources({ brick: 1 }) },
      },
    ];
    secondState.worlds = [
      {
        weight: 0.1,
        hands: { a: resources(), b: resources({ lumber: 1 }) },
      },
      {
        weight: 0.9,
        hands: { a: resources(), b: resources({ brick: 1 }) },
      },
    ];
    const board = makeBoard(names, {
      players: {
        a: publicPlayer({ handSize: 0 }),
        b: publicPlayer({ handSize: 1 }),
      },
    });

    expect(decisionSignature(firstState, board, "a")).not.toBe(
      decisionSignature(secondState, board, "a"),
    );
  });
});

describe("live recommendation rules", () => {
  it("resolves a generic Dice roll to the uniquely matching player color", () => {
    let state = createTrackerState();
    state = reduceTracker(state, {
      type: "discover",
      player: "a",
      color: "rgb(255, 0, 0)",
    });
    state = reduceTracker(state, {
      type: "discover",
      player: "b",
      color: "rgb(0, 0, 255)",
    });

    const parsed = parseLogSnapshot({
      visibleText: "Dice rolled",
      serialText: "Dice rolled :die-4: :die-2:",
      color: "rgb(255, 0, 0)",
      language: "en",
    });
    expect(parsed).toBeDefined();

    const next = reduceTracker(state, parsed!.event);
    expect(next.currentTurn.player).toBe("a");
    expect(next.players.Dice).toBeUndefined();
    expect(next.playerOrder).toEqual(["a", "b"]);
  });

  it("respects a live discard limit of nine in coaching", () => {
    const names = ["a", "b"];
    const hand = resources({ lumber: 2, brick: 2, wool: 1, grain: 2, ore: 1 });
    const state = makeState(names);
    state.worlds = [
      { weight: 1, hands: { a: hand, b: resources() } },
    ];
    const board = makeBoard(names, {
      ownHand: hand,
      players: {
        a: publicPlayer({ handSize: 8, cardDiscardLimit: 9 }),
        b: publicPlayer({ handSize: 0, cardDiscardLimit: 9 }),
      },
    });

    const report = createCoachReport(state, "a", board);
    expect(report).toBeDefined();
    expect(report!.alerts.some((alert) => /seven risk/iu.test(alert))).toBe(false);
    expect(
      report!.primary.reasons.some((reason) => /seven-roll exposure/iu.test(reason)),
    ).toBe(false);
  });

  it("forbids a Friendly Robber hex occupied by any protected player", () => {
    const names = ["a", "b", "c"];
    const board = makeBoard(names, {
      friendlyRobber: true,
      players: {
        a: publicPlayer({ visiblePoints: 3 }),
        b: publicPlayer({ visiblePoints: 2 }),
        c: publicPlayer({ visiblePoints: 4 }),
      },
      hexes: [
        { id: "old", resource: "ore", number: 5, blocked: true },
        { id: "mixed", resource: "grain", number: 8 },
        { id: "legal", resource: "brick", number: 6 },
      ],
      vertices: [
        {
          id: "v0",
          adjacentHexes: ["mixed"],
          adjacentVertices: [],
          building: { player: "b", kind: "settlement" },
        },
        {
          id: "v1",
          adjacentHexes: ["mixed"],
          adjacentVertices: [],
          building: { player: "c", kind: "settlement" },
        },
        {
          id: "v2",
          adjacentHexes: ["legal"],
          adjacentVertices: [],
          building: { player: "c", kind: "settlement" },
        },
      ],
      edges: [],
    });

    const recommendations = scoreRobberPlacements(board, { player: "a" });
    expect(recommendations.some((item) => item.id === "mixed")).toBe(false);
    expect(recommendations.some((item) => item.id === "legal")).toBe(true);
  });

  it("uses exact graph length when public Longest Road is a stale zero", () => {
    const names = ["a", "b"];
    const vertices = Array.from({ length: 6 }, (_, index) => ({
      id: `v${index}`,
      adjacentHexes: [] as string[],
      adjacentVertices: [
        ...(index > 0 ? [`v${index - 1}`] : []),
        ...(index < 5 ? [`v${index + 1}`] : []),
      ],
    }));
    const edges = Array.from({ length: 5 }, (_, index) => ({
      id: `e${index}`,
      vertices: [`v${index}`, `v${index + 1}`] as [string, string],
      ...(index < 4 ? { player: "a" } : {}),
    }));
    const board = makeBoard(names, {
      vertices,
      edges,
      legalEdgeIds: ["e4"],
      players: {
        a: publicPlayer({ visiblePoints: 2, longestRoad: 0 }),
        b: publicPlayer({ visiblePoints: 2, longestRoad: 0 }),
      },
    });

    const recommendation = scoreRoadPlacements(board, {
      player: "a",
      legalEdgeIds: ["e4"],
      requireConnection: true,
    })[0];

    expect(recommendation).toBeDefined();
    expect(recommendation!.metrics?.purpose).toBe("longest-road");
    expect(recommendation!.metrics?.strategicallyUseful).toBe(true);
    expect(recommendation!.score).toBe(18);
  });
});
