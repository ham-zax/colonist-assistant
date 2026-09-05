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

export type UnmatchedLogReason =
  | "known-bank-shortage-notice"
  | "known-ignored-system-message"
  | "known-ignored-friendly-robber-status"
  | "known-redundant-trade-offer"
  | "known-redundant-robber-move"
  | "known-ignored-production-blocked"
  | "known-ignored-empty-robbery"
  | "known-ignored-bot-status"
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

interface StoredSession {
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
  if (/^bot is selecting cards to discard for\b/iu.test(normalized)) {
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
  if (/\bwants to give\b.+\bfor\b/iu.test(normalized)) {
    // Active-trade snapshots are ingested separately with stable trade
    // identity, so the rendered chat offer is duplicate evidence.
    return { reason: "known-redundant-trade-offer", affectsIntegrity: false };
  }
  return { reason: "unrecognized-log-format", affectsIntegrity: true };
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
  private disposed = false;
  private myPlayer?: string;
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
      attributes: true,
      attributeFilter: ["data-index", "src", "style"],
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
      const id = `${source}:${event.type}:${this.events.length}:${index}:${JSON.stringify(event)}`;
      if (this.seenIds.has(id)) continue;
      this.seenIds.add(id);
      const stored = canonicalizeEvent(
        {
          ...event,
          id,
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
    this.unmatchedCount = 0;
    this.unmatchedIntegrityCount = 0;
    this.unmatchedSamples = [];
    this.seenIds.clear();
    this.syntheticSequence = 0;
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
    this.unmatchedCount = 0;
    this.unmatchedIntegrityCount = 0;
    this.unmatchedSamples = [];
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
    const changed = canonical.some(
      (event, index) => event !== this.events[index],
    );
    if (!changed) return;
    this.events = canonical;
    this.state = replayEvents(this.events);
    this.queueSave();
    this.onUpdate(this);
  }

  private scan(force = false): void {
    if (this.disposed) return;
    const language = detectLanguage();
    const elements = findMessageElements(this.root);
    const occurrence = new Map<string, number>();
    const candidates: Array<{ element: Element; id: string; index: number }> = [];

    for (const element of elements) {
      const snapshot = snapshotMessage(element, language);
      if (!snapshot) continue;
      let id = stableMessageId(snapshot);
      if (snapshot.index === undefined) {
        const base = hashString(`${snapshot.serialText}|${snapshot.visibleText}`);
        const ordinal = occurrence.get(base) ?? 0;
        occurrence.set(base, ordinal + 1);
        id = `message:${base}:${ordinal}`;
      }
      if (!force && this.seenElements.get(element) === id) continue;
      candidates.push({ element, id, index: snapshot.index ?? this.syntheticSequence++ });
    }

    candidates.sort((left, right) => left.index - right.index);
    const currentFirstIds = candidates
      .filter((candidate) => candidate.element.getAttribute("data-index") === "0")
      .map((candidate) => candidate.id);
    if (
      this.events.length &&
      currentFirstIds.length &&
      currentFirstIds.every((id) => !this.seenIds.has(id))
    ) {
      this.storageGeneration += 1;
      this.pruneSessionHistory = true;
      this.state = createTrackerState();
      this.events = [];
      this.partialHistory = false;
      this.unmatchedCount = 0;
      this.unmatchedIntegrityCount = 0;
      this.unmatchedSamples = [];
      this.startedAt = Date.now();
      this.seenIds.clear();
    }
    if (
      !this.events.length &&
      candidates.length &&
      candidates[0]!.index > 0 &&
      candidates.some((candidate) => candidate.element.hasAttribute("data-index"))
    ) {
      this.partialHistory = true;
    }

    let changed = false;
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
        this.recordUnmatched(snapshot.serialText, snapshot.index, classification);
        changed = true;
        continue;
      }
      const stored = canonicalizeEvent({
        ...parsed.event,
        id: candidate.id,
        ...(snapshot.index !== undefined ? { index: snapshot.index } : {}),
        timestamp: Date.now(),
        raw: snapshot.serialText,
      } as StoredEvent, this.myPlayer);
      this.events.push(stored);
      this.state = reduceTracker(this.state, stored, stored);
      changed = true;
    }

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

  private async restore(): Promise<void> {
    const key = sessionStorageKey(this.id);
    let result: Record<string, unknown>;
    try {
      result = await chrome.storage.local.get(key);
    } catch (error) {
      if (isExtensionContextInvalidatedError(error)) return;
      throw error;
    }
    const stored = result[key] as StoredSession | undefined;
    if (!stored || stored.schema !== 3 || stored.page !== pageIdentity()) return;
    if (this.gameKey && stored.gameKey && this.gameKey !== stored.gameKey) return;
    this.gameKey ??= stored.gameKey;
    this.startedAt = stored.startedAt;
    this.events = stored.events;
    this.partialHistory = stored.partialHistory;
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
    this.state = replayEvents(stored.events);
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
      schema: 3,
      id: this.id,
      page: pageIdentity(),
      ...(this.gameKey ? { gameKey: this.gameKey } : {}),
      startedAt: this.startedAt,
      updatedAt: now,
      events: this.events,
      seenIds: [...this.seenIds].slice(-MAX_SEEN_IDS),
      partialHistory: this.partialHistory,
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
