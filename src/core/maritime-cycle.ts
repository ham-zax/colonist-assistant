import {
  RESOURCE_ORDER,
  cloneResources,
  hasResources,
  resourceTotal,
  type Resource,
  type ResourceVector,
} from "./resources";

/**
 * Confirmed-observation maritime trade cycle memory.
 *
 * Records ONLY exact consecutive own-hand deltas that match a single bank
 * exchange (or an integer multiple of one) at unchanged ratios, asset
 * signature, and scope. No speculative dispatch data is required: manual and
 * autonomous bank trades are both captured when the visible deltas prove it.
 *
 * Parent wiring (overlay / WASM search):
 * - Call `observe()` on every validated bridge snapshot while it is my turn.
 *   Map BoardSnapshot to the observation with the narrow fields below:
 *   gameKey, turn, player (myPlayer), isMyTurn, ownHand, tradeRatios,
 *   assetSignature (roads/buildings/own cards/flags/ports hash), bank when
 *   visible.
 * - Call `exclusions()` with the current snapshot plus concrete
 *   `legalBuildCosts` (from legal placement sites/pieces/deck only) and the
 *   actual `discardLimit` to filter candidate bank trades before search.
 * - The returned list contains candidates to EXCLUDE, each as plain scalars
 *   `{ give, receive, ratio }` for the parent wire format.
 *
 * Ambiguity contract: a hidden-bank -ratio/+1 hand delta is CONSISTENT WITH
 * a bank trade, not proof of one — a domestic trade can produce the same
 * shape. The parent breaks that ambiguity with validated identity, an asset
 * signature that includes other public hand totals (so domestic legs move
 * the signature), and the actual bank where public. This module never
 * claims card identities are known and never treats unknown scope as a
 * match: recording and exclusion require a resolved nonempty gameKey and
 * player plus a defined finite nonnegative integer turn, and `exclusions`
 * requires the current hand to exactly equal the last observed hand.
 */
export interface MaritimeCycleObservation {
  gameKey?: string;
  turn?: number;
  player?: string;
  isMyTurn: boolean;
  hand: ResourceVector;
  ratios: ResourceVector;
  assetSignature: string;
  bank?: ResourceVector;
}

export interface MaritimeCycleExclusion {
  give: Resource;
  receive: Resource;
  ratio: number;
}

interface ParsedBankTrade {
  give: Resource;
  receive: Resource;
  count: number;
  ratio: number;
}

const handsEqual = (
  left: ResourceVector,
  right: ResourceVector,
): boolean =>
  RESOURCE_ORDER.every((resource) => left[resource] === right[resource]);

const ratiosEqual = (
  left: ResourceVector,
  right: ResourceVector,
): boolean =>
  RESOURCE_ORDER.every((resource) => left[resource] === right[resource]);

/**
 * Resolved scope requires a nonempty gameKey and player plus a defined
 * finite nonnegative integer turn. Anything less is unknown identity and
 * must clear the memory, never start or accumulate a segment.
 */
const isScopeResolved = (
  observation: MaritimeCycleObservation,
): boolean =>
  typeof observation.gameKey === "string" &&
  observation.gameKey.length > 0 &&
  typeof observation.player === "string" &&
  observation.player.length > 0 &&
  typeof observation.turn === "number" &&
  Number.isInteger(observation.turn) &&
  observation.turn >= 0;

const scopeEqual = (
  previous: MaritimeCycleObservation,
  next: MaritimeCycleObservation,
): boolean =>
  previous.gameKey === next.gameKey &&
  previous.turn === next.turn &&
  previous.player === next.player;

/**
 * Parse an exact bank-trade shaped hand delta.
 * Returns undefined for production, builds, discards, domestic trades,
 * robbery (+1-only), and any multi-resource change.
 * Bulk multiples (for example -6/+2 at ratio 3) are accepted because each
 * leg is still exactly a repeated bank rate.
 */
const parseBankTradeDelta = (
  previousHand: ResourceVector,
  nextHand: ResourceVector,
  ratios: ResourceVector,
): ParsedBankTrade | undefined => {
  let give: Resource | undefined;
  let receive: Resource | undefined;
  for (const resource of RESOURCE_ORDER) {
    const delta = nextHand[resource] - previousHand[resource];
    if (delta === 0) continue;
    if (delta < 0) {
      if (give !== undefined) return undefined;
      give = resource;
    } else {
      if (receive !== undefined) return undefined;
      receive = resource;
    }
  }
  if (give === undefined || receive === undefined) return undefined;
  const ratio = ratios[give];
  if (!Number.isInteger(ratio) || ratio < 2) return undefined;
  const count = nextHand[receive] - previousHand[receive];
  if (!Number.isInteger(count) || count < 1) return undefined;
  if (previousHand[give] - nextHand[give] !== count * ratio) return undefined;
  return { give, receive, count, ratio };
};

/**
 * Verify the visible bank moved opposite to the observed hand trade.
 * Missing bank on either side skips verification (bank not visible).
 * Any unrelated bank movement, shortage, or mismatch rejects the recording.
 */
const bankSupportsTrade = (
  previousBank: ResourceVector | undefined,
  nextBank: ResourceVector | undefined,
  trade: ParsedBankTrade,
): boolean => {
  if (previousBank === undefined || nextBank === undefined) return true;
  if (previousBank[trade.receive] < trade.count) return false;
  for (const resource of RESOURCE_ORDER) {
    const delta = nextBank[resource] - previousBank[resource];
    if (resource === trade.give) {
      if (delta !== trade.count * trade.ratio) return false;
    } else if (resource === trade.receive) {
      if (delta !== -trade.count) return false;
    } else if (delta !== 0) {
      return false;
    }
  }
  return true;
};

const postDominatesWitness = (
  post: ResourceVector,
  witness: ResourceVector,
): boolean => {
  let strictlyLess = false;
  for (const resource of RESOURCE_ORDER) {
    if (post[resource] > witness[resource]) return false;
    if (post[resource] < witness[resource]) strictlyLess = true;
  }
  return strictlyLess;
};

const cloneObservation = (
  observation: MaritimeCycleObservation,
): MaritimeCycleObservation => ({
  gameKey: observation.gameKey,
  turn: observation.turn,
  player: observation.player,
  isMyTurn: observation.isMyTurn,
  hand: cloneResources(observation.hand),
  ratios: cloneResources(observation.ratios),
  assetSignature: observation.assetSignature,
  bank: observation.bank ? cloneResources(observation.bank) : undefined,
});

export class MaritimeCycleMemory {
  private last: MaritimeCycleObservation | undefined;
  private readonly hands: ResourceVector[] = [];

  clear(): void {
    this.last = undefined;
    this.hands.length = 0;
  }

  private startSegment(observation: MaritimeCycleObservation): void {
    this.last = cloneObservation(observation);
    this.hands.length = 0;
    this.hands.push(cloneResources(observation.hand));
  }

  observe(observation: MaritimeCycleObservation): void {
    if (!observation.isMyTurn) {
      this.clear();
      return;
    }
    if (!isScopeResolved(observation)) {
      // Unknown scope/identity clears: it must never start a segment or
      // accumulate history, even across two identically unknown snapshots.
      this.clear();
      return;
    }
    const previous = this.last;
    if (previous === undefined) {
      this.startSegment(observation);
      return;
    }
    if (!scopeEqual(previous, observation)) {
      // Changed game/turn/player must not leak old memory into the new
      // scope. (Unknown scope already cleared above, so both sides here
      // are resolved.)
      this.startSegment(observation);
      return;
    }
    if (
      !ratiosEqual(previous.ratios, observation.ratios) ||
      previous.assetSignature !== observation.assetSignature
    ) {
      this.startSegment(observation);
      return;
    }
    if (handsEqual(previous.hand, observation.hand)) {
      // A duplicate observation preserves history only when the bank view
      // is identical too. A changed visible bank or a visibility
      // discontinuity restarts the segment: unrelated bank movement breaks
      // the segment even when our hand did not move.
      const previousBank = previous.bank;
      const nextBank = observation.bank;
      const banksIdentical =
        (previousBank === undefined && nextBank === undefined) ||
        (previousBank !== undefined &&
          nextBank !== undefined &&
          handsEqual(previousBank, nextBank));
      if (!banksIdentical) {
        this.startSegment(observation);
        return;
      }
      this.last = cloneObservation(observation);
      return;
    }
    const trade = parseBankTradeDelta(
      previous.hand,
      observation.hand,
      previous.ratios,
    );
    if (
      trade === undefined ||
      !bankSupportsTrade(previous.bank, observation.bank, trade)
    ) {
      // Production, domestic trade, discard, build spend, development-card
      // asset change already handled above, robbery-like +1-only gain, bank
      // shortage, or unrelated bank movement: conservatively restart.
      this.startSegment(observation);
      return;
    }
    this.hands.push(cloneResources(observation.hand));
    this.last = cloneObservation(observation);
  }

  exclusions(
    current: MaritimeCycleObservation,
    legalBuildCosts?: readonly Partial<ResourceVector>[],
    discardLimit?: number,
  ): MaritimeCycleExclusion[] {
    const previous = this.last;
    if (previous === undefined || this.hands.length === 0) return [];
    if (!current.isMyTurn) return [];
    if (!isScopeResolved(current)) return [];
    if (!scopeEqual(previous, current)) return [];
    if (
      !ratiosEqual(previous.ratios, current.ratios) ||
      previous.assetSignature !== current.assetSignature
    ) {
      return [];
    }
    if (!handsEqual(previous.hand, current.hand)) {
      // The current hand was never observed (a fresh unobserved gain can
      // otherwise let old history exclude a valid current decision).
      // No exclusions until observe() processes this hand.
      return [];
    }
    if (
      discardLimit !== undefined &&
      resourceTotal(current.hand) > discardLimit
    ) {
      // Burning cards through the bank can avoid a forced discard loss.
      return [];
    }
    const excluded: MaritimeCycleExclusion[] = [];
    for (const give of RESOURCE_ORDER) {
      const ratio = current.ratios[give];
      if (!Number.isInteger(ratio) || ratio < 2) continue;
      if (current.hand[give] < ratio) continue;
      for (const receive of RESOURCE_ORDER) {
        if (receive === give) continue;
        if (current.bank !== undefined && current.bank[receive] <= 0) {
          continue;
        }
        const post = cloneResources(current.hand);
        post[give] -= ratio;
        post[receive] += 1;
        const dominated = this.hands.some((witness) =>
          postDominatesWitness(post, witness),
        );
        if (!dominated) continue;
        const recoversBuild = (legalBuildCosts ?? []).some(
          (cost) =>
            !hasResources(current.hand, cost) && hasResources(post, cost),
        );
        if (recoversBuild) continue;
        excluded.push({ give, receive, ratio });
      }
    }
    return excluded;
  }
}
