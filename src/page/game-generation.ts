import { isSetupTurn } from "../core/game-progress";

// The Colonist manager object can be replaced for a new game while the URL/room
// shape remains reusable, and an extension reload re-executes this page bridge.
// Persist the manager generation in page sessionStorage so a reload does not
// reset it to 1, reuse an old gameKey, and let a fresh content script restore or
// overwrite state that belongs to the previous game generation.
const MANAGER_GENERATION_STORAGE_KEY = "colonistAssistantManagerGeneration";
const LIVE_PROGRESS_STORAGE_KEY = "colonistAssistantLiveProgress";

const validGeneration = (value: unknown): value is number =>
  typeof value === "number" &&
  Number.isSafeInteger(value) &&
  value >= 1;

export const readManagerGeneration = (
  storage: Pick<Storage, "getItem" | "setItem">,
): number => {
  let generation = 1;
  try {
    const stored = Number(storage.getItem(MANAGER_GENERATION_STORAGE_KEY));
    if (validGeneration(stored)) generation = stored;
    storage.setItem(MANAGER_GENERATION_STORAGE_KEY, String(generation));
  } catch {
    // Storage can be unavailable in restrictive browser contexts. Falling back
    // to the original in-memory generation preserves live behavior; only
    // reload continuity is unavailable in that environment.
  }
  return generation;
};

export const bumpManagerGeneration = (
  storage: Pick<Storage, "setItem">,
  current: number,
): number => {
  const next = validGeneration(current) ? current + 1 : 1;
  try {
    storage.setItem(MANAGER_GENERATION_STORAGE_KEY, String(next));
  } catch {
    // Keep the live generation monotonic even if persistence is unavailable.
  }
  return next;
};

export interface LiveGameProgress {
  maxCompletedTurns: number;
  maxPlacedPieces: number;
  hasSeenGameplay: boolean;
  hasSeenGameOver: boolean;
  playerRosterKey?: string;
  victoryTarget?: number;
}

export interface LiveGameSnapshotInput {
  completedTurns: number;
  placedPieces: number;
  initialPlacement: boolean;
  gameplayRollCount?: number;
  gameOver: boolean;
  playerRosterKey?: string;
  victoryTarget?: number;
  isReplay?: boolean;
}

const isResolvedRoster = (key?: string): boolean => {
  if (!key) return false;
  const players = key.split(",").filter(Boolean);
  if (players.length < 2) return false;
  return !players.some((player) => /^Player \d+$/i.test(player));
};

const maxRosterPlayers = (key?: string): number => {
  if (!key) return 4;
  const count = key.split(",").filter(Boolean).length;
  return count >= 2 ? count : 4;
};

export const isGameGenerationRollover = (
  previous: LiveGameProgress | undefined,
  current: LiveGameSnapshotInput,
): boolean => {
  if (current.isReplay || !previous) return false;
  if (
    !Number.isSafeInteger(current.completedTurns) ||
    current.completedTurns < 0 ||
    (current.initialPlacement &&
      !isSetupTurn(current.completedTurns, maxRosterPlayers(current.playerRosterKey)))
  ) return false;

  // 1. Resolved player roster change across games
  if (
    isResolvedRoster(previous.playerRosterKey) &&
    isResolvedRoster(current.playerRosterKey) &&
    previous.playerRosterKey !== current.playerRosterKey
  ) {
    if (
      current.initialPlacement ||
      current.completedTurns <= 1 ||
      previous.hasSeenGameOver
    ) {
      return true;
    }
  }

  // 2. Victory target change in the same room
  if (
    previous.victoryTarget !== undefined &&
    current.victoryTarget !== undefined &&
    previous.victoryTarget > 0 &&
    current.victoryTarget > 0 &&
    previous.victoryTarget !== current.victoryTarget
  ) {
    return true;
  }

  // 3. Game over observed in previous game, and now a game is active
  if (
    previous.hasSeenGameOver &&
    !current.gameOver &&
    (current.placedPieces > 0 || current.initialPlacement)
  ) {
    return true;
  }

  // 4. Gameplay occurred in previous game, and now we are back in initial placement
  if (
    previous.hasSeenGameplay &&
    current.initialPlacement
  ) {
    return true;
  }

  // 5. Total placed pieces on the board decreased, returning to early game
  if (
    previous.maxPlacedPieces >= 3 &&
    current.placedPieces < previous.maxPlacedPieces &&
    (current.initialPlacement || current.completedTurns <= 1)
  ) {
    return true;
  }

  // 6. Turn count decreased after progress, returning to the start of a game
  if (
    previous.maxCompletedTurns >= 2 &&
    current.completedTurns < previous.maxCompletedTurns &&
    (current.initialPlacement || current.completedTurns <= 1)
  ) {
    return true;
  }

  return false;
};

export const updateLiveGameProgress = (
  previous: LiveGameProgress | undefined,
  current: LiveGameSnapshotInput,
): LiveGameProgress => {
  const isGameplay =
    !current.initialPlacement &&
    ((current.gameplayRollCount !== undefined && current.gameplayRollCount > 0) ||
      current.completedTurns >= 2);
  if (!previous) {
    return {
      maxCompletedTurns: current.completedTurns,
      maxPlacedPieces: current.placedPieces,
      hasSeenGameplay: isGameplay,
      hasSeenGameOver: current.gameOver,
      playerRosterKey: current.playerRosterKey,
      victoryTarget: current.victoryTarget,
    };
  }
  return {
    maxCompletedTurns: Math.max(previous.maxCompletedTurns, current.completedTurns),
    maxPlacedPieces: Math.max(previous.maxPlacedPieces, current.placedPieces),
    hasSeenGameplay: previous.hasSeenGameplay || isGameplay,
    hasSeenGameOver: previous.hasSeenGameOver || current.gameOver,
    playerRosterKey: isResolvedRoster(current.playerRosterKey)
      ? current.playerRosterKey
      : (previous.playerRosterKey || current.playerRosterKey),
    victoryTarget: current.victoryTarget ?? previous.victoryTarget,
  };
};

export const readLiveGameProgress = (
  storage: Pick<Storage, "getItem">,
): LiveGameProgress | undefined => {
  try {
    const raw = storage.getItem(LIVE_PROGRESS_STORAGE_KEY);
    if (!raw) return undefined;
    const parsed = JSON.parse(raw);
    if (
      parsed &&
      typeof parsed === "object" &&
      typeof parsed.maxCompletedTurns === "number" &&
      typeof parsed.maxPlacedPieces === "number"
    ) {
      return parsed as LiveGameProgress;
    }
  } catch {
    // Ignore storage parse errors
  }
  return undefined;
};

export const writeLiveGameProgress = (
  storage: Pick<Storage, "setItem">,
  progress: LiveGameProgress,
): void => {
  try {
    storage.setItem(LIVE_PROGRESS_STORAGE_KEY, JSON.stringify(progress));
  } catch {
    // Ignore storage errors
  }
};
