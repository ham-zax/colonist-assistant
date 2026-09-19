// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GameSession } from "../src/content/session";
import { hashString } from "../src/content/dom";
import { AssistantOverlay, isSessionCompatibleWithBoard } from "../src/content/overlay";
import { DEFAULT_SETTINGS } from "../src/content/settings";
import {
  appendPublicDiceRoll,
  buildLiveDecisionStochasticInput,
  createDiceHistoryState,
  serializeDiceHistoryState,
} from "../src/core/dice-history";
import { sessionStorageKey } from "../src/core/local-data";
import type { BoardPlayerPublicState, BoardSnapshot } from "../src/core/placement";

const mockPublicPlayer = (
  overrides: Partial<BoardPlayerPublicState> = {},
): BoardPlayerPublicState => ({
  handSize: 0,
  tradeRatios: { lumber: 4, brick: 4, wool: 4, grain: 4, ore: 4 },
  cardDiscardLimit: 7,
  visiblePoints: 0,
  ...overrides,
});

const storage = new Map<string, unknown>();
const sessions: GameSession[] = [];
let overlay: AssistantOverlay | undefined;

beforeEach(() => {
  storage.clear();
  vi.stubGlobal("chrome", {
    runtime: {
      getURL: (path: string) => `chrome-extension://fixture/${path}`,
      getManifest: () => ({ version: "0.9.1" }),
      sendMessage: vi.fn(async (message: { id: number; type?: string }) => {
        if (message.type === "colonist-assistant:decision-status") {
          return {
            id: message.id,
            runtime: "background-wasm",
            engineRevision: "deep-maxn-v12",
            initializationMs: 1,
          };
        }
        if (message.type === "colonist-assistant:decision") {
          return {
            id: message.id,
            analysis: {
              engine: "deep-search",
              players: [],
              actionScores: { road: 0, settlement: 0, city: 0, development: 0 },
              simulations: 0,
              model: "test-fixture",
              runtime: "background-wasm",
            },
          };
        }
        return { id: message.id };
      }),
    },
    storage: {
      local: {
        get: vi.fn(async (key: string | null) =>
          key === null ? Object.fromEntries(storage) : { [key]: structuredClone(storage.get(key)) },
        ),
        set: vi.fn(async (values: Record<string, unknown>) => {
          for (const [key, value] of Object.entries(values)) storage.set(key, structuredClone(value));
        }),
        remove: vi.fn(async (keys: string | string[]) => {
          for (const key of typeof keys === "string" ? [keys] : keys) storage.delete(key);
        }),
      },
      sync: { set: async () => {} },
    },
  });
});

afterEach(async () => {
  overlay?.destroy();
  overlay = undefined;
  for (const session of sessions.splice(0)) session.stop();
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  document.body.replaceChildren();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const fourPlayerRoster = ["Merras5380", "hamzax", "Ruqaiyahhhhh", "Neda1311"];
const threePlayerRoster = ["Merras5380", "hamzax", "Ruqaiyahhhhh"];

const build4PStaleSession = async (root: HTMLElement, gameKey: string): Promise<GameSession> => {
  const session = new GameSession(root, vi.fn(), gameKey);
  sessions.push(session);
  await session.start();

  // Populate 4-player tracker state and rolls
  session.state.playerOrder = [...fourPlayerRoster];
  for (const player of fourPlayerRoster) {
    session.state.players[player] = {
      name: player,
      color: "#53a6e8",
      devCards: [],
      playedDevCards: {
        knight: 0,
        monopoly: 0,
        "road-building": 0,
        "year-of-plenty": 0,
        "victory-point": 0,
        unknown: 0,
      },
      builds: { road: 0, settlement: 0, city: 0, development: 0 },
      resourcesGained: { lumber: 0, brick: 0, wool: 0, grain: 0, ore: 0 },
      productionGained: { lumber: 0, brick: 0, wool: 0, grain: 0, ore: 0 },
      resourcesSpent: { lumber: 0, brick: 0, wool: 0, grain: 0, ore: 0 },
      opponentModel: {
        tradeAccepts: 1,
        tradeRejects: 1,
        offersMade: 0,
        countersMade: 0,
        policyPosterior: {
          balanced: 0.2,
          expansion: 0.2,
          cityDevelopment: 0.2,
          tradeFlexible: 0.2,
          tradeResistant: 0.2,
        },
      },
    };
  }

  // Add 104 historical rolls from the 4-player game
  for (let i = 0; i < 104; i++) {
    const actor = fourPlayerRoster[i % 4]!;
    appendPublicDiceRoll(session.diceHistory, {
      actor,
      total: 7,
      dice: [3, 4],
      eventId: `board-roll:${i}:${actor}`,
      logIndex: 30 + i,
    });
  }
  expect(session.diceHistory.rolls.length).toBe(104);
  return session;
};

describe("stale session game rollover regression", () => {
  it("resets 4-player dice history when 3-player setup board arrives with same gameKey", async () => {
    const gameKey = "/|sail9284|1";
    const root = document.createElement("div");
    document.body.append(root);

    const session = await build4PStaleSession(root, gameKey);

    // New 3-player game in initial placement arrives with expectedRollCount 0
    const snapshot: BoardSnapshot = {
      gameKey,
      diceMode: "balanced",
      initialPlacement: true,
      gameplayRollCount: 0,
      turn: 1,
      currentPlayer: "hamzax",
      hasRolled: false,
      playerOrder: [...threePlayerRoster],
      players: Object.fromEntries(
        threePlayerRoster.map((name) => [
          name,
          {
            handSize: 0,
            tradeRatios: { lumber: 4, brick: 4, wool: 4, grain: 4, ore: 4 },
            cardDiscardLimit: 7,
            developmentCards: 0,
            playedDevelopmentCards: {
              knight: 0,
              monopoly: 0,
              "road-building": 0,
              "year-of-plenty": 0,
              "victory-point": 0,
            },
            hasPlayedDevelopmentThisTurn: false,
            visiblePoints: 0,
            hasLargestArmy: false,
            hasLongestRoad: false,
          },
        ]),
      ),
      hexes: [],
      vertices: [],
      edges: [],
    };

    // Reconcile the incoming snapshot
    const resetOccurred = session.reconcileBoardSnapshot(snapshot);
    expect(resetOccurred).toBe(true);
    expect(session.diceHistory.rolls).toHaveLength(0);

    // Balanced stochastic input now succeeds cleanly for the new 3P game in setup
    const stochastic = buildLiveDecisionStochasticInput(
      "balanced",
      session.diceHistory,
      threePlayerRoster,
      0,
    );
    expect(stochastic.model).toBe("mref-colonist-linked-2024-v1");
    expect(stochastic.rolls).toEqual([]);
    expect(stochastic.provenance).toBe("complete-from-first-gameplay-roll");
  });

  it("resets session when initial placement is asserted after previous gameplay rolls", async () => {
    const gameKey = "/|sail9284|1";
    const root = document.createElement("div");
    document.body.append(root);

    const session = await build4PStaleSession(root, gameKey);
    expect(session.diceHistory.rolls.length).toBeGreaterThan(0);

    // Same player roster, but board enters initialPlacement with gameplayRollCount 0
    session.reconcileBoardSnapshot({
      gameKey,
      turn: 0,
      initialPlacement: true,
      gameplayRollCount: 0,
      playerOrder: [...fourPlayerRoster],
    });
    expect(session.diceHistory.rolls).toHaveLength(0);
    expect(session.events).toHaveLength(0);

    const stochastic = buildLiveDecisionStochasticInput(
      "balanced",
      session.diceHistory,
      fourPlayerRoster,
      0,
    );
    expect(stochastic.rolls).toEqual([]);
  });

  it("does not let overlay crash or schedule stale 208-roll history for 3-player setup board", async () => {
    const gameKey = "/|sail9284|1";
    const root = document.createElement("div");
    document.body.append(root);

    const session = await build4PStaleSession(root, gameKey);

    overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS, engine: "deep-search" },
      { reset: vi.fn() },
    );
    await new Promise<void>((resolve) => setTimeout(resolve, 0));

    overlay.update(session);

    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});

    // 3-player setup board snapshot arrives
    const snapshot: BoardSnapshot = {
      gameKey,
      diceMode: "balanced",
      initialPlacement: true,
      gameplayRollCount: 0,
      turn: 1,
      currentPlayer: "hamzax",
      myPlayer: "hamzax",
      isMyTurn: true,
      action: "settlement",
      hasRolled: false,
      playerOrder: [...threePlayerRoster],
      localSeatDiagnostics: {
        seatSource: "gameController.myColor+currentUserId+gameUserStates",
        identity: {
          status: "resolved",
          reason: "cross-checked",
          source: "controller+account-user-id+store-roster",
          currentUserIdAvailable: true,
          currentUserMatchColors: [1],
          myColor: 1,
          currentUserColor: 1,
        },
      },
      players: Object.fromEntries(
        threePlayerRoster.map((name) => [
          name,
          {
            handSize: 0,
            tradeRatios: { lumber: 4, brick: 4, wool: 4, grain: 4, ore: 4 },
            cardDiscardLimit: 7,
            developmentCards: 0,
            playedDevelopmentCards: {
              knight: 0,
              monopoly: 0,
              "road-building": 0,
              "year-of-plenty": 0,
              "victory-point": 0,
            },
            hasPlayedDevelopmentThisTurn: false,
            visiblePoints: 0,
            hasLargestArmy: false,
            hasLongestRoad: false,
          },
        ]),
      ),
      hexes: [],
      vertices: [{ id: "v:0,0,0", adjacentHexes: [], adjacentVertices: [] }],
      edges: [],
      legalVertexIds: ["v:0,0,0"],
    };

    overlay.updateBoard(snapshot);

    // The overlay must NOT fail with "Balanced Dice public roll sequence does not reconcile"
    const reconcileFailures = consoleError.mock.calls.filter((call) =>
      String(call[0]).includes("Balanced Dice public roll sequence does not reconcile"),
    );
    expect(reconcileFailures).toHaveLength(0);

    // Session was reconciled cleanly
    expect(session.diceHistory.rolls).toHaveLength(0);
  });

  it("resets cleanly across 1P/bot, 2P duel, 3P, and 4P configurations", async () => {
    const gameKey = "/|sail9284|1";
    const root = document.createElement("div");
    document.body.append(root);

    // Scenario A: Stale 4-player session transitioned to 2-player 1v1 duel
    const session4P = await build4PStaleSession(root, gameKey);
    const twoPlayerRoster = ["hamzax", "Zalas"];
    const duelSnapshot: BoardSnapshot = {
      gameKey,
      diceMode: "balanced",
      initialPlacement: true,
      gameplayRollCount: 0,
      turn: 1,
      currentPlayer: "hamzax",
      playerOrder: twoPlayerRoster,
      players: {
        hamzax: mockPublicPlayer(),
        Zalas: mockPublicPlayer(),
      },
      hexes: [],
      vertices: [],
      edges: [],
    };
    expect(session4P.reconcileBoardSnapshot(duelSnapshot)).toBe(true);
    expect(session4P.diceHistory.rolls).toHaveLength(0);
    const duelStochastic = buildLiveDecisionStochasticInput(
      "balanced",
      session4P.diceHistory,
      twoPlayerRoster,
      0,
    );
    expect(duelStochastic.rolls).toEqual([]);
    expect(duelStochastic.provenance).toBe("complete-from-first-gameplay-roll");

    // Scenario B: Stale session transitioned to 1P solo practice vs 3 bots
    const botRoster = ["hamzax", "Bot Easy", "Bot Medium", "Bot Hard"];
    // Add rolls to simulate gameplay in 2P
    appendPublicDiceRoll(session4P.diceHistory, {
      actor: "hamzax",
      total: 6,
      dice: [3, 3],
      eventId: "board-roll:0:hamzax",
      logIndex: 10,
    });
    session4P.state.playerOrder = [...twoPlayerRoster];
    session4P.state.players = { hamzax: {} as any, Zalas: {} as any };
    expect(session4P.diceHistory.rolls.length).toBe(1);

    const botSnapshot: BoardSnapshot = {
      gameKey,
      diceMode: "balanced",
      initialPlacement: true,
      gameplayRollCount: 0,
      turn: 1,
      currentPlayer: "hamzax",
      playerOrder: botRoster,
      players: Object.fromEntries(botRoster.map((b) => [b, mockPublicPlayer()])),
      hexes: [],
      vertices: [],
      edges: [],
    };
    expect(session4P.reconcileBoardSnapshot(botSnapshot)).toBe(true);
    expect(session4P.diceHistory.rolls).toHaveLength(0);
    const botStochastic = buildLiveDecisionStochasticInput(
      "balanced",
      session4P.diceHistory,
      botRoster,
      0,
    );
    expect(botStochastic.rolls).toEqual([]);
    expect(botStochastic.provenance).toBe("complete-from-first-gameplay-roll");

    // Scenario C: Stale 3-player session transitioned to 4-player game in setup
    const session3P = new GameSession(root, vi.fn(), gameKey);
    sessions.push(session3P);
    await session3P.start();
    session3P.state.playerOrder = [...threePlayerRoster];
    for (const p of threePlayerRoster) session3P.state.players[p] = {} as any;

    const fourPlayerSnapshot: BoardSnapshot = {
      gameKey,
      diceMode: "balanced",
      initialPlacement: true,
      gameplayRollCount: 0,
      turn: 1,
      currentPlayer: "hamzax",
      playerOrder: fourPlayerRoster,
      players: Object.fromEntries(fourPlayerRoster.map((b) => [b, mockPublicPlayer()])),
      hexes: [],
      vertices: [],
      edges: [],
    };
    // A subset alone cannot distinguish an old roster from late log hydration.
    expect(session3P.reconcileBoardSnapshot(fourPlayerSnapshot)).toBe(false);
    expect(session3P.diceHistory.rolls).toHaveLength(0);
  });

  it("uses stateFromPublicBoard when session only knows a subset of board players during late log attach", async () => {
    const gameKey = "/|sail9284|1";
    const root = document.createElement("div");
    document.body.append(root);

    const session = new GameSession(root, vi.fn(), gameKey);
    sessions.push(session);
    await session.start();

    // Session currently only knows Alice because the log is still attaching / in setup
    session.state.playerOrder = ["Alice"];
    session.state.players["Alice"] = {
      name: "Alice",
      color: "#53a6e8",
      devCards: [],
      playedDevCards: { knight: 0, monopoly: 0, "road-building": 0, "year-of-plenty": 0, "victory-point": 0, unknown: 0 },
      builds: { road: 0, settlement: 0, city: 0, development: 0 },
      resourcesGained: { lumber: 0, brick: 0, wool: 0, grain: 0, ore: 0 },
      productionGained: { lumber: 0, brick: 0, wool: 0, grain: 0, ore: 0 },
      resourcesSpent: { lumber: 0, brick: 0, wool: 0, grain: 0, ore: 0 },
      opponentModel: { tradeAccepts: 0, tradeRejects: 0, offersMade: 0, countersMade: 0, policyPosterior: { balanced: 0.2, expansion: 0.2, cityDevelopment: 0.2, tradeFlexible: 0.2, tradeResistant: 0.2 } },
    };

    overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS, engine: "deep-search" },
      { reset: vi.fn() },
    );
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    overlay.update(session);

    // Board snapshot has all 4 players
    const fullRoster = ["Alice", "Bob", "Carol", "Dave"];
    const board: BoardSnapshot = {
      gameKey,
      diceMode: "random",
      initialPlacement: false,
      gameplayRollCount: 1,
      turn: 5,
      currentPlayer: "Alice",
      myPlayer: "Alice",
      playerOrder: fullRoster,
      players: Object.fromEntries(fullRoster.map((name) => [name, mockPublicPlayer()])),
      hexes: [],
      vertices: [],
      edges: [],
    };

    overlay.updateBoard(board);

    // reconciledState must NOT use incomplete session (1 player) for 4-player board.
    // It must fall back to public board, providing all 4 players in state.
    const reconciled = (overlay as any).reconciledState();
    expect(reconciled).toBeDefined();
    expect(Object.keys(reconciled.players).sort()).toEqual(fullRoster.sort());
    expect(reconciled.playerOrder.sort()).toEqual(fullRoster.sort());
  });

  it("preserves a partial setup tracker across repeated board snapshots", async () => {
    const root = document.createElement("div");
    root.innerHTML = '<div data-index="0">Happy settling!</div><div data-index="1">Alice built a settlement</div><div data-index="2">Carol built a settlement</div>';
    document.body.append(root);
    const session = new GameSession(root, vi.fn(), "partial-setup");
    sessions.push(session);
    session.setInitialPlacement(true, "partial-setup");
    await session.start();
    expect(session.state.playerOrder).toEqual(["Alice", "Carol"]);
    expect(session.events.length).toBeGreaterThan(0);
    const state = session.state;
    const events = [...session.events];
    for (const turn of [0, 1, 2]) {
      expect(session.reconcileBoardSnapshot({
        gameKey: "partial-setup", initialPlacement: true, gameplayRollCount: 0,
        turn, playerOrder: ["Alice", "Bob", "Carol", "Dave"],
      })).toBe(false);
    }
    expect(session.reconcileBoardSnapshot({
      gameKey: "partial-setup", initialPlacement: true, gameplayRollCount: 0,
      turn: 2, playerOrder: ["Alice", "Bob", "Player 3", "Player 4"],
    })).toBe(false);
    expect(session.state).toBe(state);
    expect(session.events).toEqual(events);
  });

  it("preserves existing rolls on contradictory setup frames and midgame roster changes", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    const session = await build4PStaleSession(root, "ongoing");
    const history = session.diceHistory;
    expect(session.reconcileBoardSnapshot({
      gameKey: "ongoing", initialPlacement: true, gameplayRollCount: 0,
      turn: 110, playerOrder: fourPlayerRoster,
    })).toBe(false);
    expect(session.reconcileBoardSnapshot({
      gameKey: "ongoing", initialPlacement: false, gameplayRollCount: 104,
      turn: 110, playerOrder: threePlayerRoster,
    })).toBe(false);
    expect(session.diceHistory).toBe(history);
    expect(session.diceHistory.rolls).toHaveLength(104);
  });

  it.each([2, 3, 4])("allows dice validation independently of a partial %i-player card tracker", async (count) => {
    const root = document.createElement("div");
    document.body.append(root);
    const session = await build4PStaleSession(root, "subset");
    const boardRoster = fourPlayerRoster.slice(0, count);
    session.state.playerOrder = boardRoster.slice(0, Math.ceil(count / 2));
    session.state.players = Object.fromEntries(
      session.state.playerOrder.map((name) => [name, session.state.players[name]!]),
    );
    expect(isSessionCompatibleWithBoard(session, {
      gameKey: "subset", diceMode: "balanced", initialPlacement: false, gameplayRollCount: 1,
      playerOrder: boardRoster, hexes: [], vertices: [], edges: [],
    })).toBe(true);
  });

  it("reattaches root container without losing existing dice rolls or events", async () => {
    const root1 = document.createElement("div");
    document.body.append(root1);
    const session = await build4PStaleSession(root1, "ongoing-reattach");
    expect(session.diceHistory.rolls.length).toBe(104);
    const initialEventsCount = session.events.length;

    const root2 = document.createElement("div");
    document.body.append(root2);
    session.attachRoot(root2);

    expect(session.diceHistory.rolls.length).toBe(104);
    expect(session.events.length).toBe(initialEventsCount);
  });

  it("partitions session identity across distinct room hashes", () => {
    const originalHash = window.location.hash;
    try {
      window.location.hash = "#roomA";
      const sessionA = new GameSession(document.createElement("div"), vi.fn());
      window.location.hash = "#roomB";
      const sessionB = new GameSession(document.createElement("div"), vi.fn());
      expect(sessionA.id).not.toEqual(sessionB.id);
      expect(sessionA.id).toBe("room:roomA");
      expect(sessionB.id).toBe("room:roomB");
    } finally {
      window.location.hash = originalHash;
    }
  });

  it.each(["matching-key", "another-game"])("migrates the legacy storage key only for the same game: %s", async (storedGameKey) => {
    const originalHash = window.location.hash;
    try {
      window.location.hash = "#roomA";
      const root = document.createElement("div");
      document.body.append(root);
      const session = new GameSession(root, vi.fn(), "matching-key");
      sessions.push(session);

      const basePage = window.location.origin + window.location.pathname + window.location.search;
      const legacyId = hashString(basePage);
      const history = createDiceHistoryState();
      appendPublicDiceRoll(history, {
        actor: "Alice", total: 8, eventId: "board-roll:0:Alice",
      });
      storage.set(sessionStorageKey(legacyId), {
        schema: 4,
        id: legacyId,
        gameKey: storedGameKey,
        page: basePage,
        startedAt: 1000,
        events: [],
        seenIds: [],
        unmatchedCount: 0,
        partialHistory: false,
        diceHistory: serializeDiceHistoryState(history),
      });

      await session.start();
      expect(session.gameKey).toBe("matching-key");
      if (storedGameKey === "matching-key") {
        expect(session.startedAt).toBe(1000);
        expect(session.diceHistory.rolls).toEqual(history.rolls);
      } else {
        expect(session.startedAt).not.toBe(1000);
        expect(session.diceHistory.rolls).toEqual([]);
      }
      session.stop();
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
      expect(storage.get(sessionStorageKey(session.id))).toMatchObject({
        id: session.id,
        page: basePage + "#roomA",
        gameKey: "matching-key",
      });
    } finally {
      window.location.hash = originalHash;
    }
  });

  it("preserves history and pauses compatibility on a midgame bot replacement", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    const session = await build4PStaleSession(root, "/|wall5593|1");
    sessions.push(session);
    expect(session.diceHistory.rolls.length).toBe(104);

    const novelSnapshot: BoardSnapshot = {
      gameKey: "/|wall5593|1",
      diceMode: "balanced",
      initialPlacement: false,
      gameplayRollCount: 120,
      turn: 58,
      currentPlayer: "hamzax",
      playerOrder: [...threePlayerRoster, "Bot Easy"],
      players: Object.fromEntries([...threePlayerRoster, "Bot Easy"].map((p) => [p, mockPublicPlayer()])),
      hexes: [],
      vertices: [],
      edges: [],
    };

    expect(session.reconcileBoardSnapshot(novelSnapshot)).toBe(false);
    expect(session.diceHistory.rolls.length).toBe(104);
    expect(isSessionCompatibleWithBoard(session, novelSnapshot)).toBe(false);
  });

  it("preserves recorded rolls when a midgame board count regresses", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    const session = await build4PStaleSession(root, "/|same-roster|1");
    sessions.push(session);
    expect(session.diceHistory.rolls.length).toBe(104);

    const rollDropSnapshot: BoardSnapshot = {
      gameKey: "/|same-roster|1",
      diceMode: "balanced",
      initialPlacement: false,
      gameplayRollCount: 4,
      turn: 10,
      currentPlayer: "hamzax",
      playerOrder: fourPlayerRoster,
      players: Object.fromEntries(fourPlayerRoster.map((p) => [p, mockPublicPlayer()])),
      hexes: [],
      vertices: [],
      edges: [],
    };

    expect(session.reconcileBoardSnapshot(rollDropSnapshot)).toBe(false);
    expect(session.diceHistory.rolls.length).toBe(104);
    expect(isSessionCompatibleWithBoard(session, rollDropSnapshot)).toBe(true);
    expect(() => buildLiveDecisionStochasticInput(
      "balanced", session.diceHistory, fourPlayerRoster, 4,
    )).toThrow();
    expect(buildLiveDecisionStochasticInput(
      "balanced", session.diceHistory, fourPlayerRoster, 104,
    ).rolls).toHaveLength(104);
  });

  it("reports accurate diagnostic reasons for different incompatible session states", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS, engine: "deep-search" },
      { reset: vi.fn() },
    );
    await new Promise<void>((resolve) => setTimeout(resolve, 0));

    // 1. No session attached
    overlay.update(undefined);
    expect((overlay as any).diceEvidenceDetail()).toContain(
      "The public game log is still attaching",
    );

    // 2. Setup with rolls from previous game
    const sessionWithRolls = await build4PStaleSession(root, "diag-setup");
    sessions.push(sessionWithRolls);
    (overlay as any).session = sessionWithRolls;
    const setupBoard: BoardSnapshot = {
      gameKey: "diag-setup",
      diceMode: "balanced",
      initialPlacement: true,
      gameplayRollCount: 0,
      turn: 1,
      playerOrder: fourPlayerRoster,
      players: Object.fromEntries(fourPlayerRoster.map((p) => [p, mockPublicPlayer()])),
      hexes: [],
      vertices: [],
      edges: [],
    };
    (overlay as any).board = setupBoard;
    expect((overlay as any).diceEvidenceDetail()).toContain(
      "Game session contains rolls from a previous game while board is in setup",
    );

    // 3. Midgame with genuinely incompatible roster (novel player + extraneous player)
    const midgameRosterMismatchBoard: BoardSnapshot = {
      gameKey: "diag-roster",
      diceMode: "balanced",
      initialPlacement: false,
      gameplayRollCount: 150,
      turn: 60,
      playerOrder: ["Alice", "Xavier"],
      players: { Alice: mockPublicPlayer(), Xavier: mockPublicPlayer() },
      hexes: [],
      vertices: [],
      edges: [],
    };
    (overlay as any).board = midgameRosterMismatchBoard;
    expect((overlay as any).diceEvidenceDetail()).toContain(
      "Game session is incompatible with current board roster",
    );
  });

  it("preserves session during normal live turn rolls and 1-roll DOM skews without midgame reset", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    const session = await build4PStaleSession(root, "/|live-turn|1");
    sessions.push(session);

    // Simulate 28 recorded rolls at turn 36
    session.diceHistory.rolls = session.diceHistory.rolls.slice(0, 28);
    expect(session.diceHistory.rolls.length).toBe(28);

    // 1. Hamzax rolls: board records board-roll:28:hamzax
    appendPublicDiceRoll(session.diceHistory, {
      actor: "hamzax",
      total: 11,
      eventId: "board-roll:28:hamzax",
    });
    expect(session.diceHistory.rolls.length).toBe(29);

    // 2. An intermediate snapshot arrives with gameplayRollCount = 28 (before count increment or on turn-end toggle)
    const turn36Board: BoardSnapshot = {
      gameKey: "/|live-turn|1",
      diceMode: "balanced",
      initialPlacement: false,
      gameplayRollCount: 28,
      turn: 36,
      currentPlayer: "hamzax",
      playerOrder: fourPlayerRoster,
      players: Object.fromEntries(fourPlayerRoster.map((p) => [p, mockPublicPlayer()])),
      hexes: [],
      vertices: [],
      edges: [],
    };

    // Must NOT reset the session!
    expect(session.reconcileBoardSnapshot(turn36Board)).toBe(false);
    expect(session.diceHistory.rolls.length).toBe(29);
    expect(isSessionCompatibleWithBoard(session, turn36Board)).toBe(true);

    // 3. DOM log renders next roll before bridge snapshot advances: indexedCount = limit + 1
    const rollWithLogIndex = session.diceHistory.rolls[session.diceHistory.rolls.length - 1]!;
    (rollWithLogIndex as any).logIndex = 227;

    expect(session.reconcileBoardSnapshot(turn36Board)).toBe(false);
    expect(session.diceHistory.rolls.length).toBe(29);
    expect(isSessionCompatibleWithBoard(session, turn36Board)).toBe(true);
  });

  it("preserves session when a player departs or disconnects midgame", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    const session = await build4PStaleSession(root, "/|departure|1");
    sessions.push(session);
    expect(session.diceHistory.rolls.length).toBe(104);

    // Neda1311 disconnects/departs: board roster only has threePlayerRoster
    const departureBoard: BoardSnapshot = {
      gameKey: "/|departure|1",
      diceMode: "balanced",
      initialPlacement: false,
      gameplayRollCount: 104,
      turn: 40,
      currentPlayer: "hamzax",
      playerOrder: threePlayerRoster,
      players: Object.fromEntries(threePlayerRoster.map((p) => [p, mockPublicPlayer()])),
      hexes: [],
      vertices: [],
      edges: [],
    };

    // Midgame departure alone must NOT reset session or mark it incompatible
    expect(session.reconcileBoardSnapshot(departureBoard)).toBe(false);
    expect(session.diceHistory.rolls.length).toBe(104);
    expect(isSessionCompatibleWithBoard(session, departureBoard)).toBe(true);
  });
});
