import { describe, expect, it } from "vitest";

import {
  applyConfirmedPlacement,
  placementConfirmedByPublicLog,
  placementHasAdvanced,
  placementIsAwaitingSync,
  type PendingBoardPlacement,
} from "../src/core/board-progress";
import { openingRoadEdgeIds, type BoardSnapshot } from "../src/core/placement";
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
