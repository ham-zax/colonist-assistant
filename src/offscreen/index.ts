import { OFFSCREEN_MESSAGE_TYPE, subtractQueueBudget, type OffscreenRequest, type OffscreenResponse, type ThreadedStatus, type EngineWorkerResponse } from "./protocol";

interface Job {
  token: string;
  request: unknown;
  enqueuedAt: number;
  respond: (response: OffscreenResponse) => void;
  cancelled: boolean;
}
let worker: Worker | undefined;
let active: Job | undefined;
const queue: Job[] = [];
let resolveReady: (status: ThreadedStatus) => void;
let rejectReady: (error: Error) => void;
const ready = new Promise<ThreadedStatus>((resolve, reject) => { resolveReady = resolve; rejectReady = reject; });
// Readiness can fail before a background status listener connects.
void ready.catch(() => undefined);
let failure: string | undefined;
const fail = (detail: string): void => {
  failure = detail;
  rejectReady(new Error(detail));
  if (active && !active.cancelled) active.respond({ token: active.token, error: detail });
  active = undefined;
  for (const job of queue.splice(0)) job.respond({ token: job.token, error: detail });
};
const pump = (): void => {
  if (active || failure) return;
  const job = queue.shift();
  if (!job) return;
  try {
    const request = subtractQueueBudget(job.request, performance.now() - job.enqueuedAt);
    active = job;
    worker!.postMessage({ token: job.token, request });
  } catch (error) {
    active = undefined;
    job.respond({ token: job.token, error: error instanceof Error ? error.message : "Threaded engine dispatch failed" });
    pump();
  }
};
try {
  if (!crossOriginIsolated || typeof SharedArrayBuffer === "undefined") throw new Error("Extension page is not cross-origin isolated");
  worker = new Worker(chrome.runtime.getURL("offscreen-engine-worker.js"), { type: "module" });
  worker.onmessage = (event: MessageEvent<EngineWorkerResponse>) => {
    if (event.data.status) { resolveReady(event.data.status); return; }
    if (!active && event.data.error) { fail(event.data.error); return; }
    if (!active || event.data.token !== active.token) return;
    const job = active;
    active = undefined;
    if (!job.cancelled) job.respond(event.data);
    pump();
  };
  worker.onerror = (event) => fail(event.message || "Threaded engine worker failed");
  worker.onmessageerror = () => fail("Threaded engine worker response could not be decoded");
} catch (error) { fail(error instanceof Error ? error.message : "Threaded engine setup failed"); }

chrome.runtime.onMessage.addListener((value: unknown, sender, sendResponse) => {
  // Offscreen messages originate only in this extension's service worker.
  if (sender.id !== chrome.runtime.id || sender.tab || !value || typeof value !== "object") return undefined;
  const message = value as OffscreenRequest;
  if (message.type !== OFFSCREEN_MESSAGE_TYPE || typeof message.token !== "string") return undefined;
  if (message.operation === "cancel") {
    if (active?.token === message.token && !active.cancelled) {
      active.cancelled = true;
      active.respond({ token: message.token, error: "Decision cancelled as stale" });
      // WASM is synchronous. Keep the active slot until its cooperative budget
      // finishes; terminating its host would orphan rayon's child workers.
    }
    const index = queue.findIndex((job) => job.token === message.token);
    if (index >= 0) queue.splice(index, 1)[0]!.respond({ token: message.token, error: "Decision cancelled as stale" });
    sendResponse({ token: message.token });
    return undefined;
  }
  if (message.operation !== "status" && message.operation !== "analyze") return undefined;
  if (failure) { sendResponse({ token: message.token, error: failure } satisfies OffscreenResponse); return undefined; }
  const enqueuedAt = performance.now();
  if (message.operation === "analyze") queue.push({ token: message.token, request: message.request, enqueuedAt, respond: sendResponse, cancelled: false });
  void ready.then((status) => {
    if (failure) throw new Error(failure);
    if (message.operation === "status") sendResponse({ token: message.token, status } satisfies OffscreenResponse);
    else if (message.operation === "analyze") {
      pump();
    }
  }).catch((error: unknown) => {
    // fail() already settles every analysis queued before readiness.
    if (message.operation === "status") sendResponse({ token: message.token, error: error instanceof Error ? error.message : "Threaded engine unavailable" } satisfies OffscreenResponse);
  });
  return true;
});
