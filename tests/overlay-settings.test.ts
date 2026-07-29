// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AssistantOverlay } from "../src/content/overlay";
import { DEFAULT_SETTINGS } from "../src/content/settings";
import { createTrackerState, reduceTracker } from "../src/core/tracker";
import type { GameSession } from "../src/content/session";

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
      getManifest: () => ({ version: "0.7.11" }),
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
});
