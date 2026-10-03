// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AssistantOverlay } from "../src/content/overlay";
import * as guide from "../src/content/action-guide";
import type { NextClick } from "../src/content/action-guide";
import { DEFAULT_SETTINGS } from "../src/content/settings";
import type { DecisionTraceRecorder } from "../src/core/decision-trace";
import type { BoardSnapshot } from "../src/core/placement";
import { emptyResources } from "../src/core/resources";
import { createTrackerState, reduceTracker } from "../src/core/tracker";
import type { TrackerState } from "../src/core/types";

type Internals = {
  board?: BoardSnapshot;
  decisionKey: string;
  decisionContextInvalidated: boolean;
  decisionRuntimeError: string;
  developmentSnapshotWait?: { startedAt: number; refreshes: number };
  decisionTraces: DecisionTraceRecorder;
  warmDecisionEngine(): void;
  reconciledState(): TrackerState | undefined;
  spatialRecommendation(): undefined;
  scheduleDecisionAnalysis(state?: TrackerState, player?: string): void;
  coachReport(): undefined;
  nextClick(): NextClick | undefined;
  render(): void;
  runtimePresentation(): { label: string; state: string };
};
const overlays: AssistantOverlay[] = [];

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("chrome", {
    runtime: { getURL: (path: string) => `chrome-extension://fixture/${path}`, getManifest: () => ({ version: "0.9.2" }) },
    storage: { local: { get: async () => ({}), set: async () => undefined, remove: async () => undefined }, sync: { set: async () => undefined } },
  });
  vi.spyOn(AssistantOverlay.prototype as unknown as Internals, "warmDecisionEngine").mockImplementation(() => {});
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

const baseline = (): BoardSnapshot => ({
  gameKey: "same-game", turn: 91, diceMode: "random", currentPlayer: "You", myPlayer: "You", isMyTurn: true,
  hasRolled: true, action: "none", hexes: [],
  vertices: [{ id: "vertex", adjacentHexes: [], adjacentVertices: [] }],
  edges: [{ id: "edge", vertices: ["vertex", "vertex"] }],
  ownHand: emptyResources(),
  ownDevelopmentCards: {
    cards: { knight: 0, monopoly: 1, "road-building": 0, "year-of-plenty": 0, "victory-point": 0 },
    playable: { knight: 0, monopoly: 1, "road-building": 0, "year-of-plenty": 0, "victory-point": 0 },
    boughtThisTurn: { knight: 0, monopoly: 0, "road-building": 0, "year-of-plenty": 0, "victory-point": 0 },
    hasPlayedThisTurn: false,
  },
  localSeatDiagnostics: {
    seatSource: "gameController.myColor+currentUserId+gameUserStates",
    identity: { status: "resolved", reason: "cross-checked", source: "controller+account-user-id+store-roster", currentUserIdAvailable: true, currentUserMatchColors: [1] },
  },
});

describe("overlay execution reconciliation", () => {
  it.each(["incoming", "outgoing", "incoming-without-log"] as const)(
    "closes a disabled %s trade immediately despite a thinking delay and stale engine error",
    (kind) => {
      vi.mocked(guide.renderActionGuide).mockRestore();
      const rendered = vi.spyOn(guide, "renderActionGuide");
      vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
        x: 20, y: 20, left: 20, top: 20, right: 100, bottom: 60,
        width: 80, height: 40, toJSON: () => ({}),
      });
      const incoming = kind !== "outgoing";
      document.body.innerHTML = `<div class="gameTradeOffersWrapper-fixture"><div class="tradeContainer-fixture">
        <span>${incoming ? "Rival" : "You"}</span><button class="tradeButton-fixture">${incoming ? "Decline trade" : "Cancel offer"}</button>
      </div></div>`;
      const clicked = vi.fn();
      document.querySelector("button")!.addEventListener("click", clicked);
      const overlay = new AssistantOverlay({
        ...DEFAULT_SETTINGS, recordGame: false, disablePlayerTrades: true,
        autonomousPrivateGames: true, autopilotDelaySeconds: 5,
      }, { reset: vi.fn() });
      overlays.push(overlay);
      const view = overlay as unknown as Internals;
      const state = reduceTracker(createTrackerState(), { type: "discover", player: "You" });
      vi.spyOn(view, "reconciledState").mockReturnValue(kind === "incoming-without-log" ? undefined : state);
      vi.spyOn(view, "spatialRecommendation").mockReturnValue(undefined);
      vi.spyOn(view, "coachReport").mockReturnValue(undefined);
      view.board = { ...baseline(), activeTrades: [{
        id: "disabled-offer", creator: incoming ? "Rival" : "You", tradeExecutor: incoming ? "Rival" : "You",
        creatorGive: emptyResources(), creatorReceive: emptyResources(), incoming,
        counterOffer: false, canAccept: false, myResponse: incoming ? "pending" : undefined,
        responsesComplete: false,
      }] };
      view.decisionRuntimeError = "Balanced Dice history is incomplete";
      view.render();
      expect(clicked).toHaveBeenCalledOnce();
      expect(rendered.mock.calls.at(-1)![1].autopilotDelayMs).toBe(0);
      const signature = rendered.mock.calls.at(-1)![0]!.signature;
      view.board.ownHand = { ...emptyResources(), lumber: 1 };
      view.board.activeTrades![0]!.acceptedPlayers = ["Rival"];
      view.render();
      expect(rendered.mock.calls.at(-1)![0]!.signature).toBe(signature);
      vi.advanceTimersByTime(1_000);
      expect(clicked).toHaveBeenCalledOnce();
    },
  );

  it("bounds refreshes for a stale post-play hand and resumes only after coherent evidence", () => {
    const overlay = new AssistantOverlay({ ...DEFAULT_SETTINGS, recordGame: false }, { reset: vi.fn() });
    overlays.push(overlay);
    const view = overlay as unknown as Internals;
    const before = baseline();
    before.players = { You: {
      handSize: 0, tradeRatios: emptyResources(), cardDiscardLimit: 7,
      playedDevelopmentCards: { ...before.ownDevelopmentCards!.cards, monopoly: 1 },
    } };
    const state = reduceTracker(createTrackerState(), { type: "discover", player: "You" });
    state.players.You!.playedDevCards.monopoly = 2;
    view.board = before;
    vi.spyOn(view, "render").mockImplementation(() => view.scheduleDecisionAnalysis(state, "You"));
    const refresh = vi.fn();
    window.addEventListener("colonist-assistant-board-refresh", refresh);
    try {
      view.scheduleDecisionAnalysis(state, "You");
      expect(view.decisionRuntimeError).toContain("Waiting for your development-card hand");
      expect(view.runtimePresentation()).toMatchObject({ label: "Hand sync", state: "searching" });
      vi.advanceTimersByTime(6_000);
      expect(refresh).toHaveBeenCalledTimes(11);
      expect(view.decisionRuntimeError).toContain("Refresh the game tab to recover");
      expect(view.runtimePresentation()).toMatchObject({ label: "Reload tab", state: "error" });
      vi.advanceTimersByTime(10_000);
      expect(refresh).toHaveBeenCalledTimes(11);
      view.board = { ...before, players: { You: {
        ...before.players!.You!, playedDevelopmentCards: { ...before.ownDevelopmentCards!.cards, monopoly: 2 },
      } }, ownDevelopmentCards: {
        ...before.ownDevelopmentCards!, cards: { ...before.ownDevelopmentCards!.cards, monopoly: 0 }, hasPlayedThisTurn: true,
      } };
      view.decisionContextInvalidated = true;
      view.scheduleDecisionAnalysis(state, "You");
      expect(view.developmentSnapshotWait).toBeUndefined();
      expect(view.decisionRuntimeError).toBe("");
    } finally {
      window.removeEventListener("colonist-assistant-board-refresh", refresh);
    }
  });

  it.each(["end", "monopoly", "road", "city"] as const)(
    "confirms the original %s action when a new snapshot arrives after its guide disappears",
    (kind) => {
      const overlay = new AssistantOverlay({ ...DEFAULT_SETTINGS, recordGame: false }, { reset: vi.fn() });
      overlays.push(overlay);
      const view = overlay as unknown as Internals;
      let state = reduceTracker(createTrackerState(), { type: "discover", player: "You" });
      state = reduceTracker(state, { type: "discover", player: "Rival" });
      vi.spyOn(view, "reconciledState").mockReturnValue(state);
      vi.spyOn(view, "spatialRecommendation").mockReturnValue(undefined);
      vi.spyOn(view, "scheduleDecisionAnalysis").mockImplementation(() => {});
      vi.spyOn(view, "coachReport").mockReturnValue(undefined);
      const action: NextClick = kind === "end"
        ? { kind: "turn-control", control: "end", label: "End turn", signature: kind, confidence: 1 }
        : kind === "monopoly"
        ? { kind: "development", card: "monopoly", followupResources: ["lumber"], label: "Monopoly", signature: kind, confidence: 1 }
        : { kind: "board", boardAction: kind, targetId: kind === "road" ? "edge" : "vertex", point: { x: 1, y: 1 }, label: kind, signature: kind, confidence: 1 };
      const next = vi.spyOn(view, "nextClick").mockReturnValue(action);
      const before = baseline();
      overlay.updateBoard(before);
      view.decisionKey = `original-${kind}`;
      const selected = vi.spyOn(view.decisionTraces, "final");
      view.render();
      const call = vi.mocked(guide.renderActionGuide).mock.calls.at(-1)!;
      expect(call[0]?.signature).toBe(kind);
      const stateHash = selected.mock.calls.at(-1)![0];
      view.decisionTraces.begin(stateHash, state, before);
      view.render();
      const options = vi.mocked(guide.renderActionGuide).mock.calls.at(-1)![1];
      options.onExecutionStart?.({ signature: kind });
      next.mockReturnValue(undefined);
      view.render();
      const after = structuredClone(before);
      if (kind === "end") { after.turn = 92; after.currentPlayer = "Rival"; after.isMyTurn = false; }
      if (kind === "monopoly") after.ownDevelopmentCards!.cards.monopoly = 0;
      if (kind === "road") after.edges[0]!.player = "You";
      if (kind === "city") after.vertices[0]!.building = { kind: "city", player: "You" };
      overlay.updateBoard(after);
      expect(view.decisionTraces.snapshot(false).find((trace) => trace.stateHash === stateHash)).toMatchObject({
        lifecycleStatus: "execution-complete", executionSucceeded: true,
      });
    },
  );
});
