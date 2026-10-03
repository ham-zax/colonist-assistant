// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AssistantOverlay } from "../src/content/overlay";
import * as guide from "../src/content/action-guide";
import type { NextClick } from "../src/content/action-guide";
import { DEFAULT_SETTINGS } from "../src/content/settings";
import type { DecisionAnalysis } from "../src/core/engine";
import type { BoardSnapshot } from "../src/core/placement";
import { emptyResources } from "../src/core/resources";
import { createTrackerState, reduceTracker } from "../src/core/tracker";

type SpatialLike = {
  action: string;
  recommendation: { id: string };
  proactive: boolean;
} | undefined;

type Internals = {
  board?: BoardSnapshot;
  decisionAnalysis?: DecisionAnalysis;
  decisionRuntimeError: string;
  developmentSnapshotWait?: { startedAt: number; refreshes: number };
  freeRoadPlan?: { gameKey?: string; edgeIds: string[] };
  decisionTraces: { complete(...args: never[]): void };
  decisionWorker: { request: (...args: never[]) => string };
  reconciledState(): unknown;
  coachReport(): undefined;
  scheduleDecisionAnalysis(state?: unknown, player?: string): void;
  spatialRecommendation(state?: unknown): SpatialLike;
  nextClick(
    state: unknown,
    spatial: SpatialLike,
    report: undefined,
  ): NextClick | undefined;
  render(): void;
};

const overlays: AssistantOverlay[] = [];

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("chrome", {
    runtime: { getURL: (path: string) => `chrome-extension://fixture/${path}`, getManifest: () => ({ version: "0.9.2" }) },
    storage: { local: { get: async () => ({}), set: async () => undefined, remove: async () => undefined }, sync: { set: async () => undefined } },
  });
  vi.spyOn(AssistantOverlay.prototype as unknown as { warmDecisionEngine(): void }, "warmDecisionEngine").mockImplementation(() => {});
  vi.spyOn(guide, "renderActionGuide").mockImplementation(() => {});
});

afterEach(() => {
  for (const overlay of overlays.splice(0)) overlay.destroy();
  guide.destroyActionGuide();
  document.body.replaceChildren();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

const vertex = (id: string, adjacentVertices: string[] = []) => ({
  id,
  adjacentHexes: [] as string[],
  adjacentVertices,
});

const edge = (id: string, vertices: [string, string], player?: string) => ({
  id,
  vertices,
  screen: { x: 10, y: 20 },
  ...(player ? { player } : {}),
});

/** Post-Road-Building free-road prompt: card consumed, first prompt open. */
const roadPrompt = (overrides?: Partial<BoardSnapshot>): BoardSnapshot => ({
  gameKey: "rb-game",
  turn: 91,
  diceMode: "random",
  currentPlayer: "You",
  myPlayer: "You",
  isMyTurn: true,
  hasRolled: false,
  action: "road",
  hexes: [],
  vertices: [
    vertex("v0", ["v1"]),
    vertex("v1", ["v0", "v2"]),
    vertex("v2", ["v1", "v3"]),
    vertex("v3", ["v2"]),
  ],
  edges: [
    edge("e-stale", ["v0", "v1"]),
    edge("e-second", ["v1", "v2"]),
    edge("e-other", ["v2", "v3"]),
  ],
  ownHand: emptyResources(),
  ownDevelopmentCards: {
    cards: { knight: 0, monopoly: 0, "road-building": 0, "year-of-plenty": 0, "victory-point": 0 },
    playable: { knight: 0, monopoly: 0, "road-building": 0, "year-of-plenty": 0, "victory-point": 0 },
    boughtThisTurn: { knight: 0, monopoly: 0, "road-building": 0, "year-of-plenty": 0, "victory-point": 0 },
    hasPlayedThisTurn: true,
  },
  players: {
    You: {
      handSize: 0,
      tradeRatios: emptyResources(),
      cardDiscardLimit: 7,
      playedDevelopmentCards: { knight: 0, monopoly: 0, "road-building": 1, "year-of-plenty": 0, "victory-point": 0 },
    },
  },
  legalEdgeIds: ["e-second", "e-other"],
  localSeatDiagnostics: {
    seatSource: "gameController.myColor+currentUserId+gameUserStates",
    localActionState: 30,
    identity: { status: "resolved", reason: "cross-checked", source: "controller+account-user-id+store-roster", currentUserIdAvailable: true, currentUserMatchColors: [1] },
  },
  ...overrides,
});

const roadBuildingAnalysis = (targetId: string, secondTargetId?: string): DecisionAnalysis =>
  ({
    engine: "deep-search",
    players: [],
    actionScores: { road: 1, settlement: 0, city: 0, development: 0 },
    simulations: 1,
    model: "test",
    runtime: "background-wasm",
    deepSearch: {
      chosen: {
        kind: "play-road-building",
        targetId,
        ...(secondTargetId ? { secondTargetId } : {}),
      },
    },
  }) as unknown as DecisionAnalysis;

const setupOverlay = () => {
  const overlay = new AssistantOverlay(
    { ...DEFAULT_SETTINGS, recordGame: false },
    { reset: vi.fn() },
  );
  overlays.push(overlay);
  return overlay as unknown as Internals;
};

const tick = (view: Internals, state: unknown): NextClick | undefined => {
  // Mirror the overlay render order: spatial, then scheduler, then click.
  const spatial = view.spatialRecommendation(undefined);
  view.scheduleDecisionAnalysis(state, "You");
  return view.nextClick(undefined, spatial, undefined);
};

describe("road building free-road continuation", () => {
  it("places both retained free roads in order when both stay legal", () => {
    const view = setupOverlay();
    const state = reduceTracker(createTrackerState(), { type: "discover", player: "You" });
    view.board = roadPrompt({ legalEdgeIds: ["e-stale", "e-second", "e-other"] });
    view.freeRoadPlan = { gameKey: "rb-game", edgeIds: ["e-stale", "e-second"] };

    const first = tick(view, state);
    expect(first).toMatchObject({ kind: "board", boardAction: "road", targetId: "e-stale" });

    // First road observed on the board: the plan advances and the second
    // prompt is answered without replaying the card.
    view.board = roadPrompt({
      edges: [
        edge("e-stale", ["v0", "v1"], "You"),
        edge("e-second", ["v1", "v2"]),
        edge("e-other", ["v2", "v3"]),
      ],
      legalEdgeIds: ["e-second", "e-other"],
    });
    const second = tick(view, state);
    expect(second).toMatchObject({ kind: "board", boardAction: "road", targetId: "e-second" });
    expect(second?.kind).not.toBe("development");
  });

  it("clicks the retained first road in the same render the hand sync clears (before-roll play)", () => {
    const view = setupOverlay();
    let state = reduceTracker(createTrackerState(), { type: "discover", player: "You" });
    state = reduceTracker(state, { type: "play-dev", player: "You", card: "road-building" });
    vi.spyOn(view, "reconciledState").mockReturnValue(state);
    vi.spyOn(view, "coachReport").mockReturnValue(undefined);
    const request = vi.spyOn(view.decisionWorker, "request").mockReturnValue("duplicate");
    const rendered = vi.mocked(guide.renderActionGuide);
    // Before-roll free-road prompt with a coherent hand: the card is consumed
    // both publicly and privately, and the retained head is legal.
    view.board = roadPrompt({
      hasRolled: false,
      legalEdgeIds: ["e-stale", "e-second", "e-other"],
    });
    view.freeRoadPlan = { gameKey: "rb-game", edgeIds: ["e-stale", "e-second"] };
    const coherent = view.board;
    view.decisionAnalysis = roadBuildingAnalysis("e-stale", "e-second");
    // The public play arrives before the board and exact hand catch up.
    view.board = {
      ...coherent,
      action: "none",
      ownDevelopmentCards: {
        ...coherent.ownDevelopmentCards!,
        cards: { ...coherent.ownDevelopmentCards!.cards, "road-building": 1 },
        hasPlayedThisTurn: false,
      },
      players: { You: {
        ...coherent.players!.You!,
        playedDevelopmentCards: {
          ...coherent.players!.You!.playedDevelopmentCards!,
          "road-building": 0,
        },
      } },
    };
    view.render();
    expect(view.developmentSnapshotWait).toBeDefined();
    expect(rendered.mock.calls.at(-1)?.[0]).toBeUndefined();

    // The next snapshot contains the consumed card and first-road prompt.
    view.board = coherent;

    view.render();

    // Pre-fix the guide receives undefined in this render and nothing
    // re-renders once the error clears, idling until an unrelated board
    // change. Post-fix the retained first road is recommended immediately.
    const last = rendered.mock.calls.at(-1)?.[0];
    expect(last).toMatchObject({ kind: "board", boardAction: "road", targetId: "e-stale" });
    expect(view.developmentSnapshotWait).toBeUndefined();
    expect(request).not.toHaveBeenCalled();

    // Observing the first road advances the retained plan to the second.
    view.board = { ...coherent,
      edges: coherent.edges.map((candidate) =>
        candidate.id === "e-stale" ? { ...candidate, player: "You" } : candidate),
      legalEdgeIds: ["e-second"],
      localSeatDiagnostics: { ...coherent.localSeatDiagnostics!, localActionState: 31 },
    };
    view.render();
    expect(rendered.mock.calls.at(-1)?.[0]).toMatchObject({
      kind: "board", boardAction: "road", targetId: "e-second",
    });
    expect(request).not.toHaveBeenCalled();
  });
});
