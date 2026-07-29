// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AssistantOverlay } from "../src/content/overlay";
import { DEFAULT_SETTINGS } from "../src/content/settings";

beforeEach(() => {
  vi.stubGlobal("chrome", {
    runtime: {
      getURL: (path: string) => `chrome-extension://fixture/${path}`,
      getManifest: () => ({ version: "0.7.9" }),
      sendMessage: (message: { id: number }) =>
        Promise.resolve({
          id: message.id,
          runtime: "background-wasm",
          engineRevision: "test-engine",
          initializationMs: 1,
        }),
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
});
