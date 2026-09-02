// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  AssistantOverlay,
  autonomousExecutionAllowed,
} from "../src/content/overlay";
import type { NextClick } from "../src/content/action-guide";
import { DEFAULT_SETTINGS } from "../src/content/settings";
import {
  createTrackerState,
  getPlayerEstimate,
  reduceTracker,
} from "../src/core/tracker";
import type { GameSession } from "../src/content/session";
import { emptyResources } from "../src/core/resources";

let sendMessage = vi.fn();

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
      getManifest: () => ({ version: "0.7.12" }),
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
  vi.stubGlobal(
    "getComputedStyle",
    () =>
      ({
        display: "block",
        visibility: "visible",
        opacity: "1",
      }) as CSSStyleDeclaration,
  );
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    x: 20,
    y: 20,
    left: 20,
    top: 20,
    right: 100,
    bottom: 60,
    width: 80,
    height: 40,
    toJSON: () => ({}),
  } as DOMRect);
});

afterEach(() => {
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("overlay settings interaction", () => {
  it("does not reread the extension manifest when settings rerender after context invalidation", () => {
    const getManifest = vi.fn(() => ({
      manifest_version: 3 as const,
      name: "Colonist Assistant",
      version: "0.7.12",
    }));
    chrome.runtime.getManifest = getManifest;

    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    const root = document.querySelector<HTMLDivElement>(
      "#colonist-assistant-root",
    )!;
    const shadow = root.shadowRoot!;

    shadow
      .querySelector<HTMLElement>("[data-action='view'][data-view='settings']")!
      .click();

    getManifest.mockImplementation(() => {
      throw new Error("Extension context invalidated.");
    });

    expect(() => overlay.update(undefined)).not.toThrow();
    expect(
      shadow.querySelector<HTMLElement>(".settings-version strong")?.textContent,
    ).toContain("v0.7.12");
    overlay.destroy();
  });

  it("allows autonomous clicks whenever autopilot is enabled", () => {
    expect(autonomousExecutionAllowed(false)).toBe(false);
    expect(autonomousExecutionAllowed(true)).toBe(true);
  });

  it("exposes an autopilot delay select with 0, 1, 3, and 5 second options", () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS, autonomousPrivateGames: true },
      { reset: vi.fn() },
    );
    const root = document.querySelector<HTMLDivElement>(
      "#colonist-assistant-root",
    )!;
    const shadow = root.shadowRoot!;
    shadow
      .querySelector<HTMLElement>("[data-action='view'][data-view='settings']")!
      .click();
    const select = shadow.querySelector<HTMLSelectElement>(
      "select[data-setting='autopilotDelaySeconds']",
    );
    expect(select).not.toBeNull();
    expect(
      [...(select?.options ?? [])].map((option) => option.value),
    ).toEqual(["0", "1", "3", "5"]);
    expect(select?.value).toBe("0");
    const autopilotLabel = shadow
      .querySelector<HTMLInputElement>(
        "input[data-setting='autonomousPrivateGames']",
      )
      ?.closest("label")
      ?.textContent;
    expect(autopilotLabel).toMatch(/any Colonist game/i);
    expect(autopilotLabel).not.toMatch(/private or bot/i);
    overlay.destroy();
  });

  it("exposes Disable player trades unchecked by default", () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    const root = document.querySelector<HTMLDivElement>(
      "#colonist-assistant-root",
    )!;
    const shadow = root.shadowRoot!;
    shadow
      .querySelector<HTMLElement>("[data-action='view'][data-view='settings']")!
      .click();
    const input = shadow.querySelector<HTMLInputElement>(
      "input[data-setting='disablePlayerTrades']",
    );
    expect(input).not.toBeNull();
    expect(input?.checked).toBe(false);
    expect(input?.closest("label")?.textContent).toMatch(
      /Only bank and port trades are allowed/i,
    );
    overlay.destroy();
  });

  it("blocks player exchange execution while leaving bank trade execution legal", () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS, disablePlayerTrades: true },
      { reset: vi.fn() },
    );
    const internals = overlay as unknown as {
      board: {
        hexes: [];
        vertices: [];
        edges: [];
        isMyTurn: boolean;
        action: "none";
        activeTrades: Array<{
          id: string;
          creator: string;
          tradeExecutor: string;
          creatorGive: ReturnType<typeof emptyResources>;
          creatorReceive: ReturnType<typeof emptyResources>;
          incoming: boolean;
          counterOffer: boolean;
          canAccept: boolean;
          acceptedPlayers?: string[];
        }>;
      };
      nextClickStillLegal: (next: NextClick) => boolean;
    };
    internals.board = {
      hexes: [],
      vertices: [],
      edges: [],
      isMyTurn: true,
      action: "none",
      activeTrades: [
        {
          id: "incoming",
          creator: "Rival",
          tradeExecutor: "Rival",
          creatorGive: emptyResources(),
          creatorReceive: emptyResources(),
          incoming: true,
          counterOffer: false,
          canAccept: true,
        },
        {
          id: "outgoing",
          creator: "You",
          tradeExecutor: "You",
          creatorGive: emptyResources(),
          creatorReceive: emptyResources(),
          incoming: false,
          counterOffer: false,
          canAccept: false,
          acceptedPlayers: ["Rival"],
        },
      ],
    };

    const common = { label: "trade", signature: "trade", confidence: 1 };
    expect(
      internals.nextClickStillLegal({
        ...common,
        kind: "trade",
        offerIndex: 0,
        tradeId: "incoming",
        verdict: "accept",
      }),
    ).toBe(false);
    expect(
      internals.nextClickStillLegal({
        ...common,
        kind: "trade",
        offerIndex: 0,
        tradeId: "incoming",
        verdict: "counter",
      }),
    ).toBe(false);
    expect(
      internals.nextClickStillLegal({
        ...common,
        kind: "trade-partner",
        offerIndex: 1,
        tradeId: "outgoing",
        acceptedIndex: 0,
        player: "Rival",
      }),
    ).toBe(false);
    expect(
      internals.nextClickStillLegal({
        ...common,
        kind: "trade-builder",
        mode: "player",
        give: emptyResources(),
        receive: emptyResources(),
      }),
    ).toBe(false);
    expect(
      internals.nextClickStillLegal({
        ...common,
        kind: "trade-builder",
        mode: "bank",
        give: emptyResources(),
        receive: emptyResources(),
      }),
    ).toBe(true);
    expect(
      internals.nextClickStillLegal({
        ...common,
        kind: "trade",
        offerIndex: 0,
        tradeId: "incoming",
        verdict: "decline",
      }),
    ).toBe(true);
    expect(
      internals.nextClickStillLegal({
        ...common,
        kind: "trade-cancel",
        offerIndex: 1,
        tradeId: "outgoing",
      }),
    ).toBe(true);
    overlay.destroy();
  });

  it("does not recommend or execute a discard owned by another player", async () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    await Promise.resolve();
    const internals = overlay as unknown as {
      board: {
        hexes: [];
        vertices: [];
        edges: [];
        myPlayer: string;
        ownHand: ReturnType<typeof emptyResources>;
        isMyTurn: false;
        action: "discard";
        discardCount: number;
      };
      discardRecommendation: (
        state?: ReturnType<typeof createTrackerState>,
      ) => unknown;
      nextClickStillLegal: (next: {
        kind: "discard";
        cards: ReturnType<typeof emptyResources>;
        label: string;
        signature: string;
        confidence: number;
      }) => boolean;
    };
    internals.board = {
      hexes: [],
      vertices: [],
      edges: [],
      myPlayer: "rodrgds",
      ownHand: {
        lumber: 2,
        brick: 2,
        wool: 2,
        grain: 2,
        ore: 2,
      },
      isMyTurn: false,
      action: "discard",
      discardCount: 5,
    };

    expect(internals.discardRecommendation()).toBeUndefined();
    expect(
      internals.nextClickStillLegal({
        kind: "discard",
        cards: emptyResources(),
        label: "Discard 5 cards",
        signature: "other-player-discard",
        confidence: 1,
      }),
    ).toBe(false);
    overlay.destroy();
  });

  it("uses a public-log-confirmed post-build hand until Colonist's hand snapshot catches up", () => {
    let tracker = reduceTracker(createTrackerState(), {
      type: "discover",
      player: "rodrgds",
    });
    tracker = reduceTracker(tracker, {
      type: "gain",
      player: "rodrgds",
      cards: {
        lumber: 0,
        brick: 0,
        wool: 0,
        grain: 2,
        ore: 3,
      },
      reason: "other",
    });
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    const before = {
      lumber: 0,
      brick: 0,
      wool: 0,
      grain: 2,
      ore: 3,
    };
    const after = emptyResources();
    const internals = overlay as unknown as {
      session: { state: typeof tracker };
      board: Parameters<AssistantOverlay["updateBoard"]>[0];
      confirmedPlacementSpend: {
        gameKey: string;
        player: string;
        before: typeof before;
        after: typeof after;
        expiresAt: number;
      };
      reconciledState: () => typeof tracker;
    };
    internals.session = { state: tracker };
    internals.board = {
      hexes: [],
      vertices: [],
      edges: [],
      gameKey: "city-spend-sync",
      myPlayer: "rodrgds",
      currentPlayer: "rodrgds",
      isMyTurn: true,
      action: "none",
      ownHand: before,
      players: {
        rodrgds: {
          handSize: 5,
          visiblePoints: 4,
          tradeRatios: {
            lumber: 4,
            brick: 4,
            wool: 4,
            grain: 4,
            ore: 4,
          },
          cardDiscardLimit: 7,
        },
      },
    };
    internals.confirmedPlacementSpend = {
      gameKey: "city-spend-sync",
      player: "rodrgds",
      before,
      after,
      expiresAt: Date.now() + 5_000,
    };

    expect(
      getPlayerEstimate(internals.reconciledState(), "rodrgds").average,
    ).toEqual(after);
    overlay.destroy();
  });

  it("does not click a stale dice control during robber placement", () => {
    const roll = document.createElement("button");
    roll.id = "roll-dice-button";
    roll.textContent = "Roll dice";
    document.body.append(roll);
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    const internals = overlay as unknown as {
      board: {
        hexes: [];
        vertices: [];
        edges: [];
        isMyTurn: boolean;
        action: "robber";
      };
      nextClick: (
        state: undefined,
        spatial: undefined,
        report: undefined,
      ) => unknown;
    };
    internals.board = {
      hexes: [],
      vertices: [],
      edges: [],
      isMyTurn: true,
      action: "robber",
    };

    expect(internals.nextClick(undefined, undefined, undefined)).toBeUndefined();
    overlay.destroy();
  });

  it("presents the connected MaxN and Weighted engine selector", () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    const root = document.querySelector<HTMLDivElement>(
      "#colonist-assistant-root",
    )!;
    const shadow = root.shadowRoot!;
    shadow
      .querySelector<HTMLElement>("[data-action='view'][data-view='settings']")!
      .click();
    const engine = shadow.querySelector<HTMLSelectElement>(
      "select[data-setting='engine']",
    );
    expect(engine).not.toBeNull();
    expect(engine?.value).toBe("deep-search");
    expect([...(engine?.options ?? [])].map((option) => option.value)).toEqual([
      "deep-search",
      "weighted",
    ]);
    overlay.destroy();
  });

  it("rejects volatile trade indices when the stable offer ID changed", () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    const internals = overlay as unknown as {
      board: {
        hexes: [];
        vertices: [];
        edges: [];
        activeTrades: Array<{
          id: string;
          creator: string;
          tradeExecutor: string;
          creatorGive: ReturnType<typeof emptyResources>;
          creatorReceive: ReturnType<typeof emptyResources>;
          incoming: boolean;
          counterOffer: boolean;
          canAccept: boolean;
          acceptedPlayers?: string[];
        }>;
      };
      nextClickStillLegal: (next: NextClick) => boolean;
    };
    internals.board = {
      hexes: [],
      vertices: [],
      edges: [],
      activeTrades: [{
        id: "replacement-offer",
        creator: "Rival",
        tradeExecutor: "Rival",
        creatorGive: emptyResources(),
        creatorReceive: emptyResources(),
        incoming: true,
        counterOffer: false,
        canAccept: true,
        acceptedPlayers: ["You"],
      }],
    };
    const common = {
      offerIndex: 0,
      tradeId: "original-offer",
      label: "Stale trade action",
      signature: "stale-trade-action",
      confidence: 1,
    };

    expect(
      internals.nextClickStillLegal({
        ...common,
        kind: "trade",
        verdict: "accept",
      }),
    ).toBe(false);
    internals.board.activeTrades[0]!.incoming = false;
    expect(
      internals.nextClickStillLegal({
        ...common,
        kind: "trade-partner",
        acceptedIndex: 0,
        player: "You",
      }),
    ).toBe(false);
    expect(
      internals.nextClickStillLegal({
        ...common,
        kind: "trade-cancel",
      }),
    ).toBe(false);
    overlay.destroy();
  });

  it("ends a proven idle turn without sending a WASM decision request", async () => {
    const endTurn = document.createElement("button");
    endTurn.id = "action-button-pass-turn";
    endTurn.textContent = "End turn";
    document.body.append(endTurn);
    const tracker = reduceTracker(createTrackerState(), {
      type: "discover",
      player: "rodrgds",
    });
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    overlay.update({ state: tracker } as GameSession);
    overlay.updateBoard({
      hexes: [],
      vertices: [],
      edges: [],
      gameKey: "idle-turn",
      myPlayer: "rodrgds",
      currentPlayer: "rodrgds",
      isMyTurn: true,
      action: "none",
      hasRolled: true,
      domesticTradeUsed: true,
      ownHand: {
        lumber: 0,
        brick: 0,
        wool: 1,
        grain: 0,
        ore: 0,
      },
      players: {
        rodrgds: {
          handSize: 1,
          tradeRatios: {
            lumber: 3,
            brick: 3,
            wool: 3,
            grain: 3,
            ore: 3,
          },
          cardDiscardLimit: 7,
          visiblePoints: 3,
        },
      },
    });
    await Promise.resolve();
    await Promise.resolve();

    expect(sendMessage).toHaveBeenCalledOnce();
    const shadow = document
      .querySelector<HTMLDivElement>("#colonist-assistant-root")!
      .shadowRoot!;
    expect(shadow.textContent).toContain("End your turn");
    expect(shadow.textContent).not.toContain("Calculating the next action");
    overlay.destroy();
  });

  it("searches the live outgoing counteroffer while continuing to show its wait state", async () => {
    const tracker = reduceTracker(createTrackerState(), {
      type: "discover",
      player: "rodrgds",
    });
    const give = emptyResources();
    give.wool = 1;
    const receive = emptyResources();
    receive.brick = 1;
    const incoming = {
      id: "incoming-1",
      creator: "Bot",
      tradeExecutor: "Bot",
      creatorGive: give,
      creatorReceive: receive,
      incoming: true,
      counterOffer: false,
      canAccept: true,
      myResponse: "pending" as const,
    };
    const outgoing = {
      id: "counter-1",
      creator: "rodrgds",
      tradeExecutor: "rodrgds",
      creatorGive: give,
      creatorReceive: receive,
      incoming: false,
      counterOffer: true,
      canAccept: false,
      pendingPlayers: ["Bot"],
      responsesComplete: false,
    };
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    await Promise.resolve();
    sendMessage.mockClear();
    const internals = overlay as unknown as {
      board: Parameters<AssistantOverlay["updateBoard"]>[0];
      completedIncomingTradeIds: Set<string>;
      outgoingTradeSeenAt: Map<string, number>;
      scheduleDecisionAnalysis: (
        state: ReturnType<typeof createTrackerState>,
        player: string,
      ) => void;
      renderAdvice: (
        state: ReturnType<typeof createTrackerState>,
        spatial: undefined,
        report: undefined,
        next: undefined,
      ) => string;
    };
    internals.board = {
      hexes: [],
      vertices: [],
      edges: [],
      gameKey: "counter-wait",
      myPlayer: "rodrgds",
      currentPlayer: "rodrgds",
      isMyTurn: true,
      action: "none",
      activeTrades: [incoming, outgoing],
    };
    internals.completedIncomingTradeIds.add(incoming.id);
    internals.outgoingTradeSeenAt.set(outgoing.id, Date.now());

    internals.scheduleDecisionAnalysis(tracker, "rodrgds");

    expect(sendMessage).toHaveBeenCalledOnce();
    expect(sendMessage.mock.calls[0]?.[0]).toEqual(
      expect.objectContaining({
        engine: "deep-search",
        board: expect.objectContaining({
          activeTrades: expect.arrayContaining([
            expect.objectContaining({ id: outgoing.id }),
          ]),
        }),
      }),
    );
    const advice = internals.renderAdvice(
      tracker,
      undefined,
      undefined,
      undefined,
    );
    expect(advice).toContain("Calculating the next action");
    expect(advice).not.toContain("Send a counteroffer");
    overlay.destroy();
  });

  it("reuses the completed deep target for the placement-modal continuation", async () => {
    const tracker = reduceTracker(createTrackerState(), {
      type: "discover",
      player: "rodrgds",
    });
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    await Promise.resolve();
    sendMessage.mockClear();
    const internals = overlay as unknown as {
      board: {
        hexes: [];
        vertices: [];
        edges: [];
        gameKey: string;
        myPlayer: string;
        currentPlayer: string;
        isMyTurn: true;
        action: "road";
      };
      queuedPlacement: {
        gameKey: string;
        action: "road";
        targetId: string;
        point: { x: number; y: number };
      };
      decisionAnalysis: {
        engine: "deep-search";
        runtime: "background-wasm";
        players: [];
      };
      decisionPendingKey: string;
      scheduleDecisionAnalysis: (
        state: ReturnType<typeof createTrackerState>,
        player: string,
      ) => void;
    };
    internals.board = {
      hexes: [],
      vertices: [],
      edges: [],
      gameKey: "retained-road",
      myPlayer: "rodrgds",
      currentPlayer: "rodrgds",
      isMyTurn: true,
      action: "road",
    };
    internals.queuedPlacement = {
      gameKey: "retained-road",
      action: "road",
      targetId: "e:planned",
      point: { x: 20, y: 20 },
    };
    internals.decisionAnalysis = {
      engine: "deep-search",
      runtime: "background-wasm",
      players: [],
    };

    internals.scheduleDecisionAnalysis(tracker, "rodrgds");

    expect(sendMessage).not.toHaveBeenCalled();
    expect(internals.decisionPendingKey).toBe("");
    overlay.destroy();
  });

  it("carries the selected WASM build target into Colonist's placement modal", async () => {
    const overlay = new AssistantOverlay(
      { ...DEFAULT_SETTINGS },
      { reset: vi.fn() },
    );
    await Promise.resolve();
    const internals = overlay as unknown as {
      board: {
        hexes: [];
        vertices: [];
        edges: Array<{
          id: string;
          vertices: [string, string];
          screen: { x: number; y: number };
        }>;
        gameKey: string;
        myPlayer: string;
        currentPlayer: string;
        isMyTurn: true;
        action: "none";
      };
      decisionAnalysis: {
        engine: "deep-search";
        runtime: "background-wasm";
        players: [];
        deepSearch: {
          chosen: { kind: "build-road"; targetId: string };
        };
      };
      queuedPlacement?: {
        gameKey?: string;
        action: "road" | "settlement" | "city";
        targetId: string;
        point: { x: number; y: number };
      };
      rememberBuildPlacement: (
        next: {
          kind: "build";
          build: "road";
          label: string;
          signature: string;
          confidence: number;
        },
        spatial: undefined,
      ) => void;
    };
    internals.board = {
      hexes: [],
      vertices: [],
      edges: [
        {
          id: "e:deep-road",
          vertices: ["v:a", "v:b"],
          screen: { x: 320, y: 240 },
        },
      ],
      gameKey: "deep-road-modal",
      myPlayer: "rodrgds",
      currentPlayer: "rodrgds",
      isMyTurn: true,
      action: "none",
    };
    internals.decisionAnalysis = {
      engine: "deep-search",
      runtime: "background-wasm",
      players: [],
      deepSearch: {
        chosen: { kind: "build-road", targetId: "e:deep-road" },
      },
    };

    internals.rememberBuildPlacement(
      {
        kind: "build",
        build: "road",
        label: "Choose build road",
        signature: "deep-build-road",
        confidence: 0.9,
      },
      undefined,
    );

    expect(internals.queuedPlacement).toEqual({
      gameKey: "deep-road-modal",
      action: "road",
      targetId: "e:deep-road",
      point: { x: 320, y: 240 },
    });
    overlay.destroy();
  });
});
