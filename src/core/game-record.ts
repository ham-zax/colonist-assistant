import type { DecisionTrace } from "./decision-trace";
import {
  CompactGameBuilder,
  formatCompactGameRecord,
  normalizeCompactRecordIntegrity,
  type CompactGameCapture,
  type CompactGameRecord,
} from "./llm-record";
import type { BoardSnapshot } from "./placement";
import type { StoredEvent } from "./types";
import {
  ACTIVE_GAME_RECORD_STORAGE_KEY,
  LATEST_GAME_RECORD_STORAGE_KEY,
} from "./local-data";

export type RecordedGame = CompactGameRecord;
export type GameRecordCapture = CompactGameCapture;

const normalizeRecordedGame = (value: unknown): RecordedGame | undefined => {
  if (!value || typeof value !== "object") return undefined;
  const record = value as Partial<RecordedGame>;
  if (
    record.schema !== "catan-evidence/1" ||
    !record.scope ||
    !record.sessionId ||
    !record.contracts ||
    !record.aliases
  ) {
    return undefined;
  }
  if (
    !Array.isArray(record.boardHexes) ||
    !Array.isArray(record.boardVertices) ||
    !Array.isArray(record.boardEdges) ||
    !Array.isArray(record.frames) ||
    !Array.isArray(record.events) ||
    !Array.isArray(record.decisions) ||
    !Array.isArray(record.decisionContexts) ||
    !Array.isArray(record.decisionTrades) ||
    !Array.isArray(record.attempts) ||
    !Array.isArray(record.candidates) ||
    !Array.isArray(record.roots) ||
    !Array.isArray(record.replacements) ||
    !Array.isArray(record.beliefs) ||
    !Array.isArray(record.beliefSummaries) ||
    !Array.isArray(record.beliefWorlds) ||
    !Array.isArray(record.archetypes) ||
    !Array.isArray(record.handVectors)
  ) {
    return undefined;
  }
  return normalizeCompactRecordIntegrity(record as RecordedGame);
};

interface LegacyBoardFrame {
  capturedAt: number;
  turn?: number;
  currentPlayer?: string;
  isMyTurn?: boolean;
  action?: BoardSnapshot["action"];
  hasRolled?: boolean;
  gameOver?: boolean;
  winner?: string;
  myPlayer?: string;
  robberHex?: string;
  ownHand?: BoardSnapshot["ownHand"];
  ownDevelopmentCards?: BoardSnapshot["ownDevelopmentCards"];
  bank?: BoardSnapshot["bank"];
  players?: BoardSnapshot["players"];
  buildings?: Array<{
    vertexId: string;
    player: string;
    kind: "settlement" | "city";
  }>;
  roads?: Array<{ edgeId: string; player: string }>;
}

interface LegacyGameRecord {
  schema: 1;
  scope: string;
  status: RecordedGame["status"];
  sessionId: string;
  gameKey?: string;
  startedAt: number;
  updatedAt: number;
  completedAt?: number;
  partialHistory: boolean;
  unmatchedCount: number;
  assistant: RecordedGame["assistant"];
  events: StoredEvent[];
  decisions: DecisionTrace[];
  boardTimeline: LegacyBoardFrame[];
}

const legacyBoardSnapshot = (frame: LegacyBoardFrame): BoardSnapshot => ({
  hexes: frame.robberHex ? [{ id: frame.robberHex, blocked: true }] : [],
  vertices: (frame.buildings ?? []).map((building) => ({
    id: building.vertexId,
    adjacentHexes: [],
    adjacentVertices: [],
    building: {
      player: building.player,
      kind: building.kind,
    },
  })),
  edges: (frame.roads ?? []).map((road) => ({
    id: road.edgeId,
    vertices: ["", ""],
    player: road.player,
  })),
  turn: frame.turn,
  currentPlayer: frame.currentPlayer,
  isMyTurn: frame.isMyTurn,
  action: frame.action,
  hasRolled: frame.hasRolled,
  gameOver: frame.gameOver,
  winner: frame.winner,
  myPlayer: frame.myPlayer,
  ownHand: frame.ownHand,
  ownDevelopmentCards: frame.ownDevelopmentCards,
  bank: frame.bank,
  bankVisible: Boolean(frame.bank),
  players: frame.players,
  observedAt: frame.capturedAt,
});

const migrateLegacyRecordedGame = (value: unknown): RecordedGame | undefined => {
  if (!value || typeof value !== "object") return undefined;
  const legacy = value as Partial<LegacyGameRecord>;
  if (
    legacy.schema !== 1 ||
    !legacy.scope ||
    !legacy.sessionId ||
    !legacy.startedAt ||
    !legacy.assistant ||
    !Array.isArray(legacy.events) ||
    !Array.isArray(legacy.decisions) ||
    !Array.isArray(legacy.boardTimeline)
  ) {
    return undefined;
  }
  const builder = new CompactGameBuilder();
  const base: Omit<CompactGameCapture, "events" | "decisions" | "board"> = {
    scope: legacy.scope,
    sessionId: legacy.sessionId,
    ...(legacy.gameKey ? { gameKey: legacy.gameKey } : {}),
    startedAt: legacy.startedAt,
    partialHistory: Boolean(legacy.partialHistory),
    unmatchedCount: legacy.unmatchedCount ?? 0,
    assistant: legacy.assistant,
  };
  for (const frame of legacy.boardTimeline) {
    builder.apply(
      {
        ...base,
        events: [],
        decisions: [],
        board: legacyBoardSnapshot(frame),
      },
      false,
    );
  }
  const migrated = builder.apply(
    {
      ...base,
      events: legacy.events,
      decisions: legacy.decisions,
    },
    legacy.status === "completed",
  );
  migrated.status = legacy.status ?? "recording";
  migrated.updatedAt = legacy.updatedAt ?? migrated.updatedAt;
  if (legacy.completedAt !== undefined) migrated.completedAt = legacy.completedAt;
  return migrated;
};

const normalizeAnyRecordedGame = (value: unknown): RecordedGame | undefined =>
  normalizeRecordedGame(value) ?? migrateLegacyRecordedGame(value);

export class GameRecordRecorder {
  private active?: RecordedGame;
  private builder?: CompactGameBuilder;
  private loaded?: Promise<void>;
  private persistTimer?: ReturnType<typeof globalThis.setTimeout>;
  private storageOperations: Promise<void> = Promise.resolve();

  private snapshotCapture(input: GameRecordCapture): GameRecordCapture {
    return {
      ...input,
      assistant: { ...input.assistant },
      events: structuredClone(input.events),
      // snapshotForRecord() already returns detached changed evidence,
      // including bounded replay worlds only on the first capture per decision.
      decisions: input.decisions,
      ...(input.board ? { board: structuredClone(input.board) } : {}),
    };
  }

  capture(input: GameRecordCapture): void {
    const snapshot = this.snapshotCapture(input);
    void this.ensureLoaded().then(() => {
      this.applyCapture(snapshot, false);
    });
  }

  finalize(input: GameRecordCapture): void {
    const snapshot = this.snapshotCapture(input);
    void this.ensureLoaded().then(() => {
      this.applyCapture(snapshot, true);
    });
  }

  async flush(): Promise<void> {
    await this.ensureLoaded();
    if (this.persistTimer !== undefined) {
      globalThis.clearTimeout(this.persistTimer);
      this.persistTimer = undefined;
      await this.persist();
    }
    await this.storageOperations;
  }

  async reset(): Promise<void> {
    await this.flush();
    this.active = undefined;
    this.builder = undefined;
    this.loaded = Promise.resolve();
    await this.enqueueStorage(() =>
      chrome.storage.local.remove([
        ACTIVE_GAME_RECORD_STORAGE_KEY,
        LATEST_GAME_RECORD_STORAGE_KEY,
      ]),
    );
  }

  private ensureLoaded(): Promise<void> {
    this.loaded ??= chrome.storage.local
      .get([
        ACTIVE_GAME_RECORD_STORAGE_KEY,
        LATEST_GAME_RECORD_STORAGE_KEY,
      ])
      .then((stored) => {
        this.active =
          normalizeAnyRecordedGame(stored[ACTIVE_GAME_RECORD_STORAGE_KEY]) ??
          normalizeAnyRecordedGame(stored[LATEST_GAME_RECORD_STORAGE_KEY]);
        this.builder = this.active ? new CompactGameBuilder(this.active) : undefined;
      })
      .catch(() => undefined);
    return this.loaded;
  }

  private applyCapture(input: GameRecordCapture, completed: boolean): void {
    const now = Date.now();
    if (this.active && this.active.scope !== input.scope) {
      const previous: RecordedGame = {
        ...this.active,
        status:
          this.active.status === "completed" ? "completed" : "interrupted",
        updatedAt: now,
      };
      void this.enqueueStorage(() =>
        chrome.storage.local.set({ [LATEST_GAME_RECORD_STORAGE_KEY]: previous }),
      );
      this.active = undefined;
      this.builder = undefined;
    }

    this.builder ??= new CompactGameBuilder(this.active);
    this.active = this.builder.apply(input, completed);
    this.schedulePersist();
  }

  private schedulePersist(): void {
    if (this.persistTimer !== undefined) return;
    this.persistTimer = globalThis.setTimeout(() => {
      this.persistTimer = undefined;
      void this.persist();
    }, 220);
  }

  private async persist(): Promise<void> {
    const record = this.active ? structuredClone(this.active) : undefined;
    if (!record) return;
    await this.enqueueStorage(async () => {
      if (record.status === "completed") {
        await chrome.storage.local.set({
          [LATEST_GAME_RECORD_STORAGE_KEY]: record,
        });
        await chrome.storage.local.remove(ACTIVE_GAME_RECORD_STORAGE_KEY);
        return;
      }
      await chrome.storage.local.set({
        [ACTIVE_GAME_RECORD_STORAGE_KEY]: record,
      });
    });
  }

  private enqueueStorage(operation: () => Promise<void>): Promise<void> {
    const next = this.storageOperations.then(operation, operation);
    this.storageOperations = next.catch(() => undefined);
    return next;
  }
}

export const readRecordedGame = async (): Promise<RecordedGame | undefined> => {
  const stored = await chrome.storage.local.get([
    ACTIVE_GAME_RECORD_STORAGE_KEY,
    LATEST_GAME_RECORD_STORAGE_KEY,
  ]);
  const active = normalizeAnyRecordedGame(stored[ACTIVE_GAME_RECORD_STORAGE_KEY]);
  const latest = normalizeAnyRecordedGame(stored[LATEST_GAME_RECORD_STORAGE_KEY]);
  if (!active) return latest;
  if (!latest) return active;
  // A newly started game can be captured in the same millisecond that the
  // previous game is archived. Prefer the active record on an updatedAt tie.
  return active.updatedAt >= latest.updatedAt ? active : latest;
};

const safeFilenamePart = (value: string): string =>
  value.replace(/[^a-z0-9._-]+/giu, "-").replace(/^-+|-+$/gu, "").slice(0, 80);

export const downloadRecordedGame = (record: RecordedGame): void => {
  const identity = safeFilenamePart(record.gameKey ?? record.sessionId) || "game";
  const stamp = new Date(record.startedAt).toISOString().replace(/[:.]/gu, "-");
  const blob = new Blob([formatCompactGameRecord(record)], {
    type: "text/plain;charset=utf-8",
  });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `colonist-evidence-${identity}-${stamp}.txt`;
  anchor.style.display = "none";
  document.documentElement.append(anchor);
  anchor.click();
  anchor.remove();
  globalThis.setTimeout(() => URL.revokeObjectURL(url), 0);
};
