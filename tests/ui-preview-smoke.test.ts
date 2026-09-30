// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { installChromeStub } from "../scripts/ui-preview/chrome-stub";
import {
  mountScenario,
  SCENARIO_NAMES,
  type ScenarioName,
} from "../scripts/ui-preview/scenarios";

const DUMP = process.env.UI_PREVIEW_DUMP === "1";

/** Text each scenario must show, proving it reached its intended renderer path. */
const EXPECTED_TEXT: Record<ScenarioName, string[]> = {
  empty: ["Waiting for your game", "No cards tracked yet"],
  build: ["Save for a city", "Bouton#4976_TheLongestName", "% win", "BANK"],
  alternatives: ["TOP MOVES", "#1", "#2", "#3", "READY TO BUILD", "% win"],
  spatial: ["PLACE YOUR SETTLEMENT", "Build here"],
  trade: ["INCOMING TRADE", "Accept this offer", "Bouton#4976_TheLongestName"],
  discard: ["SEVEN ROLLED", "Discard these 4"],
  "opponent-turn": ["Bouton#4976_TheLongestName"],
  thinking: ["Calculating the next action", "WASM SEARCHING"],
  paused: ["STRATEGIST PAUSED", "Retry Strategist"],
  settings: ["ASSISTANT SETTINGS", "Show alternatives"],
  details: ["ROLL DISTRIBUTION", "30 OBSERVED"],
  collapsed: ["Colonist Ally"],
};

beforeEach(() => {
  installChromeStub();
  vi.stubGlobal(
    "getComputedStyle",
    () => ({ display: "block", visibility: "visible", opacity: "1" }) as CSSStyleDeclaration,
  );
});

afterEach(() => {
  document.body.innerHTML = "";
  document.querySelectorAll("#colonist-assistant-root").forEach((root) => root.remove());
  vi.unstubAllGlobals();
});

const shadowOf = (): ShadowRoot =>
  document.querySelector<HTMLElement>("#colonist-assistant-root")!.shadowRoot!;

describe("ui preview scenarios", () => {
  it.each(SCENARIO_NAMES)("renders %s through the real overlay without errors", (name: ScenarioName) => {
    const errors = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const mounted = mountScenario(name);
    try {
      const shadow = shadowOf();
      const mount = shadow.querySelector("#mount")!;
      expect(mount.innerHTML.length).toBeGreaterThan(200);
      expect(shadow.querySelector(".assistant")).not.toBeNull();
      const text = shadow.textContent ?? "";
      for (const expected of EXPECTED_TEXT[name]) {
        expect(text, `${name} should show "${expected}"`).toContain(expected);
      }
      expect(Boolean(shadow.querySelector(".assistant.collapsed"))).toBe(name === "collapsed");
      if (name === "spatial") expect(shadow.querySelector(".board-marker")).not.toBeNull();
      if (DUMP) {
        const panel = shadow.querySelector(".panel");
        console.log(`--- ${name}\n${(panel?.textContent ?? "").replace(/\s+/g, " ").trim()}\nmarker=${shadow.querySelector(".board-marker") ? "yes" : "no"}`);
      }
      expect(errors).not.toHaveBeenCalled();
    } finally {
      mounted.overlay.destroy();
    }
  });
});
