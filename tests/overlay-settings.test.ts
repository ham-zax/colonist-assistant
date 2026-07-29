// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AssistantOverlay } from "../src/content/overlay";
import { DEFAULT_SETTINGS } from "../src/content/settings";
import { createTrackerState, reduceTracker } from "../src/core/tracker";
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

  it("does not replace the engine select while its native picker is open", () => {
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
    const select = shadow.querySelector<HTMLSelectElement>(
      "select[data-setting='engine']",
    )!;

    select.dispatchEvent(
      new Event("pointerdown", { bubbles: true, composed: true }),
    );
    select.focus();
    overlay.updateBoard({
      hexes: [],
      vertices: [],
      edges: [],
      gameKey: "same-game",
      isMyTurn: false,
      action: "none",
    });

    expect(
      shadow.querySelector("select[data-setting='engine']"),
    ).toBe(select);

    select.value = "deep-alpha-beta";
    select.dispatchEvent(
      new Event("change", { bubbles: true, composed: true }),
    );
    expect(
      shadow.querySelector<HTMLSelectElement>(
        "select[data-setting='engine']",
      )?.value,
    ).toBe("deep-alpha-beta");
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

  it("waits for an outgoing counteroffer without re-searching the completed incoming offer", async () => {
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
      give: receive,
      receive: give,
      incoming: true,
      counterOffer: false,
      canAccept: true,
      myResponse: "pending" as const,
    };
    const outgoing = {
      id: "counter-1",
      creator: "rodrgds",
      tradeExecutor: "rodrgds",
      give,
      receive,
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

    expect(sendMessage).not.toHaveBeenCalled();
    const advice = internals.renderAdvice(
      tracker,
      undefined,
      undefined,
      undefined,
    );
    expect(advice).toContain("Waiting for 1 response");
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
