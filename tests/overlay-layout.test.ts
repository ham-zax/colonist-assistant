// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { installChromeStub } from "../scripts/ui-preview/chrome-stub";
import { mountScenario } from "../scripts/ui-preview/scenarios";
import type { CoachReport } from "../src/core/coach";
import { emptyResources } from "../src/core/resources";
import type { TrackerState } from "../src/core/types";

beforeEach(() => {
  installChromeStub();
  vi.stubGlobal(
    "getComputedStyle",
    () =>
      ({
        display: "block",
        visibility: "visible",
        opacity: "1",
      }) as CSSStyleDeclaration,
  );
});

afterEach(() => {
  document.body.innerHTML = "";
  document
    .querySelectorAll("#colonist-assistant-root")
    .forEach((root) => root.remove());
  vi.unstubAllGlobals();
});

const shadowOf = (): ShadowRoot =>
  document.querySelector<HTMLElement>("#colonist-assistant-root")!.shadowRoot!;

type AlternativesInternals = {
  decisionKey: string;
  alternativesOpenKey?: string;
  renderAlternativesPanel: () => string;
};

describe("overlay layout stability", () => {
  it("shares one decision skeleton across thinking and ready states", () => {
    for (const name of ["thinking", "build"] as const) {
      const mounted = mountScenario(name);
      try {
        const shadow = shadowOf();
        expect(
          shadow.querySelector(".decision-meta"),
          `${name} needs a meta row`,
        ).not.toBeNull();
        expect(
          shadow.querySelector(".meta-engine-chip"),
          `${name} needs an engine chip`,
        ).not.toBeNull();
        expect(
          shadow.querySelector(".decision-command h1"),
          `${name} needs a next-action title`,
        ).not.toBeNull();
      } finally {
        mounted.overlay.destroy();
        document.body.innerHTML = "";
      }
    }
  });

  it("always shows the engine chip: ready, searching, and error", () => {
    const ready = mountScenario("build");
    expect(shadowOf().querySelector(".meta-engine-chip")?.textContent).toContain(
      "READY",
    );
    ready.overlay.destroy();
    document.body.innerHTML = "";

    const thinking = mountScenario("thinking");
    expect(shadowOf().querySelector(".meta-engine-chip")?.textContent).toContain(
      "WASM SEARCHING",
    );
    thinking.overlay.destroy();
    document.body.innerHTML = "";

    const paused = mountScenario("paused");
    const chip = shadowOf().querySelector(".meta-engine-chip");
    expect(chip?.textContent).toContain("STRATEGIST ERROR");
    expect(shadowOf().textContent).toContain("Retry Strategist");
    paused.overlay.destroy();
  });

  it("collapses top moves by default and reopens them per decision", () => {
    const mounted = mountScenario("alternatives");
    try {
      const shadow = shadowOf();
      const panel = shadow.querySelector<HTMLDetailsElement>(
        "details.alternatives-panel",
      )!;
      expect(panel).not.toBeNull();
      expect(panel.open).toBe(false);
      expect(panel.querySelector("summary")?.textContent).toContain("TOP MOVES");
      const previews = [...panel.querySelectorAll("[data-action='preview-alternative']")];
      expect(previews).toHaveLength(3);
      expect(
        previews.every((button) => button.getAttribute("data-alternative-key")),
      ).toBe(true);

      const internals = mounted.overlay as unknown as AlternativesInternals;
      internals.alternativesOpenKey = internals.decisionKey;
      const openRoot = document.createElement("div");
      openRoot.innerHTML = internals.renderAlternativesPanel();
      expect(
        openRoot.querySelector("details.alternatives-panel")?.hasAttribute("open"),
      ).toBe(true);

      internals.alternativesOpenKey = "another-decision";
      const closedRoot = document.createElement("div");
      closedRoot.innerHTML = internals.renderAlternativesPanel();
      expect(
        closedRoot.querySelector("details.alternatives-panel")?.hasAttribute("open"),
      ).toBe(false);
    } finally {
      mounted.overlay.destroy();
    }
  });

  it("never hides urgent mandatory advice inside collapsed disclosures", () => {
    const trade = mountScenario("trade");
    const tradeShadow = shadowOf();
    expect(tradeShadow.querySelector(".trade-decision h1")?.textContent).toContain(
      "Accept this offer",
    );
    expect(tradeShadow.querySelector(".trade-decision .board-confirm")).not.toBeNull();
    expect(tradeShadow.querySelector("details .board-confirm")).toBeNull();
    trade.overlay.destroy();
    document.body.innerHTML = "";

    const discard = mountScenario("discard");
    const discardShadow = shadowOf();
    expect(discardShadow.querySelector(".discard-plan")).not.toBeNull();
    expect(discardShadow.querySelector("details .discard-plan")).toBeNull();
    discard.overlay.destroy();
  });

  it("holds long names and the card matrix steady across player counts", () => {
    const offTurn = mountScenario("opponent-turn");
    const heading = shadowOf().querySelector(".between-heading h1")!;
    expect(heading.textContent?.length).toBeGreaterThan(10);
    expect(heading.getAttribute("title")).toBe(heading.textContent);
    offTurn.overlay.destroy();
    document.body.innerHTML = "";

    const build = mountScenario("build");
    const internals = build.overlay as unknown as {
      renderCards: (state: TrackerState) => string;
    };
    for (const count of [3, 4]) {
      const state = structuredClone(build.state!);
      const removed = state.playerOrder.splice(count);
      for (const player of removed) {
        delete state.players[player];
        for (const world of state.worlds) delete world.hands[player];
      }
      const table = document.createElement("div");
      table.innerHTML = internals.renderCards(state);
      const rows = [...table.querySelectorAll(".matrix-row:not(.bank-row)")];
      expect(rows).toHaveLength(count);
      for (const row of rows) {
        const name = row.querySelector(".player-name b")!;
        expect(name.getAttribute("title")).toBe(name.textContent);
      }
      expect(table.querySelector(".bank-row")).not.toBeNull();
    }
    build.overlay.destroy();
  });
});

describe("build advice semantics", () => {
  type BuildAdviceInternals = {
    renderBuildAdvice: (state: unknown, report: CoachReport) => string;
    renderDeepTradeAdvice: (action: Record<string, unknown>) => string;
  };

  const cityReport = (affordable: boolean, withTrade: boolean): CoachReport => ({
    player: "Me",
    phase: "middle",
    strategy: "city-engine",
    primary: {
      kind: "city",
      label: "City",
      score: 1,
      confidence: 80,
      progress: 0.5,
      affordableProbability: affordable ? 1 : 0.2,
      deficit: affordable
        ? emptyResources()
        : { ...emptyResources(), ore: 1, grain: 1 },
      reasons: ["Strongest production gain", "Keeps the win race", "Holds tempo"],
    },
    alternatives: [],
    ...(withTrade
      ? {
          trade: {
            give: { ...emptyResources(), lumber: 1 },
            receive: { ...emptyResources(), ore: 1 },
            partner: "Bob",
            acceptanceProbability: 0.3,
            score: 1,
            ownTempoGain: 1,
            opponentTempoGain: 0,
            reason: "Close to a modeled conversion",
          },
        }
      : {}),
    developmentDeck: {
      remainingCards: 10,
      expectedComposition: { knight: 1, victoryPoint: 0, progress: 0 },
      next: { knight: 0.5, victoryPoint: 0.2, progress: 0.1 },
      atLeastOneVictoryPoint: () => 0,
      atLeastOneKnight: () => 0,
    },
    alerts: [],
  });

  const parse = (html: string): HTMLElement => {
    const root = document.createElement("div");
    root.innerHTML = html;
    return root;
  };

  it("labels an unaffordable goal BUILD GOAL with the trade as an option", () => {
    const mounted = mountScenario("build");
    try {
      const internals = mounted.overlay as unknown as BuildAdviceInternals;
      const root = parse(internals.renderBuildAdvice({ worlds: [{}] }, cityReport(false, true)));
      expect(root.querySelector(".decision-meta > span:first-child")?.textContent).toBe(
        "BUILD GOAL",
      );
      expect(root.querySelector(".decision-command h1")?.textContent).toBe(
        "Save for a city",
      );
      const tactic = root.querySelector(".single-tactic")!;
      expect(tactic.querySelector("span")?.textContent).toBe("TRADE OPTION");
      expect(tactic.textContent).toContain("Offer lumber for ore");
      expect(tactic.textContent).toContain("Bob");
      expect(tactic.textContent).toContain("30% modeled acceptance");
      expect(root.textContent).not.toContain("YOUR NEXT MOVE");
    } finally {
      mounted.overlay.destroy();
    }
  });

  it("keeps YOUR NEXT MOVE for an affordable goal", () => {
    const mounted = mountScenario("build");
    try {
      const internals = mounted.overlay as unknown as BuildAdviceInternals;
      const root = parse(internals.renderBuildAdvice({ worlds: [{}] }, cityReport(true, false)));
      expect(root.querySelector(".decision-meta > span:first-child")?.textContent).toBe(
        "YOUR NEXT MOVE",
      );
      expect(root.querySelector(".decision-command h1")?.textContent).toBe(
        "Build a city now",
      );
      expect(root.textContent).not.toContain("BUILD GOAL");
    } finally {
      mounted.overlay.destroy();
    }
  });

  it("leaves the selected bank-trade execution advice unchanged", () => {
    const mounted = mountScenario("build");
    try {
      const internals = mounted.overlay as unknown as BuildAdviceInternals;
      const root = parse(
        internals.renderDeepTradeAdvice({
          kind: "maritime-trade",
          resource: "grain",
          otherResource: "ore",
          ratio: 3,
        }),
      );
      expect(root.querySelector(".decision-meta > span:first-child")?.textContent).toBe(
        "BANK TRADE",
      );
      expect(root.querySelector(".decision-command h1")?.textContent).toBe(
        "Trade with the bank",
      );
    } finally {
      mounted.overlay.destroy();
    }
  });
});
