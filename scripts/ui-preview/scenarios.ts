import { AssistantOverlay } from "../../src/content/overlay";
import { DEFAULT_SETTINGS, type AssistantSettings } from "../../src/content/settings";
import type { GameSession } from "../../src/content/session";
import type { DecisionAnalysis } from "../../src/core/engine";
import type { BoardSnapshot } from "../../src/core/placement";
import type { TrackerState } from "../../src/core/types";
import {
  ME,
  LONG,
  MIRA,
  KASPER,
  actionStats,
  buildTracker,
  freeEdgesTouching,
  generateBoard,
  incomingTrade,
  makeAnalysis,
  makeBoard,
  makeDeepSearch,
  midGamePlacements,
  openVertexIds,
  populateBoard,
  res,
  type GeneratedBoard,
} from "./fixtures";

export const SCENARIO_NAMES = [
  "empty",
  "build",
  "alternatives",
  "spatial",
  "trade",
  "discard",
  "opponent-turn",
  "thinking",
  "paused",
  "settings",
  "details",
  "collapsed",
] as const;
export type ScenarioName = (typeof SCENARIO_NAMES)[number];

export const SCENARIO_DESCRIPTIONS: Record<ScenarioName, string> = {
  empty: "No game yet (waiting state)",
  build: "My turn: save for a city, card matrix, win %, bank row",
  alternatives: "Build a city now with the TOP MOVES panel (showAlternatives)",
  spatial: "Opening settlement placement with board marker",
  trade: "Incoming trade offer: accept verdict",
  discard: "Seven rolled: discard advice",
  "opponent-turn": "Another player's turn (long name), waiting state",
  thinking: "Decision pending: engine is searching",
  paused: "Engine runtime error: strategist paused",
  settings: "Settings view",
  details: "Details view with a 30-roll dice distribution",
  collapsed: "Collapsed header only",
};

interface Internals {
  board?: BoardSnapshot;
  session?: Partial<GameSession>;
  decisionAnalysis?: DecisionAnalysis;
  decisionKey: string;
  decisionPendingKey: string;
  decisionRuntime?: "background-wasm";
  decisionRuntimeError: string;
  decisionRuntimeDetail: string;
  activeView: "advice" | "details" | "settings";
  scheduleDecisionAnalysis: () => void;
}

export interface MountedScenario {
  overlay: AssistantOverlay;
  board?: BoardSnapshot;
  state?: TrackerState;
}

const previewSettings = (
  overrides: Partial<AssistantSettings> = {},
): AssistantSettings => ({
  ...DEFAULT_SETTINGS,
  // The next-action highlight targets Colonist's own DOM, which this page lacks.
  highlightNextAction: false,
  ...overrides,
});

interface ScenarioInput {
  settings?: Partial<AssistantSettings>;
  board?: BoardSnapshot;
  state?: TrackerState;
  analysis?: DecisionAnalysis;
  pendingKey?: string;
  runtimeError?: string;
  view?: Internals["activeView"];
}

const mount = (input: ScenarioInput): MountedScenario => {
  const overlay = new AssistantOverlay(previewSettings(input.settings), {
    reset: () => undefined,
  });
  const internals = overlay as unknown as Internals;
  // Only inputs are stubbed. Search scheduling is a no-op so the preview keeps
  // the exact decision state each scenario injects instead of calling a worker.
  internals.scheduleDecisionAnalysis = () => undefined;
  internals.decisionRuntime = "background-wasm";
  internals.decisionRuntimeDetail = "Packaged search engine ready.";
  internals.board = input.board;
  internals.decisionAnalysis = input.analysis;
  internals.decisionKey = input.analysis ? "ui-preview-decision" : "";
  internals.decisionPendingKey = input.pendingKey ?? "";
  internals.decisionRuntimeError = input.runtimeError ?? "";
  if (input.view) internals.activeView = input.view;
  overlay.update(
    input.state
      ? ({ state: input.state, partialHistory: false } as unknown as GameSession)
      : undefined,
  );
  return { overlay, board: input.board, state: input.state };
};

/** Mid-game position shared by most scenarios. */
const midGame = (ownHand = res({ lumber: 1, grain: 2, ore: 2 })) => {
  const geometry: GeneratedBoard = populateBoard(generateBoard(), midGamePlacements());
  const mine = geometry.vertices
    .filter((vertex) => vertex.building?.player === ME)
    .map((vertex) => vertex.id);
  return {
    geometry,
    mine,
    ownHand,
    state: buildTracker({ ownHand, rolls: 30 }),
    boardFor: (overrides: Partial<BoardSnapshot> = {}): BoardSnapshot =>
      makeBoard({
        geometry,
        ownHand,
        overrides: {
          buildableCityIds: mine,
          buildableSettlementIds: openVertexIds(geometry).filter((id) =>
            freeEdgesTouching(geometry, ME).some((edgeId) =>
              geometry.edges.find((edge) => edge.id === edgeId)!.vertices.includes(id),
            ),
          ),
          buildableRoadIds: freeEdgesTouching(geometry, ME),
          ...overrides,
        },
      }),
  };
};

const WIN_ODDS = [0.31, 0.29, 0.22, 0.18];

const scenarios: Record<ScenarioName, () => MountedScenario> = {
  empty: () => mount({}),

  build: () => {
    const game = midGame();
    return mount({
      board: game.boardFor(),
      state: game.state,
      analysis: makeAnalysis(WIN_ODDS),
    });
  },

  alternatives: () => {
    const game = midGame(res({ lumber: 1, grain: 2, ore: 3 }));
    const [first, second] = game.mine;
    const cityA = { kind: "build-city", targetId: first! };
    const cityB = { kind: "build-city", targetId: second! };
    const board = game.boardFor();
    const deepSearch = makeDeepSearch(cityA, [
      actionStats(cityA, 0.412),
      actionStats(cityB, 0.396),
      actionStats({ kind: "buy-development" }, 0.371),
      actionStats({ kind: "end-turn" }, 0.322),
    ]);
    return mount({
      settings: { showAlternatives: true },
      board,
      state: game.state,
      analysis: makeAnalysis(WIN_ODDS, deepSearch),
    });
  },

  spatial: () => {
    const order = [LONG, MIRA, ME, KASPER] as const;
    const geometry = populateBoard(generateBoard(), [
      { player: LONG, vertex: "v20", roadTo: "v21" },
      { player: MIRA, vertex: "v41", roadTo: "v42" },
    ]);
    const ownHand = res();
    const board = makeBoard({
      geometry,
      ownHand,
      playerOverrides: {
        [ME]: { handSize: 0, visiblePoints: 0, longestRoad: 0 },
        [LONG]: { handSize: 0, visiblePoints: 1, developmentCards: 0, hasLargestArmy: false },
        [MIRA]: { handSize: 0, visiblePoints: 1, longestRoad: 0 },
        [KASPER]: { handSize: 0, visiblePoints: 0, developmentCards: 0 },
      },
      overrides: {
        playerOrder: [...order],
        action: "settlement",
        initialPlacement: true,
        hasRolled: false,
        turn: 3,
        legalVertexIds: openVertexIds(geometry),
        bank: res({ lumber: 19, brick: 19, wool: 19, grain: 19, ore: 19 }),
      },
    });
    return mount({
      board,
      state: buildTracker({ ownHand, setup: true, order }),
      analysis: makeAnalysis([0.26, 0.26, 0.25, 0.23]),
    });
  },

  trade: () => {
    const game = midGame();
    const offer = incomingTrade({
      creatorGive: res({ ore: 1 }),
      creatorReceive: res({ lumber: 1 }),
    });
    const board = game.boardFor({
      isMyTurn: false,
      currentPlayer: LONG,
      activeTrades: [offer],
    });
    const accept = { kind: "respond-trade", tradeId: offer.id, accept: true };
    return mount({
      board,
      state: game.state,
      analysis: makeAnalysis(
        WIN_ODDS,
        makeDeepSearch(accept, [
          actionStats(accept, 0.36),
          actionStats({ kind: "respond-trade", tradeId: offer.id, accept: false }, 0.33),
        ]),
      ),
    });
  },

  discard: () => {
    const ownHand = res({ lumber: 2, brick: 1, wool: 1, grain: 3, ore: 2 });
    const game = midGame(ownHand);
    const cards: [number, number, number, number, number] = [1, 0, 1, 1, 1];
    const chosen = { kind: "discard", cards };
    const board = game.boardFor({ action: "discard", discardCount: 4 });
    return mount({
      board,
      state: game.state,
      analysis: makeAnalysis(
        WIN_ODDS,
        makeDeepSearch(chosen, [
          actionStats(chosen, 0.34),
          actionStats({ kind: "discard", cards: [0, 1, 1, 1, 1] }, 0.3),
        ]),
      ),
    });
  },

  "opponent-turn": () => {
    const game = midGame();
    return mount({
      board: game.boardFor({ isMyTurn: false, currentPlayer: LONG }),
      state: game.state,
    });
  },

  thinking: () => {
    const game = midGame();
    return mount({
      board: game.boardFor(),
      state: game.state,
      pendingKey: "ui-preview-pending",
    });
  },

  paused: () => {
    const game = midGame();
    return mount({
      board: game.boardFor(),
      state: game.state,
      runtimeError:
        "Strategist worker stopped unexpectedly. No other algorithm was substituted.",
    });
  },

  settings: () => {
    const game = midGame();
    return mount({
      board: game.boardFor(),
      state: game.state,
      analysis: makeAnalysis(WIN_ODDS),
      view: "settings",
    });
  },

  details: () => {
    const game = midGame();
    return mount({
      board: game.boardFor(),
      state: game.state,
      analysis: makeAnalysis(WIN_ODDS),
      view: "details",
    });
  },

  collapsed: () => {
    const game = midGame();
    return mount({
      settings: { startCollapsed: true },
      board: game.boardFor(),
      state: game.state,
      analysis: makeAnalysis(WIN_ODDS),
    });
  },
};

export const isScenarioName = (value: string | null): value is ScenarioName =>
  (SCENARIO_NAMES as readonly string[]).includes(value ?? "");

export const mountScenario = (name: ScenarioName): MountedScenario =>
  scenarios[name]();
