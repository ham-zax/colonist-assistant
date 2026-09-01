import type { DecisionTrace } from "./decision-trace";
import type { BoardSnapshot } from "./placement";
import type { StoredEvent } from "./types";
import {
  ACTIVE_GAME_RECORD_STORAGE_KEY,
  LATEST_GAME_RECORD_STORAGE_KEY,
} from "./local-data";

export type GameRecordStatus = "recording" | "completed" | "interrupted";

export interface RecordedAssistantSettings {
  engine: string;
  disablePlayerTrades: boolean;
  autopilot: boolean;
}

export type RecordedDecisionTrace = Omit<
  DecisionTrace,
  "replayState" | "replayBoard"
>;

export interface RecordedBoardFrame {
  capturedAt: number;
  turn?: number;
  currentPlayer?: string;
  isMyTurn: boolean;
  action: BoardSnapshot["action"];
  hasRolled?: boolean;
  gameOver?: boolean;
  winner?: string;
  myPlayer?: string;
  robberHex?: string;
  ownHand?: BoardSnapshot["ownHand"];
  ownDevelopmentCards?: BoardSnapshot["ownDevelopmentCards"];
  bank?: BoardSnapshot["bank"];
  players?: BoardSnapshot["players"];
  buildings: Array<{
    vertexId: string;
    player: string;
    kind: "settlement" | "city";
  }>;
  roads: Array<{ edgeId: string; player: string }>;
}

export interface RecordedGame {
  schema: 1;
  scope: string;
  status: GameRecordStatus;
  sessionId: string;
  gameKey?: string;
  startedAt: number;
  updatedAt: number;
  completedAt?: number;
  partialHistory: boolean;
  unmatchedCount: number;
  playerOrder: string[];
  assistant: RecordedAssistantSettings;
  events: StoredEvent[];
  decisions: RecordedDecisionTrace[];
  boardTimeline: RecordedBoardFrame[];
}

export interface GameRecordCapture {
  scope: string;
  sessionId: string;
  gameKey?: string;
  startedAt: number;
  partialHistory: boolean;
  unmatchedCount: number;
  playerOrder: string[];
  assistant: RecordedAssistantSettings;
  events: StoredEvent[];
  decisions: DecisionTrace[];
  board?: BoardSnapshot;
}

const MAX_DECISION_CANDIDATES = 12;
const MAX_ROOT_ITEMS = 12;
const MAX_BOARD_FRAMES = 800;

const compactDecisionTrace = (trace: DecisionTrace): RecordedDecisionTrace => {
  const {
    replayState: _replayState,
    replayBoard: _replayBoard,
    deepCandidates,
    rootProvenance,
    ...rest
  } = trace;
  return {
    ...rest,
    ...(deepCandidates
      ? { deepCandidates: deepCandidates.slice(0, MAX_DECISION_CANDIDATES) }
      : {}),
    ...(rootProvenance
      ? {
          rootProvenance: {
            ...rootProvenance,
            rankedRoots: rootProvenance.rankedRoots.slice(0, MAX_ROOT_ITEMS),
            retainedRoots: rootProvenance.retainedRoots.slice(0, MAX_ROOT_ITEMS),
            prunedRoots: rootProvenance.prunedRoots.slice(0, MAX_ROOT_ITEMS),
          },
        }
      : {}),
  };
};

const compactBoardFrame = (board: BoardSnapshot): RecordedBoardFrame => ({
  capturedAt: Date.now(),
  turn: board.turn,
  currentPlayer: board.currentPlayer,
  isMyTurn: Boolean(board.isMyTurn),
  action: board.action,
  hasRolled: board.hasRolled,
  gameOver: board.gameOver,
  winner: board.winner,
  myPlayer: board.myPlayer,
  robberHex: board.hexes.find((hex) => hex.blocked)?.id,
  ownHand: board.ownHand ? structuredClone(board.ownHand) : undefined,
  ownDevelopmentCards: board.ownDevelopmentCards
    ? structuredClone(board.ownDevelopmentCards)
    : undefined,
  bank: board.bankVisible && board.bank ? structuredClone(board.bank) : undefined,
  players: board.players ? structuredClone(board.players) : undefined,
  buildings: board.vertices.flatMap((vertex) =>
    vertex.building
      ? [
          {
            vertexId: vertex.id,
            player: vertex.building.player,
            kind: vertex.building.kind,
          },
        ]
      : [],
  ),
  roads: board.edges.flatMap((edge) =>
    edge.player ? [{ edgeId: edge.id, player: edge.player }] : [],
  ),
});

const boardFrameSignature = (frame: RecordedBoardFrame): string =>
  JSON.stringify({
    turn: frame.turn,
    currentPlayer: frame.currentPlayer,
    isMyTurn: frame.isMyTurn,
    action: frame.action,
    hasRolled: frame.hasRolled,
    gameOver: frame.gameOver,
    winner: frame.winner,
    robberHex: frame.robberHex,
    ownHand: frame.ownHand,
    ownDevelopmentCards: frame.ownDevelopmentCards,
    bank: frame.bank,
    players: frame.players,
    buildings: frame.buildings,
    roads: frame.roads,
  });

const normalizeRecordedGame = (value: unknown): RecordedGame | undefined => {
  if (!value || typeof value !== "object") return undefined;
  const record = value as Partial<RecordedGame>;
  if (record.schema !== 1 || !record.scope || !record.sessionId) return undefined;
  if (!Array.isArray(record.events) || !Array.isArray(record.decisions)) return undefined;
  if (!Array.isArray(record.boardTimeline)) return undefined;
  return record as RecordedGame;
};

export class GameRecordRecorder {
  private active?: RecordedGame;
  private loaded?: Promise<void>;
  private persistTimer?: ReturnType<typeof globalThis.setTimeout>;
  private storageOperations: Promise<void> = Promise.resolve();

  capture(input: GameRecordCapture): void {
    const snapshot: GameRecordCapture = structuredClone(input);
    void this.ensureLoaded().then(() => {
      this.applyCapture(snapshot, false);
    });
  }

  finalize(input: GameRecordCapture): void {
    const snapshot: GameRecordCapture = structuredClone(input);
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
          normalizeRecordedGame(stored[ACTIVE_GAME_RECORD_STORAGE_KEY]) ??
          normalizeRecordedGame(stored[LATEST_GAME_RECORD_STORAGE_KEY]);
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
    }

    const existing = this.active;
    const decisions = new Map<string, RecordedDecisionTrace>(
      existing?.decisions.map((decision) => [decision.stateHash, decision]) ?? [],
    );
    for (const decision of input.decisions) {
      decisions.set(decision.stateHash, compactDecisionTrace(decision));
    }

    const boardTimeline = existing ? [...existing.boardTimeline] : [];
    if (input.board) {
      const frame = compactBoardFrame(input.board);
      const previous = boardTimeline.at(-1);
      if (!previous || boardFrameSignature(previous) !== boardFrameSignature(frame)) {
        boardTimeline.push(frame);
        if (boardTimeline.length > MAX_BOARD_FRAMES) {
          boardTimeline.splice(0, boardTimeline.length - MAX_BOARD_FRAMES);
        }
      }
    }

    this.active = {
      schema: 1,
      scope: input.scope,
      status: completed ? "completed" : "recording",
      sessionId: input.sessionId,
      ...(input.gameKey ? { gameKey: input.gameKey } : {}),
      startedAt: existing?.startedAt ?? input.startedAt,
      updatedAt: now,
      ...(completed
        ? { completedAt: existing?.completedAt ?? now }
        : existing?.completedAt
          ? { completedAt: existing.completedAt }
          : {}),
      partialHistory: input.partialHistory,
      unmatchedCount: input.unmatchedCount,
      playerOrder: [...input.playerOrder],
      assistant: { ...input.assistant },
      events: structuredClone(input.events),
      decisions: [...decisions.values()].sort(
        (left, right) => left.recordedAt - right.recordedAt,
      ),
      boardTimeline,
    };
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
  return (
    normalizeRecordedGame(stored[LATEST_GAME_RECORD_STORAGE_KEY]) ??
    normalizeRecordedGame(stored[ACTIVE_GAME_RECORD_STORAGE_KEY])
  );
};

const safeFilenamePart = (value: string): string =>
  value.replace(/[^a-z0-9._-]+/giu, "-").replace(/^-+|-+$/gu, "").slice(0, 80);

export const downloadRecordedGame = (record: RecordedGame): void => {
  const identity = safeFilenamePart(record.gameKey ?? record.sessionId) || "game";
  const stamp = new Date(record.startedAt).toISOString().replace(/[:.]/gu, "-");
  const blob = new Blob([JSON.stringify(record, null, 2)], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `colonist-record-${identity}-${stamp}.json`;
  anchor.style.display = "none";
  document.documentElement.append(anchor);
  anchor.click();
  anchor.remove();
  globalThis.setTimeout(() => URL.revokeObjectURL(url), 0);
};
