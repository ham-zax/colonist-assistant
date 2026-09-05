// @vitest-environment jsdom
// Independent review acceptance cases. Failures are intentional evidence of unresolved defects.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AssistantOverlay } from "../src/content/overlay";
import { GameSession } from "../src/content/session";
import { DEFAULT_SETTINGS } from "../src/content/settings";
import { buildLiveDecisionStochasticInput, type PublicStochasticInput } from "../src/core/dice-history";
import type { BoardSnapshot } from "../src/core/placement";

const order = ["Chapin7947", "hamzax"];
const gameKey = "review-win3576";
const live = [[16, 2, 4], [20, 2, 3], [24, 6, 3], [28, 3, 6], [34, 4, 1], [39, 5, 2], [44, 1, 6], [49, 6, 4]];
const sessions: GameSession[] = [];
let overlay: AssistantOverlay | undefined;
const sendMessage = vi.fn(async (message: { id: number; stochastic?: PublicStochasticInput }) => message.stochastic
  ? { id: message.id, analysis: { engine: "deep-search", runtime: "background-gpu", players: [] } }
  : { id: message.id, runtime: "background-gpu", engineRevision: "deep-maxn-v12", initializationMs: 1 });

beforeEach(() => {
  sendMessage.mockClear();
  vi.stubGlobal("chrome", {
    runtime: { getURL: (path: string) => `chrome-extension://fixture/${path}`, getManifest: () => ({ version: "0.9.1" }), sendMessage },
    storage: { local: { get: async () => ({}), set: async () => {}, remove: async () => {} }, sync: { set: async () => {} } },
  });
  vi.spyOn(console, "warn").mockImplementation(() => {});
  vi.spyOn(console, "error").mockImplementation(() => {});
});
afterEach(async () => {
  for (const session of sessions.splice(0)) session.stop();
  overlay?.destroy();
  overlay = undefined;
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});

function row(index: number, text: string, dice?: number[]): HTMLDivElement {
  const element = document.createElement("div");
  element.dataset.index = String(index);
  element.textContent = text;
  for (const value of dice ?? []) {
    const image = document.createElement("img");
    image.alt = `dice_white${value}`;
    element.append(image);
  }
  return element;
}
const flush = async () => { await new Promise<void>((resolve) => setTimeout(resolve, 0)); };
function board(count: number, rolled: boolean, actor: string, extra: Partial<BoardSnapshot> = {}): BoardSnapshot {
  return {
    hexes: [{ id: "desert", blocked: true }, { id: "other" }], vertices: [], edges: [],
    diceMode: "balanced", gameKey, playerOrder: order, currentPlayer: actor, myPlayer: "hamzax",
    isMyTurn: actor === "hamzax", initialPlacement: false, gameplayRollCount: count,
    turn: 4 + count - Number(rolled), hasRolled: rolled, action: "none", friendlyRobber: true, victoryTarget: 15,
    localSeatDiagnostics: {
      seatSource: "gameController.myColor+currentUserId+gameUserStates",
      identity: { status: "resolved", reason: "cross-checked", source: "controller+account-user-id+store-roster", currentUserIdAvailable: true, currentUserMatchColors: [2], myColor: 2, currentUserColor: 2 },
    }, ...extra,
  };
}
type Internals = {
  session?: GameSession;
  board?: BoardSnapshot;
  decisionRuntimeError: string;
  decisionAnalysis?: unknown;
  render: () => void;
  scheduleDecisionAnalysis: (state: GameSession["state"], player: string) => void;
  awaitingLocalSevenProtocol: () => boolean;
  nextClick: () => unknown;
};
async function fixture(withOverlay = false, initialRows = [row(0, "Happy settling!")]) {
  const root = document.createElement("div");
  root.append(...initialRows);
  document.body.append(root);
  let insideRender = false;
  let internals: Internals | undefined;
  if (withOverlay) {
    overlay = new AssistantOverlay({ ...DEFAULT_SETTINGS, recordGame: false }, { reset: vi.fn() });
    await flush();
    internals = overlay as unknown as Internals;
    // Preserve update()/updateBoard() publication and real scheduling. Only replace
    // HTML rendering (which recursively calls render on status updates).
    vi.spyOn(internals, "render").mockImplementation(() => {
      if (insideRender || !internals?.session) return;
      insideRender = true;
      try { internals.scheduleDecisionAnalysis(internals.session.state, "hamzax"); }
      finally { insideRender = false; }
    });
  }
  const session = new GameSession(root, (updated) => overlay?.update(updated), gameKey);
  sessions.push(session);
  session.setInitialPlacement(true, gameKey);
  await session.start();
  session.setMyPlayer("hamzax");
  session.setInitialPlacement(false, gameKey);
  sendMessage.mockClear();
  return { root, session, internals };
}
async function appendRoll(root: HTMLElement, ordinal: number) {
  const [index, left, right] = live[ordinal]!;
  root.append(row(index!, `${order[ordinal % 2]} rolled`, [left!, right!]));
  await flush();
}
const input = (session: GameSession, count: number) => buildLiveDecisionStochasticInput("balanced", session.diceHistory, order, count);

describe("independent win3576 publication review", () => {
  it("replays every DOM-first roll, seven notices, and board catch-up without permanent failure", async () => {
    const { root, session, internals } = await fixture(true);
    for (let ordinal = 0; ordinal < live.length; ordinal++) {
      const actor = order[ordinal % 2]!;
      overlay!.updateBoard(board(ordinal, false, actor));
      await flush();
      sendMessage.mockClear();
      await appendRoll(root, ordinal);
      expect(() => input(session, ordinal)).toThrow(/does not reconcile/);
      expect(input(session, ordinal + 1).rolls).toHaveLength(ordinal + 1);
      if (actor === "hamzax") {
        expect(internals!.decisionRuntimeError).toMatch(/waiting for the board.*catch up/i);
        expect(sendMessage).not.toHaveBeenCalled();
      }
      const [, left, right] = live[ordinal]!;
      if (left! + right! === 7) {
        root.append(row(ordinal === 5 ? 40 : 45, "Friendly Robber is active, tiles available to block are limited"));
        root.append(row(ordinal === 5 ? 41 : 46, `${actor} moved Robber to`));
        root.append(row(ordinal === 5 ? 42 : 47, "No player to steal from"));
        await flush();
        expect(session.diceHistory.ambiguousLogIndices).toEqual([]);
        // This sequence samples the robber prompt, then its completion.
        overlay!.updateBoard(board(ordinal + 1, true, actor, { lastRoll: 7, action: "robber" }));
      }
      overlay!.updateBoard(board(ordinal + 1, true, actor, {
        lastRoll: left! + right!, hexes: [{ id: "desert" }, { id: "other", blocked: true }],
      }));
      await flush();
      expect(session.diceHistory.rolls).toHaveLength(ordinal + 1);
      expect(session.diceHistory.gaps).toEqual([]);
      const stochastic = input(session, ordinal + 1);
      expect(stochastic.rolls?.map((roll) => roll.ordinal)).toEqual(Array.from({ length: ordinal + 1 }, (_, i) => i));
      expect(stochastic.gaps ?? []).toEqual([]);
      expect(stochastic.model).toBe("mref-colonist-linked-2024-v1");
      if (actor === "hamzax") {
        expect(internals!.decisionRuntimeError).toBe("");
        expect(sendMessage.mock.calls.some(([message]) => message.stochastic?.rolls?.length === ordinal + 1)).toBe(true);
      }
    }
    expect(console.warn).not.toHaveBeenCalled();
    expect(console.error).not.toHaveBeenCalled();
  });

  it.each([[2, 1], [2, 2], [4, 3], [4, 4], [8, 7], [8, 8]])("checks %i log rolls against board count %i", async (rolls, count) => {
    const { root, session } = await fixture();
    for (let ordinal = 0; ordinal < rolls; ordinal++) await appendRoll(root, ordinal);
    if (rolls > count) expect(() => input(session, count)).toThrow(/does not reconcile/);
    else expect(input(session, count).rolls).toHaveLength(rolls);
  });

  it("accepts eight converged rolls with the exact unmatched live row at index 47", async () => {
    const { root, session } = await fixture();
    for (let ordinal = 0; ordinal < live.length; ordinal++) await appendRoll(root, ordinal);
    root.append(row(45, "Friendly Robber is active, tiles available to block are limited"), row(46, "Chapin7947 moved Robber to"), row(47, "No player to steal from"));
    await flush();
    expect(session.diceHistory.ambiguousLogIndices).toEqual([]);
    expect(input(session, 8).rolls).toHaveLength(8);
  });

  it.each(["Bot is selecting who to rob for Chapin7947", "Players don't have any cards"])("does not treat shipped robber status as a concealed roll: %s", async (text) => {
    const { root, session } = await fixture();
    await appendRoll(root, 0);
    root.append(row(17, text));
    await flush();
    expect(session.diceHistory.ambiguousLogIndices).toEqual([]);
    expect(input(session, 1).rolls).toHaveLength(1);
  });

  it("keeps a real indexed contradiction fail-closed after board catch-up", async () => {
    const { root, session, internals } = await fixture(true);
    await appendRoll(root, 0);
    await appendRoll(root, 1);
    overlay!.updateBoard(board(1, false, "hamzax"));
    root.append(row(20, "hamzax rolled", [3, 3]));
    await flush();
    sendMessage.mockClear();
    overlay!.updateBoard(board(2, true, "hamzax", { lastRoll: 5 }));
    await flush();
    expect(() => input(session, 2)).toThrow(/does not reconcile/);
    expect(internals!.decisionRuntimeError).toMatch(/does not reconcile/);
    expect(internals!.decisionAnalysis).toBeUndefined();
    expect(sendMessage).not.toHaveBeenCalled();
  });

  it("exposes no heuristic or autonomous click in the real renderer while evidence is unresolved", async () => {
    const { root, internals } = await fixture(true);
    await appendRoll(root, 0);
    await appendRoll(root, 1);
    overlay!.updateBoard(board(1, false, "hamzax"));
    expect(internals!.decisionAnalysis).toBeUndefined();
    sendMessage.mockClear();
    vi.mocked(internals!.render).mockRestore();
    const nextClick = vi.spyOn(internals!, "nextClick");
    internals!.render();
    expect(nextClick).not.toHaveBeenCalled();
    expect(sendMessage).not.toHaveBeenCalled();
    expect(document.querySelector("#colonist-assistant-root")!.shadowRoot!.querySelector(".board-marker")).toBeNull();
  });

  it("waits and resumes with actual mixed session capture and nested board publication", async () => {
    const { root, session, internals } = await fixture(true);
    for (let ordinal = 0; ordinal < 3; ordinal++) {
      const [, left, right] = live[ordinal]!;
      session.observeBoardDiceSnapshot(board(ordinal + 1, true, order[ordinal % 2]!, { lastRoll: left! + right!, botOnlyGame: true }));
      await appendRoll(root, ordinal);
    }
    expect(session.diceHistory.rolls).toHaveLength(6);
    overlay!.updateBoard(board(3, false, "hamzax", { botOnlyGame: true }));
    await flush();
    sendMessage.mockClear();
    await appendRoll(root, 3);
    expect(session.diceHistory.rolls).toHaveLength(7);
    expect(internals!.decisionRuntimeError).toMatch(/waiting for the board.*catch up/i);
    expect(sendMessage).not.toHaveBeenCalled();
    const next = board(4, true, "hamzax", { lastRoll: 9, botOnlyGame: true });
    // The exact ordering in index.ts: capture -> synchronous session update -> board update.
    session.observeBoardDiceSnapshot(next);
    expect(internals!.decisionRuntimeError).toMatch(/waiting for the board.*catch up/i);
    overlay!.updateBoard(next);
    await flush();
    expect(input(session, 4).rolls?.map((roll) => roll.ordinal)).toEqual([0, 1, 2, 3]);
    expect(input(session, 4).gaps ?? []).toEqual([]);
    expect(internals!.decisionRuntimeError).toBe("");
    expect(sendMessage.mock.calls.some(([message]) => message.stochastic?.rolls?.length === 4)).toBe(true);
    expect(console.warn).not.toHaveBeenCalled();
  });

  it("joins a board-only prefix to a sparse DOM suffix using all public constraints", async () => {
    const { root, session } = await fixture();
    session.observeBoardDiceSnapshot(board(1, true, "Chapin7947", { lastRoll: 6 }));
    session.observeBoardDiceSnapshot(board(2, true, "hamzax", { lastRoll: 5 }));
    // Human game's virtualizer attaches now. index.ts stops board roll capture.
    // Chapin's 9 cannot be ordinal 0 (independently observed as 6), so ordinal 2
    // is the only compatible placement under public count 3.
    await appendRoll(root, 2);
    expect(input(session, 3).rolls).toEqual([
      { ordinal: 0, actor: 0, total: 6 },
      { ordinal: 1, actor: 1, total: 5 },
      { ordinal: 2, actor: 0, total: 9 },
    ]);
  });

  it("resumes after a seven whose robber workflow completed between bridge samples", async () => {
    const { root, session, internals } = await fixture(true);
    for (let ordinal = 0; ordinal < 5; ordinal++) await appendRoll(root, ordinal);
    overlay!.updateBoard(board(5, false, "hamzax"));
    await appendRoll(root, 5);
    root.append(row(40, "Friendly Robber is active, tiles available to block are limited"), row(41, "hamzax moved Robber to"), row(42, "No player to steal from"));
    await flush();
    sendMessage.mockClear();
    const final = board(6, true, "hamzax", { lastRoll: 7, hexes: [{ id: "desert" }, { id: "other", blocked: true }] });
    overlay!.updateBoard(final);
    overlay!.updateBoard({ ...final });
    await flush();
    expect(input(session, 6).rolls).toHaveLength(6);
    expect(session.diceHistory.ambiguousLogIndices).toEqual([]);
    expect(internals!.awaitingLocalSevenProtocol()).toBe(false);
    expect(sendMessage.mock.calls.some(([message]) => message.stochastic)).toBe(true);
  });

  it("does not clear genuinely missing setup events when only index zero backfills", async () => {
    const { root, session } = await fixture(false, [row(3, "Chapin7947 placed a Road")]);
    expect(session.partialHistory).toBe(true);
    // Indexes 1/2 (potential settlement/starting resources) never arrived.
    root.prepend(row(0, "Happy settling!"));
    await flush();
    expect(session.partialHistory).toBe(true);
  });

  it("rejects contradictory board and DOM totals in a mounted human-game log", async () => {
    const { root, internals } = await fixture(true);
    await appendRoll(root, 0);
    await appendRoll(root, 1);
    sendMessage.mockClear();
    // index.ts does not call observeBoardDiceSnapshot for a mounted human log.
    overlay!.updateBoard(board(2, true, "hamzax", { lastRoll: 6, botOnlyGame: false }));
    await flush();
    expect(internals!.decisionAnalysis).toBeUndefined();
    expect(sendMessage).not.toHaveBeenCalled();
  });
});
