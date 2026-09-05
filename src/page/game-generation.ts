const MANAGER_GENERATION_STORAGE_KEY = "colonistAssistantManagerGeneration";

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
