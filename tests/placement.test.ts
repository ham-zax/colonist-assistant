import { describe, expect, it } from "vitest";

import {
  applyConfirmedPlacement,
  placementConfirmedByPublicLog,
  placementHasAdvanced,
  placementIsAwaitingSync,
  type PendingBoardPlacement,
} from "../src/core/board-progress";
import {
  legalRobberHexIds,
  openingRoadEdgeIds,
  openingSettlementMeetsProductionFloor,
  scoreRobberPlacements,
  type BoardSnapshot,
} from "../src/core/placement";
import { parseLogSnapshot } from "../src/core/parser";
import { createTrackerState, reduceTracker } from "../src/core/tracker";

describe("opening road legality", () => {
  it("anchors the second opening road to the newly placed settlement", () => {
    const board = {
      hexes: [],
      vertices: [
        {
          id: "old",
          adjacentHexes: [],
          adjacentVertices: ["middle"],
          building: { player: "You", kind: "settlement" },
        },
        {
          id: "middle",
          adjacentHexes: [],
          adjacentVertices: ["old", "new"],
        },
        {
          id: "new",
          adjacentHexes: [],
          adjacentVertices: ["middle", "coast"],
          building: { player: "You", kind: "settlement" },
        },
        {
          id: "coast",
          adjacentHexes: [],
          adjacentVertices: ["new"],
        },
      ],
      edges: [
        { id: "old-road", vertices: ["old", "middle"], player: "You" },
        { id: "new-road", vertices: ["new", "middle"] },
        { id: "coast-road", vertices: ["new", "coast"] },
      ],
    } satisfies BoardSnapshot;

    expect(openingRoadEdgeIds(board, "You")).toEqual([
      "new-road",
      "coast-road",
    ]);
  });

  it("refuses to guess when the opening anchor is ambiguous", () => {
    const board = {
      hexes: [],
      vertices: [
        {
          id: "a",
          adjacentHexes: [],
          adjacentVertices: ["b"],
          building: { player: "You", kind: "settlement" },
        },
        {
          id: "b",
          adjacentHexes: [],
          adjacentVertices: ["a"],
          building: { player: "You", kind: "settlement" },
        },
      ],
      edges: [{ id: "ab", vertices: ["a", "b"] }],
    } satisfies BoardSnapshot;

    expect(openingRoadEdgeIds(board, "You")).toBeUndefined();
  });
});

describe("Colonist spatial legality and opening sanity", () => {
  it("excludes every friendly-robber hex touching a protected player", () => {
    const board = {
      hexes: [
        { id: "current", resource: "ore", number: 5, blocked: true },
        { id: "protected", resource: "grain", number: 6 },
        { id: "mixed", resource: "brick", number: 8 },
        { id: "open", resource: "lumber", number: 10 },
        { id: "desert" },
      ],
      vertices: [
        {
          id: "rival-low",
          adjacentHexes: ["protected", "mixed"],
          adjacentVertices: [],
          building: { player: "Rival", kind: "settlement" },
        },
        {
          id: "rival-high",
          adjacentHexes: ["mixed"],
          adjacentVertices: [],
          building: { player: "Leader", kind: "city" },
        },
      ],
      players: {
        Rival: {
          handSize: 3,
          tradeRatios: { lumber: 4, brick: 4, wool: 4, grain: 4, ore: 4 },
          cardDiscardLimit: 9,
          visiblePoints: 2,
        },
        Leader: {
          handSize: 5,
          tradeRatios: { lumber: 4, brick: 4, wool: 4, grain: 4, ore: 4 },
          cardDiscardLimit: 9,
          visiblePoints: 4,
        },
      },
      friendlyRobber: true,
    } satisfies Pick<
      BoardSnapshot,
      "hexes" | "vertices" | "players" | "friendlyRobber"
    >;

    expect(legalRobberHexIds(board)).toEqual(["open", "desert"]);
    expect(
      scoreRobberPlacements(
        { ...board, edges: [], legalHexIds: ["open", "desert"] },
        { player: "You" },
      ).map((choice) => choice.id),
    ).toEqual(["open", "desert"]);
  });

  it("rejects a 5-pip generic-port opener when 12- and 13-pip sites are legal", () => {
    const board = {
      hexes: [
        { id: "weak-12", resource: "grain", number: 12 },
        { id: "weak-9", resource: "wool", number: 9 },
        { id: "strong-6", resource: "grain", number: 6 },
        { id: "strong-9", resource: "wool", number: 9 },
        { id: "strong-5", resource: "ore", number: 5 },
        { id: "brick-8", resource: "brick", number: 8 },
        { id: "brick-10", resource: "brick", number: 10 },
        { id: "wood-5", resource: "lumber", number: 5 },
      ],
      vertices: [
        {
          id: "weak-port",
          adjacentHexes: ["weak-12", "weak-9"],
          adjacentVertices: [],
          port: "generic",
        },
        {
          id: "grain-wool-ore",
          adjacentHexes: ["strong-6", "strong-9", "strong-5"],
          adjacentVertices: [],
        },
        {
          id: "brick-brick-wood",
          adjacentHexes: ["brick-8", "brick-10", "wood-5"],
          adjacentVertices: [],
        },
      ],
      edges: [],
      legalVertexIds: ["weak-port", "grain-wool-ore", "brick-brick-wood"],
      action: "settlement",
      initialPlacement: true,
    } satisfies BoardSnapshot;

    expect(openingSettlementMeetsProductionFloor(board, "weak-port")).toBe(
      false,
    );
    expect(
      openingSettlementMeetsProductionFloor(board, "grain-wool-ore"),
    ).toBe(true);
    expect(
      openingSettlementMeetsProductionFloor(board, "brick-brick-wood"),
    ).toBe(true);
  });
});

describe("placement state synchronization", () => {
  const pending: PendingBoardPlacement = {
    action: "settlement",
    targetId: "target",
    point: { x: 100, y: 200 },
    gameKey: "game-1",
    startedAt: 1_000,
  };
  const board = {
    hexes: [],
    vertices: [
      {
        id: "target",
        adjacentHexes: [],
        adjacentVertices: [],
      },
    ],
    edges: [],
    action: "settlement",
    gameKey: "game-1",
  } satisfies BoardSnapshot;

  it("suppresses a clicked target while Colonist catches up", () => {
    expect(placementIsAwaitingSync(pending, board, 2_000)).toBe(true);
    expect(placementHasAdvanced(pending, board)).toBe(false);
  });

  it("accepts either an occupied target or the next action as progress", () => {
    expect(
      placementHasAdvanced(pending, {
        ...board,
        vertices: [
          {
            ...board.vertices[0]!,
            building: { player: "You", kind: "settlement" },
          },
        ],
      }),
    ).toBe(true);
    expect(placementHasAdvanced(pending, { ...board, action: "road" })).toBe(
      true,
    );
  });

  it("uses a public settlement log to advance a stale opening prompt", () => {
    const event = {
      type: "spend" as const,
      player: "You",
      cost: { lumber: 1, brick: 1, wool: 1, grain: 1, ore: 0 },
      reason: "settlement" as const,
      id: "placed",
      timestamp: 1_200,
      raw: "You built a settlement",
    };
    const state = reduceTracker(createTrackerState(), event, event);
    expect(placementConfirmedByPublicLog(pending, state, "You")).toBe(true);

    const openingBoard = {
      ...board,
      initialPlacement: true,
      edges: [
        { id: "left", vertices: ["target", "left-end"] },
        { id: "right", vertices: ["target", "right-end"] },
      ],
      vertices: [
        ...board.vertices,
        {
          id: "left-end",
          adjacentHexes: [],
          adjacentVertices: ["target"],
        },
        {
          id: "right-end",
          adjacentHexes: [],
          adjacentVertices: ["target"],
        },
      ],
    } satisfies BoardSnapshot;
    const advanced = applyConfirmedPlacement(pending, openingBoard, "You");
    expect(advanced.action).toBe("road");
    expect(advanced.legalEdgeIds).toEqual(["left", "right"]);
    expect(advanced.vertices[0]?.building).toEqual({
      player: "You",
      kind: "settlement",
    });
  });

  it("tracks free opening placements without charging resources", () => {
    const parsed = parseLogSnapshot({
      visibleText: "You placed a settlement",
      serialText: "You placed a :settlement:",
      language: "en",
    });
    expect(parsed?.event).toEqual({
      type: "spend",
      player: "You",
      cost: { lumber: 0, brick: 0, wool: 0, grain: 0, ore: 0 },
      reason: "settlement",
    });
  });
});
