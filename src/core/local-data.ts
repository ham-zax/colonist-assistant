export const ACTIVE_SESSION_STORAGE_KEY = "colonistAssistantActiveSession";
export const LATEST_SUMMARY_STORAGE_KEY = "colonistAssistantLatestSummary";
export const SESSION_STORAGE_PREFIX = "colonistAssistantSession:";
export const DECISION_TRACE_STORAGE_KEY =
  "colonist-assistant-decision-traces-v1";
export const ACTIVE_GAME_RECORD_STORAGE_KEY =
  "colonist-assistant-active-game-record-v1";
export const LATEST_GAME_RECORD_STORAGE_KEY =
  "colonist-assistant-latest-game-record-v1";

export const sessionStorageKey = (sessionId: string): string =>
  `${SESSION_STORAGE_PREFIX}${sessionId}`;

const storedSessionKeys = (stored: Record<string, unknown>): string[] =>
  Object.keys(stored).filter((key) => key.startsWith(SESSION_STORAGE_PREFIX));

/**
 * Remove all persisted game evidence without touching preferences such as
 * overlay position, hints, or autopilot. Enumerating the prefix also removes
 * orphaned records left by earlier games or extension versions.
 */
export const clearCurrentGameStorage = async (): Promise<void> => {
  const stored = await chrome.storage.local.get(null);
  await chrome.storage.local.remove([
    ACTIVE_SESSION_STORAGE_KEY,
    LATEST_SUMMARY_STORAGE_KEY,
    DECISION_TRACE_STORAGE_KEY,
    ACTIVE_GAME_RECORD_STORAGE_KEY,
    LATEST_GAME_RECORD_STORAGE_KEY,
    ...storedSessionKeys(stored),
  ]);
};

/**
 * Keep only the record that has just become current. Call this after writing
 * the replacement record so pruning cannot remove it when the session ID is
 * reused across consecutive games.
 */
export const pruneHistoricalSessionStorage = async (
  currentSessionId: string,
): Promise<void> => {
  const currentKey = sessionStorageKey(currentSessionId);
  const stored = await chrome.storage.local.get(null);
  const staleKeys = storedSessionKeys(stored).filter(
    (key) => key !== currentKey,
  );
  if (staleKeys.length) await chrome.storage.local.remove(staleKeys);
};
