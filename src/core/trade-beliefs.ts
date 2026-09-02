import type { ActiveTradeOffer } from "./placement";
import type { ResourceVector } from "./resources";
import { emptyResources } from "./resources";
import type { TrackerEvent } from "./types";

export interface TradeOfferSnapshot {
  id: string;
  creator: string;
  creatorGive: ResourceVector;
  creatorReceive: ResourceVector;
  creatorGiveOpenEnded: boolean;
  creatorReceiveOpenEnded: boolean;
  counterOffer: boolean;
  counterOfferInResponseToTradeId?: string;
  acceptedPlayers: string[];
  rejectedPlayers: string[];
  embargoedPlayers: string[];
  pendingPlayers: string[];
  responsesComplete: boolean;
  myResponse?: ActiveTradeOffer["myResponse"];
}

export const snapshotActiveTrades = (
  trades: readonly ActiveTradeOffer[] | undefined,
): Map<string, TradeOfferSnapshot> => {
  const snapshots = new Map<string, TradeOfferSnapshot>();
  for (const trade of trades ?? []) {
    snapshots.set(trade.id, {
      id: trade.id,
      creator: trade.creator,
      creatorGive: { ...emptyResources(), ...trade.creatorGive },
      creatorReceive: { ...emptyResources(), ...trade.creatorReceive },
      creatorGiveOpenEnded: Boolean(trade.creatorGiveOpenEnded),
      creatorReceiveOpenEnded: Boolean(trade.creatorReceiveOpenEnded),
      counterOffer: trade.counterOffer,
      ...(trade.counterOfferInResponseToTradeId
        ? {
            counterOfferInResponseToTradeId:
              trade.counterOfferInResponseToTradeId,
          }
        : {}),
      acceptedPlayers: [...(trade.acceptedPlayers ?? [])],
      rejectedPlayers: [...(trade.rejectedPlayers ?? [])],
      embargoedPlayers: [...(trade.embargoedPlayers ?? [])],
      pendingPlayers: [...(trade.pendingPlayers ?? [])],
      responsesComplete: Boolean(trade.responsesComplete),
      ...(trade.myResponse ? { myResponse: trade.myResponse } : {}),
    });
  }
  return snapshots;
};

/**
 * Diff consecutive Colonist active-trade snapshots into durable tracker events.
 * Offers, accepts, rejects, counters, and expirations are not reliably present
 * in the public game log, so the board trade panel is the observation source.
 */
export const tradeBeliefEventsFromDiff = (
  previous: Map<string, TradeOfferSnapshot>,
  next: Map<string, TradeOfferSnapshot>,
  knownEmbargoes: Readonly<Record<string, readonly string[]>> = {},
): TrackerEvent[] => {
  const events: TrackerEvent[] = [];
  const counterPlayersByParent = new Map<string, Set<string>>();
  for (const trade of next.values()) {
    const parentId = trade.counterOfferInResponseToTradeId;
    if (!trade.counterOffer || !parentId) continue;
    const players = counterPlayersByParent.get(parentId) ?? new Set<string>();
    players.add(trade.creator);
    counterPlayersByParent.set(parentId, players);
  }
  const counteredPreviousIds = new Set<string>();

  for (const [id, trade] of next) {
    const before = previous.get(id);
    if (!before) {
      if (trade.counterOffer) {
        const parent = trade.counterOfferInResponseToTradeId
          ? previous.get(trade.counterOfferInResponseToTradeId)
          : undefined;
        if (parent) counteredPreviousIds.add(parent.id);
        // Colonist normally creates a new offer id for a counter and exposes
        // the parent id. If the parent snapshot predates this session attach,
        // retain the conservative inverse fallback used before this migration.
        events.push({
          type: "trade-countered",
          player: trade.creator,
          creator: parent?.creator ?? trade.pendingPlayers[0] ?? trade.creator,
          give: parent?.creatorGive ?? trade.creatorReceive,
          receive: parent?.creatorReceive ?? trade.creatorGive,
          ...((parent?.creatorGiveOpenEnded ?? trade.creatorReceiveOpenEnded)
            ? { giveOpenEnded: true }
            : {}),
          ...((parent?.creatorReceiveOpenEnded ?? trade.creatorGiveOpenEnded)
            ? { receiveOpenEnded: true }
            : {}),
          counterGive: trade.creatorGive,
          counterReceive: trade.creatorReceive,
        });
      } else {
        events.push({
          type: "trade-offered",
          player: trade.creator,
          recipients: trade.pendingPlayers.length
            ? [...trade.pendingPlayers]
            : [
                ...trade.acceptedPlayers,
                ...trade.rejectedPlayers,
                ...trade.embargoedPlayers,
              ].filter((player, index, all) => all.indexOf(player) === index),
          give: trade.creatorGive,
          receive: trade.creatorReceive,
          ...(trade.creatorGiveOpenEnded ? { giveOpenEnded: true } : {}),
          ...(trade.creatorReceiveOpenEnded ? { receiveOpenEnded: true } : {}),
        });
      }
    }

    const priorEmbargoed = new Set(before?.embargoedPlayers ?? []);
    for (const player of trade.embargoedPlayers) {
      if (!priorEmbargoed.has(player)) {
        events.push({
          type: "trade-embargoed",
          player,
          creator: trade.creator,
        });
      }
    }
    const nonEmbargoedResponders = [
      ...trade.acceptedPlayers,
      ...trade.pendingPlayers,
      ...trade.rejectedPlayers,
    ].filter((player, index, all) => all.indexOf(player) === index);
    for (const player of nonEmbargoedResponders) {
      const wasEmbargoed = priorEmbargoed.has(player);
      const knownEmbargo = knownEmbargoes[player]?.includes(trade.creator) ?? false;
      if (wasEmbargoed || (!before && knownEmbargo)) {
        events.push({
          type: "trade-embargo-cleared",
          player,
          creator: trade.creator,
        });
      }
    }

    const priorAccepted = new Set(before?.acceptedPlayers ?? []);
    for (const player of trade.acceptedPlayers) {
      if (!priorAccepted.has(player)) {
        events.push({
          type: "trade-accepted",
          player,
          creator: trade.creator,
          give: trade.creatorGive,
          receive: trade.creatorReceive,
          ...(trade.creatorGiveOpenEnded ? { giveOpenEnded: true } : {}),
          ...(trade.creatorReceiveOpenEnded ? { receiveOpenEnded: true } : {}),
        });
      }
    }

    const priorRejected = new Set(before?.rejectedPlayers ?? []);
    const counteringPlayers = counterPlayersByParent.get(id);
    for (const player of trade.rejectedPlayers) {
      if (!priorRejected.has(player) && !counteringPlayers?.has(player)) {
        events.push({
          type: "trade-rejected",
          player,
          creator: trade.creator,
          give: trade.creatorGive,
          receive: trade.creatorReceive,
          ...(trade.creatorGiveOpenEnded ? { giveOpenEnded: true } : {}),
          ...(trade.creatorReceiveOpenEnded ? { receiveOpenEnded: true } : {}),
        });
      }
    }

    if (
      before &&
      !before.counterOffer &&
      trade.counterOffer
    ) {
      events.push({
        type: "trade-countered",
        player: trade.creator,
        creator: before.creator,
        give: before.creatorGive,
        receive: before.creatorReceive,
        ...(before.creatorGiveOpenEnded ? { giveOpenEnded: true } : {}),
        ...(before.creatorReceiveOpenEnded ? { receiveOpenEnded: true } : {}),
        counterGive: trade.creatorGive,
        counterReceive: trade.creatorReceive,
      });
    }
  }

  for (const [id, trade] of previous) {
    if (next.has(id) || counteredPreviousIds.has(id)) continue;
    const completed =
      trade.acceptedPlayers.length > 0 || trade.responsesComplete;
    if (completed) continue;
    events.push({
      type: "trade-expired",
      player: trade.creator,
      recipients: [
        ...trade.pendingPlayers,
        ...trade.rejectedPlayers,
        ...trade.embargoedPlayers,
      ].filter((player, index, all) => all.indexOf(player) === index),
      give: trade.creatorGive,
      receive: trade.creatorReceive,
      ...(trade.creatorGiveOpenEnded ? { giveOpenEnded: true } : {}),
      ...(trade.creatorReceiveOpenEnded ? { receiveOpenEnded: true } : {}),
    });
  }

  return events;
};
