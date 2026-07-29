// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { DecisionAnalysis } from "../src/core/engine";
import {
  destroyWinOdds,
  renderWinOdds,
} from "../src/content/win-odds";
import type { TrackerState } from "../src/core/types";

const analysis = (probability = 0.42): DecisionAnalysis => ({
  engine: "deep-search",
  players: [
    {
      player: "Leckie",
      probability,
      etaTurns: 8,
      samples: 100,
      confidence: "medium",
      reasons: [],
    },
    {
      player: "rodrgds",
      probability: 1 - probability,
      etaTurns: 7,
      samples: 100,
      confidence: "high",
      reasons: [],
    },
  ],
  actionScores: {
    road: 0,
    settlement: 0,
    city: 0,
    development: 0,
  },
  simulations: 100,
  model: "fixture",
});

const state = {
  playerOrder: ["Leckie", "rodrgds"],
} as TrackerState;

const panel = (player: string, top: number): HTMLElement => {
  const element = document.createElement("section");
  element.dataset.top = String(top);
  const name = document.createElement("span");
  name.dataset.top = String(top);
  name.textContent = player;
  element.append(name);
  return element;
};

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("chrome", {
    runtime: {
      getURL: (path: string) => `chrome-extension://fixture/${path}`,
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
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    function getFixtureRect(this: HTMLElement) {
      const top = Number(this.dataset.top ?? 20);
      return {
        x: 900,
        y: top,
        left: 900,
        top,
        right: 1140,
        bottom: top + 80,
        width: 240,
        height: 80,
        toJSON: () => ({}),
      } as DOMRect;
    },
  );
  Object.defineProperty(window, "innerWidth", {
    configurable: true,
    value: 1200,
  });
  Object.defineProperty(window, "innerHeight", {
    configurable: true,
    value: 800,
  });
  document.body.append(panel("Leckie", 100), panel("rodrgds", 200));
});

afterEach(() => {
  destroyWinOdds();
  document.body.replaceChildren();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("live win odds", () => {
  it("keeps the last valid badges through a transient missing scan", async () => {
    renderWinOdds(analysis(), state);

    expect(
      document.querySelectorAll("#colonist-assistant-win-odds [data-player]"),
    ).toHaveLength(2);

    renderWinOdds(undefined, undefined);
    await vi.advanceTimersByTimeAsync(1_000);

    expect(
      document.querySelectorAll("#colonist-assistant-win-odds [data-player]"),
    ).toHaveLength(2);
  });

  it("retains an unmatched player badge and repositions it after a rerender", async () => {
    renderWinOdds(analysis(), state);
    const original = document.querySelector<HTMLElement>(
      "[data-player='Leckie']",
    )!;
    expect(original.style.top).toBe("105px");

    document
      .querySelector<HTMLElement>("section[data-top='100']")
      ?.remove();
    renderWinOdds(analysis(0.44), state);

    expect(
      document.querySelector<HTMLElement>("[data-player='Leckie']"),
    ).toBe(original);
    expect(original.textContent).toBe("44% WIN");

    document.body.append(panel("Leckie", 320));
    await vi.advanceTimersByTimeAsync(100);

    expect(original.style.top).toBe("325px");
  });

  it("clears retained odds on an explicit lifecycle end", () => {
    renderWinOdds(analysis(), state);
    destroyWinOdds();

    expect(
      document.getElementById("colonist-assistant-win-odds"),
    ).toBeNull();
  });
});
