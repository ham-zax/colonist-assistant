import type { WasmSearchResponse } from "../generated/wasm/colonist_search";

export const OFFSCREEN_MESSAGE_TYPE = "colonist-assistant:offscreen-engine";
export interface ThreadedStatus {
  engineRevision: string;
  initializationMs: number;
  threadCount: number;
}
export type OffscreenRequest = {
  type: typeof OFFSCREEN_MESSAGE_TYPE;
  token: string;
} & ({ operation: "status" } | { operation: "analyze"; request: unknown } | { operation: "cancel" });
export interface OffscreenResponse {
  token: string;
  status?: ThreadedStatus;
  result?: WasmSearchResponse;
  error?: string;
}
export type EngineWorkerRequest = { token: string; request: unknown };
export type EngineWorkerResponse = OffscreenResponse;

/** Queue time is measured in the receiving document's own clock. */
export const subtractQueueBudget = (request: unknown, elapsedMs: number): unknown => {
  if (!request || typeof request !== "object") return request;
  const raw = request as Record<string, unknown>;
  if (typeof raw.timeBudgetMs !== "number" || raw.timeBudgetMs <= 0) return request;
  const remaining = Math.floor(raw.timeBudgetMs - Math.max(0, elapsedMs));
  // A zero WASM budget means unlimited fixed work, not an expired live request.
  if (remaining < 1) throw new Error("Decision budget expired in threaded engine queue");
  return { ...raw, timeBudgetMs: remaining, ...(raw.effort && typeof raw.effort === "object" ? { effort: { ...raw.effort, decisionTimeMs: remaining } } : {}) };
};
