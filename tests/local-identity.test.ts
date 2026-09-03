import { describe, expect, it } from "vitest";

import { CompactGameBuilder } from "../src/core/llm-record";
import { resolveLocalIdentity } from "../src/core/local-identity";
import type { BoardSnapshot } from "../src/core/placement";

const players = [
  { color: 1, name: "RedPlayer", userId: 101 },
  { color: 2, name: "BluePlayer", userId: 202 },
  { color: 3, name: "OrangePlayer", userId: 303 },
  { color: 4, name: "GreenPlayer", userId: 404 },
];

describe("local seat identity", () => {
  it("resolves only when controller color and current account identify the same seat", () => {
    expect(
      resolveLocalIdentity({
        myColor: 1,
        mappedMyPlayer: "RedPlayer",
        playOrder: [1, 2, 3, 4],
        players,
        currentUserId: 101,
      }),
    ).toMatchObject({
      status: "resolved",
      reason: "cross-checked",
      source: "controller+account-user-id+store-roster",
      currentUserIdAvailable: true,
      currentUserMatchColors: [1],
      myColor: 1,
      myPlayer: "RedPlayer",
      currentUserColor: 1,
    });
  });

  it("fails closed when manager discovery falls back to a cached instance", () => {
    expect(
      resolveLocalIdentity({
        myColor: 1,
        mappedMyPlayer: "RedPlayer",
        playOrder: [1, 2, 3, 4],
        players,
        currentUserId: 101,
        managerResolutionSource: "cached-fallback",
      }),
    ).toMatchObject({
      status: "unresolved",
      reason: "manager-cached-fallback",
      source: "none",
      currentUserIdAvailable: true,
      currentUserMatchColors: [1],
      myColor: 1,
      myPlayer: "RedPlayer",
    });
  });

  it("fails closed when the current account belongs to a different color", () => {
    expect(
      resolveLocalIdentity({
        myColor: 3,
        mappedMyPlayer: "OrangePlayer",
        playOrder: [1, 2, 3, 4],
        players,
        currentUserId: 101,
      }),
    ).toMatchObject({
      status: "unresolved",
      reason: "controller-color-account-color-disagreement",
      source: "none",
      currentUserIdAvailable: true,
      currentUserMatchColors: [1],
      myColor: 3,
      myPlayer: "OrangePlayer",
      currentUserColor: 1,
      currentUserPlayer: "RedPlayer",
    });
  });

  it("fails closed when color-to-name mapping disagrees with the store roster", () => {
    expect(
      resolveLocalIdentity({
        myColor: 1,
        mappedMyPlayer: "WrongPlayer",
        playOrder: [1, 2, 3, 4],
        players,
        currentUserId: 101,
      }),
    ).toMatchObject({
      status: "unresolved",
      reason: "color-name-mapping-disagreement",
      source: "none",
      currentUserIdAvailable: true,
      currentUserMatchColors: [1],
      myColor: 1,
      myPlayer: "WrongPlayer",
    });
  });

  it("keeps a bounded history when identity evidence changes", () => {
    const builder = new CompactGameBuilder();
    const captureBase = {
      scope: "seat-diagnostic",
      sessionId: "session",
      gameKey: "game",
      startedAt: 1_000,
      partialHistory: false,
      unmatchedCount: 0,
      assistant: {
        engine: "weighted",
        disablePlayerTrades: false,
        autopilot: false,
      },
      events: [],
      decisions: [],
    };
    const board = (
      observedAt: number,
      identity: BoardSnapshot["localSeatDiagnostics"],
    ): BoardSnapshot => ({
      hexes: [],
      vertices: [],
      edges: [],
      playerOrder: ["RedPlayer", "BluePlayer", "OrangePlayer", "GreenPlayer"],
      observedAt,
      localSeatDiagnostics: identity,
    });
    const resolved: NonNullable<BoardSnapshot["localSeatDiagnostics"]> = {
      identity: {
        status: "resolved",
        reason: "cross-checked",
        source: "controller+account-user-id+store-roster",
        currentUserIdAvailable: true,
        currentUserMatchColors: [1],
        myColor: 1,
        myPlayer: "RedPlayer",
        currentUserColor: 1,
        currentUserPlayer: "RedPlayer",
      },
      rawMyColor: 1,
      rawPlayOrderColors: [1, 2, 3, 4],
      seatSource: "gameController.myColor+currentUserId+gameUserStates",
    };
    const unresolved: NonNullable<BoardSnapshot["localSeatDiagnostics"]> = {
      identity: {
        status: "unresolved",
        reason: "controller-color-account-color-disagreement",
        source: "none",
        currentUserIdAvailable: true,
        currentUserMatchColors: [1],
        myColor: 3,
        myPlayer: "OrangePlayer",
        currentUserColor: 1,
        currentUserPlayer: "RedPlayer",
      },
      rawMyColor: 3,
      rawPlayOrderColors: [1, 2, 3, 4],
      seatSource: "unresolved",
    };

    builder.apply({ ...captureBase, board: board(1_100, resolved) }, false);
    builder.apply({ ...captureBase, board: board(1_200, resolved) }, false);
    const record = builder.apply(
      { ...captureBase, board: board(1_300, unresolved) },
      false,
    );

    expect(record.meta.localSeatIdentityHistory).toHaveLength(2);
    expect(record.meta.localSeatIdentityHistory?.map((entry) => entry.identity.reason)).toEqual([
      "cross-checked",
      "controller-color-account-color-disagreement",
    ]);
    expect(record.meta.localSeatIdentityHistory?.map((entry) => entry.capturedAt)).toEqual([
      1_100,
      1_300,
    ]);
  });
});
