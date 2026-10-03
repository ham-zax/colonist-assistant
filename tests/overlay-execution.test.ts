// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AssistantOverlay } from "../src/content/overlay";
import * as guide from "../src/content/action-guide";
import type { NextClick } from "../src/content/action-guide";
import { DEFAULT_SETTINGS } from "../src/content/settings";
import type { DecisionSearchConstraints } from "../src/core/engine";
import type { DecisionTraceRecorder } from "../src/core/decision-trace";
import type { BoardSnapshot } from "../src/core/placement";
import { emptyResources } from "../src/core/resources";
import { createTrackerState, reduceTracker } from "../src/core/tracker";
import type { TrackerState } from "../src/core/types";

type Internals = {
  board?: BoardSnapshot;
  decisionAnalysis: unknown;
  decisionKey: string;
  decisionPendingKey: string;
  decisionContextInvalidated: boolean;
  decisionRuntimeError: string;
  developmentSnapshotWait?: { startedAt: number; refreshes: number };
  decisionTraces: DecisionTraceRecorder;
  decisionWorker: { reset: () => void };
  decisionSearchConstraints(): DecisionSearchConstraints;
  warmDecisionEngine(): void;
  reconciledState(): TrackerState | undefined;
  spatialRecommendation(): undefined;
  scheduleDecisionAnalysis(state?: TrackerState, player?: string): void;
  coachReport(): undefined;
  currentDecisionRationale(): undefined;
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

  const bankAction: NextClick = {
    kind: "trade-builder", mode: "bank",
    give: { ...emptyResources(), wool: 4 },
    receive: { ...emptyResources(), grain: 1 },
    label: "Open recommended bank trade", signature: "bank-trade-guard", confidence: 1,
  };
  const endAction: NextClick = {
    kind: "turn-control", control: "end", label: "End turn", signature: "alternative-end", confidence: 1,
  };
  const recoveryFixture = () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS, recordGame: false, autonomousPrivateGames: true, autopilotDelaySeconds: 0 },
      { reset: vi.fn() },
    );
    overlays.push(overlay);
    const view = overlay as unknown as Internals;
    const reset = vi.spyOn(view.decisionWorker, "reset");
    let state = reduceTracker(createTrackerState(), { type: "discover", player: "You" });
    state = reduceTracker(state, { type: "discover", player: "Rival" });
    vi.spyOn(view, "reconciledState").mockReturnValue(state);
    vi.spyOn(view, "spatialRecommendation").mockReturnValue(undefined);
    vi.spyOn(view, "coachReport").mockReturnValue(undefined);
    vi.spyOn(view, "currentDecisionRationale").mockReturnValue(undefined);
    let chosen: NextClick = bankAction;
    // Model the search seam: the engine chooses an alternative legal root only
    // when the production overlay supplies the dispatched bank bundle constraint.
    const search = vi.spyOn(view, "scheduleDecisionAnalysis").mockImplementation(() => {
      const exclusions = view.decisionSearchConstraints().rootExclusions ?? [];
      chosen = exclusions.some((action) => action.kind === "maritime-trade" &&
        action.give.wool === 4 && action.receive.grain === 1) ? endAction : bankAction;
      view.decisionAnalysis = { deepSearch: { chosen: chosen === bankAction
        ? { kind: "maritime-trade", cards: [0, 0, 4, 0, 0], receiveCards: [0, 0, 0, 1, 0] }
        : { kind: "end-turn" } } };
    });
    vi.spyOn(view, "nextClick").mockImplementation(() => chosen);
    const before = { ...baseline(), ownHand: { ...emptyResources(), wool: 4 } };
    overlay.updateBoard(before);
    view.render();
    const options = vi.mocked(guide.renderActionGuide).mock.calls.at(-1)![1];
    const unconfirmed = () => options.onExecution?.({
      succeeded: false, unconfirmed: true, signature: bankAction.signature,
      reason: "Colonist did not commit the submitted trade after bounded observation",
      diagnostic: { actionKind: "trade-builder" },
    });
    return { overlay, view, reset, before, unconfirmed, search };
  };

  it("replans an unconfirmed bank bundle to another autonomous legal root", () => {
    const { view, reset, unconfirmed } = recoveryFixture();
    unconfirmed();
    expect(reset).toHaveBeenCalled();
    expect(view.decisionSearchConstraints().rootExclusions).toEqual([{
      kind: "maritime-trade", give: bankAction.kind === "trade-builder" ? bankAction.give : {},
      receive: bankAction.kind === "trade-builder" ? bankAction.receive : {},
    }]);
    const next = vi.mocked(guide.renderActionGuide).mock.calls.at(-1)!;
    expect(next[0]).toEqual(endAction);
    expect(next[1].autonomous).toBe(true);
  });

  it("releases the real submitted workflow before replanning and clicking End Turn", async () => {
    vi.mocked(guide.renderActionGuide).mockRestore();
    const rendered = vi.spyOn(guide, "renderActionGuide");
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 20, y: 20, left: 20, top: 20, right: 100, bottom: 60,
      width: 80, height: 40, toJSON: () => ({}),
    });
    const open = document.createElement("button");
    open.id = "action-button-trade";
    const submitted = vi.fn();
    open.addEventListener("click", () => {
      const existing = document.querySelector(".bankTradePanel-fixture");
      if (existing) { existing.remove(); return; }
      const panel = document.createElement("div");
      panel.className = "bankTradePanel-fixture";
      panel.innerHTML = `
        <div class="bankTradeAvailableCards-fixture"><button><img src="card_wool.svg"></button></div>
        <div class="bankTradeReceiveCards-fixture"><button><img src="card_grain.svg"></button></div>
        <div class="proposalOfferedHalfContainer-fixture"></div>
        <div class="proposalWantedHalfContainer-fixture"></div>
        <button id="action-button-trade-bank">Trade with bank</button>`;
      panel.querySelector(".bankTradeAvailableCards-fixture button")!.addEventListener("click", () => {
        panel.querySelector(".proposalOfferedHalfContainer-fixture")!.insertAdjacentHTML("beforeend",
          '<button data-card-enum="3"><img src="card_wool.svg"></button>');
      });
      panel.querySelector(".bankTradeReceiveCards-fixture button")!.addEventListener("click", () => {
        panel.querySelector(".proposalWantedHalfContainer-fixture")!.innerHTML =
          '<button data-card-enum="4"><img src="card_grain.svg"></button>';
      });
      panel.querySelector("#action-button-trade-bank")!.addEventListener("click", submitted);
      document.body.append(panel);
    });
    const end = document.createElement("button");
    end.textContent = "End turn";
    const ended = vi.fn();
    end.addEventListener("click", ended);
    document.body.append(open, end);
    const { view } = recoveryFixture();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(submitted).toHaveBeenCalledOnce();
    expect(guide.hasPendingTradeOutcome()).toBe(true);
    expect(ended).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(20_000);
    expect(guide.hasPendingTradeOutcome()).toBe(false);
    expect(view.decisionSearchConstraints().rootExclusions).toHaveLength(1);
    expect(rendered.mock.calls.at(-1)![0]).toEqual(endAction);
    expect(rendered.mock.calls.at(-1)![1].autonomous).toBe(true);
    expect(ended).toHaveBeenCalledOnce();
    expect(submitted).toHaveBeenCalledOnce();
    expect(document.querySelector(".bankTradePanel-fixture")).toBeNull();
  });

  it.each(["hand", "turn", "game", "out-of-turn"] as const)(
    "releases an ambiguous bank exclusion after a meaningful %s change",
    (change) => {
      const { overlay, view, before, unconfirmed } = recoveryFixture();
      unconfirmed();
      expect(view.decisionSearchConstraints().rootExclusions).toHaveLength(1);
      const after = structuredClone(before);
      if (change === "hand") after.ownHand = { ...emptyResources(), grain: 1 };
      if (change === "turn") after.turn! += 1;
      if (change === "game") after.gameKey = "new-game";
      if (change === "out-of-turn") { after.isMyTurn = false; after.currentPlayer = "Rival"; }
      overlay.updateBoard(after);
      expect(view.decisionSearchConstraints().rootExclusions).toBeUndefined();
      overlay.updateBoard(before);
      const next = vi.mocked(guide.renderActionGuide).mock.calls.at(-1)!;
      expect(next[0]).toEqual(bankAction);
      expect(next[1].autonomous).toBe(true);
    },
  );

  it.each(["hand", "turn", "game", "out-of-turn"] as const)(
    "does not attach a late callback to the replacement %s scope",
    (change) => {
      const { overlay, view, before, unconfirmed } = recoveryFixture();
      const after = structuredClone(before);
      if (change === "hand") after.ownHand = { ...emptyResources(), grain: 1 };
      if (change === "turn") after.turn! += 1;
      if (change === "game") after.gameKey = "new-game";
      if (change === "out-of-turn") { after.isMyTurn = false; after.currentPlayer = "Rival"; }
      overlay.updateBoard(after);
      unconfirmed();
      expect(view.decisionSearchConstraints().rootExclusions).toBeUndefined();
    },
  );

  it("retains the original exclusion across a temporarily missing exact hand", () => {
    const { overlay, view, before, unconfirmed } = recoveryFixture();
    unconfirmed();
    overlay.updateBoard({ ...before, ownHand: undefined });
    expect(view.decisionSearchConstraints().rootExclusions).toBeUndefined();
    overlay.updateBoard(before);
    expect(view.decisionSearchConstraints().rootExclusions).toHaveLength(1);
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
