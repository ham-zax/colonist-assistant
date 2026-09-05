// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { AssistantOverlay } from "../src/content/overlay";
import { DEFAULT_SETTINGS } from "../src/content/settings";
import { createTrackerState, reduceTracker } from "../src/core/tracker";
import { appendPublicDiceRoll, createDiceHistoryState, observeLogCoverage } from "../src/core/dice-history";
import type { BoardSnapshot } from "../src/core/placement";

let overlay: AssistantOverlay | undefined;
afterEach(() => {
  overlay?.destroy();
  overlay = undefined;
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});

describe("live stochastic evidence recovery", () => {
  it("does not expose a heuristic click while the authoritative engine is paused", async () => {
    vi.stubGlobal("chrome", {
      runtime: {
        getURL: (path: string) => `chrome-extension://fixture/${path}`,
        getManifest: () => ({ version: "0.9.1" }),
        sendMessage: async (message: { id: number }) => ({ id: message.id, runtime: "background-gpu", engineRevision: "deep-maxn-v12", initializationMs: 1 }),
      },
      storage: { local: { get: async () => ({}), set: async () => {}, remove: async () => {} }, sync: { set: async () => {} } },
    });
    overlay = new AssistantOverlay({ ...DEFAULT_SETTINGS }, { reset: vi.fn() });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    const internals = overlay as unknown as {
      board: BoardSnapshot;
      decisionRuntimeError: string;
      scheduleDecisionAnalysis: () => void;
      nextClick: () => unknown;
      render: () => void;
    };
    vi.spyOn(internals, "scheduleDecisionAnalysis").mockImplementation(() => {});
    const nextClick = vi.spyOn(internals, "nextClick");
    internals.board = {
      hexes: [], vertices: [], edges: [], diceMode: "balanced", gameKey: "paused-evidence",
      myPlayer: "Alice", currentPlayer: "Alice", playerOrder: ["Alice", "Bob"],
      isMyTurn: true, hasRolled: true, action: "none",
      localSeatDiagnostics: {
        seatSource: "gameController.myColor+currentUserId+gameUserStates",
        identity: { status: "resolved", reason: "cross-checked", source: "controller+account-user-id+store-roster", currentUserIdAvailable: true, currentUserMatchColors: [1], myColor: 1, currentUserColor: 1 },
      },
    };
    internals.decisionRuntimeError = "Balanced Dice requires usable public reference-dice history";
    internals.render();
    expect(nextClick).not.toHaveBeenCalled();
    const shadow = document.querySelector("#colonist-assistant-root")!.shadowRoot!;
    expect(shadow.querySelector(".board-marker")).toBeNull();
    expect(shadow.textContent).toContain("Balanced Dice requires usable public reference-dice history");
  });

  it("uses public gameplay-roll count to keep sparse log indexes from pausing Balanced Dice", async () => {
    const sendMessage = vi.fn(async (message: { id: number; stochastic?: unknown }) =>
      message.stochastic
        ? { id: message.id, analysis: { engine: "deep-search", runtime: "background-gpu", players: [] } }
        : { id: message.id, runtime: "background-gpu", engineRevision: "deep-maxn-v12", initializationMs: 1 });
    vi.stubGlobal("chrome", {
      runtime: { getURL: (path: string) => `chrome-extension://fixture/${path}`, getManifest: () => ({ version: "0.9.1" }), sendMessage },
      storage: { local: { get: async () => ({}), set: async () => {}, remove: async () => {} }, sync: { set: async () => {} } },
    });
    let tracker = reduceTracker(createTrackerState(), { type: "discover", player: "Alice" });
    tracker = reduceTracker(tracker, { type: "discover", player: "Bob" });
    const history = createDiceHistoryState();
    observeLogCoverage(history, [0, 1, 3]);
    appendPublicDiceRoll(history, {
      actor: "Alice", total: 8, dice: [3, 5], eventId: "sparse-first-roll", logIndex: 3,
    });
    expect(history.provenance).toBe("gapped");

    overlay = new AssistantOverlay({ ...DEFAULT_SETTINGS }, { reset: vi.fn() });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    const internals = overlay as unknown as {
      board: BoardSnapshot;
      session: { diceHistory: typeof history };
      decisionRuntimeError: string;
      scheduleDecisionAnalysis: (state: typeof tracker, player: string) => void;
      render: () => void;
    };
    vi.spyOn(internals, "render").mockImplementation(() => {});
    internals.session = { diceHistory: history };
    internals.board = {
      hexes: [], vertices: [], edges: [], diceMode: "balanced", gameKey: "sparse-live-evidence",
      myPlayer: "Alice", currentPlayer: "Alice", playerOrder: ["Alice", "Bob"],
      isMyTurn: true, hasRolled: true, gameplayRollCount: 1, action: "none",
      localSeatDiagnostics: {
        seatSource: "gameController.myColor+currentUserId+gameUserStates",
        identity: { status: "resolved", reason: "cross-checked", source: "controller+account-user-id+store-roster", currentUserIdAvailable: true, currentUserMatchColors: [1], myColor: 1, currentUserColor: 1 },
      },
    };
    sendMessage.mockClear();

    internals.scheduleDecisionAnalysis(tracker, "Alice");

    await vi.waitFor(() => expect(sendMessage).toHaveBeenCalledOnce());
    expect(sendMessage).toHaveBeenCalledWith(expect.objectContaining({
      stochastic: expect.objectContaining({
        model: "mref-colonist-linked-2024-v1",
        provenance: "complete-from-first-gameplay-roll",
        rolls: [{ ordinal: 0, actor: 0, total: 8 }],
      }),
    }));
    expect(internals.decisionRuntimeError).toBe("");
  });

  it("waits for the board snapshot when the current log roll arrives one update first", async () => {
    const sendMessage = vi.fn(async (message: { id: number; stochastic?: unknown }) =>
      message.stochastic
        ? { id: message.id, analysis: { engine: "deep-search", runtime: "background-gpu", players: [] } }
        : { id: message.id, runtime: "background-gpu", engineRevision: "deep-maxn-v12", initializationMs: 1 });
    vi.stubGlobal("chrome", {
      runtime: { getURL: (path: string) => `chrome-extension://fixture/${path}`, getManifest: () => ({ version: "0.9.1" }), sendMessage },
      storage: { local: { get: async () => ({}), set: async () => {}, remove: async () => {} }, sync: { set: async () => {} } },
    });
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    const consoleWarn = vi.spyOn(console, "warn").mockImplementation(() => {});
    let tracker = reduceTracker(createTrackerState(), { type: "discover", player: "Alice" });
    tracker = reduceTracker(tracker, { type: "discover", player: "Bob" });
    const history = createDiceHistoryState();
    observeLogCoverage(history, [0, 1, 16, 20]);
    appendPublicDiceRoll(history, { actor: "Alice", total: 6, dice: [2, 4], eventId: "first", logIndex: 16 });
    appendPublicDiceRoll(history, { actor: "Bob", total: 5, dice: [2, 3], eventId: "second", logIndex: 20 });

    overlay = new AssistantOverlay({ ...DEFAULT_SETTINGS }, { reset: vi.fn() });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    const internals = overlay as unknown as {
      board: BoardSnapshot;
      session: { diceHistory: typeof history };
      decisionRuntimeError: string;
      scheduleDecisionAnalysis: (state: typeof tracker, player: string) => void;
      render: () => void;
    };
    vi.spyOn(internals, "render").mockImplementation(() => {});
    internals.session = { diceHistory: history };
    internals.board = {
      hexes: [], vertices: [], edges: [], diceMode: "balanced", gameKey: "roll-race",
      myPlayer: "Bob", currentPlayer: "Bob", playerOrder: ["Alice", "Bob"],
      isMyTurn: true, hasRolled: false, gameplayRollCount: 1, action: "none",
      localSeatDiagnostics: {
        seatSource: "gameController.myColor+currentUserId+gameUserStates",
        identity: { status: "resolved", reason: "cross-checked", source: "controller+account-user-id+store-roster", currentUserIdAvailable: true, currentUserMatchColors: [2], myColor: 2, currentUserColor: 2 },
      },
    };
    sendMessage.mockClear();

    internals.scheduleDecisionAnalysis(tracker, "Bob");

    expect(sendMessage).not.toHaveBeenCalled();
    expect(consoleError).not.toHaveBeenCalled();
    expect(consoleWarn).not.toHaveBeenCalled();
    expect(internals.decisionRuntimeError).toMatch(/waiting for the board.*catch up/i);

    internals.board = { ...internals.board, hasRolled: true, gameplayRollCount: 2 };
    internals.scheduleDecisionAnalysis(tracker, "Bob");

    await vi.waitFor(() => expect(sendMessage).toHaveBeenCalledOnce());
    expect(sendMessage).toHaveBeenCalledWith(expect.objectContaining({
      stochastic: expect.objectContaining({
        model: "mref-colonist-linked-2024-v1",
        rolls: [
          { ordinal: 0, actor: 0, total: 6 },
          { ordinal: 1, actor: 1, total: 5 },
        ],
      }),
    }));
  });

  it("waits for board catch-up when board and sparse DOM evidence both contain prior rolls", async () => {
    const sendMessage = vi.fn(async (message: { id: number; stochastic?: unknown }) =>
      message.stochastic
        ? { id: message.id, analysis: { engine: "deep-search", runtime: "background-gpu", players: [] } }
        : { id: message.id, runtime: "background-gpu", engineRevision: "deep-maxn-v12", initializationMs: 1 });
    vi.stubGlobal("chrome", {
      runtime: { getURL: (path: string) => `chrome-extension://fixture/${path}`, getManifest: () => ({ version: "0.9.1" }), sendMessage },
      storage: { local: { get: async () => ({}), set: async () => {}, remove: async () => {} }, sync: { set: async () => {} } },
    });
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    const consoleWarn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const players = ["hamzax", "Kalk", "Malti", "Avrit"];
    let tracker = createTrackerState();
    for (const player of players) tracker = reduceTracker(tracker, { type: "discover", player });
    const history = createDiceHistoryState();
    observeLogCoverage(history, [0, 1, 2, 15, 16, 18, 20, 22, 24]);
    const totals = [6, 8, 9, 5] as const;
    totals.forEach((total, ordinal) => {
      appendPublicDiceRoll(history, {
        actor: players[ordinal]!, total, eventId: `board-roll:${ordinal}:${players[ordinal]}`,
      });
      appendPublicDiceRoll(history, {
        actor: players[ordinal]!, total, eventId: `log-${ordinal}`, logIndex: 15 + ordinal * 2,
      });
    });
    appendPublicDiceRoll(history, {
      actor: "hamzax", total: 10, eventId: "current-log", logIndex: 24,
    });
    expect(history.rolls).toHaveLength(9);

    overlay = new AssistantOverlay({ ...DEFAULT_SETTINGS }, { reset: vi.fn() });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    const internals = overlay as unknown as {
      board: BoardSnapshot;
      session: { diceHistory: typeof history };
      decisionRuntimeError: string;
      scheduleDecisionAnalysis: (state: typeof tracker, player: string) => void;
      render: () => void;
    };
    vi.spyOn(internals, "render").mockImplementation(() => {});
    internals.session = { diceHistory: history };
    internals.board = {
      hexes: [], vertices: [], edges: [], diceMode: "balanced", gameKey: "mixed-roll-race",
      myPlayer: "hamzax", currentPlayer: "hamzax", playerOrder: players,
      isMyTurn: true, hasRolled: false, gameplayRollCount: 4, action: "none",
      localSeatDiagnostics: {
        seatSource: "gameController.myColor+currentUserId+gameUserStates",
        identity: { status: "resolved", reason: "cross-checked", source: "controller+account-user-id+store-roster", currentUserIdAvailable: true, currentUserMatchColors: [1], myColor: 1, currentUserColor: 1 },
      },
    };
    sendMessage.mockClear();

    internals.scheduleDecisionAnalysis(tracker, "hamzax");

    expect(sendMessage).not.toHaveBeenCalled();
    expect(consoleError).not.toHaveBeenCalled();
    expect(consoleWarn).not.toHaveBeenCalled();
    expect(internals.decisionRuntimeError).toMatch(/waiting for the board.*catch up/i);

    // The board bridge can publish the same current roll into the session
    // before overlay.updateBoard applies that snapshot. The overlay still sees
    // count 4 while the mixed history now has ten raw source observations.
    appendPublicDiceRoll(history, {
      actor: "hamzax", total: 10, eventId: "board-roll:4:hamzax",
    });
    expect(history.rolls).toHaveLength(10);
    internals.scheduleDecisionAnalysis(tracker, "hamzax");
    expect(sendMessage).not.toHaveBeenCalled();
    expect(consoleError).not.toHaveBeenCalled();
    expect(consoleWarn).not.toHaveBeenCalled();
    expect(internals.decisionRuntimeError).toMatch(/waiting for the board.*catch up/i);

    internals.board = { ...internals.board, hasRolled: true, gameplayRollCount: 5 };
    internals.scheduleDecisionAnalysis(tracker, "hamzax");

    await vi.waitFor(() => expect(sendMessage).toHaveBeenCalledOnce());
    expect(sendMessage).toHaveBeenCalledWith(expect.objectContaining({
      stochastic: expect.objectContaining({
        model: "mref-colonist-linked-2024-v1",
        rolls: [
          { ordinal: 0, actor: 0, total: 6 },
          { ordinal: 1, actor: 1, total: 8 },
          { ordinal: 2, actor: 2, total: 9 },
          { ordinal: 3, actor: 3, total: 5 },
          { ordinal: 4, actor: 0, total: 10 },
        ],
      }),
    }));
  });

  it("resumes when missing startup evidence arrives without retrying unchanged evidence", async () => {
    const sendMessage = vi.fn(async (message: { id: number; stochastic?: unknown }) =>
      message.stochastic
        ? { id: message.id, analysis: { engine: "deep-search", runtime: "background-gpu", players: [] } }
        : { id: message.id, runtime: "background-gpu", engineRevision: "deep-maxn-v12", initializationMs: 1 });
    vi.stubGlobal("chrome", {
      runtime: { getURL: (path: string) => `chrome-extension://fixture/${path}`, getManifest: () => ({ version: "0.9.1" }), sendMessage },
      storage: { local: { get: async () => ({}), set: async () => {}, remove: async () => {} }, sync: { set: async () => {} } },
    });
    vi.spyOn(console, "error").mockImplementation(() => {});
    let tracker = reduceTracker(createTrackerState(), { type: "discover", player: "Alice" });
    tracker = reduceTracker(tracker, { type: "discover", player: "Bob" });
    const history = createDiceHistoryState();
    overlay = new AssistantOverlay({ ...DEFAULT_SETTINGS }, { reset: vi.fn() });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    const internals = overlay as unknown as {
      board: BoardSnapshot;
      session: { diceHistory: typeof history };
      decisionRuntimeError: string;
      decisionAnalysis?: unknown;
      scheduleDecisionAnalysis: (state: typeof tracker, player: string) => void;
      render: () => void;
    };
    vi.spyOn(internals, "render").mockImplementation(() => {});
    internals.session = { diceHistory: history };
    internals.board = {
      hexes: [], vertices: [], edges: [], diceMode: "balanced", gameKey: "startup-evidence",
      myPlayer: "Alice", currentPlayer: "Alice", playerOrder: ["Alice", "Bob"],
      isMyTurn: true, hasRolled: true, action: "none",
      localSeatDiagnostics: {
        seatSource: "gameController.myColor+currentUserId+gameUserStates",
        identity: { status: "resolved", reason: "cross-checked", source: "controller+account-user-id+store-roster", currentUserIdAvailable: true, currentUserMatchColors: [1], myColor: 1, currentUserColor: 1 },
      },
    };
    sendMessage.mockClear();
    internals.scheduleDecisionAnalysis(tracker, "Alice");
    expect(internals.decisionRuntimeError).toMatch(/usable public reference-dice history/);
    expect(sendMessage).not.toHaveBeenCalled();
    internals.scheduleDecisionAnalysis(tracker, "Alice");
    expect(sendMessage).not.toHaveBeenCalled();

    observeLogCoverage(history, [0]);
    appendPublicDiceRoll(history, { actor: "Alice", total: 8, dice: [3, 5], eventId: "first-roll", logIndex: 0 });
    internals.scheduleDecisionAnalysis(tracker, "Alice");
    await vi.waitFor(() => expect(sendMessage).toHaveBeenCalledOnce());
    expect(sendMessage).toHaveBeenCalledWith(expect.objectContaining({
      stochastic: expect.objectContaining({ model: "mref-colonist-linked-2024-v1" }),
    }));
    await vi.waitFor(() => expect(internals.decisionAnalysis).toBeDefined());
    expect(internals.decisionRuntimeError).toBe("");
  });
});
