// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import type { BoardSnapshot } from "../src/core/placement";
import { GameSession } from "../src/content/session";
import { DEFAULT_SETTINGS } from "../src/content/settings";
import { buildLiveDecisionStochasticInput } from "../src/core/dice-history";

const fixture = vi.hoisted(() => ({
  publish: undefined as undefined | ((board: BoardSnapshot) => void),
  session: undefined as GameSession | undefined,
  root: undefined as HTMLElement | undefined,
}));
vi.mock("../src/content/board", () => ({
  readPublicBoardSnapshot: () => undefined,
  installPublicBoardBridge: (publish: (board: BoardSnapshot) => void) => {
    fixture.publish = publish;
    return () => {};
  },
}));
vi.mock("../src/content/dom", async (original) => ({
  ...await original<typeof import("../src/content/dom")>(),
  findLogRoot: () => fixture.root,
}));
vi.mock("../src/content/overlay", () => ({ AssistantOverlay: class {
  setSettings() {} updateBoard() {} destroy() {}
  update(session?: GameSession) { if (session) fixture.session = session; }
} }));
vi.mock("../src/content/settings", async (original) => ({
  ...await original<typeof import("../src/content/settings")>(),
  readSettings: async () => ({ ...DEFAULT_SETTINGS, investigationLog: false }),
}));
afterEach(async () => {
  window.dispatchEvent(new Event("pagehide"));
  fixture.session?.stop();
  await new Promise(resolve => setTimeout(resolve, 0));
  document.body.replaceChildren();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
it("captures a human-game board roll before its mounted log hydrates", async () => {
  vi.stubGlobal("chrome", { storage: { local: {
    get: async () => ({}), set: async () => {}, remove: async () => {},
  }, onChanged: { addListener: () => {} } } });
  document.body.innerHTML = '<canvas id="game-canvas"></canvas><div id="log"><div data-index="0">Happy settling!</div></div>';
  fixture.root = document.getElementById("log")!;
  await import("../src/content/index");
  await vi.waitFor(() => expect(fixture.session).toBeDefined());
  const base = {
    hexes: [], vertices: [], edges: [], diceMode: "balanced", gameKey: "grain8695",
    botOnlyGame: false, initialPlacement: true, gameplayRollCount: 0,
    currentPlayer: "Alice", turn: 0, hasRolled: false,
  } satisfies BoardSnapshot;
  fixture.publish!(base);
  const appendRoll = (index: number, actor: string, dice: [number, number]) => {
    const row = document.createElement("div");
    row.dataset.index = String(index);
    row.textContent = `${actor} rolled`;
    for (const value of dice) {
      const image = document.createElement("img");
      image.alt = `dice_white${value}`;
      row.append(image);
    }
    fixture.root!.append(row);
  };
  appendRoll(16, "Alice", [5, 3]);
  appendRoll(20, "Bob", [6, 5]);
  await vi.waitFor(() => expect(fixture.session!.diceHistory.rolls).toHaveLength(2));
  fixture.publish!({ ...base, initialPlacement: false, turn: 6, gameplayRollCount: 3,
    hasRolled: true, lastRoll: 7 });
  const input = () => buildLiveDecisionStochasticInput("balanced", fixture.session!.diceHistory,
    ["Alice", "Bob"], 3);
  const expected = [
    { ordinal: 0, actor: 0, total: 8 },
    { ordinal: 1, actor: 1, total: 11 },
    { ordinal: 2, actor: 0, total: 7 },
  ];
  expect(input().rolls).toEqual(expected);
  appendRoll(23, "Alice", [5, 2]);
  await vi.waitFor(() => expect(fixture.session!.diceHistory.rolls.some(
    roll => roll.logIndex === 23,
  )).toBe(true));
  expect(input().rolls).toEqual(expected);
});
