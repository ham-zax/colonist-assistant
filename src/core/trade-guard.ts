import type {
  DeepSearchAction,
  DeepSearchResult,
} from "./engine";
import type {
  ActiveTradeOffer,
  BoardSnapshot,
} from "./placement";
import {
  RESOURCE_ORDER,
  hasResources,
  type ResourceVector,
} from "./resources";
import type { TrackerState } from "./types";

export const tradeOfferKey = (
  give: ResourceVector,
  receive: ResourceVector,
): string =>
  `${RESOURCE_ORDER.map((resource) => give[resource]).join(",")}>${RESOURCE_ORDER.map((resource) => receive[resource]).join(",")}`;

const tupleResources = (
  tuple: [number, number, number, number, number],
): ResourceVector => ({
  lumber: tuple[0],
  brick: tuple[1],
  wool: tuple[2],
  grain: tuple[3],
  ore: tuple[4],
});

export const canAnyOpponentFulfillTrade = (
  state: TrackerState | undefined,
  player: string | undefined,
  receive: ResourceVector,
  recipients?: string[],
  board?: BoardSnapshot,
): boolean => {
  if (!player) return true;
  const candidates =
    recipients?.length
      ? recipients.filter((candidate) => candidate !== player)
      : (state?.playerOrder ?? Object.keys(board?.players ?? {}))
          .filter((candidate) => candidate !== player);
  if (!candidates.length) return false;
  const requestedCards = RESOURCE_ORDER.reduce(
    (sum, resource) => sum + receive[resource],
    0,
  );
  const candidatesWithEnoughCards = candidates.filter(
    (candidate) =>
      (board?.players?.[candidate]?.handSize ?? requestedCards) >=
      requestedCards,
  );
  if (!candidatesWithEnoughCards.length) return false;

  if (board?.bankVisible && board.bank) {
    const standardSupply =
      Object.keys(board.players ?? {}).length > 4 ? 24 : 19;
    const impossibleFromExactBank = RESOURCE_ORDER.some((resource) => {
      const opponentsHoldAtMost =
        standardSupply -
        board.bank![resource] -
        (board.ownHand?.[resource] ?? 0);
      return opponentsHoldAtMost < receive[resource];
    });
    if (impossibleFromExactBank) return false;
  }

  if (!state?.worlds.length) return true;
  return state.worlds.some((world) =>
    candidatesWithEnoughCards.some((candidate) => {
      const hand = world.hands[candidate];
      return Boolean(hand && hasResources(hand, receive));
    }),
  );
};

export const selectUsableDeepAction = (
  search: DeepSearchResult | undefined,
  state: TrackerState | undefined,
  player: string | undefined,
  attemptedTradeOffers: ReadonlySet<string>,
  board?: BoardSnapshot,
): DeepSearchAction | undefined => {
  const chosen = search?.chosen;
  if (!search || !chosen) return chosen;
  const usable = (action: DeepSearchAction): boolean => {
    if (
      (
        action.kind !== "offer-trade" &&
        action.kind !== "counter-trade"
      ) ||
      !action.cards ||
      !action.receiveCards
    ) {
      return true;
    }
    const give = tupleResources(action.cards);
    const receive = tupleResources(action.receiveCards);
    const counterparty =
      action.kind === "counter-trade"
        ? board?.activeTrades?.find(
            (trade) =>
              trade.incoming &&
              (!trade.myResponse || trade.myResponse === "pending"),
          )?.creator
        : undefined;
    return (
      !attemptedTradeOffers.has(tradeOfferKey(give, receive)) &&
      canAnyOpponentFulfillTrade(
        state,
        player,
        receive,
        counterparty ? [counterparty] : action.recipients,
        board,
      )
    );
  };
  if (usable(chosen)) return chosen;
  const root = Math.max(0, state?.playerOrder.indexOf(player ?? "") ?? 0);
  const invalidCounter = chosen.kind === "counter-trade";
  return [...search.actions]
    .filter(
      ({ action }) => {
        if (invalidCounter) {
          return (
            (
              action.kind === "respond-trade" ||
              action.kind === "counter-trade"
            ) &&
            usable(action)
          );
        }
        return (
          !["respond-trade", "counter-trade", "confirm-trade"].includes(
            action.kind,
          ) && usable(action)
        );
      },
    )
    .sort(
      (left, right) =>
        right.visits - left.visits ||
        (right.value[root] ?? 0) - (left.value[root] ?? 0),
    )[0]?.action;
};

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

/**
 * A counteroffer has one intended counterparty. Once that player accepts,
 * waiting for another strategic search can only add latency and risks losing
 * an already-agreed transaction. Ordinary broadcast offers still wait for
 * their response window when more partners may answer.
 */
export const shouldConfirmAcceptedTradeImmediately = (
  trade: Pick<
    ActiveTradeOffer,
    | "counterOffer"
    | "acceptedPlayers"
    | "pendingPlayers"
    | "responsesComplete"
  >,
): boolean =>
  trade.acceptedPlayers?.length === 1 &&
  (
    trade.counterOffer ||
    trade.responsesComplete ||
    !trade.pendingPlayers?.length
  );

export const tradeMemoryScopeChanged = (
  previous:
    | { gameKey?: string; currentPlayer?: string; isMyTurn?: boolean }
    | undefined,
  next:
    | { gameKey?: string; currentPlayer?: string; isMyTurn?: boolean }
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
  );
