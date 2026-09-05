import { parseBankShortageNotice, parseLogSnapshot } from "../core/parser";
import { createTrackerState, reduceTracker, replayEvents } from "../core/tracker";
import type { StoredEvent, TrackerEvent, TrackerState } from "../core/types";
import {
  ACTIVE_SESSION_STORAGE_KEY,
  clearCurrentGameStorage,
  LATEST_SUMMARY_STORAGE_KEY,
  pruneHistoricalSessionStorage,
  sessionStorageKey,
} from "../core/local-data";
import {
  detectLanguage,
  findMessageElements,
  hashString,
  snapshotMessage,
  stableMessageId,
} from "./dom";
import { isExtensionContextInvalidatedError } from "./extension-context";
import {
  appendPublicDiceRoll,
  cloneDiceHistoryState,
  createDiceHistoryState,
  DICE_HISTORY_INTEGRITY_VERSION,
  noteMissingPublicRoll,
  notePublicRollConflict,
  reconcilePublicDiceSources,
  noteRollCapableLogAmbiguity,
  observeLogCoverage,
  observeDiceSetupBoundary,
  restoreDiceHistoryState,
  serializeDiceHistoryState,
  type DiceHistoryState,
  type StoredDiceHistoryState,
} from "../core/dice-history";

export type UnmatchedLogReason =
  | "known-bank-shortage-notice"
  | "known-ignored-system-message"
  | "known-ignored-friendly-robber-status"
  | "known-redundant-trade-offer"
  | "known-redundant-robber-move"
  | "known-redundant-award"
  | "known-ignored-production-blocked"
  | "known-ignored-empty-robbery"
  | "known-ignored-bot-status"
  | "conflicting-log-index"
  | "unrecognized-log-format";

export interface UnmatchedLogSample {
  signature: string;
  count: number;
  firstSeenAt: number;
  lastSeenAt: number;
  firstLogIndex?: number;
  lastLogIndex?: number;
  reason: UnmatchedLogReason;
  affectsIntegrity: boolean;
  sample: string;
}

interface StoredSessionV3 {
  schema: 3;
  id: string;
  page: string;
  gameKey?: string;
  startedAt: number;
  updatedAt: number;
  events: StoredEvent[];
  seenIds: string[];
  partialHistory: boolean;
  unmatchedCount: number;
  unmatchedIntegrityCount?: number;
  unmatchedSamples?: UnmatchedLogSample[];
}

interface StoredSession extends Omit<StoredSessionV3, "schema"> {
  schema: 4;
  diceHistory: StoredDiceHistoryState;
}

type RestorableSession = StoredSessionV3 | StoredSession;

export interface SessionSummary {
  active: boolean;
  sessionId: string;
  playerCount: number;
  eventCount: number;
  possibilities: number;
  partialHistory: boolean;
  updatedAt: number;
}

const MAX_STORED_EVENTS = 1600;

const classifyUnmatchedLog = (
  snapshot: Parameters<typeof parseLogSnapshot>[0],
): { reason: UnmatchedLogReason; affectsIntegrity: boolean } => {
  if (parseBankShortageNotice(snapshot)) {
    // Bank-shortage warnings are real game evidence, but they are not a
    // resource-transfer event by themselves. The following production log
    // entries carry the actual cards received, so keep this as a recognized,
    // non-mutating notice rather than corrupting hidden-hand tracking.
    return { reason: "known-bank-shortage-notice", affectsIntegrity: false };
  }
  const normalized = snapshot.serialText.replace(/\s+/gu, " ").trim();
  if (/^happy settling!|\blist of commands:\s*\/help\b/iu.test(normalized)) {
    return { reason: "known-ignored-system-message", affectsIntegrity: false };
  }
  if (/^bot is (?:selecting cards to discard|placing (?:a |an )?(?:road|settlement)) for\b/iu.test(normalized) && !/:die-[1-6]:/u.test(normalized)) {
    return { reason: "known-ignored-bot-status", affectsIntegrity: false };
  }
  if (
    /\bfriendly robber is active,\s*(?:tiles available to block are limited|no available tiles to block)\b/iu.test(
      normalized,
    )
  ) {
    // Colonist emits this informational status beside robber actions. The
    // authoritative setting is captured from game state, so the rendered log
    // line is recognized but must not create a second state transition.
    return {
      reason: "known-ignored-friendly-robber-status",
      affectsIntegrity: false,
    };
  }
  if (/^player has no cards\.?$/iu.test(normalized)) {
    return { reason: "known-ignored-empty-robbery", affectsIntegrity: false };
  }
  if (/is blocked by the robber.*no resources produced/iu.test(normalized)) {
    return { reason: "known-ignored-production-blocked", affectsIntegrity: false };
  }
  if (/\bmoved robber to\b/iu.test(normalized)) {
    return { reason: "known-redundant-robber-move", affectsIntegrity: false };
  }
  if (/\b(?:received|lost)\s+(?:the\s+)?(?:longest road|largest army)\b/iu.test(normalized)) {
    // Award ownership and VP totals come from the validated board snapshot.
    // The rendered announcement is useful audit text, not an extra state transition.
    return { reason: "known-redundant-award", affectsIntegrity: false };
  }
  if (
    /\bwants to give\b.+\bfor\b/iu.test(normalized) ||
    /\bproposed counter offer to\b.+\boffering\b.+\bfor\b/iu.test(normalized)
  ) {
    // Active-trade snapshots are ingested separately with stable trade
    // identity, so the rendered chat offer/counter is duplicate evidence.
    return { reason: "known-redundant-trade-offer", affectsIntegrity: false };
  }
  return { reason: "unrecognized-log-format", affectsIntegrity: true };
};

const unmatchedCanConcealGameplayRoll = (
  classification: ReturnType<typeof classifyUnmatchedLog>,
): boolean => classification.reason === "unrecognized-log-format";

const validStoredLogIndex = (value: number | undefined): value is number =>
  value !== undefined && Number.isInteger(value) && value >= 0;

const legacyDiceAmbiguityEvidence = (
  stored: RestorableSession,
): { ambiguousLogIndices: number[]; hasUnlocatedRollAmbiguity: boolean } => {
  const ambiguous = new Set<number>();
  let retainedIntegrityCount = 0;
  let retainedUnmatchedCount = 0;
  let hasUnlocatedRollAmbiguity = false;

  for (const sample of stored.unmatchedSamples ?? []) {
    const count =
      Number.isInteger(sample.count) && sample.count > 0 ? sample.count : 1;
    retainedUnmatchedCount += count;
    if (sample.reason !== "unrecognized-log-format") continue;
    retainedIntegrityCount += count;

    const first = validStoredLogIndex(sample.firstLogIndex)
      ? sample.firstLogIndex
      : undefined;
    const last = validStoredLogIndex(sample.lastLogIndex)
      ? sample.lastLogIndex
      : undefined;
    if (first !== undefined) ambiguous.add(first);
    if (last !== undefined) ambiguous.add(last);

    const fullyLocated =
      count === 1
        ? first !== undefined && last !== undefined && first === last
        : count === 2 && first !== undefined && last !== undefined;
    if (!fullyLocated) hasUnlocatedRollAmbiguity = true;
  }

  const storedIntegrityCount =
    stored.unmatchedIntegrityCount !== undefined &&
    Number.isInteger(stored.unmatchedIntegrityCount) &&
    stored.unmatchedIntegrityCount > 0
      ? stored.unmatchedIntegrityCount
      : 0;
  if (
    storedIntegrityCount > retainedIntegrityCount ||
    (stored.unmatchedIntegrityCount === undefined &&
      stored.unmatchedCount > retainedUnmatchedCount)
  ) {
    // Old writers may retain only the unmatched total. Missing diagnostics
    // cannot certify that the unaccounted entries were harmless.
    hasUnlocatedRollAmbiguity = true;
  }

  return {
    ambiguousLogIndices: [...ambiguous].sort((left, right) => left - right),
    hasUnlocatedRollAmbiguity,
  };
};

const reconcileRetainedDiceRolls = (
  history: DiceHistoryState,
  events: readonly StoredEvent[],
): DiceHistoryState => {
  // Audit all retained presentations against each other and the independent
  // ledger. Generic events may be truncated, so never rebuild that ledger here.
  const audit = cloneDiceHistoryState(history);
  const conflicts = new Set<number>();
  for (const event of events) {
    if (event.type !== "roll" || !event.dice || !validStoredLogIndex(event.index)) continue;
    try {
      appendPublicDiceRoll(audit, {
        actor: event.player,
        total: event.dice[0] + event.dice[1],
        dice: [...event.dice] as [number, number],
        eventId: event.id,
        logIndex: event.index,
      });
    } catch (error) {
      if (!(error instanceof Error) ||
          error.message !== `Conflicting public dice evidence for log index ${event.index}`) throw error;
      conflicts.add(event.index);
    }
  }
  if (!conflicts.size) return history;
  return restoreDiceHistoryState({
    ...serializeDiceHistoryState(history),
    ambiguousLogIndices: [...history.ambiguousLogIndices, ...conflicts],
    // Without a ledger observation the conflicting roll has no established
    // stochastic ordinal. A subsequent presentation cannot repair that gap.
    hasUnlocatedRollAmbiguity: history.hasUnlocatedRollAmbiguity ||
      [...conflicts].some((index) => !history.rolls.some((roll) => roll.logIndex === index)),
  });
};

const restoreSchema4DiceHistory = (stored: StoredSession): DiceHistoryState => {
  if (stored.diceHistory.integrityVersion === DICE_HISTORY_INTEGRITY_VERSION) {
    // Version 1 records can already contain an incorrectly certified legacy
    // conflict. Always reconcile retained dice evidence, including this version.
    return reconcileRetainedDiceRolls(restoreDiceHistoryState(stored.diceHistory), stored.events);
  }
  // Earlier repairs could re-save unsafe legacy history with both ambiguity
  // fields present. Field presence is not proof of conservative restoration.
  const legacy = legacyDiceAmbiguityEvidence(stored);
  return reconcileRetainedDiceRolls(restoreDiceHistoryState({
    ...stored.diceHistory,
    ambiguousLogIndices: [
      ...(stored.diceHistory.ambiguousLogIndices ?? []),
      ...legacy.ambiguousLogIndices,
    ],
    hasUnlocatedRollAmbiguity:
      stored.diceHistory.hasUnlocatedRollAmbiguity === true ||
      legacy.hasUnlocatedRollAmbiguity ||
      stored.partialHistory,
  }), stored.events);
};

const MAX_SEEN_IDS = 2600;
const MAX_UNMATCHED_SAMPLES = 24;
const MAX_UNMATCHED_SAMPLE_CHARS = 220;

let storageOperations: Promise<void> = Promise.resolve();

const enqueueStorage = (operation: () => Promise<void>): Promise<void> => {
  const next = storageOperations.then(operation, operation);
  storageOperations = next.catch(() => undefined);
  return next;
};

const pageIdentity = (): string => `${location.origin}${location.pathname}${location.search}`;

const canonicalPlayer = (
  player: string,
  myPlayer?: string,
): string => (myPlayer && player === "You" ? myPlayer : player);

export const canonicalizeEvent = (
  event: StoredEvent,
  myPlayer?: string,
): StoredEvent => {
  if (!myPlayer || myPlayer === "You") return event;
  if (event.type === "transfer" || event.type === "unknown-transfer") {
    return {
      ...event,
      from: canonicalPlayer(event.from, myPlayer),
      to: canonicalPlayer(event.to, myPlayer),
    };
  }
  if (event.type === "trade") {
    return {
      ...event,
      player: canonicalPlayer(event.player, myPlayer),
      ...(event.acceptingPlayer
        ? {
            acceptingPlayer: canonicalPlayer(
              event.acceptingPlayer,
              myPlayer,
            ),
          }
        : {}),
    };
  }
  if (event.type === "trade-offered") {
    return {
      ...event,
      player: canonicalPlayer(event.player, myPlayer),
      recipients: event.recipients.map((player) =>
        canonicalPlayer(player, myPlayer),
      ),
    };
  }
  if (
    event.type === "trade-accepted" ||
    event.type === "trade-rejected" ||
    event.type === "trade-countered" ||
    event.type === "trade-embargoed" ||
    event.type === "trade-embargo-cleared"
  ) {
    return {
      ...event,
      player: canonicalPlayer(event.player, myPlayer),
      creator: canonicalPlayer(event.creator, myPlayer),
    };
  }
  if (event.type === "trade-expired") {
    return {
      ...event,
      player: canonicalPlayer(event.player, myPlayer),
      ...(event.recipients
        ? {
            recipients: event.recipients.map((player) =>
              canonicalPlayer(player, myPlayer),
            ),
          }
        : {}),
    };
  }
  return {
    ...event,
    player: canonicalPlayer(event.player, myPlayer),
  };
};

type IndexedEventRelation = "same" | "enrich" | "conflict";

const storedEventSemantics = (event: StoredEvent): Record<string, unknown> => {
  const semantics = { ...(event as unknown as Record<string, unknown>) };
  for (const key of ["id", "index", "logWatermark", "timestamp", "raw"]) {
    delete semantics[key];
  }
  return semantics;
};

const compatibleEventColor = (
  left: unknown,
  right: unknown,
): boolean =>
  typeof left !== "string" || typeof right !== "string" || left === right;

const indexedEventRelation = (
  existing: StoredEvent,
  incoming: StoredEvent,
): IndexedEventRelation => {
  const left = storedEventSemantics(existing);
  const right = storedEventSemantics(incoming);
  const leftColor = left.color;
  const rightColor = right.color;
  delete left.color;
  delete right.color;

  if (JSON.stringify(left) === JSON.stringify(right)) {
    if (!compatibleEventColor(leftColor, rightColor)) return "conflict";
    return leftColor === undefined && rightColor !== undefined ? "enrich" : "same";
  }

  if (existing.type === "roll" && incoming.type === "roll") {
    const leftDice = left.dice;
    const rightDice = right.dice;
    delete left.dice;
    delete right.dice;
    if (
      JSON.stringify(left) === JSON.stringify(right) &&
      compatibleEventColor(leftColor, rightColor)
    ) {
      if (leftDice === undefined && rightDice !== undefined) return "enrich";
      if (leftDice !== undefined && rightDice === undefined) return "same";
      if (JSON.stringify(leftDice) === JSON.stringify(rightDice)) {
        return leftColor === undefined && rightColor !== undefined ? "enrich" : "same";
      }
    }
  }
  return "conflict";
};

const enrichIndexedEvent = (
  existing: StoredEvent,
  incoming: StoredEvent,
): StoredEvent => {
  const existingColor = (existing as TrackerEvent & { color?: string }).color;
  const incomingColor = (incoming as TrackerEvent & { color?: string }).color;
  const enriched = {
    ...existing,
    ...(existingColor === undefined && incomingColor !== undefined
      ? { color: incomingColor }
      : {}),
    raw: incoming.raw,
  } as StoredEvent;
  if (
    existing.type === "roll" &&
    incoming.type === "roll" &&
    existing.dice === undefined &&
    incoming.dice !== undefined
  ) {
    return { ...enriched, dice: [...incoming.dice] as [number, number] } as StoredEvent;
  }
  return enriched;
};

const validLogWatermark = (value: number | undefined): value is number =>
  value !== undefined && Number.isInteger(value) && value >= -1;

const normalizeEventJournal = (
  source: readonly StoredEvent[],
): {
  events: StoredEvent[];
  conflictingLogIndices: number[];
  maxLogIndex: number;
} => {
  const entries: Array<{ event: StoredEvent; ordinal: number }> = [];
  const indexedEntry = new Map<number, number>();
  const conflicts = new Set<number>();
  let maxLogIndex = -1;

  source.forEach((original, ordinal) => {
    if (validStoredLogIndex(original.index)) {
      maxLogIndex = Math.max(maxLogIndex, original.index);
      const priorEntry = indexedEntry.get(original.index);
      if (priorEntry !== undefined) {
        const prior = entries[priorEntry]!;
        const relation = indexedEventRelation(prior.event, original);
        if (relation === "enrich") prior.event = enrichIndexedEvent(prior.event, original);
        if (relation === "conflict") conflicts.add(original.index);
        return;
      }
      indexedEntry.set(original.index, entries.length);
      entries.push({ event: original, ordinal });
      return;
    }

    const event = validLogWatermark(original.logWatermark)
      ? original
      : { ...original, logWatermark: maxLogIndex };
    entries.push({ event, ordinal });
  });

  entries.sort((left, right) => {
    const leftAnchor = left.event.index ?? left.event.logWatermark ?? -1;
    const rightAnchor = right.event.index ?? right.event.logWatermark ?? -1;
    if (leftAnchor !== rightAnchor) return leftAnchor - rightAnchor;
    const leftPhase = left.event.index === undefined ? 1 : 0;
    const rightPhase = right.event.index === undefined ? 1 : 0;
    return leftPhase - rightPhase || left.ordinal - right.ordinal;
  });

  return {
    events: entries.map(({ event }) => event),
    conflictingLogIndices: [...conflicts].sort((left, right) => left - right),
    maxLogIndex,
  };
};

const deriveId = (): string => {
  const pathGameId = location.pathname.match(/\/game\/([^/?#]+)/)?.[1];
  const queryGameId = new URLSearchParams(location.search).get("gameId");
  return pathGameId || queryGameId || hashString(pageIdentity());
};

export class GameSession {
  readonly id: string;
  state: TrackerState = createTrackerState();
  events: StoredEvent[] = [];
  partialHistory = false;
  diceHistory: DiceHistoryState = createDiceHistoryState();
  unmatchedCount = 0;
  unmatchedIntegrityCount = 0;
  unmatchedSamples: UnmatchedLogSample[] = [];
  startedAt = Date.now();
  gameKey?: string;

  private readonly root: HTMLElement;
  private readonly seenIds = new Set<string>();
  /**
   * Colonist virtualizes its log and can reuse one DOM element for several
   * messages. Remember the last logical identity observed on each element
   * rather than treating the element itself as permanently consumed.
   */
  private readonly seenElements = new WeakMap<Element, string>();
  private observer?: MutationObserver;
  private saveTimer?: number;
  private syntheticSequence = 0;
  private maxObservedLogIndex = -1;
  private disposed = false;
  private myPlayer?: string;
  private initialPlacement = false;
  private storageGeneration = 0;
  private storageSuppressed = false;
  private pruneSessionHistory = true;

  constructor(
    root: HTMLElement,
    private readonly onUpdate: (session: GameSession) => void,
    gameKey?: string,
  ) {
    this.root = root;
    this.id = deriveId();
    this.gameKey = gameKey;
  }

  async start(): Promise<void> {
    await this.restore();
    this.scan();
    this.observer = new MutationObserver(() => this.scan());
    this.observer.observe(this.root, {
      childList: true,
      subtree: true,
      characterData: true,
      attributes: true,
      // Keep this contract synchronized with snapshotMessage()/serializeNode().
      // Colonist hydrates virtualized rows in place, so semantic text/icon
      // changes must trigger a rescan even when no child node is replaced.
      attributeFilter: [
        "data-index",
        "src",
        "alt",
        "title",
        "aria-label",
        "data-tooltip-content",
        "class",
        "style",
        "href",
      ],
    });
    // A restored session can have no unseen log entries. Still claim it as the
    // current game and prune records retained by older extension versions.
    this.queueSave();
    this.onUpdate(this);
  }

  stop(): void {
    this.disposed = true;
    this.observer?.disconnect();
    if (this.saveTimer) window.clearTimeout(this.saveTimer);
    void this.save();
  }

  /**
   * Ingest durable tracker events that do not originate from the chat log
   * (for example active-trade panel diffs). Deduplicates by synthetic id.
   */
  ingestEvents(events: TrackerEvent[], source = "board"): boolean {
    if (this.disposed || !events.length) return false;
    let changed = false;
    for (const [index, event] of events.entries()) {
      const id = `${source}:${event.type}:${this.syntheticSequence++}:${index}:${JSON.stringify(event)}`;
      if (this.seenIds.has(id)) continue;
      this.seenIds.add(id);
      const stored = canonicalizeEvent(
        {
          ...event,
          id,
          logWatermark: this.maxObservedLogIndex,
          timestamp: Date.now(),
          raw: source,
        } as StoredEvent,
        this.myPlayer,
      );
      this.events.push(stored);
      this.state = reduceTracker(this.state, stored, stored);
      changed = true;
    }
    if (changed) {
      if (this.events.length > MAX_STORED_EVENTS) {
        this.events = this.events.slice(-MAX_STORED_EVENTS);
        this.state = replayEvents(this.events);
        this.partialHistory = true;
      }
      this.queueSave();
      this.onUpdate(this);
    }
    return changed;
  }

  reset(rescan = true): void {
    this.storageGeneration += 1;
    this.storageSuppressed = false;
    this.pruneSessionHistory = true;
    this.state = createTrackerState();
    this.events = [];
    this.partialHistory = false;
    this.diceHistory = createDiceHistoryState();
    this.unmatchedCount = 0;
    this.unmatchedIntegrityCount = 0;
    this.unmatchedSamples = [];
    this.seenIds.clear();
    this.syntheticSequence = 0;
    this.maxObservedLogIndex = -1;
    if (rescan) this.scan(true);
    this.queueSave();
    this.onUpdate(this);
  }

  /**
   * User-requested deletion differs from a game-key rollover: preserve only
   * in-memory message identities so the still-mounted historical log cannot
   * be immediately re-ingested, suppress persistence until a genuinely new
   * message arrives, and remove all persisted game records.
   */
  async clearStoredData(): Promise<void> {
    if (this.saveTimer) {
      window.clearTimeout(this.saveTimer);
      this.saveTimer = undefined;
    }
    this.storageGeneration += 1;
    this.storageSuppressed = true;
    this.pruneSessionHistory = true;
    this.state = createTrackerState();
    this.events = [];
    this.partialHistory = false;
    this.diceHistory = createDiceHistoryState();
    this.unmatchedCount = 0;
    this.unmatchedIntegrityCount = 0;
    this.unmatchedSamples = [];
    this.syntheticSequence = 0;
    this.maxObservedLogIndex = -1;
    try {
      await enqueueStorage(clearCurrentGameStorage);
    } catch (error) {
      if (!isExtensionContextInvalidatedError(error)) throw error;
    }
  }

  setGameKey(gameKey: string): void {
    if (!gameKey || gameKey === this.gameKey) return;
    const hadGameKey = Boolean(this.gameKey);
    this.gameKey = gameKey;
    if (hadGameKey) {
      // Local identity belongs to the game generation just ended. Require the
      // bridge to resolve it again before canonicalizing any new "You" logs.
      this.myPlayer = undefined;
      this.initialPlacement = false;
      // A new game can reuse the same mounted log/session object. Start its
      // record clock before reset() synchronously publishes the empty state.
      this.startedAt = Date.now();
      // Do not immediately re-ingest the previous game's still-mounted log.
      // Colonist can publish the new game identity a frame before replacing it.
      this.reset(false);
      return;
    }
    this.queueSave();
  }

  setInitialPlacement(active: boolean, gameKey?: string): void {
    this.initialPlacement = Boolean(active && gameKey && gameKey === this.gameKey);
    if (this.observer && this.initialPlacement && observeDiceSetupBoundary(this.diceHistory)) {
      this.queueSave();
      this.onUpdate(this);
    }
  }

  /**
   * Bot-only Colonist games can omit the rendered game-log virtualizer entirely.
   * In that mode the page bridge remains public authority for the active turn
   * and rolled total, so retain one roll per completed-turn ordinal. Skipped
   * turn ordinals become explicit known-count gaps and stay fail-closed.
   */
  observeBoardDiceSnapshot(snapshot: {
    gameKey?: string;
    botOnlyGame?: boolean;
    initialPlacement?: boolean;
    hasRolled?: boolean;
    lastRoll?: number;
    currentPlayer?: string;
    turn?: number;
  } | undefined): boolean {
    if (
      this.disposed ||
      !snapshot?.botOnlyGame ||
      !snapshot.gameKey ||
      snapshot.gameKey !== this.gameKey
    ) {
      return false;
    }
    if (snapshot.initialPlacement) {
      return observeDiceSetupBoundary(this.diceHistory);
    }
    if (
      !snapshot.hasRolled ||
      !Number.isInteger(snapshot.lastRoll) ||
      snapshot.lastRoll! < 2 ||
      snapshot.lastRoll! > 12 ||
      !snapshot.currentPlayer?.trim() ||
      !Number.isInteger(snapshot.turn) ||
      snapshot.turn! < 0
    ) {
      return false;
    }
    const turn = snapshot.turn!;
    const actor = snapshot.currentPlayer.trim();
    const eventId = `board-roll:${turn}:${actor}`;
    // A log observed earlier in the same game stays authoritative by ordinal.
    // This only avoids double-counting the current roll when Colonist unmounts
    // the log virtualizer midgame; any contradiction stays fail-closed.
    // (A complete log-anchored history may leave missingPrefixRolls undefined,
    // so provenance completeness — not the explicit zero — is the anchor.)
    if (
      this.diceHistory.provenance === "complete-from-first-gameplay-roll" &&
      this.diceHistory.gaps.length === 0
    ) {
      const ordinalRoll = this.diceHistory.rolls[turn];
      if (ordinalRoll) {
        if (ordinalRoll.actor === actor && ordinalRoll.total === snapshot.lastRoll) return false;
        noteRollCapableLogAmbiguity(this.diceHistory, undefined);
        this.queueSave();
        this.onUpdate(this);
        return true;
      }
    }
    const existing = this.diceHistory.rolls.find((roll) =>
      roll.eventId.startsWith(`board-roll:${turn}:`));
    if (existing) {
      if (existing.actor === actor && existing.total === snapshot.lastRoll) return false;
      noteRollCapableLogAmbiguity(this.diceHistory, undefined);
      this.queueSave();
      this.onUpdate(this);
      return true;
    }
    const priorBoardTurns = this.diceHistory.rolls.flatMap((roll) => {
      const matched = roll.eventId.match(/^board-roll:(\d+):/u);
      return matched ? [Number(matched[1])] : [];
    });
    if (priorBoardTurns.some((prior) => prior > turn)) {
      // A delayed bridge snapshot cannot append a historical turn at the tail.
      notePublicRollConflict(this.diceHistory, undefined);
      this.queueSave();
      this.onUpdate(this);
      return true;
    }
    const anchored =
      this.diceHistory.provenance === "complete-from-first-gameplay-roll" ||
      this.diceHistory.missingPrefixRolls !== undefined;
    const expectedTurn = anchored
      ? (this.diceHistory.missingPrefixRolls ?? 0) + this.diceHistory.rolls.length
        + this.diceHistory.gaps.reduce((count, gap) => count + (gap.missingRolls ?? 0), 0)
      : priorBoardTurns.length
        ? Math.max(...priorBoardTurns) + 1
        : turn;
    for (let missingTurn = expectedTurn; missingTurn < turn; missingTurn += 1) {
      noteMissingPublicRoll(this.diceHistory);
    }
    appendPublicDiceRoll(this.diceHistory, {
      actor,
      total: snapshot.lastRoll!,
      eventId,
    });
    this.queueSave();
    this.onUpdate(this);
    return true;
  }

  setMyPlayer(myPlayer?: string): void {
    const normalized = myPlayer?.trim();
    if (!normalized || normalized === "You") {
      this.myPlayer = undefined;
      return;
    }
    if (normalized === this.myPlayer) return;
    this.myPlayer = normalized;
    const canonical = this.events.map((event) =>
      canonicalizeEvent(event, normalized),
    );
    const canonicalDice = this.diceHistory.rolls.map((roll) =>
      roll.actor === "You" ? { ...roll, actor: normalized } : roll,
    );
    const changed =
      canonical.some((event, index) => event !== this.events[index]) ||
      canonicalDice.some((roll, index) => roll !== this.diceHistory.rolls[index]);
    if (!changed) return;
    this.events = canonical;
    this.diceHistory.rolls = canonicalDice;
    this.state = replayEvents(this.events);
    this.queueSave();
    this.onUpdate(this);
  }

  private scan(force = false): void {
    if (this.disposed) return;
    const priorMaxLogIndex = this.maxObservedLogIndex;
    const language = detectLanguage();
    const elements = findMessageElements(this.root);
    const occurrence = new Map<string, number>();
    const candidates: Array<{
      element: Element;
      id: string;
      index: number;
      logIndex?: number;
    }> = [];

    for (const element of elements) {
      const snapshot = snapshotMessage(element, language);
      if (!snapshot) continue;
      if (validStoredLogIndex(snapshot.index)) {
        this.maxObservedLogIndex = Math.max(this.maxObservedLogIndex, snapshot.index);
      }
      let id = stableMessageId(snapshot);
      if (snapshot.index === undefined) {
        const base = hashString(`${snapshot.serialText}|${snapshot.visibleText}`);
        const ordinal = occurrence.get(base) ?? 0;
        occurrence.set(base, ordinal + 1);
        id = `message:${base}:${ordinal}`;
      }
      if (!force && this.seenElements.get(element) === id) continue;
      candidates.push({
        element,
        id,
        index: snapshot.index ?? this.syntheticSequence++,
        ...(snapshot.index !== undefined ? { logIndex: snapshot.index } : {}),
      });
    }

    candidates.sort((left, right) => left.index - right.index);
    // Presentation changes, including index zero, are not game identity.
    // setGameKey() and explicit reset own history replacement; otherwise a
    // rerender could erase both accepted rolls and unresolved conflicts.
    if (
      !this.events.length &&
      candidates.length &&
      candidates[0]!.index > 0 &&
      candidates.some((candidate) => candidate.element.hasAttribute("data-index"))
    ) {
      this.partialHistory = true;
    }

    let changed = false;
    let journalReplayRequired = false;
    for (const candidate of candidates) {
      this.seenElements.set(candidate.element, candidate.id);
      if (this.seenIds.has(candidate.id)) continue;
      this.seenIds.add(candidate.id);
      const snapshot = snapshotMessage(candidate.element, language);
      if (!snapshot) continue;
      const parsed = parseLogSnapshot(snapshot);
      if (!parsed) {
        const classification = classifyUnmatchedLog(snapshot);
        this.unmatchedCount += 1;
        if (classification.affectsIntegrity) this.unmatchedIntegrityCount += 1;
        if (unmatchedCanConcealGameplayRoll(classification)) {
          noteRollCapableLogAmbiguity(this.diceHistory, candidate.logIndex);
        } else if (candidate.logIndex !== undefined) {
          observeLogCoverage(this.diceHistory, [candidate.logIndex]);
        }
        this.recordUnmatched(snapshot.serialText, snapshot.index, classification);
        changed = true;
        continue;
      }
      if (candidate.logIndex !== undefined) {
        observeLogCoverage(this.diceHistory, [candidate.logIndex]);
      }
      const stored = canonicalizeEvent({
        ...parsed.event,
        id: candidate.id,
        ...(snapshot.index !== undefined
          ? { index: snapshot.index }
          : { logWatermark: this.maxObservedLogIndex }),
        timestamp: Date.now(),
        raw: snapshot.serialText,
      } as StoredEvent, this.myPlayer);
      const existingPosition =
        stored.index === undefined
          ? -1
          : this.events.findIndex((event) => event.index === stored.index);
      const existing =
        existingPosition >= 0 ? this.events[existingPosition] : undefined;
      const relation = existing
        ? indexedEventRelation(existing, stored)
        : undefined;
      if (existing && relation === "conflict") {
        // One server/log identity cannot own two semantic events. Retain the
        // first event for generic tracking and fail stochastic authority closed.
        notePublicRollConflict(this.diceHistory, stored.index);
        this.partialHistory = true;
        this.unmatchedCount += 1;
        this.unmatchedIntegrityCount += 1;
        this.recordUnmatched(
          `${existing.raw} <> ${stored.raw}`,
          stored.index,
          { reason: "conflicting-log-index", affectsIntegrity: true },
        );
        changed = true;
        continue;
      }
      if (stored.type === "roll") {
        try {
          if (stored.dice) {
            appendPublicDiceRoll(this.diceHistory, {
              actor: stored.player,
              total: stored.dice[0] + stored.dice[1],
              dice: [...stored.dice] as [number, number],
              eventId: stored.id,
              ...(stored.index !== undefined ? { logIndex: stored.index } : {}),
            });
          } else if (stored.index !== undefined) {
            // A rendered roll row can hydrate its dice icons later. Keep exact
            // index uncertainty so that a later semantic rerender can resolve it
            // instead of permanently inventing a missing gameplay ordinal.
            noteRollCapableLogAmbiguity(this.diceHistory, stored.index);
          } else {
            noteMissingPublicRoll(this.diceHistory);
          }
        } catch (error) {
          const isIndexedDiceConflict =
            stored.index !== undefined &&
            this.diceHistory.ambiguousLogIndices.includes(stored.index) &&
            error instanceof Error &&
            error.message ===
              `Conflicting public dice evidence for log index ${stored.index}`;
          if (!isIndexedDiceConflict) throw error;
          // The dice history now carries a sticky exact-index ambiguity. Do not
          // admit the contradictory rerender into generic tracker history, but
          // do persist/publish the stochastic authority downgrade.
          changed = true;
          continue;
        }
      }
      if (existing && relation === "same") continue;
      if (existing && relation === "enrich") {
        this.events[existingPosition] = enrichIndexedEvent(existing, stored);
        journalReplayRequired = true;
        changed = true;
        continue;
      }
      this.events.push(stored);
      if (stored.index !== undefined && stored.index <= priorMaxLogIndex) {
        journalReplayRequired = true;
      }
      if (!journalReplayRequired) {
        this.state = reduceTracker(this.state, stored, stored);
      }
      changed = true;
    }

    if (journalReplayRequired) {
      const journal = normalizeEventJournal(this.events);
      this.events = journal.events;
      for (const index of journal.conflictingLogIndices) {
        notePublicRollConflict(this.diceHistory, index);
      }
      if (journal.conflictingLogIndices.length) this.partialHistory = true;
      this.state = replayEvents(this.events);
    }

    reconcilePublicDiceSources(this.diceHistory);
    if (this.initialPlacement && observeDiceSetupBoundary(this.diceHistory)) changed = true;
    if (changed) {
      if (this.storageSuppressed) {
        this.storageSuppressed = false;
        this.storageGeneration += 1;
      }
      if (this.events.length > MAX_STORED_EVENTS) {
        this.events = this.events.slice(-MAX_STORED_EVENTS);
        this.state = replayEvents(this.events);
        this.partialHistory = true;
      }
      this.queueSave();
      this.onUpdate(this);
    }
  }

  private recordUnmatched(
    serialText: string,
    logIndex: number | undefined,
    classification: { reason: UnmatchedLogReason; affectsIntegrity: boolean },
  ): void {
    const normalized = serialText.replace(/\s+/gu, " ").trim();
    const signature = hashString(normalized);
    const now = Date.now();
    const existing = this.unmatchedSamples.find(
      (sample) => sample.signature === signature,
    );
    if (existing) {
      existing.count += 1;
      existing.lastSeenAt = now;
      if (logIndex !== undefined) existing.lastLogIndex = logIndex;
      return;
    }
    if (this.unmatchedSamples.length >= MAX_UNMATCHED_SAMPLES) {
      let replacement = 0;
      for (let index = 1; index < this.unmatchedSamples.length; index += 1) {
        const candidate = this.unmatchedSamples[index]!;
        const current = this.unmatchedSamples[replacement]!;
        if (
          candidate.count < current.count ||
          (candidate.count === current.count && candidate.lastSeenAt < current.lastSeenAt)
        ) {
          replacement = index;
        }
      }
      this.unmatchedSamples.splice(replacement, 1);
    }
    this.unmatchedSamples.push({
      signature,
      count: 1,
      firstSeenAt: now,
      lastSeenAt: now,
      ...(logIndex !== undefined
        ? { firstLogIndex: logIndex, lastLogIndex: logIndex }
        : {}),
      reason: classification.reason,
      affectsIntegrity: classification.affectsIntegrity,
      sample: normalized.slice(0, MAX_UNMATCHED_SAMPLE_CHARS),
    });
  }

  private migrateLegacyDiceHistory(stored: StoredSessionV3): DiceHistoryState {
    const history = createDiceHistoryState();
    observeLogCoverage(
      history,
      stored.events.flatMap((event) =>
        event.index === undefined ? [] : [event.index],
      ),
    );
    for (const event of stored.events) {
      if (event.type !== "roll") continue;
      if (event.dice) {
        appendPublicDiceRoll(history, {
          actor: event.player,
          total: event.dice[0] + event.dice[1],
          dice: [...event.dice] as [number, number],
          eventId: event.id,
          ...(event.index !== undefined ? { logIndex: event.index } : {}),
        });
      } else {
        noteMissingPublicRoll(history);
      }
    }
    const legacy = legacyDiceAmbiguityEvidence(stored);
    return restoreDiceHistoryState({
      ...serializeDiceHistoryState(history),
      ambiguousLogIndices: legacy.ambiguousLogIndices,
      // Persist the missing evidence, not just a derived provenance label that
      // the next coverage update or schema-4 restore would overwrite.
      hasUnlocatedRollAmbiguity:
        legacy.hasUnlocatedRollAmbiguity || stored.partialHistory,
    });
  }

  private async restore(): Promise<void> {
    const key = sessionStorageKey(this.id);
    let result: Record<string, unknown>;
    try {
      result = await chrome.storage.local.get(key);
    } catch (error) {
      if (isExtensionContextInvalidatedError(error)) return;
      throw error;
    }
    const stored = result[key] as RestorableSession | undefined;
    if (
      !stored ||
      (stored.schema !== 3 && stored.schema !== 4) ||
      stored.page !== pageIdentity()
    ) {
      return;
    }
    if (this.gameKey && stored.gameKey && this.gameKey !== stored.gameKey) return;
    this.gameKey ??= stored.gameKey;
    this.startedAt = stored.startedAt;
    const journal = normalizeEventJournal(stored.events);
    const normalizedStored = { ...stored, events: journal.events } as RestorableSession;
    this.events = journal.events;
    this.partialHistory = stored.partialHistory || journal.conflictingLogIndices.length > 0;
    this.diceHistory =
      normalizedStored.schema === 4
        ? restoreSchema4DiceHistory(normalizedStored)
        : this.migrateLegacyDiceHistory(normalizedStored);
    for (const index of journal.conflictingLogIndices) {
      notePublicRollConflict(this.diceHistory, index);
    }
    this.unmatchedCount = stored.unmatchedCount;
    this.unmatchedIntegrityCount =
      stored.unmatchedIntegrityCount ??
      (stored.unmatchedSamples ?? []).reduce(
        (count, sample) => count + ((sample.affectsIntegrity ?? true) ? sample.count : 0),
        0,
      );
    this.unmatchedSamples = (stored.unmatchedSamples ?? [])
      .slice(-MAX_UNMATCHED_SAMPLES)
      .map((sample) => ({
        ...sample,
        affectsIntegrity: sample.affectsIntegrity ?? sample.reason === "unrecognized-log-format",
      }));
    // Reclassify retained legacy misses that the current parser now proves are
    // harmless/redundant. Exact covered indexes can clear parser ambiguity,
    // never occupied-roll conflicts.
    for (const sample of this.unmatchedSamples) {
      if (sample.reason !== "unrecognized-log-format") continue;
      const classification = classifyUnmatchedLog({
        serialText: sample.sample, visibleText: sample.sample, language: "en",
      });
      if (classification.affectsIntegrity) continue;
      if (sample.affectsIntegrity) {
        this.unmatchedIntegrityCount = Math.max(0, this.unmatchedIntegrityCount - sample.count);
      }
      Object.assign(sample, classification);
      observeLogCoverage(
        this.diceHistory,
        [sample.firstLogIndex, sample.lastLogIndex].filter(validStoredLogIndex),
      );
    }
    this.maxObservedLogIndex = Math.max(
      journal.maxLogIndex,
      ...this.diceHistory.coverage.ranges.map(([, end]) => end),
      ...this.diceHistory.ambiguousLogIndices,
      ...this.unmatchedSamples.flatMap((sample) =>
        [sample.firstLogIndex, sample.lastLogIndex].filter(validStoredLogIndex),
      ),
    );
    this.syntheticSequence = Math.max(this.syntheticSequence, this.events.length);
    this.state = replayEvents(this.events);
    for (const id of stored.seenIds) this.seenIds.add(id);
  }

  private queueSave(): void {
    if (this.storageSuppressed) return;
    if (this.saveTimer) window.clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => void this.save(), 180);
  }

  private async save(): Promise<void> {
    if (!this.id || this.storageSuppressed) return;
    const generation = this.storageGeneration;
    const shouldPruneHistory = this.pruneSessionHistory;
    const now = Date.now();
    const record: StoredSession = {
      schema: 4,
      id: this.id,
      page: pageIdentity(),
      ...(this.gameKey ? { gameKey: this.gameKey } : {}),
      startedAt: this.startedAt,
      updatedAt: now,
      events: this.events,
      seenIds: [...this.seenIds].slice(-MAX_SEEN_IDS),
      partialHistory: this.partialHistory,
      diceHistory: serializeDiceHistoryState(this.diceHistory),
      unmatchedCount: this.unmatchedCount,
      unmatchedIntegrityCount: this.unmatchedIntegrityCount,
      unmatchedSamples: this.unmatchedSamples.map((sample) => ({ ...sample })),
    };
    const summary: SessionSummary = {
      active: true,
      sessionId: this.id,
      playerCount: this.state.playerOrder.length,
      eventCount: this.state.eventCount,
      possibilities: this.state.worlds.length,
      partialHistory: this.partialHistory || Boolean(this.state.warnings.length),
      updatedAt: now,
    };
    try {
      await enqueueStorage(async () => {
        if (
          this.storageSuppressed ||
          generation !== this.storageGeneration
        ) {
          return;
        }
        await chrome.storage.local.set({
          [sessionStorageKey(this.id)]: record,
          [ACTIVE_SESSION_STORAGE_KEY]: this.id,
          [LATEST_SUMMARY_STORAGE_KEY]: summary,
        });
        if (shouldPruneHistory) {
          await pruneHistoricalSessionStorage(this.id);
          if (generation === this.storageGeneration) {
            this.pruneSessionHistory = false;
          }
        }
      });
    } catch (error) {
      if (!isExtensionContextInvalidatedError(error)) throw error;
    }
  }
}

export const latestSummaryKey = LATEST_SUMMARY_STORAGE_KEY;
