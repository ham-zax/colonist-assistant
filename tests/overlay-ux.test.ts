// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AssistantOverlay } from "../src/content/overlay";
import * as actionGuide from "../src/content/action-guide";
import { DEFAULT_SETTINGS } from "../src/content/settings";
import { createTrackerState } from "../src/core/tracker";
import type { PlayerMeta, TrackerState } from "../src/core/types";
import { emptyResources, type ResourceVector } from "../src/core/resources";
import type { NextClick } from "../src/content/action-guide";
import type { DecisionRationale } from "../src/core/engine";
import type { BoardSnapshot } from "../src/core/placement";
import { createCoachReport, type CoachReport } from "../src/core/coach";
import { actionStats, makeDeepSearch } from "../scripts/ui-preview/fixtures";

beforeEach(() => {
  vi.stubGlobal("chrome", {
    runtime: {
      getURL: (path: string) => `chrome-extension://fixture/${path}`,
      getManifest: () => ({ version: "0.7.12" }),
      sendMessage: vi.fn(() => Promise.resolve({})),
    },
    storage: {
      local: {
        get: () => Promise.resolve({}),
        set: () => Promise.resolve(),
        remove: () => Promise.resolve(),
      },
      sync: { set: () => Promise.resolve() },
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
  vi.spyOn(actionGuide, "renderActionGuide").mockImplementation(() => {});
});

afterEach(() => {
  document.body.innerHTML = "";
  document
    .querySelectorAll("#colonist-assistant-root")
    .forEach((root) => root.remove());
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const LONG_NAME = "Bartholomew_The_Extremely_Long_Named_Settler_Of_Catan";

const hand = (values: Partial<ResourceVector>): ResourceVector => ({
  ...emptyResources(),
  ...values,
});

const meta = (name: string, color: string): PlayerMeta =>
  ({
    name,
    color,
    devCards: [],
    playedDevCards: { knight: 0, victoryPoint: 0, monopoly: 0, roadBuilding: 0, yearOfPlenty: 0 },
    builds: { road: 0, settlement: 0, city: 0, development: 0 },
    resourcesGained: emptyResources(),
    productionGained: emptyResources(),
    resourcesSpent: emptyResources(),
    opponentModel: {},
  }) as unknown as PlayerMeta;

const publicState = (visiblePoints: number, handSize = 5) => ({
  handSize,
  tradeRatios: { lumber: 4, brick: 4, wool: 4, grain: 4, ore: 4 },
  cardDiscardLimit: 7,
  visiblePoints,
});

const trackerFixture = (): TrackerState => {
  const state = createTrackerState();
  state.playerOrder = ["Me", "Bob", "Carol", LONG_NAME];
  state.players = {
    Me: meta("Me", "#e03b3b"),
    Bob: meta("Bob", "#3b82e0"),
    Carol: meta("Carol", "#3be06a"),
    [LONG_NAME]: meta(LONG_NAME, "#e0c03b"),
  };
  // Bob's hand is uncertain: two of three worlds afford a city.
  state.worlds = [
    {
      hands: { Me: hand({ wool: 9 }), Bob: hand({ ore: 3, grain: 2 }), Carol: hand({}), [LONG_NAME]: hand({}) },
      weight: 1 / 3,
    },
    {
      hands: { Me: hand({ wool: 9 }), Bob: hand({ ore: 3, grain: 2, wool: 1 }), Carol: hand({}), [LONG_NAME]: hand({}) },
      weight: 1 / 3,
    },
    {
      hands: { Me: hand({ wool: 9 }), Bob: hand({ ore: 3, grain: 1, wool: 1 }), Carol: hand({}), [LONG_NAME]: hand({}) },
      weight: 1 / 3,
    },
  ];
  return state;
};

const boardFixture = () => ({
  hexes: [
    { id: "h1", resource: "ore", number: 8 },
    { id: "h2", resource: "grain", number: 8 },
    { id: "h3", resource: "wool", number: 6 },
    { id: "h4", resource: "lumber", number: 5, blocked: true },
    { id: "h5", resource: "brick", number: 9 },
  ],
  vertices: [
    { id: "v1", adjacentHexes: ["h1", "h2"], adjacentVertices: [], building: { player: "Me", kind: "settlement" } },
    { id: "v2", adjacentHexes: ["h3", "h4"], adjacentVertices: [], building: { player: "Me", kind: "city" } },
    { id: "v3", adjacentHexes: ["h5"], adjacentVertices: [], building: { player: "Bob", kind: "settlement" } },
    { id: "v4", adjacentHexes: ["h5"], adjacentVertices: [], building: { player: "Carol", kind: "settlement" } },
    { id: "v5", adjacentHexes: ["h5"], adjacentVertices: [], building: { player: LONG_NAME, kind: "settlement" } },
  ],
  edges: [],
  myPlayer: "Me",
  currentPlayer: "Bob",
  isMyTurn: false,
  victoryTarget: 10,
  diceMode: "random",
  ownHand: hand({ wool: 9 }),
  players: {
    Me: publicState(3, 9),
    Bob: publicState(8),
    Carol: publicState(5),
    [LONG_NAME]: publicState(6),
  },
  localSeatDiagnostics: { identity: { status: "resolved" } },
});

const winAnalysis = {
  engine: "deep-search",
  players: [
    { player: "Me", probability: 0.1 },
    { player: "Bob", probability: 0.3 },
    { player: "Carol", probability: 0.45 },
    { player: LONG_NAME, probability: 0.15 },
  ],
};

type Internals = {
  board: unknown;
  activeView: string;
  advancedSettingsOpen: boolean;
  decisionAnalysis: unknown;
  decisionKey: string;
  settings: typeof DEFAULT_SETTINGS;
  render: () => void;
  renderBetweenTurns: (state: TrackerState, analysis: unknown) => string;
  renderCards: (state: TrackerState, analysis: unknown) => string;
  renderAlternativesPanel: () => string;
  currentDecisionRationale: () => unknown;
  renderIncomingTradeAdvice(next: Extract<NextClick, { kind: "trade" }>): string;
  renderTradeCancelAdvice(next: Extract<NextClick, { kind: "trade-cancel" }>): string;
  decisionRationaleForNext(next: NextClick): DecisionRationale | undefined;
  renderBuildAdvice(state: TrackerState, report: CoachReport): string;
};

const create = (settings = DEFAULT_SETTINGS) => {
  const overlay = new AssistantOverlay(settings, { reset: vi.fn() });
  return { overlay, internals: overlay as unknown as Internals };
};

const parse = (html: string): HTMLElement => {
  const root = document.createElement("div");
  root.innerHTML = html;
  return root;
};

describe("overlay UX upgrade", () => {
  it.each(["incoming", "outgoing"] as const)("explains disabled %s trades as a setting rather than a strategic judgment", (kind) => {
    const { overlay, internals } = create({ ...DEFAULT_SETTINGS, disablePlayerTrades: true });
    const chosen = kind === "incoming"
      ? { kind: "respond-trade", tradeId: "offer", accept: false }
      : { kind: "cancel-trade", tradeId: "offer" };
    internals.board = boardFixture();
    internals.decisionAnalysis = { deepSearch: makeDeepSearch(chosen, [actionStats(chosen, 0.6)]) };
    const base = { offerIndex: 0, tradeId: "offer", signature: "disabled-offer", label: "Cancel player trade", confidence: 1 };
    const root = parse(kind === "incoming"
      ? internals.renderIncomingTradeAdvice({ ...base, kind: "trade", verdict: "decline" })
      : internals.renderTradeCancelAdvice({ ...base, kind: "trade-cancel" }));
    expect(root.querySelector(".why")?.textContent).toContain("Player trades are disabled in settings");
    expect(root.textContent).not.toMatch(/helps the opponent|no useful live response|same bundle stays blocked|completed root value/);
    expect(root.querySelector("details")).toBeNull();
    overlay.destroy();
  });

  it("does not borrow a rationale from another action kind or bank-trade bundle", () => {
    const { overlay, internals } = create();
    internals.board = boardFixture();
    const city = { kind: "build-city", targetId: "v1" };
    internals.decisionAnalysis = { deepSearch: makeDeepSearch(city, [actionStats(city, 0.6)]) };
    const board: NextClick = { kind: "board", boardAction: "settlement", targetId: "v1", point: { x: 1, y: 1 }, label: "Build here", signature: "target", confidence: 1 };
    expect(internals.decisionRationaleForNext(board)).toBeUndefined();
    expect(internals.decisionRationaleForNext({ ...board, boardAction: "city" })).toBeDefined();
    const trade = { kind: "maritime-trade", resource: "lumber" as const, otherResource: "ore" as const, ratio: 4 };
    internals.decisionAnalysis = { deepSearch: makeDeepSearch(trade, [actionStats(trade, 0.6)]) };
    const bank: NextClick = { kind: "trade-builder", mode: "bank", give: hand({ lumber: 4 }), receive: hand({ grain: 1 }), label: "Bank trade", signature: "bundle", confidence: 1 };
    expect(internals.decisionRationaleForNext(bank)).toBeUndefined();
    expect(internals.decisionRationaleForNext({ ...bank, receive: hand({ ore: 1 }) })).toBeDefined();
    overlay.destroy();
  });

  it("describes observed hand evidence without presenting a heuristic as a confidence percentage", () => {
    const { overlay, internals } = create();
    const state = trackerFixture();
    const board = boardFixture() as unknown as BoardSnapshot;
    internals.board = board;
    const report = createCoachReport(state, "Me", board)!;
    const root = parse(internals.renderBuildAdvice(state, report));
    expect(root.textContent).toContain("Your resource hand is read directly from the game");
    expect(root.textContent).not.toContain("% hand certainty");
    internals.board = { ...board, ownHand: undefined };
    expect(parse(internals.renderBuildAdvice(state, report)).textContent).toContain("Your resource hand is estimated from public evidence");
    overlay.destroy();
  });

  it("renders threats, dice coverage and seven risk on an opponent turn", () => {
    const { overlay, internals } = create();
    internals.board = boardFixture();
    const html = internals.renderBetweenTurns(trackerFixture(), winAnalysis);
    const root = parse(html);
    expect(root.querySelector(".between-heading h1")?.textContent).toContain("Bob");
    expect(root.textContent).toContain(
      "Your next move appears when your turn or a trade starts.",
    );

    // Threats: top three opponents, sorted by estimated win probability.
    const rows = [...root.querySelectorAll(".threat-row")];
    expect(rows).toHaveLength(3);
    expect(rows.map((row) => row.querySelector(".threat-name")?.textContent)).toEqual([
      "Carol",
      "Bob",
      LONG_NAME,
    ]);
    expect(rows[0]!.textContent).toContain("45%");
    expect(rows[0]!.textContent).toContain("est. win");
    // Bob: 8/10 VP, uncertain hand that affords a city on average.
    expect(rows[1]!.textContent).toContain("8/10 VP");
    expect(rows[1]!.textContent).toContain("Likely can build city");
    expect(rows[1]!.textContent).not.toMatch(/(^|[^y] )Can build city/);
    expect(rows[1]!.textContent).toContain("2 VP from winning");
    expect(rows[0]!.textContent).not.toContain("from winning");

    // Income: per-resource rows with the number tokens that pay each one.
    const tokensFor = (resource: string): string[] =>
      [...root.querySelectorAll(".income-row")]
        .filter((row) => row.querySelector(".income-art")?.getAttribute("title") === resource)
        .flatMap((row) =>
          [...row.querySelectorAll(".number-token:not(.blocked) b")].map(
            (token) => token.textContent ?? "",
          ),
        );
    expect(tokensFor("Ore")).toEqual(["8"]);
    expect(tokensFor("Grain")).toEqual(["8"]);
    expect(tokensFor("Wool")).toEqual(["6"]);
    expect(root.querySelector(".income-head")?.textContent).toContain("28%");
    expect(root.querySelector(".income-head")?.textContent).toContain("of rolls pay you");
    expect(root.querySelectorAll(".number-token.blocked").length).toBeGreaterThan(0);
    expect(root.textContent).toContain("No income from");

    // Seven risk: 9 cards held against a limit of 7.
    expect(root.textContent).toContain(
      "Holding 9 cards — a 7 (17% per roll) makes you discard 4",
    );
    overlay.destroy();
  });

  it("omits seven risk under the limit and falls back when nothing is known", () => {
    const { overlay, internals } = create();
    internals.board = {
      ...boardFixture(),
      ownHand: hand({ wool: 4 }),
    };
    const html = internals.renderBetweenTurns(trackerFixture(), winAnalysis);
    expect(html).not.toContain("Seven risk");

    internals.board = {
      hexes: [],
      vertices: [],
      edges: [],
      currentPlayer: "Bob",
      isMyTurn: false,
      diceMode: "random",
    };
    const fallback = internals.renderBetweenTurns(createTrackerState(), undefined);
    expect(fallback).toContain("Waiting for Bob");
    expect(fallback).not.toContain("between-block");
    overlay.destroy();
  });

  it("flags threatening opponents in the card table without breaking long names", () => {
    const { overlay, internals } = create();
    internals.board = boardFixture();
    const root = parse(internals.renderCards(trackerFixture(), winAnalysis));
    const rows = [...root.querySelectorAll<HTMLElement>(".matrix-row")];
    const byName = (name: string) =>
      rows.find((row) => row.querySelector(".player-name b")?.textContent?.startsWith(name))!;
    expect(byName("Bob").classList.contains("is-threat")).toBe(true);
    expect(byName("Bob").querySelector("em.threat-tag")).not.toBeNull();
    expect(byName("Carol").classList.contains("is-threat")).toBe(false);
    expect(byName("Me").classList.contains("is-threat")).toBe(false);
    const longRow = byName(LONG_NAME);
    expect(longRow.querySelector(".player-name b")?.textContent).toContain(LONG_NAME);
    overlay.destroy();
  });

  it("keeps a long opponent name on a single ellipsized line", () => {
    const { overlay, internals } = create();
    internals.board = boardFixture();
    const root = parse(internals.renderBetweenTurns(trackerFixture(), winAnalysis));
    const name = [...root.querySelectorAll<HTMLElement>(".threat-name")].find(
      (element) => element.textContent === LONG_NAME,
    );
    expect(name).toBeDefined();
    expect(name!.getAttribute("title")).toBe(LONG_NAME);
    // The class carries the ellipsis rules in OVERLAY_STYLES.
    return import("../src/content/styles").then(({ OVERLAY_STYLES }) => {
      const block = OVERLAY_STYLES.slice(OVERLAY_STYLES.indexOf(".threat-name {"));
      expect(block.slice(0, block.indexOf("}"))).toContain("text-overflow: ellipsis");
      expect(block.slice(0, block.indexOf("}"))).toContain("white-space: nowrap");
      overlay.destroy();
    });
  });

  describe("top moves panel", () => {
    const setup = (values: Record<string, number>) => {
      const { overlay, internals } = create({ ...DEFAULT_SETTINGS, showAlternatives: true });
      internals.board = {
        hexes: [],
        vertices: Object.keys(values).map((id) => ({ id, label: `Spot ${id}` })),
        edges: [],
      };
      internals.currentDecisionRationale = () => ({
        summary: "Strategist chose the first settlement",
        reasons: ["Best completed root value"],
        evidence: [],
      });
      const action = (targetId: string) => ({ kind: "place-settlement", targetId });
      const stats = (targetId: string, value: number) => ({
        action: action(targetId),
        visits: 10,
        availability: 1,
        availabilityWeight: 1,
        legalWeight: 0.8,
        prior: 0,
        value: [value, 0],
        lowerConfidenceValue: [value, 0],
      });
      internals.decisionKey = "ux-alternatives";
      internals.decisionAnalysis = {
        deepSearch: {
          chosen: action("v:one"),
          rootIndex: 0,
          actions: Object.entries(values).map(([id, value]) => stats(id, value)),
          rootProvenance: { rootEvidence: [] },
        },
      };
      return { overlay, internals };
    };

    it("uses plain language and hides raw search values from visible text", () => {
      const { overlay, internals } = setup({ "v:one": 3, "v:two": 2.995, "v:three": 2.5 });
      const root = parse(internals.renderAlternativesPanel());
      const panel = root.querySelector<HTMLDetailsElement>("details.alternatives-panel")!;
      expect(panel).not.toBeNull();
      expect(panel.open).toBe(false);
      expect(panel.querySelector("summary")?.textContent).toContain("TOP MOVES");
      expect(panel.querySelector("summary")?.textContent).toContain("Tap to preview on board");
      const text = root.textContent ?? "";
      expect(text).toContain("Engine pick");
      expect(text).toContain("Near tie");
      expect(text).toContain("Clearly worse");
      expect(text).toContain("Tap to preview on board");
      expect(text).toContain("Works in 80% of likely hands");
      expect(text).not.toMatch(/\d\.\d{3}/);
      expect(text).not.toContain("behind #1");
      expect(text).not.toContain("raw-value");
      expect(text).not.toContain("belief mass");
      expect(root.querySelector(".alternatives-panel > p")).toBeNull();
      const titles = [...root.querySelectorAll("button")].map((button) => button.title);
      expect(titles.some((title) => /Search value 2\.995 · 0\.005 behind #1/.test(title))).toBe(true);
      overlay.destroy();
    });

    it("labels a raw lead that was not chosen as a safety override", () => {
      const { overlay, internals } = setup({ "v:one": 3, "v:two": 3.2, "v:three": 2.995 });
      const text = parse(internals.renderAlternativesPanel()).textContent ?? "";
      expect(text).toContain("Ranked lower by final checks");
      expect(text).toContain("Near tie");
      expect(text).not.toContain("raw-value lead");
      overlay.destroy();
    });
  });

  describe("advanced settings", () => {
    const shadow = () =>
      document.querySelector("#colonist-assistant-root")!.shadowRoot!;

    it("groups diagnostics and keeps core settings visible", () => {
      const { overlay, internals } = create();
      internals.activeView = "settings";
      internals.render();
      const advanced = shadow().querySelector<HTMLDetailsElement>("details.settings-advanced")!;
      expect(advanced).not.toBeNull();
      expect(advanced.open).toBe(false);
      expect(advanced.textContent).toContain("Record game");
      expect(advanced.textContent).toContain("Investigation log");
      expect(advanced.textContent).toContain("Export investigation log");
      const outside = [...shadow().querySelectorAll(".settings-field")]
        .filter((field) => !advanced.contains(field))
        .map((field) => field.querySelector("b")?.textContent);
      expect(outside).toEqual([
        "Interface size",
        "Highlight next click",
        "Autopilot",
        "Autopilot delay",
        "Show alternatives",
        "Disable player trades",
      ]);
      expect(shadow().querySelector('[data-action="reset"]')).not.toBeNull();
      overlay.destroy();
    });

    it("keeps the advanced section open across re-renders", () => {
      const { overlay, internals } = create();
      internals.activeView = "settings";
      internals.render();
      const first = shadow().querySelector<HTMLDetailsElement>("details.settings-advanced")!;
      first.querySelector("summary")!.click();
      expect(internals.advancedSettingsOpen).toBe(true);

      internals.render();
      const second = shadow().querySelector<HTMLDetailsElement>("details.settings-advanced")!;
      expect(second).not.toBe(first);
      expect(second.open).toBe(true);

      second.querySelector("summary")!.click();
      internals.render();
      expect(shadow().querySelector<HTMLDetailsElement>("details.settings-advanced")!.open).toBe(false);
      overlay.destroy();
    });
  });

  it("replaces live comma-containing board ids with readable labels", () => {
    const { overlay } = create();
    const internals = overlay as unknown as {
      board: unknown;
      labelledRationale: (rationale: {
        summary: string;
        plain?: string;
        reasons: string[];
        evidence: string[];
      }) => { plain?: string } | undefined;
    };
    internals.board = {
      hexes: [],
      vertices: [
        { id: "v:1,-2,0", label: "6 brick / 8 ore", adjacentHexes: [], adjacentVertices: [] },
        { id: "v:1,-2", label: "wrong prefix match", adjacentHexes: [], adjacentVertices: [] },
      ],
      edges: [],
    };
    const result = internals.labelledRationale({
      summary: "Deep MaxN chose to build a city at v:1,-2,0",
      plain: "Better than the next option, build a city at v:1,-2,0",
      reasons: [],
      evidence: [],
    });
    expect(result?.plain).toBe(
      "Better than the next option, build a city at 6 brick / 8 ore",
    );
    overlay.destroy();
  });
});
