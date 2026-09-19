import { buildSync } from "esbuild";
import { JSDOM } from "jsdom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GameSession } from "../src/content/session";
import { appendPublicDiceRoll } from "../src/core/dice-history";
import type { BoardSnapshot } from "../src/core/placement";

const bridgeSource = buildSync({
  entryPoints: ["src/page/bridge.ts"], bundle: true, write: false, format: "iife",
}).outputFiles[0]!.text;
const pages: JSDOM[] = [];
const sessions: GameSession[] = [];

afterEach(async () => {
  for (const session of sessions.splice(0)) session.stop();
  await new Promise((resolve) => setTimeout(resolve, 0));
  for (const page of pages.splice(0)) page.window.close();
  vi.unstubAllGlobals();
});

const openPage = (saved: Record<string, string> = {}, turn = 25) => {
  const page = new JSDOM('<canvas id="game-canvas"></canvas>', {
    url: "https://colonist.io/#room", runScripts: "outside-only",
  });
  pages.push(page);
  const { window } = page;
  for (const [key, value] of Object.entries(saved)) window.sessionStorage.setItem(key, value);
  const currentState: {
    turnState?: number;
    completedTurns?: number;
    currentTurnPlayerColor?: number;
  } = {
    turnState: turn === 0 ? 0 : 1,
    completedTurns: turn,
    currentTurnPlayerColor:
      turn === 0 ? 1 : [1, 2, 3, 4][(turn - 8) % 4],
  };
  const playerStates: Record<number, object> = { 1: {}, 2: {}, 3: {}, 4: {} };
  const gameState: {
    mapState: {
      tileState: {
        _tiles: Array<{ hexFace: { x: number; y: number }; state: { type: number; diceNumber: number } }>;
        _tileCorners: Array<{ hexCorner: { x: number; y: number; z: number }; owner: number; buildingType: number }>;
        _tileEdges: unknown[];
      };
    };
    playerStates: Record<number, object>;
    experimentalMechanicState?: {
      currentState: { currentTurnPlayers: number[] };
    };
  } = {
    mapState: { tileState: {
      _tiles: [{ hexFace: { x: 0, y: 0 }, state: { type: 1, diceNumber: 6 } }],
      _tileCorners: [{ hexCorner: { x: 0, y: 0, z: 0 }, owner: 1, buildingType: 1 }],
      _tileEdges: [],
    } },
    playerStates,
  };
  const controller = {
    myColor: undefined as number | undefined,
    isMyTurn: false,
    currentState, playOrder: [1, 2, 3, 4],
    getPlayerNameWithColor: (color: number) => ["Alice", "Bob", "Carol", "Dave"][color - 1],
    getPlayerState: (color: number) => playerStates[color],
    diceState: { diceThrown: true, dice1: 3, dice2: 4 },
  };
  const store = {
    gameState,
    gameSettings: { roomId: "same-room", victoryPointsToWin: 10 },
    gameClientConfig: { currentUserId: undefined as string | undefined },
    gameUserStates: { userStates: [] as Array<{ username: string; selectedColor: number; userId: string }> },
    actionBox: { actionBoxData: undefined as unknown },
  };
  const manager = {
    gameController: controller, gameState, mapController: { mapView: {} },
    gameStore: { getState: () => store },
  };
  const runtime = Object.assign(() => ({ manager }), {
    m: { manager: function initializeGameManager() { return "completeInitialization"; } },
  });
  const chunks: unknown[] = [];
  chunks.push = (...entries: unknown[]) => {
    for (const entry of entries) (entry as [unknown, unknown, (require: typeof runtime) => void])[2](runtime);
    return 0;
  };
  Object.assign(window, { webpackChunkkatan: chunks });
  const messages: Array<{ type: string; payload?: BoardSnapshot }> = [];
  window.postMessage = (message) => { messages.push(message); };
  window.eval(bridgeSource);
  return {
    window, currentState, playerStates, messages, controller, store,
    refresh: () => window.dispatchEvent(new window.Event("resize")),
    snapshot: () => messages.at(-1)?.payload,
    saved: () => Object.fromEntries(Array.from({ length: window.sessionStorage.length }, (_, index) => {
      const key = window.sessionStorage.key(index)!;
      return [key, window.sessionStorage.getItem(key)!];
    })),
  };
};

describe("page bridge progress boundaries", () => {
  it("reads a robber victim prompt from public store evidence without DOM text or selectors", () => {
    const page = openPage();
    page.controller.myColor = 1;
    page.controller.isMyTurn = true;
    page.store.gameClientConfig.currentUserId = "local-user";
    page.store.gameUserStates.userStates = [
      { username: "Alice", selectedColor: 1, userId: "local-user" },
      { username: "Bob", selectedColor: 2, userId: "bob-user" },
    ];
    page.playerStates[2] = { resourceCards: { cards: [0, 0] } };
    const prompt = {
      type: "pickPlayer",
      props: {
        title: { key: "strings:game.tips.robber.title" },
        body: { key: "strings:game.prompts.selectWhoToRobFrom" },
        playerValidators: [{ color: 1 }, { color: 2 }, { color: 2 }, { color: 3 }, { color: 99 }, null],
      },
    };
    page.store.actionBox.actionBoxData = prompt;
    page.refresh();
    expect(page.snapshot()).toMatchObject({
      myPlayer: "Alice", robberVictimSelection: true, robberVictimPlayers: ["Bob"],
    });
    prompt.props.body.key = "strings:game.prompts.masterMerchant";
    page.refresh();
    expect(page.snapshot()?.robberVictimSelection).toBeUndefined();
    prompt.props.body.key = "strings:game.prompts.selectWhoToRobFrom";
    page.controller.isMyTurn = false;
    page.refresh();
    expect(page.snapshot()?.robberVictimSelection).toBeUndefined();
    page.controller.isMyTurn = true;
    page.store.gameClientConfig.currentUserId = "unresolved-user";
    page.refresh();
    expect(page.snapshot()?.robberVictimSelection).toBeUndefined();
  });

  it("rejects midgame setup hydration before publishing or overwriting persisted progress", () => {
    const page = openPage();
    const before = page.snapshot();
    expect(before?.turn).toBe(25);
    const saved = page.saved();
    page.currentState.turnState = 0;
    page.refresh();
    expect(page.messages.at(-1)?.type).toBe("clear");
    expect(page.saved()).toEqual(saved);
    page.currentState.turnState = 1;
    page.refresh();
    expect(page.snapshot()?.gameKey).toBe(before?.gameKey);
    expect(page.snapshot()?.gameplayRollCount).toBe(before?.gameplayRollCount);
  });

  it("waits for initialized turns and all player states", () => {
    const page = openPage();
    const saved = page.saved();
    page.currentState.completedTurns = undefined;
    page.refresh();
    expect(page.messages.at(-1)?.type).toBe("clear");
    expect(page.saved()).toEqual(saved);
    page.currentState.completedTurns = 25;
    delete page.playerStates[4];
    page.refresh();
    expect(page.messages.at(-1)?.type).toBe("clear");
    expect(page.saved()).toEqual(saved);
    page.playerStates[4] = {};
    page.refresh();
    expect(page.snapshot()?.playerOrder).toEqual(["Alice", "Bob", "Carol", "Dave"]);
  });

  it("keeps actor and roll ordinal on the same turn-progress generation", () => {
    const page = openPage({}, 25);
    page.store.gameState.experimentalMechanicState = {
      currentState: { currentTurnPlayers: [3] },
    };
    page.currentState.currentTurnPlayerColor = 2;
    page.refresh();

    expect(page.snapshot()).toMatchObject({
      turn: 25,
      currentPlayer: "Bob",
      gameplayRollCount: 18,
      hasRolled: true,
      lastRoll: 7,
      localSeatDiagnostics: {
        currentActorColor: 2,
        currentActorPlayer: "Bob",
        currentActorSource: "turn-progress",
      },
    });

    page.currentState.completedTurns = 26;
    page.currentState.currentTurnPlayerColor = 2;
    page.refresh();
    expect(page.messages.at(-1)?.type).toBe("clear");

    page.currentState.currentTurnPlayerColor = 3;
    page.refresh();
    expect(page.snapshot()).toMatchObject({
      turn: 26,
      currentPlayer: "Carol",
      gameplayRollCount: 19,
      localSeatDiagnostics: {
        currentActorColor: 3,
        currentActorPlayer: "Carol",
        currentActorSource: "turn-progress",
      },
    });
  });

  it("documents the irreducible same-pair dice ambiguity across a turn boundary", () => {
    const page = openPage({}, 25);
    page.currentState.currentTurnPlayerColor = 2;
    page.refresh();
    const before = page.snapshot();
    expect(before).toMatchObject({
      turn: 25,
      currentPlayer: "Bob",
      gameplayRollCount: 18,
      hasRolled: true,
      lastRoll: 7,
    });

    // No public dice-generation token exists in this bridge contract. If the
    // turn advances while diceThrown and the pair remain unchanged, the same
    // observed fields are compatible with either a stale pair or a legitimate
    // immediate repeat roll. Do not invent a timeout-based distinction.
    page.currentState.completedTurns = 26;
    page.currentState.currentTurnPlayerColor = 3;
    page.refresh();
    expect(page.snapshot()).toMatchObject({
      turn: 26,
      currentPlayer: "Carol",
      gameplayRollCount: 19,
      hasRolled: true,
      lastRoll: 7,
    });
    expect(page.snapshot()?.lastRoll).toBe(before?.lastRoll);
  });

  it("restores same-game dice after reload and rejects them for a new game in the same room", async () => {
    const stored = new Map<string, unknown>();
    vi.stubGlobal("chrome", { storage: { local: {
      get: async (key: string | null) => key === null ? Object.fromEntries(stored) : { [key]: stored.get(key) },
      set: async (values: Record<string, unknown>) => {
        for (const [key, value] of Object.entries(values)) stored.set(key, structuredClone(value));
      },
      remove: async (keys: string[]) => { for (const key of keys) stored.delete(key); },
    } } });
    const startSession = async (page: ReturnType<typeof openPage>) => {
      for (const name of ["window", "document", "location", "sessionStorage", "MutationObserver", "Element", "HTMLElement"] as const) {
        vi.stubGlobal(name, name === "window" ? page.window : page.window[name]);
      }
      const root = page.window.document.createElement("div");
      const session = new GameSession(root, vi.fn(), page.snapshot()!.gameKey);
      sessions.push(session);
      await session.start();
      return session;
    };
    const first = openPage();
    const original = await startSession(first);
    appendPublicDiceRoll(original.diceHistory, {
      actor: "Alice", total: 7, dice: [3, 4], eventId: "saved-roll", logIndex: 0,
    });
    original.stop();
    sessions.splice(sessions.indexOf(original), 1);
    await new Promise((resolve) => setTimeout(resolve, 0));

    const reload = openPage(first.saved());
    expect(reload.snapshot()?.gameKey).toBe(first.snapshot()?.gameKey);
    const restored = await startSession(reload);
    expect(restored.diceHistory.rolls).toHaveLength(1);
    restored.stop();
    sessions.splice(sessions.indexOf(restored), 1);
    await new Promise((resolve) => setTimeout(resolve, 0));

    const nextGame = openPage(reload.saved(), 0);
    expect(nextGame.snapshot()?.gameKey).not.toBe(reload.snapshot()?.gameKey);
    const fresh = await startSession(nextGame);
    expect(fresh.diceHistory.rolls).toHaveLength(0);
    expect(fresh.events).toHaveLength(0);
    const stableKey = nextGame.snapshot()?.gameKey;
    nextGame.refresh();
    expect(nextGame.snapshot()?.gameKey).toBe(stableKey);
  });
});
