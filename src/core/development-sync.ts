import type { BoardSnapshot, DevelopmentCardVector, KnownDevelopmentCard } from "./placement";
import type { TrackerState } from "./types";

const CARDS: KnownDevelopmentCard[] = [
  "knight", "monopoly", "road-building", "year-of-plenty", "victory-point",
];

/** A public play must catch up in both the public tally and the exact local hand. */
export class DevelopmentSnapshotSync {
  private baseline?: {
    game?: string;
    player: string;
    turn?: number;
    held: DevelopmentCardVector;
    bought: DevelopmentCardVector;
    played: Partial<DevelopmentCardVector>;
  };

  isWaiting(state: TrackerState, board: BoardSnapshot): boolean {
    const player = board.myPlayer;
    const own = board.ownDevelopmentCards;
    if (!player || !own) return false;
    if (this.baseline?.game !== board.gameKey || this.baseline?.player !== player) {
      this.baseline = undefined;
    }
    const played: Partial<DevelopmentCardVector> = board.players?.[player]?.playedDevelopmentCards ?? {};
    for (const card of CARDS) {
      const logged = state.players[player]?.playedDevCards[card] ?? 0;
      if (played[card] !== undefined && logged > played[card]!) return true;
      if (this.baseline && played[card] !== undefined) {
        const consumed = Math.max(0, played[card]! - (this.baseline.played[card] ?? 0));
        if (consumed > 0 && board.turn === this.baseline.turn && !own.hasPlayedThisTurn) return true;
        const purchased = Math.max(0, own.boughtThisTurn[card] - this.baseline.bought[card]);
        if (consumed > 0 && board.turn === this.baseline.turn &&
          own.cards[card] > this.baseline.held[card] - consumed + purchased) {
          return true;
        }
      }
    }
    this.baseline = {
      game: board.gameKey, player, turn: board.turn,
      held: { ...own.cards }, bought: { ...own.boughtThisTurn }, played: { ...played },
    };
    return false;
  }

  reset(): void { this.baseline = undefined; }
}

/** Recognize a confirmed, unfinished Road Building transaction, never a paid road prompt. */
export const hasPendingFreeRoad = (state: TrackerState, board: BoardSnapshot): boolean => {
  if (board.initialPlacement || board.action !== "road" || !board.isMyTurn ||
    !board.myPlayer || !board.legalEdgeIds?.length) return false;
  const localAction = board.localSeatDiagnostics?.localActionState;
  // Only the bridge's dedicated 30/31 road prompts may receive free credit;
  // corroborate with the public card play below. Ordinary paid road prompts
  // (3/4/5) and unfamiliar states must never inherit this credit.
  if (localAction !== undefined && localAction !== 30 && localAction !== 31) return false;
  const events = state.recentEvents;
  let play = events.length - 1;
  for (; play >= 0; play--) {
    const candidate = events[play];
    if (candidate?.type === "play-dev" && candidate.player === board.myPlayer) break;
  }
  const event = events[play];
  if (event?.type !== "play-dev" || event.card !== "road-building") return false;
  // A later opponent roll proves that this play belonged to an earlier turn.
  if (events.slice(play + 1).some((event) =>
    event.type === "roll" && event.player !== board.myPlayer)) return false;
  if (events.slice(play + 1).filter((event) =>
    event.type === "spend" && event.player === board.myPlayer && event.reason === "road").length >= 2) return false;
  return true;
};
