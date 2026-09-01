import { describe, expect, it } from "vitest";

import type {
  DeepSearchAction,
  DeepSearchResult,
} from "../src/core/engine";
import type { BoardSnapshot } from "../src/core/placement";
import { emptyResources } from "../src/core/resources";
import {
  outgoingTradeDisposition,
  selectUsableDeepAction,
  shouldConfirmAcceptedTradeImmediately,
  tradeMemoryScopeChanged,
  tradeOfferKey,
  unansweredIncomingTrades,
} from "../src/core/trade-guard";
import { localTradeBundles } from "../src/core/trades";
import {
  createTrackerState,
  reduceTracker,
} from "../src/core/tracker";

const searchResult = (
  chosen: DeepSearchAction,
  alternative: DeepSearchAction,
): DeepSearchResult => ({
  engineRevision: "test",
  rootIndex: 0,
  algorithm: "maxn",
  chosen,
  rootValue: [0.25, 0.25],
  tacticalWinProbability: 0,
  tacticalLowerBound: 0,
  tacticalProven: false,
  exactDecision: false,
  exactWorlds: 0,
  tacticalLine: [],
  actions: [
    {
      action: chosen,
      visits: 100,
      availability: 1,
      availabilityWeight: 1,
      legalWeight: 1,
      prior: 0.8,
      value: [0.6, 0.4],
      lowerConfidenceValue: [0.5, 0.3],
    },
    {
      action: alternative,
      visits: 60,
      availability: 1,
      availabilityWeight: 1,
      legalWeight: 1,
      prior: 0.2,
      value: [0.5, 0.5],
      lowerConfidenceValue: [0.4, 0.4],
    },
  ],
  iterations: 100,
  nodes: 200,
  deepestDecisionDepth: 3,
  rollouts: 100,
  particles: 1,
  effectiveParticleCount: 1,
  elapsedMs: 10,
  seed: 1,
});

const stateWithBotOre = (ore: number) => {
  let state = createTrackerState();
  state = reduceTracker(state, { type: "discover", player: "You" });
  state = reduceTracker(state, { type: "discover", player: "Bot" });
  const own = emptyResources();
  own.brick = 1;
  const bot = emptyResources();
  bot.ore = ore;
  state.worlds = [{ hands: { You: own, Bot: bot }, weight: 1 }];
  return state;
};

describe("live trade guard", () => {
  it("derives local trade bundles from creator-relative storage", () => {
    const creatorGive = { ...emptyResources(), lumber: 1 };
    const creatorReceive = { ...emptyResources(), brick: 1 };

    expect(
      localTradeBundles({ creatorGive, creatorReceive, incoming: true }),
    ).toEqual({ give: creatorReceive, receive: creatorGive });
    expect(
      localTradeBundles({ creatorGive, creatorReceive, incoming: false }),
    ).toEqual({ give: creatorGive, receive: creatorReceive });
  });

  const offer: DeepSearchAction = {
    kind: "offer-trade",
    cards: [0, 1, 0, 0, 0],
    receiveCards: [0, 0, 0, 0, 1],
    recipients: ["Bot"],
  };
  const endTurn: DeepSearchAction = { kind: "end-turn" };

  it("skips an offer that no modeled recipient can fulfill", () => {
    const selected = selectUsableDeepAction(
      searchResult(offer, endTurn),
      stateWithBotOre(0),
      "You",
      new Set(),
    );

    expect(selected).toEqual(endTurn);
  });

  it("does not repeat an identical rejected offer in the same turn", () => {
    const give = emptyResources();
    give.brick = 1;
    const receive = emptyResources();
    receive.ore = 1;
    const selected = selectUsableDeepAction(
      searchResult(offer, endTurn),
      stateWithBotOre(2),
      "You",
      new Set([tradeOfferKey(give, receive)]),
    );

    expect(selected).toEqual(endTurn);
  });

  it("keeps a fresh offer when a recipient can fulfill it", () => {
    const selected = selectUsableDeepAction(
      searchResult(offer, endTurn),
      stateWithBotOre(2),
      "You",
      new Set(),
    );

    expect(selected).toEqual(offer);
  });

  it("trusts an exact bank count over a contradictory hidden-hand particle", () => {
    const bank = {
      lumber: 19,
      brick: 19,
      wool: 19,
      grain: 19,
      ore: 19,
    };
    const board: BoardSnapshot = {
      hexes: [],
      vertices: [],
      edges: [],
      myPlayer: "You",
      ownHand: emptyResources(),
      bank,
      bankVisible: true,
      players: {
        You: {
          handSize: 1,
          tradeRatios: emptyResources(),
          cardDiscardLimit: 7,
        },
        Bot: {
          handSize: 3,
          tradeRatios: emptyResources(),
          cardDiscardLimit: 7,
        },
      },
    };
    const selected = selectUsableDeepAction(
      searchResult(offer, endTurn),
      stateWithBotOre(2),
      "You",
      new Set(),
      board,
    );

    expect(selected).toEqual(endTurn);
  });

  it("replaces an impossible counteroffer using exact bank evidence", () => {
    const counter: DeepSearchAction = {
      kind: "counter-trade",
      cards: [0, 0, 0, 0, 1],
      receiveCards: [0, 1, 0, 0, 0],
    };
    const decline: DeepSearchAction = {
      kind: "respond-trade",
      accept: false,
    };
    const noBrick = emptyResources();
    noBrick.brick = 19;
    const board: BoardSnapshot = {
      hexes: [],
      vertices: [],
      edges: [],
      myPlayer: "You",
      ownHand: emptyResources(),
      bank: noBrick,
      bankVisible: true,
      players: {
        You: {
          handSize: 1,
          tradeRatios: emptyResources(),
          cardDiscardLimit: 7,
        },
        Bot: {
          handSize: 3,
          tradeRatios: emptyResources(),
          cardDiscardLimit: 7,
        },
      },
      activeTrades: [
        {
          id: "incoming",
          creator: "Bot",
          tradeExecutor: "Bot",
          creatorGive: emptyResources(),
          creatorReceive: emptyResources(),
          incoming: true,
          counterOffer: false,
          canAccept: true,
          myResponse: "pending",
        },
      ],
    };

    expect(
      selectUsableDeepAction(
        searchResult(counter, decline),
        stateWithBotOre(2),
        "You",
        new Set(),
        board,
      ),
    ).toEqual(decline);
  });

  it("does not repeat an identical counteroffer in the same turn", () => {
    const counter: DeepSearchAction = {
      kind: "counter-trade",
      cards: [0, 1, 0, 0, 0],
      receiveCards: [0, 0, 0, 0, 1],
    };
    const decline: DeepSearchAction = {
      kind: "respond-trade",
      accept: false,
    };
    const give = emptyResources();
    give.brick = 1;
    const receive = emptyResources();
    receive.ore = 1;
    const board: BoardSnapshot = {
      hexes: [],
      vertices: [],
      edges: [],
      myPlayer: "You",
      ownHand: give,
      players: {
        You: {
          handSize: 1,
          tradeRatios: emptyResources(),
          cardDiscardLimit: 7,
        },
        Bot: {
          handSize: 2,
          tradeRatios: emptyResources(),
          cardDiscardLimit: 7,
        },
      },
      activeTrades: [
        {
          id: "incoming-again",
          creator: "Bot",
          tradeExecutor: "Bot",
          creatorGive: emptyResources(),
          creatorReceive: emptyResources(),
          incoming: true,
          counterOffer: false,
          canAccept: true,
          myResponse: "pending",
        },
      ],
    };

    expect(
      selectUsableDeepAction(
        searchResult(counter, decline),
        stateWithBotOre(2),
        "You",
        new Set([tradeOfferKey(give, receive)]),
        board,
      ),
    ).toEqual(decline);
  });

  it("closes rejected offers immediately and unanswered offers after the watchdog", () => {
    expect(outgoingTradeDisposition(true, 1_000, 1_001)).toBe("cancel");
    expect(outgoingTradeDisposition(false, 1_000, 18_999)).toBe("wait");
    expect(outgoingTradeDisposition(false, 1_000, 19_000)).toBe("cancel");
  });

  it("confirms an accepted counteroffer without waiting for another search", () => {
    expect(
      shouldConfirmAcceptedTradeImmediately({
        counterOffer: true,
        acceptedPlayers: ["Bot"],
        pendingPlayers: ["Other bot"],
        responsesComplete: false,
      }),
    ).toBe(true);
    expect(
      shouldConfirmAcceptedTradeImmediately({
        counterOffer: false,
        acceptedPlayers: ["Bot"],
        pendingPlayers: ["Other bot"],
        responsesComplete: false,
      }),
    ).toBe(false);
  });

  it("keeps rejection memory through transient trade UI turn flags", () => {
    expect(
      tradeMemoryScopeChanged(
        { gameKey: "g", currentPlayer: "You", isMyTurn: true },
        { gameKey: "g", currentPlayer: "You", isMyTurn: false },
      ),
    ).toBe(false);
    expect(
      tradeMemoryScopeChanged(
        { gameKey: "g", currentPlayer: "You", isMyTurn: false },
        { gameKey: "g", currentPlayer: "Bot", isMyTurn: false },
      ),
    ).toBe(true);
  });

  it("resets rejection memory on a new turn without a current-player label", () => {
    expect(
      tradeMemoryScopeChanged(
        { gameKey: "g", turn: 8, isMyTurn: false },
        { gameKey: "g", turn: 9, isMyTurn: false },
      ),
    ).toBe(true);
    expect(
      tradeMemoryScopeChanged(
        { gameKey: "g", isMyTurn: false },
        { gameKey: "g", isMyTurn: true },
      ),
    ).toBe(true);
  });

  it("does not re-analyse an incoming offer after its response workflow completed", () => {
    const incoming = {
      id: "incoming-1",
      creator: "Bot",
      tradeExecutor: "Bot",
      creatorGive: emptyResources(),
      creatorReceive: emptyResources(),
      incoming: true,
      counterOffer: false,
      canAccept: true,
      myResponse: "pending" as const,
    };

    expect(unansweredIncomingTrades([incoming], new Set())).toEqual([
      incoming,
    ]);
    expect(
      unansweredIncomingTrades([incoming], new Set(["incoming-1"])),
    ).toEqual([]);
  });
});
