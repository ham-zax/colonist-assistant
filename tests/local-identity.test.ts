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

let sendMessage = vi.fn();

const players = [
  { color: 1, name: "RedPlayer", userId: 101 },
  { color: 2, name: "BluePlayer", userId: 202 },
  { color: 3, name: "OrangePlayer", userId: 303 },
  { color: 4, name: "GreenPlayer", userId: 404 },
];

describe("local seat identity", () => {
  beforeEach(() => {
    sendMessage = vi.fn((message: { id: number }) =>
      Promise.resolve({
        id: message.id,
        runtime: "background-wasm",
        engineRevision: "test-engine",
        initializationMs: 1,
      }),
    );
    vi.stubGlobal("chrome", {
      runtime: {
        getURL: (path: string) => `chrome-extension://fixture/${path}`,
        getManifest: () => ({ version: "0.9.1" }),
        sendMessage,
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

  it("preserves pendingPlacement across transient unresolved identity until resolved advanced snapshot clears it", () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );

    const edge: BoardSnapshot["edges"][number] = {
      id: "e:1",
      vertices: ["v:1", "v:2"],
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

    const resolvedRoadBoard: BoardSnapshot = {
      hexes: [],
      vertices: [],
      edges: [edge],
      gameKey: "game-42",
      turn: 3,
      currentPlayer: "RedPlayer",
      isMyTurn: true,
      myPlayer: "RedPlayer",
      action: "road",
      playerOrder: ["RedPlayer", "BluePlayer", "OrangePlayer", "GreenPlayer"],
      localSeatDiagnostics: resolvedIdentity,
    };

    // 1. Initial resolved road phase
    overlay.updateBoard(resolvedRoadBoard);

    // Register pendingPlacement for edge e:1 and record hand before
    const pending: PendingBoardPlacement = {
      action: "road",
      targetId: "e:1",
      point: { x: 0, y: 0 },
      gameKey: "game-42",
      startedAt: Date.now(),
    };
    (overlay as unknown as { pendingPlacement: PendingBoardPlacement | undefined }).pendingPlacement = pending;
    const handBefore = { brick: 1, lumber: 1, wool: 0, grain: 0, ore: 0 };
    (overlay as unknown as { pendingPlacementHandBefore: unknown }).pendingPlacementHandBefore = handBefore;

    // 2. Unresolved board in the same game (bridge publishes action: "none")
    overlay.updateBoard({
      ...resolvedRoadBoard,
      action: "none",
      isMyTurn: false,
      localSeatDiagnostics: unresolvedIdentity,
    });
    // Trigger render() to verify render-side check also preserves pendingPlacement
    (overlay as unknown as { render: () => void }).render();

    // Assert pendingPlacement and handBefore still exist across unresolved phase
    const pendingDuringUnresolved = (
      overlay as unknown as { pendingPlacement: PendingBoardPlacement | undefined }
    ).pendingPlacement;
    expect(pendingDuringUnresolved).toBeDefined();
    expect(pendingDuringUnresolved?.targetId).toBe("e:1");
    const handDuringUnresolved = (
      overlay as unknown as { pendingPlacementHandBefore: unknown }
    ).pendingPlacementHandBefore;
    expect(handDuringUnresolved).toEqual(handBefore);

    // 3. Resolved advanced snapshot arrives (action advanced from "road" to "none", edge now owned)
    const advancedResolvedBoard: BoardSnapshot = {
      ...resolvedRoadBoard,
      action: "none",
      edges: [{ ...edge, player: "RedPlayer" }],
      localSeatDiagnostics: resolvedIdentity,
    };
    overlay.updateBoard(advancedResolvedBoard);
    (overlay as unknown as { render: () => void }).render();

    // Assert pendingPlacement and handBefore are now cleared
    const pendingAfterAdvance = (
      overlay as unknown as { pendingPlacement: PendingBoardPlacement | undefined }
    ).pendingPlacement;
    expect(pendingAfterAdvance).toBeUndefined();
    const handAfterAdvance = (
      overlay as unknown as { pendingPlacementHandBefore: unknown }
    ).pendingPlacementHandBefore;
    expect(handAfterAdvance).toBeUndefined();

    overlay.destroy();
  });

  it("preserves queuedPlacement across transient unresolved identity until resolved turn loss or action change", () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );

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
      currentPlayer: "RedPlayer",
      isMyTurn: true,
      myPlayer: "RedPlayer",
      action: "none",
      playerOrder: ["RedPlayer", "BluePlayer", "OrangePlayer", "GreenPlayer"],
      localSeatDiagnostics: resolvedIdentity,
    };

    overlay.updateBoard(baseBoard);

    // Set queuedPlacement (e.g. remembered build target e:12 before opening placement modal)
    (overlay as unknown as { queuedPlacement: unknown }).queuedPlacement = {
      gameKey: "game-42",
      action: "road",
      targetId: "e:12",
      point: { x: 10, y: 20 },
    };

    // Unresolved snapshot: bridge sets isMyTurn: false, action: "none"
    overlay.updateBoard({
      ...baseBoard,
      isMyTurn: false,
      action: "none",
      localSeatDiagnostics: unresolvedIdentity,
    });

    // Assert queuedPlacement is preserved across unresolved identity
    const queuedDuringUnresolved = (
      overlay as unknown as { queuedPlacement: unknown }
    ).queuedPlacement;
    expect(queuedDuringUnresolved).toEqual({
      gameKey: "game-42",
      action: "road",
      targetId: "e:12",
      point: { x: 10, y: 20 },
    });

    // Resolved board with road placement modal active: queuedPlacement remains
    overlay.updateBoard({
      ...baseBoard,
      action: "road",
      localSeatDiagnostics: resolvedIdentity,
    });
    expect(
      (overlay as unknown as { queuedPlacement: unknown }).queuedPlacement,
    ).toBeDefined();

    // Resolved board where turn is lost: queuedPlacement now cleared
    overlay.updateBoard({
      ...baseBoard,
      isMyTurn: false,
      currentPlayer: "BluePlayer",
      turn: 4,
      localSeatDiagnostics: resolvedIdentity,
    });
    expect(
      (overlay as unknown as { queuedPlacement: unknown }).queuedPlacement,
    ).toBeUndefined();

    overlay.destroy();
  });

  it("reuses a queued placement after identity recovery without starting a new search", async () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    const internals = overlay as unknown as {
      board?: BoardSnapshot;
      queuedPlacement?: {
        gameKey?: string;
        action: "road" | "settlement" | "city";
        targetId: string;
        point: { x: number; y: number };
      };
      activeSpatial?: { recommendation: { id: string } };
      retainedPlacementTarget: (board: BoardSnapshot) => string | undefined;
      reconciledState: () => ReturnType<typeof createTrackerState> | undefined;
      nextClick: (
        state: ReturnType<typeof createTrackerState> | undefined,
        spatial: unknown,
        report: undefined,
      ) => { kind: string; targetId?: string } | undefined;
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
    const roadEdge: BoardSnapshot["edges"][number] = {
      id: "e:queued",
      vertices: ["v:1", "v:2"],
      screen: { x: 20, y: 20 },
    };
    const baseBoard: BoardSnapshot = {
      hexes: [],
      vertices: [
        { id: "v:1", adjacentHexes: [], adjacentVertices: ["v:2"] },
        { id: "v:2", adjacentHexes: [], adjacentVertices: ["v:1"] },
      ],
      edges: [roadEdge],
      gameKey: "game-queued",
      turn: 3,
      currentPlayer: "RedPlayer",
      isMyTurn: true,
      myPlayer: "RedPlayer",
      action: "none",
      playerOrder: ["RedPlayer", "BluePlayer"],
      players: {
        RedPlayer: {
          handSize: 2,
          tradeRatios: { brick: 4, lumber: 4, wool: 4, grain: 4, ore: 4 },
          cardDiscardLimit: 7,
        },
        BluePlayer: {
          handSize: 2,
          tradeRatios: { brick: 4, lumber: 4, wool: 4, grain: 4, ore: 4 },
          cardDiscardLimit: 7,
        },
      },
      ownHand: { brick: 1, lumber: 1, wool: 0, grain: 0, ore: 0 },
      buildableRoadIds: ["e:queued"],
      localSeatDiagnostics: resolvedIdentity,
    };
    overlay.updateBoard(baseBoard);
    internals.queuedPlacement = {
      gameKey: "game-queued",
      action: "road",
      targetId: "e:queued",
      point: { x: 20, y: 20 },
    };

    overlay.updateBoard({
      ...baseBoard,
      isMyTurn: false,
      action: "none",
      localSeatDiagnostics: unresolvedIdentity,
    });
    await Promise.resolve();
    await Promise.resolve();
    sendMessage.mockClear();

    overlay.updateBoard({
      ...baseBoard,
      action: "road",
      legalEdgeIds: ["e:queued"],
      localSeatDiagnostics: resolvedIdentity,
    });
    await Promise.resolve();
    await Promise.resolve();

    const recoveredBoard = (overlay as unknown as { board?: BoardSnapshot }).board;
    expect(internals.queuedPlacement?.targetId).toBe("e:queued");
    expect(recoveredBoard && internals.retainedPlacementTarget(recoveredBoard)).toBe(
      "e:queued",
    );
    const state = internals.reconciledState();
    const next = internals.nextClick(state, internals.activeSpatial, undefined);

    expect(
      sendMessage.mock.calls.filter(
        ([message]) => message.type === "colonist-assistant:decision",
      ),
    ).toHaveLength(0);
    expect(internals.activeSpatial?.recommendation.id).toBe("e:queued");
    expect(next).toMatchObject({ kind: "board", targetId: "e:queued" });

    overlay.destroy();
  });

  it("requires an authoritative legal set before reusing a queued placement", () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    const internals = overlay as unknown as {
      queuedPlacement?: {
        gameKey?: string;
        action: "road" | "settlement" | "city";
        targetId: string;
        point: { x: number; y: number };
      };
      retainedPlacementTarget: (board: BoardSnapshot) => string | undefined;
    };
    internals.queuedPlacement = {
      gameKey: "game-legal-set",
      action: "road",
      targetId: "e:queued",
      point: { x: 20, y: 20 },
    };
    const board: BoardSnapshot = {
      hexes: [],
      vertices: [],
      edges: [{ id: "e:queued", vertices: ["v:1", "v:2"] }],
      gameKey: "game-legal-set",
      turn: 3,
      currentPlayer: "RedPlayer",
      isMyTurn: true,
      myPlayer: "RedPlayer",
      action: "road",
      playerOrder: ["RedPlayer", "BluePlayer"],
    };

    expect(internals.retainedPlacementTarget(board)).toBeUndefined();

    overlay.destroy();
  });

  it("preserves searched free-road and robber-victim continuations across unresolved identity", () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    const internals = overlay as unknown as {
      freeRoadPlan?: { gameKey?: string; edgeIds: string[] };
      robberVictimPlan?: { gameKey?: string; turn?: number; player: string };
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
    const board: BoardSnapshot = {
      hexes: [],
      vertices: [],
      edges: [],
      gameKey: "game-continuation",
      turn: 8,
      currentPlayer: "RedPlayer",
      isMyTurn: true,
      myPlayer: "RedPlayer",
      action: "road",
      playerOrder: ["RedPlayer", "BluePlayer"],
      localSeatDiagnostics: resolvedIdentity,
    };
    overlay.updateBoard(board);
    internals.freeRoadPlan = {
      gameKey: "game-continuation",
      edgeIds: ["e:first", "e:second"],
    };
    internals.robberVictimPlan = {
      gameKey: "game-continuation",
      turn: 8,
      player: "BluePlayer",
    };

    overlay.updateBoard({
      ...board,
      isMyTurn: false,
      action: "none",
      localSeatDiagnostics: unresolvedIdentity,
    });

    expect(internals.freeRoadPlan).toEqual({
      gameKey: "game-continuation",
      edgeIds: ["e:first", "e:second"],
    });
    expect(internals.robberVictimPlan).toEqual({
      gameKey: "game-continuation",
      turn: 8,
      player: "BluePlayer",
    });

    overlay.destroy();
  });

  it("clears exact placement continuations when the public turn advances while identity is unresolved", () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    const internals = overlay as unknown as {
      queuedPlacement?: {
        gameKey?: string;
        action: "road" | "settlement" | "city";
        targetId: string;
        point: { x: number; y: number };
      };
      freeRoadPlan?: { gameKey?: string; edgeIds: string[] };
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
      rawPlayOrderColors: [1, 2],
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
      rawPlayOrderColors: [1, 2],
      seatSource: "unresolved",
    };
    const board: BoardSnapshot = {
      hexes: [],
      vertices: [],
      edges: [],
      gameKey: "game-turn-scope",
      turn: 3,
      currentPlayer: "RedPlayer",
      isMyTurn: true,
      myPlayer: "RedPlayer",
      action: "none",
      playerOrder: ["RedPlayer", "BluePlayer"],
      localSeatDiagnostics: resolvedIdentity,
    };
    overlay.updateBoard(board);
    internals.queuedPlacement = {
      gameKey: "game-turn-scope",
      action: "road",
      targetId: "e:old",
      point: { x: 10, y: 20 },
    };
    internals.freeRoadPlan = {
      gameKey: "game-turn-scope",
      edgeIds: ["e:first", "e:second"],
    };

    overlay.updateBoard({
      ...board,
      turn: 4,
      currentPlayer: "BluePlayer",
      isMyTurn: false,
      action: "none",
      localSeatDiagnostics: unresolvedIdentity,
    });

    expect(internals.queuedPlacement).toBeUndefined();
    expect(internals.freeRoadPlan).toBeUndefined();

    overlay.destroy();
  });

  it("uses the searched robber victim after identity recovers", () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    const internals = overlay as unknown as {
      robberVictimPlan?: { gameKey?: string; turn?: number; player: string };
      nextClick: (
        state: undefined,
        spatial: undefined,
        report: undefined,
      ) => { kind: string; player?: string } | undefined;
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
      rawPlayOrderColors: [1, 2, 3],
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
      rawPlayOrderColors: [1, 2, 3],
      seatSource: "unresolved",
    };
    const board: BoardSnapshot = {
      hexes: [],
      vertices: [],
      edges: [],
      gameKey: "game-robber",
      turn: 9,
      currentPlayer: "RedPlayer",
      isMyTurn: true,
      myPlayer: "RedPlayer",
      action: "robber",
      playerOrder: ["RedPlayer", "BluePlayer", "OrangePlayer"],
      localSeatDiagnostics: resolvedIdentity,
    };
    overlay.updateBoard(board);
    internals.robberVictimPlan = {
      gameKey: "game-robber",
      turn: 9,
      player: "BluePlayer",
    };

    overlay.updateBoard({
      ...board,
      isMyTurn: false,
      action: "none",
      localSeatDiagnostics: unresolvedIdentity,
    });
    overlay.updateBoard({
      ...board,
      action: "none",
      robberVictimSelection: true,
      robberVictimPlayers: ["BluePlayer", "OrangePlayer"],
      localSeatDiagnostics: resolvedIdentity,
    });

    expect(internals.nextClick(undefined, undefined, undefined)).toMatchObject({
      kind: "player",
      player: "BluePlayer",
    });

    overlay.destroy();
  });

  it("does not seed a new-game seven protocol from the previous game board", () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
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
      rawPlayOrderColors: [1, 2],
      seatSource: "gameController.myColor+currentUserId+gameUserStates",
    };
    overlay.updateBoard({
      hexes: [
        { id: "h:old", blocked: true },
        { id: "h:new", blocked: false },
      ],
      vertices: [],
      edges: [],
      gameKey: "old-game",
      turn: 7,
      currentPlayer: "BluePlayer",
      isMyTurn: false,
      action: "none",
      playerOrder: ["RedPlayer", "BluePlayer"],
      localSeatDiagnostics: resolvedIdentity,
    });

    overlay.updateBoard({
      hexes: [
        { id: "h:old", blocked: false },
        { id: "h:new", blocked: true },
      ],
      vertices: [],
      edges: [],
      gameKey: "new-game",
      turn: 1,
      currentPlayer: "RedPlayer",
      isMyTurn: true,
      myPlayer: "RedPlayer",
      hasRolled: true,
      lastRoll: 7,
      action: "robber",
      playerOrder: ["RedPlayer", "BluePlayer"],
      localSeatDiagnostics: resolvedIdentity,
    });

    expect(
      (overlay as unknown as {
        localSevenProtocol?: { robberHexBefore?: string };
      }).localSevenProtocol?.robberHexBefore,
    ).toBe("h:new");

    overlay.destroy();
  });

  it("preserves localSevenProtocol across transient unresolved identity during roll of 7", () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );

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

    const hex1: BoardSnapshot["hexes"][number] = {
      id: "h:1",
      blocked: true,
    };
    const hex2: BoardSnapshot["hexes"][number] = {
      id: "h:2",
      blocked: false,
    };

    const preRollBoard: BoardSnapshot = {
      hexes: [hex1, hex2],
      vertices: [],
      edges: [],
      gameKey: "game-42",
      turn: 5,
      currentPlayer: "RedPlayer",
      isMyTurn: true,
      myPlayer: "RedPlayer",
      hasRolled: false,
      action: "none",
      playerOrder: ["RedPlayer", "BluePlayer", "OrangePlayer", "GreenPlayer"],
      localSeatDiagnostics: resolvedIdentity,
    };

    // 1. Initial pre-roll board
    overlay.updateBoard(preRollBoard);

    // 2. Rolled 7, robber action active
    const postRoll7Board: BoardSnapshot = {
      ...preRollBoard,
      hasRolled: true,
      lastRoll: 7,
      action: "robber",
      localSeatDiagnostics: resolvedIdentity,
    };
    overlay.updateBoard(postRoll7Board);

    const protocolEstablished = (
      overlay as unknown as { localSevenProtocol: unknown }
    ).localSevenProtocol;
    expect(protocolEstablished).toBeDefined();

    // 3. Transient unresolved snapshot: bridge sets isMyTurn: false, action: "none"
    overlay.updateBoard({
      ...postRoll7Board,
      isMyTurn: false,
      action: "none",
      localSeatDiagnostics: unresolvedIdentity,
    });

    // Assert protocol is NOT destroyed while unresolved
    const protocolDuringUnresolved = (
      overlay as unknown as { localSevenProtocol: unknown }
    ).localSevenProtocol;
    expect(protocolDuringUnresolved).toBeDefined();

    // 4. Identity resolves again with robber still pending
    overlay.updateBoard(postRoll7Board);
    expect(
      (overlay as unknown as { localSevenProtocol: unknown }).localSevenProtocol,
    ).toBeDefined();

    overlay.destroy();
  });
});
