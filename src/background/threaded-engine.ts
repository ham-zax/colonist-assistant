import { OFFSCREEN_MESSAGE_TYPE, type OffscreenRequest, type OffscreenResponse, type ThreadedStatus } from "../offscreen/protocol";
import type { DeepSearchExecutor } from "../worker/deep-search";

const SETUP_TIMEOUT_MS = 5_000;
const RESPONSE_TIMEOUT_MS = 12_000;
export let threadedFallbackReason: string | undefined;
let setup: Promise<ThreadedStatus | undefined> | undefined;

const bounded = async <T>(work: Promise<T>, timeoutMs: number): Promise<T> => {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([work, new Promise<never>((_resolve, reject) => {
      timer = setTimeout(() => reject(new Error("Threaded engine response timed out")), timeoutMs);
    })]);
  } finally { if (timer !== undefined) clearTimeout(timer); }
};
const send = async (message: OffscreenRequest): Promise<OffscreenResponse> => {
  const response = await bounded(chrome.runtime.sendMessage<OffscreenResponse>(message), RESPONSE_TIMEOUT_MS);
  if (!response || response.token !== message.token) throw new Error("Threaded engine returned an invalid response");
  if (response.error) throw new Error(response.error);
  return response;
};

/** Concurrent tabs share one document creation and pool initialization. Setup
 * failures select the packaged single-thread engine before any work is sent. */
const initializeThreadedEngine = (): Promise<ThreadedStatus | undefined> => {
  if (setup) return setup;
  let created = false;
  let failed = false;
  const closeOwnedDocument = async (): Promise<void> => {
    if (!created) return;
    created = false;
    try { await bounded(chrome.offscreen.closeDocument(), 1_000); } catch { /* Browser may already have closed it. */ }
  };
  const initialize = async (): Promise<ThreadedStatus | undefined> => {
    if (!chrome.offscreen?.createDocument || !chrome.runtime.getContexts) {
      threadedFallbackReason = "Offscreen worker API unavailable; using single-threaded WASM";
      return undefined;
    }
    const documentUrl = chrome.runtime.getURL("offscreen.html");
    const contexts = await chrome.runtime.getContexts({
      contextTypes: [chrome.runtime.ContextType.OFFSCREEN_DOCUMENT], documentUrls: [documentUrl],
    });
    if (failed) return undefined;
    if (!contexts.length) {
      await chrome.offscreen.createDocument({
        url: "offscreen.html", reasons: [chrome.offscreen.Reason.WORKERS],
        justification: "Run local WASM search in a dedicated worker with a shared-memory thread pool",
      });
      created = true;
      // A timed-out create call may finish later. Close our document before
      // starting its engine; never close a document inherited after a restart.
      if (failed) { await closeOwnedDocument(); return undefined; }
    }
    const response = await send({ type: OFFSCREEN_MESSAGE_TYPE, operation: "status", token: crypto.randomUUID() });
    if (!response.status || response.status.threadCount < 1) throw new Error("Threaded engine returned no readiness status");
    return response.status;
  };
  setup = bounded(initialize(), SETUP_TIMEOUT_MS).catch(async (error: unknown) => {
    failed = true;
    await closeOwnedDocument();
    threadedFallbackReason = `${error instanceof Error ? error.message : "Thread pool setup failed"}; using single-threaded WASM`;
    return undefined;
  });
  return setup;
};

export const warmThreadedEngine = async (): Promise<ThreadedStatus | undefined> => {
  const status = await initializeThreadedEngine();
  if (!status) return undefined;
  // The service worker can outlive the offscreen worker. Check readiness before
  // dispatching each new decision so a later failure selects fallback for the
  // next request, without repeating a search that was already dispatched.
  try {
    const response = await bounded(send({
      type: OFFSCREEN_MESSAGE_TYPE,
      operation: "status",
      token: crypto.randomUUID(),
    }), SETUP_TIMEOUT_MS);
    if (!response.status || response.status.threadCount < 1) {
      throw new Error("Threaded engine returned no readiness status");
    }
    return response.status;
  } catch (error: unknown) {
    threadedFallbackReason = `${error instanceof Error ? error.message : "Thread pool unavailable"}; using single-threaded WASM`;
    setup = Promise.resolve(undefined);
    return undefined;
  }
};

export const threadedExecutor = (signal: AbortSignal): DeepSearchExecutor => async (request) => {
  signal.throwIfAborted();
  const token = crypto.randomUUID();
  const cancel = () => {
    void chrome.runtime.sendMessage({ type: OFFSCREEN_MESSAGE_TYPE, operation: "cancel", token } satisfies OffscreenRequest).catch(() => undefined);
  };
  signal.addEventListener("abort", cancel, { once: true });
  try {
    const response = await send({ type: OFFSCREEN_MESSAGE_TYPE, operation: "analyze", token, request });
    signal.throwIfAborted();
    if (!response.result) throw new Error("Threaded engine returned no analysis");
    return response.result;
  } catch (error) {
    cancel();
    throw error;
  } finally { signal.removeEventListener("abort", cancel); }
};
