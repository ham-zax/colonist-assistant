import { describe, expect, it } from "vitest";
import { DevelopmentSnapshotSync, hasPendingFreeRoad } from "../src/core/development-sync";
import type { BoardSnapshot, DevelopmentCardVector } from "../src/core/placement";
import { emptyResources } from "../src/core/resources";
import { createTrackerState, reduceTracker } from "../src/core/tracker";
import type { StoredEvent } from "../src/core/types";

const cards = (monopoly = 0): DevelopmentCardVector => ({
  knight: 0, monopoly, "road-building": 0, "year-of-plenty": 0, "victory-point": 1,
});
const fixture = () => {
  let state = reduceTracker(createTrackerState(), { type: "discover", player: "You" });
  state.players.You!.playedDevCards.monopoly = 1;
  const board: BoardSnapshot = {
    hexes: [], vertices: [], edges: [], diceMode: "unknown", gameKey: "one", turn: 91,
    myPlayer: "You", isMyTurn: true, action: "none",
    ownDevelopmentCards: {
      cards: cards(1), playable: cards(1), boughtThisTurn: { ...cards(), "victory-point": 0 },
      hasPlayedThisTurn: false,
    },
    players: { You: {
      handSize: 0, tradeRatios: emptyResources(), cardDiscardLimit: 7,
      playedDevelopmentCards: { ...cards(1), "victory-point": 0 },
    } },
  };
  return { state, board };
};

describe("development hand synchronization", () => {
  it("waits when a logged Monopoly play arrives 21 ms before its hand snapshot", () => {
    const { state, board } = fixture();
    const sync = new DevelopmentSnapshotSync();
    expect(sync.isWaiting(state, board)).toBe(false);
    const played = reduceTracker(state, { type: "play-dev", player: "You", card: "monopoly" });
    expect(sync.isWaiting(played, board)).toBe(true);
    // Multiple renders cannot accidentally adopt the stale hand as a new baseline.
    expect(sync.isWaiting(played, board)).toBe(true);
    const fresh: BoardSnapshot = {
      ...board,
      ownDevelopmentCards: { ...board.ownDevelopmentCards!, cards: cards(), playable: cards(), hasPlayedThisTurn: true },
      players: { You: { ...board.players!.You!, playedDevelopmentCards: { ...cards(2), "victory-point": 0 } } },
    };
    expect(sync.isWaiting(played, fresh)).toBe(false);
    expect(sync.isWaiting(played, fresh)).toBe(false);
  });

  it("still waits if the public tally catches up before the private hand or played flag", () => {
    const { state, board } = fixture();
    const sync = new DevelopmentSnapshotSync();
    sync.isWaiting(state, board);
    const played = reduceTracker(state, { type: "play-dev", player: "You", card: "monopoly" });
    const publicFresh = { ...board, players: { You: { ...board.players!.You!, playedDevelopmentCards: { ...cards(2), "victory-point": 0 } } } };
    expect(sync.isWaiting(played, publicFresh)).toBe(true);
    expect(sync.isWaiting(played, { ...publicFresh, ownDevelopmentCards: {
      ...board.ownDevelopmentCards!, cards: cards(),
    } })).toBe(true);
    expect(sync.isWaiting(played, { ...publicFresh, ownDevelopmentCards: {
      ...board.ownDevelopmentCards!, cards: cards(), hasPlayedThisTurn: true,
    } })).toBe(false);
  });

  it("allows a later exact purchase and a new game's inventory", () => {
    const { state, board } = fixture();
    const sync = new DevelopmentSnapshotSync();
    sync.isWaiting(state, board);
    const played = reduceTracker(state, { type: "play-dev", player: "You", card: "monopoly" });
    const afterPurchase = { ...board, ownDevelopmentCards: {
      ...board.ownDevelopmentCards!, boughtThisTurn: cards(1), hasPlayedThisTurn: true,
    }, players: { You: { ...board.players!.You!, playedDevelopmentCards: { ...cards(2), "victory-point": 0 } } } };
    expect(sync.isWaiting(played, afterPurchase)).toBe(false);
    expect(sync.isWaiting(state, { ...board, gameKey: "two" })).toBe(false);
  });

  it("does not require a same-turn played flag after the turn advances", () => {
    const { state, board } = fixture();
    const sync = new DevelopmentSnapshotSync();
    sync.isWaiting(state, board);
    const played = reduceTracker(state, { type: "play-dev", player: "You", card: "monopoly" });
    expect(sync.isWaiting(played, { ...board, turn: 92, ownDevelopmentCards: {
      ...board.ownDevelopmentCards!, cards: cards(),
    }, players: { You: { ...board.players!.You!, playedDevelopmentCards: { ...cards(2), "victory-point": 0 } } } })).toBe(false);
  });

  it("keeps an outstanding consumed Monopoly unresolved across a turn advance", () => {
    const { state, board } = fixture();
    const sync = new DevelopmentSnapshotSync();
    sync.isWaiting(state, board);
    const played = reduceTracker(state, { type: "play-dev", player: "You", card: "monopoly" });
    const tally = { ...cards(2), "victory-point": 0 };
    // Same-turn stale hand waits, then the turn advances with the consumed
    // card still present: repeated stale snapshots must keep waiting without
    // adopting the stale hand, and no same-turn played flag is required.
    expect(sync.isWaiting(played, { ...board, ownDevelopmentCards: {
      ...board.ownDevelopmentCards!, cards: cards(1),
    }, players: { You: { ...board.players!.You!, playedDevelopmentCards: tally } } })).toBe(true);
    const staleNextTurn: BoardSnapshot = { ...board, turn: 92, ownDevelopmentCards: {
      ...board.ownDevelopmentCards!, cards: cards(1), hasPlayedThisTurn: false,
    }, players: { You: { ...board.players!.You!, playedDevelopmentCards: tally } } };
    expect(sync.isWaiting(played, staleNextTurn)).toBe(true);
    expect(sync.isWaiting(played, staleNextTurn)).toBe(true);
    // The corrected hand resolves the wait even with a false same-turn flag.
    const corrected: BoardSnapshot = { ...staleNextTurn, ownDevelopmentCards: {
      ...staleNextTurn.ownDevelopmentCards!, cards: cards(), hasPlayedThisTurn: false,
    } };
    expect(sync.isWaiting(played, corrected)).toBe(false);
    expect(sync.isWaiting(played, corrected)).toBe(false);
  });

  it("accepts a legitimate fresh purchase on the next turn", () => {
    const { state, board } = fixture();
    const sync = new DevelopmentSnapshotSync();
    sync.isWaiting(state, board);
    // Same-turn purchase with a consistent hand refreshes the baseline,
    // which now carries an old-turn purchase count.
    const withPurchase: BoardSnapshot = { ...board, ownDevelopmentCards: {
      ...board.ownDevelopmentCards!, cards: cards(2), boughtThisTurn: cards(1),
    }, players: { You: { ...board.players!.You!, playedDevelopmentCards: { ...cards(1), "victory-point": 0 } } } };
    expect(sync.isWaiting(state, withPurchase)).toBe(false);
    // Next turn brings both a logged play and a fresh purchase. Only the
    // current turn's counter counts as new, so the consistent hand is
    // accepted while a genuinely stale hand still waits.
    const played = reduceTracker(state, { type: "play-dev", player: "You", card: "monopoly" });
    const tally = { ...cards(2), "victory-point": 0 };
    const stale: BoardSnapshot = { ...board, turn: 92, ownDevelopmentCards: {
      ...board.ownDevelopmentCards!, cards: cards(3), boughtThisTurn: cards(1), hasPlayedThisTurn: false,
    }, players: { You: { ...board.players!.You!, playedDevelopmentCards: tally } } };
    expect(sync.isWaiting(played, stale)).toBe(true);
    const nextTurn: BoardSnapshot = { ...stale, ownDevelopmentCards: {
      ...stale.ownDevelopmentCards!, cards: cards(2),
    } };
    expect(sync.isWaiting(played, nextTurn)).toBe(false);
    expect(sync.isWaiting(played, nextTurn)).toBe(false);
  });

  it("preserves the pre-play baseline when the exact hand runs ahead of the public tally", () => {
    const { state, board } = fixture();
    const sync = new DevelopmentSnapshotSync();
    sync.isWaiting(state, board);
    // The exact hand (and played flag) observe the Monopoly play before the
    // tracker log and public tally catch up. Adopting the decremented hand
    // here would orphan the private decrement and invent negative
    // consumption debt once the tally arrives.
    const privateFirst: BoardSnapshot = { ...board, ownDevelopmentCards: {
      ...board.ownDevelopmentCards!, cards: cards(), hasPlayedThisTurn: true,
    } };
    expect(sync.isWaiting(state, privateFirst)).toBe(true);
    expect(sync.isWaiting(state, privateFirst)).toBe(true);
    // Tracker and public tally catch up with the already-seen hand: no wait,
    // with no negative expected count and no adopted held-0 baseline.
    const played = reduceTracker(state, { type: "play-dev", player: "You", card: "monopoly" });
    const caughtUp: BoardSnapshot = { ...privateFirst, players: { You: {
      ...board.players!.You!, playedDevelopmentCards: { ...cards(2), "victory-point": 0 },
    } } };
    expect(sync.isWaiting(played, caughtUp)).toBe(false);
    expect(sync.isWaiting(played, caughtUp)).toBe(false);
    // No permanent pause across the turn boundary either.
    expect(sync.isWaiting(played, { ...caughtUp, turn: 92, ownDevelopmentCards: {
      ...caughtUp.ownDevelopmentCards!, hasPlayedThisTurn: false,
    } })).toBe(false);
  });

  it("accepts a legitimate purchase alongside the late public tally", () => {
    const { state, board } = fixture();
    const sync = new DevelopmentSnapshotSync();
    sync.isWaiting(state, board);
    const privateFirst: BoardSnapshot = { ...board, ownDevelopmentCards: {
      ...board.ownDevelopmentCards!, cards: cards(), hasPlayedThisTurn: true,
    } };
    expect(sync.isWaiting(state, privateFirst)).toBe(true);
    // The late tally arrives together with a legitimate new purchase: the
    // purchase belongs in the bound, so the consistent hand is accepted
    // rather than held against a clamped-zero expectation.
    const played = reduceTracker(state, { type: "play-dev", player: "You", card: "monopoly" });
    const withPurchase: BoardSnapshot = { ...board, ownDevelopmentCards: {
      ...board.ownDevelopmentCards!, cards: cards(1), boughtThisTurn: cards(1), hasPlayedThisTurn: true,
    }, players: { You: {
      ...board.players!.You!, playedDevelopmentCards: { ...cards(2), "victory-point": 0 },
    } } };
    expect(sync.isWaiting(played, withPurchase)).toBe(false);
    expect(sync.isWaiting(played, withPurchase)).toBe(false);
  });
});

describe("free road prompt recognition", () => {
  const play: StoredEvent = {
    type: "play-dev", player: "You", card: "road-building", id: "rb", timestamp: 1_000, raw: "You played Road Building",
  };
  it("recognizes the live owed-road shape with no card left in hand", () => {
    const { state, board } = fixture();
    state.recentEvents = [play];
    const prompt: BoardSnapshot = { ...board, action: "road", legalEdgeIds: ["edge"] };
    expect(hasPendingFreeRoad(state, prompt)).toBe(true);
    expect(prompt.ownDevelopmentCards?.cards["road-building"]).toBe(0);
    expect(hasPendingFreeRoad(state, { ...prompt, initialPlacement: true })).toBe(false);
    expect(hasPendingFreeRoad(state, { ...prompt, legalEdgeIds: [] })).toBe(false);
  });

  it("does not turn paid roads or an older turn's card play into free roads", () => {
    const { state, board } = fixture();
    const prompt: BoardSnapshot = { ...board, action: "road", legalEdgeIds: ["edge"] };
    expect(hasPendingFreeRoad(state, prompt)).toBe(false);
    expect(hasPendingFreeRoad(state, { ...prompt, localSeatDiagnostics: {
      seatSource: "gameController.myColor", localActionState: 30,
      identity: { status: "unresolved", reason: "invalid-my-color", source: "none", currentUserIdAvailable: false, currentUserMatchColors: [] },
    } })).toBe(false);
    state.recentEvents = [play, { type: "roll", player: "Rival", id: "roll", timestamp: 2_000, raw: "Rival rolled" }];
    expect(hasPendingFreeRoad(state, prompt)).toBe(false);
    state.recentEvents = [{ ...play, card: "knight" }];
    expect(hasPendingFreeRoad(state, prompt)).toBe(false);
  });

  it("requires a public Road Building play and distinguishes paid bridge prompts", () => {
    const { state, board } = fixture();
    state.recentEvents = [play];
    for (const localActionState of [3, 4, 5, 30, 31, 99]) {
      const prompt: BoardSnapshot = { ...board, action: "road", legalEdgeIds: ["edge"], localSeatDiagnostics: {
        seatSource: "gameController.myColor", localActionState,
        identity: { status: "unresolved", reason: "invalid-my-color", source: "none", currentUserIdAvailable: false, currentUserMatchColors: [] },
      } };
      expect(hasPendingFreeRoad(state, prompt)).toBe(localActionState === 30 || localActionState === 31);
    }
    state.recentEvents.push(...[1, 2].map((index): StoredEvent => ({
      type: "spend", player: "You", reason: "road", cost: emptyResources(),
      id: `road-${index}`, timestamp: 1_000 + index, raw: "You built a road",
    })));
    expect(hasPendingFreeRoad(state, { ...board, action: "road", legalEdgeIds: ["edge"] })).toBe(false);
  });
});
