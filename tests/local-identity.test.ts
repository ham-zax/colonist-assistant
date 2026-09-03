// @vitest-environment jsdom

import { beforeEach, describe, expect, it, vi } from "vitest";

import { AssistantOverlay } from "../src/content/overlay";
import type { GameSession } from "../src/content/session";
import { DEFAULT_SETTINGS } from "../src/content/settings";
import { CompactGameBuilder } from "../src/core/llm-record";
import { resolveLocalIdentity } from "../src/core/local-identity";
import type { PendingBoardPlacement } from "../src/core/board-progress";
import type {
  ActiveTradeOffer,
  BoardSnapshot,
} from "../src/core/placement";
import { createTrackerState, reduceTracker } from "../src/core/tracker";

const players = [
  { color: 1, name: "RedPlayer", userId: 101 },
  { color: 2, name: "BluePlayer", userId: 202 },
  { color: 3, name: "OrangePlayer", userId: 303 },
  { color: 4, name: "GreenPlayer", userId: 404 },
];

describe("local seat identity", () => {
  beforeEach(() => {
    vi.stubGlobal("chrome", {
      runtime: {
        getURL: (path: string) => `chrome-extension://fixture/${path}`,
        getManifest: () => ({ version: "0.9.1" }),
        sendMessage: vi.fn().mockResolvedValue({}),
      },
      storage: {
        local: {
          get: () => Promise.resolve({}),
          set: () => Promise.resolve(),
          remove: () => Promise.resolve(),
        },
        sync: {
          set: () => Promise.resolve(),
        },
      },
    });
  });
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

  it("preserves active trade snapshots across transient unresolved identity without emitting expiry or re-offer", () => {
    const ingestEvents = vi.fn();
    const tracker = reduceTracker(createTrackerState(), {
      type: "discover",
      player: "RedPlayer",
    });
    const mockSession = {
      ingestEvents,
      state: tracker,
    } as unknown as GameSession;

    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    overlay.update(mockSession);

    const trade: ActiveTradeOffer = {
      id: "trade-42",
      creator: "BluePlayer",
      tradeExecutor: "BluePlayer",
      creatorGive: { brick: 1, lumber: 0, wool: 0, grain: 0, ore: 0 },
      creatorReceive: { brick: 0, lumber: 1, wool: 0, grain: 0, ore: 0 },
      incoming: true,
      counterOffer: false,
      canAccept: true,
      acceptedPlayers: [],
      pendingPlayers: [],
      rejectedPlayers: [],
    };

    const resolvedIdentity: NonNullable<BoardSnapshot["localSeatDiagnostics"]> = {
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

    const unresolvedIdentity: NonNullable<BoardSnapshot["localSeatDiagnostics"]> = {
      identity: {
        status: "unresolved",
        reason: "manager-cached-fallback",
        source: "none",
        currentUserIdAvailable: true,
        currentUserMatchColors: [1],
        myColor: 1,
        myPlayer: "RedPlayer",
      },
      rawMyColor: 1,
      rawPlayOrderColors: [1, 2, 3, 4],
      seatSource: "unresolved",
    };

    const baseBoard: BoardSnapshot = {
      hexes: [],
      vertices: [],
      edges: [],
      gameKey: "game-42",
      turn: 3,
      currentPlayer: "BluePlayer",
      isMyTurn: false,
      myPlayer: "RedPlayer",
      playerOrder: ["RedPlayer", "BluePlayer", "OrangePlayer", "GreenPlayer"],
      activeTrades: [trade],
      localSeatDiagnostics: resolvedIdentity,
    };

    // 1. Initial resolved board with active trade
    overlay.updateBoard(baseBoard);
    expect(ingestEvents).toHaveBeenCalledTimes(1);
    expect(ingestEvents.mock.calls[0]?.[0]).toEqual([
      expect.objectContaining({ type: "trade-offered" }),
    ]);
    ingestEvents.mockClear();

    // 2. Unresolved board in the same game and turn; bridge publishes empty activeTrades
    overlay.updateBoard({
      ...baseBoard,
      activeTrades: [],
      localSeatDiagnostics: unresolvedIdentity,
    });
    // Must NOT emit trade-expired
    expect(ingestEvents).not.toHaveBeenCalled();

    // 3. Identity resolves again with the same trade still active
    overlay.updateBoard({
      ...baseBoard,
      localSeatDiagnostics: resolvedIdentity,
    });
    // Must NOT re-emit trade-offered or trade-expired
    expect(ingestEvents).not.toHaveBeenCalled();

    overlay.destroy();
  });

  it("does not patch unresolved snapshots with confirmedPlacement", () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );

    const pending: PendingBoardPlacement = {
      action: "settlement",
      targetId: "v:1",
      point: { x: 0, y: 0 },
      startedAt: Date.now(),
      gameKey: "game-42",
    };

    (overlay as unknown as { confirmedPlacement: unknown }).confirmedPlacement = {
      pending,
      player: "RedPlayer",
      expiresAt: Date.now() + 10_000,
    };

    const vertex: BoardSnapshot["vertices"][number] = {
      id: "v:1",
      adjacentHexes: [],
      adjacentVertices: [],
    };

    const unresolvedBoard: BoardSnapshot = {
      hexes: [],
      vertices: [vertex],
      edges: [],
      gameKey: "game-42",
      turn: 3,
      currentPlayer: "RedPlayer",
      isMyTurn: false,
      playerOrder: ["RedPlayer", "BluePlayer", "OrangePlayer", "GreenPlayer"],
      localSeatDiagnostics: {
        identity: {
          status: "unresolved",
          reason: "manager-cached-fallback",
          source: "none",
          currentUserIdAvailable: true,
          currentUserMatchColors: [1],
        },
        seatSource: "unresolved",
      },
    };

    overlay.updateBoard(unresolvedBoard);

    const boardAfterUnresolved = (overlay as unknown as { board?: BoardSnapshot }).board;
    expect(boardAfterUnresolved).toBeDefined();
    const vertexAfter = boardAfterUnresolved?.vertices.find((v) => v.id === "v:1");
    // Assert unresolved snapshot was NOT patched with the confirmed settlement
    expect(vertexAfter?.building).toBeUndefined();

    overlay.destroy();
  });

  it("preserves contradiction in seat identity history across more than 32 runtime updates", () => {
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
      runtimeFields: Partial<NonNullable<BoardSnapshot["localSeatDiagnostics"]>> = {},
    ): BoardSnapshot => ({
      hexes: [],
      vertices: [],
      edges: [],
      playerOrder: ["RedPlayer", "BluePlayer", "OrangePlayer", "GreenPlayer"],
      observedAt,
      localSeatDiagnostics: identity ? { ...identity, ...runtimeFields } : undefined,
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
    const contradiction: NonNullable<BoardSnapshot["localSeatDiagnostics"]> = {
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

    // 1. Initial resolved snapshot
    builder.apply({ ...captureBase, board: board(1_000, resolved, { currentActorColor: 1, isMyTurn: true, managerPlacedPieceCount: 0 }) }, false);
    // 2. Transient unresolved contradiction near the beginning
    builder.apply({ ...captureBase, board: board(1_050, contradiction, { currentActorColor: 1, isMyTurn: true, managerPlacedPieceCount: 0 }) }, false);

    // 3. Feed more than 32 subsequent snapshots with unchanged resolved identity but changing runtime diagnostics
    let lastRecord: ReturnType<typeof builder.apply> | undefined;
    for (let i = 1; i <= 36; i++) {
      lastRecord = builder.apply(
        {
          ...captureBase,
          board: board(1_100 + i * 10, resolved, {
            currentActorColor: (i % 4) + 1,
            currentActorPlayer: ["RedPlayer", "BluePlayer", "OrangePlayer", "GreenPlayer"][i % 4],
            isMyTurn: i % 4 === 0,
            localActionState: i % 3,
            managerPlacedPieceCount: i,
            storePlacedPieceCount: i,
          }),
        },
        false,
      );
    }

    expect(lastRecord?.meta.localSeatIdentityHistory).toBeDefined();
    const reasons = lastRecord?.meta.localSeatIdentityHistory?.map((entry) => entry.identity.reason);
    expect(reasons).toContain("controller-color-account-color-disagreement");
    const contradictionEntry = lastRecord?.meta.localSeatIdentityHistory?.find(
      (entry) => entry.identity.reason === "controller-color-account-color-disagreement",
    );
    expect(contradictionEntry?.capturedAt).toBe(1_050);
  });
});
