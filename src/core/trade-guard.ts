import type { ActiveTradeOffer } from "./placement";
import { RESOURCE_ORDER, type ResourceVector } from "./resources";

export const tradeOfferKey = (
  give: ResourceVector,
  receive: ResourceVector,
): string =>
  `${RESOURCE_ORDER.map((resource) => give[resource]).join(",")}>${RESOURCE_ORDER.map((resource) => receive[resource]).join(",")}`;

export const isFullySpecifiedTrade = (trade: ActiveTradeOffer): boolean =>
  RESOURCE_ORDER.some((resource) => trade.creatorGive[resource] > 0) &&
  RESOURCE_ORDER.some((resource) => trade.creatorReceive[resource] > 0);

export const outgoingTradeDisposition = (
  responsesComplete: boolean | undefined,
  firstSeenAt: number,
  now: number,
  timeoutMs = 18_000,
): "wait" | "cancel" =>
  responsesComplete || now - firstSeenAt >= timeoutMs
    ? "cancel"
    : "wait";

/**
 * Colonist can keep the original incoming offer in the DOM after we have
 * accepted, declined, or converted it into an outgoing counteroffer. Treating
 * that stale panel as unanswered launches duplicate searches and can replay
 * the response workflow. The executor-owned completion set is therefore part
 * of the protocol state, not merely presentation bookkeeping.
 */
export const unansweredIncomingTrades = (
  trades: readonly ActiveTradeOffer[] | undefined,
  completedTradeIds: ReadonlySet<string>,
): ActiveTradeOffer[] =>
  (trades ?? []).filter(
    (trade) =>
      trade.incoming &&
      !completedTradeIds.has(trade.id) &&
      (!trade.myResponse || trade.myResponse === "pending"),
  );

export const tradeMemoryScopeChanged = (
  previous:
    | {
        gameKey?: string;
        currentPlayer?: string;
        isMyTurn?: boolean;
        turn?: number;
      }
    | undefined,
  next:
    | {
        gameKey?: string;
        currentPlayer?: string;
        isMyTurn?: boolean;
        turn?: number;
      }
    | undefined,
): boolean =>
  Boolean(
    previous?.gameKey &&
      next?.gameKey &&
      previous.gameKey !== next.gameKey,
  ) ||
  Boolean(
    previous?.currentPlayer &&
      next?.currentPlayer &&
      previous.currentPlayer !== next.currentPlayer,
  ) ||
  Boolean(
    previous?.turn !== undefined &&
      next?.turn !== undefined &&
      previous.turn !== next.turn,
  ) ||
  Boolean(
    previous?.isMyTurn === false &&
      next?.isMyTurn === true &&
      (
        previous.turn === undefined ||
        next.turn === undefined
      ),
  );
