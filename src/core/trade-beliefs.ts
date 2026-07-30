import type { ActiveTradeOffer } from "./placement";
import type { ResourceVector } from "./resources";
import { RESOURCE_ORDER, emptyResources } from "./resources";
import type { TrackerEvent } from "./types";

export interface TradeOfferSnapshot {
  id: string;
  creator: string;
  give: ResourceVector;
  receive: ResourceVector;
  counterOffer: boolean;
  acceptedPlayers: string[];
  rejectedPlayers: string[];
  pendingPlayers: string[];
  responsesComplete: boolean;
  myResponse?: ActiveTradeOffer["myResponse"];
}

const sameVector = (left: ResourceVector, right: ResourceVector): boolean =>
  RESOURCE_ORDER.every((key) => (left[key] ?? 0) === (right[key] ?? 0));

export const snapshotActiveTrades = (
  trades: readonly ActiveTradeOffer[] | undefined,
): Map<string, TradeOfferSnapshot> => {
  const snapshots = new Map<string, TradeOfferSnapshot>();
  for (const trade of trades ?? []) {
    snapshots.set(trade.id, {
      id: trade.id,
      creator: trade.creator,
      give: { ...emptyResources(), ...trade.give },
      receive: { ...emptyResources(), ...trade.receive },
      counterOffer: trade.counterOffer,
      acceptedPlayers: [...(trade.acceptedPlayers ?? [])],
      rejectedPlayers: [...(trade.rejectedPlayers ?? [])],
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
): TrackerEvent[] => {
  const events: TrackerEvent[] = [];

  for (const [id, trade] of next) {
    const before = previous.get(id);
    if (!before) {
      if (trade.counterOffer) {
        // A counter arrives as a new offer object; the original creator is the
        // counterparty we are responding to when the panel still lists them.
        events.push({
          type: "trade-countered",
          player: trade.creator,
          creator: trade.pendingPlayers[0] ?? trade.creator,
          give: trade.receive,
          receive: trade.give,
          counterGive: trade.give,
          counterReceive: trade.receive,
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
              ].filter((player, index, all) => all.indexOf(player) === index),
          give: trade.give,
          receive: trade.receive,
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
          give: trade.give,
          receive: trade.receive,
        });
      }
    }

    const priorRejected = new Set(before?.rejectedPlayers ?? []);
    for (const player of trade.rejectedPlayers) {
      if (!priorRejected.has(player)) {
        events.push({
          type: "trade-rejected",
          player,
          creator: trade.creator,
          give: trade.give,
          receive: trade.receive,
        });
      }
    }

    if (
      before &&
      !before.counterOffer &&
      trade.counterOffer &&
      sameVector(before.give, trade.give) === false
    ) {
      events.push({
        type: "trade-countered",
        player: trade.creator,
        creator: before.creator,
        give: before.give,
        receive: before.receive,
        counterGive: trade.give,
        counterReceive: trade.receive,
      });
    }
  }

  for (const [id, trade] of previous) {
    if (next.has(id)) continue;
    const completed =
      trade.acceptedPlayers.length > 0 || trade.responsesComplete;
    if (completed) continue;
    events.push({
      type: "trade-expired",
      player: trade.creator,
      recipients: [
        ...trade.pendingPlayers,
        ...trade.rejectedPlayers,
      ].filter((player, index, all) => all.indexOf(player) === index),
      give: trade.give,
      receive: trade.receive,
    });
  }

  return events;
};
