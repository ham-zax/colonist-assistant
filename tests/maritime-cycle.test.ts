import { describe, expect, it } from "vitest";

import {
  MaritimeCycleMemory,
  type MaritimeCycleObservation,
} from "../src/core/maritime-cycle";
import type { ResourceVector } from "../src/core/resources";

const hand = (
  lumber: number,
  brick: number,
  wool: number,
  grain: number,
  ore: number,
): ResourceVector => ({ lumber, brick, wool, grain, ore });

const allRatios = (value: number): ResourceVector =>
  hand(value, value, value, value, value);

const observeHand = (
  memory: MaritimeCycleMemory,
  current: ResourceVector,
  overrides: Partial<MaritimeCycleObservation> = {},
): void => {
  memory.observe({
    gameKey: "game-1",
    turn: 5,
    player: "You",
    isMyTurn: true,
    assetSignature: "assets-v1",
    ...overrides,
    hand: overrides.hand ? { ...overrides.hand } : { ...current },
    ratios: overrides.ratios ? { ...overrides.ratios } : allRatios(3),
  });
};

const currentSnapshot = (
  current: ResourceVector,
  overrides: Partial<MaritimeCycleObservation> = {},
): MaritimeCycleObservation => ({
  gameKey: "game-1",
  turn: 5,
  player: "You",
  isMyTurn: true,
  assetSignature: "assets-v1",
  ...overrides,
  hand: overrides.hand ? { ...overrides.hand } : { ...current },
  ratios: overrides.ratios ? { ...overrides.ratios } : allRatios(3),
});

const DEVELOPMENT_COST: Partial<ResourceVector> = {
  wool: 1,
  grain: 1,
  ore: 1,
};

// D221 can already afford development: 2L 0B 1W 3G 1O.
const D221 = hand(2, 0, 1, 3, 1);
// D222 cannot afford development: 3L 0B 1W 0G 1O.
const D222 = hand(3, 0, 1, 0, 1);

describe("maritime cycle memory", () => {
  it("allows the first bank leg before any cycle exists", () => {
    const memory = new MaritimeCycleMemory();
    observeHand(memory, D221);
    const excluded = memory.exclusions(currentSnapshot(D221));
    expect(
      excluded.some(
        (candidate) =>
          candidate.give === "grain" && candidate.receive === "lumber",
      ),
    ).toBe(false);
  });

  it("excludes the wasteful reverse of an exact cycle without recovery", () => {
    const memory = new MaritimeCycleMemory();
    observeHand(memory, D221);
    // 3 grain -> 1 lumber.
    observeHand(memory, D222);
    const excluded = memory.exclusions(currentSnapshot(D222));
    expect(excluded).toContainEqual({
      give: "lumber",
      receive: "grain",
      ratio: 3,
    });
  });

  it("preserves the legitimate D222 recovery when it restores development", () => {
    const memory = new MaritimeCycleMemory();
    observeHand(memory, D221);
    observeHand(memory, D222);
    const excluded = memory.exclusions(currentSnapshot(D222), [
      DEVELOPMENT_COST,
    ]);
    expect(
      excluded.some(
        (candidate) =>
          candidate.give === "lumber" && candidate.receive === "grain",
      ),
    ).toBe(false);
  });

  it("catches a three-resource loop while preserving unrelated trades", () => {
    const memory = new MaritimeCycleMemory();
    const start = hand(3, 0, 2, 2, 1);
    const afterLumber = hand(0, 0, 3, 2, 1);
    const afterWool = hand(0, 0, 0, 3, 1);
    observeHand(memory, start);
    observeHand(memory, afterLumber);
    observeHand(memory, afterWool);
    const excluded = memory.exclusions(currentSnapshot(afterWool));
    expect(excluded).toContainEqual({
      give: "grain",
      receive: "lumber",
      ratio: 3,
    });
    expect(
      excluded.some(
        (candidate) =>
          candidate.give === "grain" && candidate.receive === "brick",
      ),
    ).toBe(false);
  });

  it("preserves history across duplicate observations", () => {
    const memory = new MaritimeCycleMemory();
    observeHand(memory, D221);
    observeHand(memory, D221);
    observeHand(memory, D222);
    observeHand(memory, D222);
    const excluded = memory.exclusions(currentSnapshot(D222));
    expect(excluded).toContainEqual({
      give: "lumber",
      receive: "grain",
      ratio: 3,
    });
  });

  it("resets on a production-like single-card gain", () => {
    const memory = new MaritimeCycleMemory();
    observeHand(memory, D221);
    observeHand(memory, D222);
    // +1 lumber with no payment is production/robbery shaped, never a bank.
    observeHand(memory, hand(4, 0, 1, 0, 1));
    const excluded = memory.exclusions(
      currentSnapshot(hand(4, 0, 1, 0, 1)),
    );
    expect(
      excluded.some(
        (candidate) =>
          candidate.give === "lumber" && candidate.receive === "grain",
      ),
    ).toBe(false);
  });

  it("resets on a build-spend delta", () => {
    const memory = new MaritimeCycleMemory();
    observeHand(memory, D221);
    observeHand(memory, D222);
    // Spend one wool: -1 with no bank receipt.
    observeHand(memory, hand(3, 0, 0, 0, 1));
    const excluded = memory.exclusions(
      currentSnapshot(hand(3, 0, 0, 0, 1)),
    );
    expect(excluded).toHaveLength(0);
  });

  it("resets when development-card assets change even with a trade-shaped delta", () => {
    const memory = new MaritimeCycleMemory();
    observeHand(memory, D221);
    observeHand(memory, D222, { assetSignature: "assets-dev-bought" });
    const excluded = memory.exclusions(currentSnapshot(D222));
    expect(excluded).toHaveLength(0);
  });

  it("resets on a new turn", () => {
    const memory = new MaritimeCycleMemory();
    observeHand(memory, D221);
    observeHand(memory, D222);
    observeHand(memory, D222, { turn: 6 });
    const excluded = memory.exclusions(
      currentSnapshot(D222, { turn: 6 }),
    );
    expect(excluded).toHaveLength(0);
  });

  it("resets when the turn is lost or identity changes", () => {
    const memory = new MaritimeCycleMemory();
    observeHand(memory, D221);
    observeHand(memory, D222);
    const lost = new MaritimeCycleMemory();
    lost.observe({
      ...currentSnapshot(D221),
      isMyTurn: false,
    });
    expect(lost.exclusions(currentSnapshot(D222))).toHaveLength(0);

    observeHand(memory, D222, { player: "Opponent" });
    expect(memory.exclusions(currentSnapshot(D222))).toHaveLength(0);
  });

  it("resets on unknown scope rather than leaking old memory", () => {
    const memory = new MaritimeCycleMemory();
    observeHand(memory, D221);
    observeHand(memory, D222);
    const unknown: MaritimeCycleObservation = currentSnapshot(D222);
    delete unknown.gameKey;
    delete unknown.player;
    memory.observe(unknown);
    expect(memory.exclusions(currentSnapshot(D222))).toHaveLength(0);
  });

  it("resets when ratios change", () => {
    const memory = new MaritimeCycleMemory();
    observeHand(memory, D221);
    observeHand(memory, D222);
    observeHand(memory, D222, { ratios: hand(2, 3, 3, 3, 3) });
    const excluded = memory.exclusions(
      currentSnapshot(D222, { ratios: hand(2, 3, 3, 3, 3) }),
    );
    expect(excluded).toHaveLength(0);
  });

  it("accepts a visible-bank trade and rejects shortages and unrelated bank moves", () => {
    const accepted = new MaritimeCycleMemory();
    accepted.observe({
      ...currentSnapshot(D221),
      bank: hand(10, 10, 10, 10, 10),
    });
    accepted.observe({
      ...currentSnapshot(D222),
      // Bank takes 3 grain and gives 1 lumber.
      bank: hand(9, 10, 10, 13, 10),
    });
    expect(
      accepted.exclusions({
        ...currentSnapshot(D222),
        bank: hand(9, 10, 10, 13, 10),
      }),
    ).toContainEqual({ give: "lumber", receive: "grain", ratio: 3 });

    const shortage = new MaritimeCycleMemory();
    shortage.observe({
      ...currentSnapshot(D221),
      bank: hand(0, 10, 10, 10, 10),
    });
    shortage.observe({
      ...currentSnapshot(D222),
      // Bank absorbed the grain but had no lumber to give.
      bank: hand(0, 10, 10, 13, 10),
    });
    expect(
      shortage.exclusions({
        ...currentSnapshot(D222),
        bank: hand(0, 10, 10, 13, 10),
      }),
    ).toHaveLength(0);

    const unrelated = new MaritimeCycleMemory();
    unrelated.observe({
      ...currentSnapshot(D221),
      bank: hand(10, 10, 10, 10, 10),
    });
    unrelated.observe({
      ...currentSnapshot(D222),
      // Lumber/grain moved correctly but an unrelated brick shift invalidates.
      bank: hand(9, 9, 10, 13, 10),
    });
    expect(
      unrelated.exclusions({
        ...currentSnapshot(D222),
        bank: hand(9, 9, 10, 13, 10),
      }),
    ).toHaveLength(0);
  });

  it("never excludes stale or unaffordable trades", () => {
    const memory = new MaritimeCycleMemory();
    observeHand(memory, D221);
    observeHand(memory, D222);
    const excluded = memory.exclusions({
      ...currentSnapshot(D222),
      bank: hand(10, 10, 10, 10, 0),
    });
    // Ore is unavailable from this visible bank.
    expect(
      excluded.some((candidate) => candidate.receive === "ore"),
    ).toBe(false);
    // D222 holds no grain, so no grain payment is legal.
    expect(
      excluded.some((candidate) => candidate.give === "grain"),
    ).toBe(false);
  });

  it("permits burning cards above the real discard limit", () => {
    const memory = new MaritimeCycleMemory();
    observeHand(memory, D221);
    observeHand(memory, D222);
    const overLimit = hand(4, 0, 1, 0, 3);
    const resetting = new MaritimeCycleMemory();
    resetting.observe(currentSnapshot(D221));
    resetting.observe(currentSnapshot(D222));
    resetting.observe(currentSnapshot(overLimit));
    // Even if a witness existed, safety takes priority. Here the fresh
    // segment alone must not exclude, and the discard guard keeps it open.
    expect(resetting.exclusions(currentSnapshot(overLimit), [], 7)).toHaveLength(
      0,
    );
    expect(memory.exclusions(currentSnapshot(D222), [], 1)).toHaveLength(0);
  });

  it("records an integer bulk multiple of the bank rate", () => {
    const memory = new MaritimeCycleMemory();
    const start = hand(3, 0, 1, 6, 1);
    const afterBulk = hand(5, 0, 1, 0, 1);
    observeHand(memory, start);
    // -6 grain / +2 lumber at ratio 3.
    observeHand(memory, afterBulk);
    const excluded = memory.exclusions(currentSnapshot(afterBulk));
    expect(excluded).toContainEqual({
      give: "lumber",
      receive: "grain",
      ratio: 3,
    });
  });

  it("clears on unknown scope instead of accumulating a segment", () => {
    const memory = new MaritimeCycleMemory();
    const first: MaritimeCycleObservation = currentSnapshot(D221);
    delete first.gameKey;
    delete first.turn;
    delete first.player;
    const second: MaritimeCycleObservation = currentSnapshot(D222);
    delete second.gameKey;
    delete second.turn;
    delete second.player;
    // Trade-shaped delta, but scope is unresolved on both snapshots.
    memory.observe(first);
    memory.observe(second);
    expect(memory.exclusions(second)).toHaveLength(0);

    // Empty identities are unresolved too.
    const empty = new MaritimeCycleMemory();
    empty.observe(currentSnapshot(D221, { gameKey: "", player: "" }));
    empty.observe(currentSnapshot(D222, { gameKey: "", player: "" }));
    expect(
      empty.exclusions(currentSnapshot(D222, { gameKey: "", player: "" })),
    ).toHaveLength(0);

    // So are invalid turns.
    const badTurn = new MaritimeCycleMemory();
    badTurn.observe(currentSnapshot(D221, { turn: -1 }));
    badTurn.observe(currentSnapshot(D222, { turn: -1 }));
    expect(
      badTurn.exclusions(currentSnapshot(D222, { turn: -1 })),
    ).toHaveLength(0);
  });

  it("restarts when the visible bank changes under an identical hand", () => {
    const bankA = hand(9, 10, 10, 13, 10);
    const preserved = new MaritimeCycleMemory();
    preserved.observe({
      ...currentSnapshot(D221),
      bank: hand(10, 10, 10, 10, 10),
    });
    preserved.observe({ ...currentSnapshot(D222), bank: bankA });
    preserved.observe({ ...currentSnapshot(D222), bank: { ...bankA } });
    expect(
      preserved.exclusions({ ...currentSnapshot(D222), bank: bankA }),
    ).toContainEqual({ give: "lumber", receive: "grain", ratio: 3 });

    const restarted = new MaritimeCycleMemory();
    restarted.observe({
      ...currentSnapshot(D221),
      bank: hand(10, 10, 10, 10, 10),
    });
    restarted.observe({ ...currentSnapshot(D222), bank: bankA });
    // Same own hand, but the visible bank moved underneath us.
    restarted.observe({
      ...currentSnapshot(D222),
      bank: hand(9, 7, 10, 13, 10),
    });
    expect(
      restarted.exclusions({
        ...currentSnapshot(D222),
        bank: hand(9, 7, 10, 13, 10),
      }),
    ).toHaveLength(0);

    const visibilityGap = new MaritimeCycleMemory();
    visibilityGap.observe({
      ...currentSnapshot(D221),
      bank: hand(10, 10, 10, 10, 10),
    });
    visibilityGap.observe({ ...currentSnapshot(D222), bank: bankA });
    visibilityGap.observe(currentSnapshot(D222));
    expect(visibilityGap.exclusions(currentSnapshot(D222))).toHaveLength(0);
  });

  it("excludes nothing until the current hand has been observed", () => {
    const memory = new MaritimeCycleMemory();
    observeHand(memory, D221);
    observeHand(memory, D222);
    // Never observed: affordable for lumber, and its lumber->grain post
    // hand ([1,0,1,1,1]) is dominated by D221 — yet no exclusion applies
    // until observe() processes this hand.
    const fresh = hand(4, 0, 1, 0, 1);
    expect(memory.exclusions(currentSnapshot(fresh))).toHaveLength(0);
  });
});
