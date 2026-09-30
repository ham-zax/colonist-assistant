import initWasm, { analyze, engine_version as engineVersion, initThreadPool } from "../generated/wasm-threads/colonist_search.js";
import type { EngineWorkerRequest, EngineWorkerResponse } from "./protocol";

const startedAt = performance.now();
const threadCount = Math.min(Math.max((navigator.hardwareConcurrency || 2) - 1, 1), 8);
const ready = (async () => {
  if (!crossOriginIsolated || typeof SharedArrayBuffer === "undefined") throw new Error("Threaded WASM worker is not cross-origin isolated");
  await initWasm({ module_or_path: new URL("./wasm-threads/colonist_search_bg.wasm", import.meta.url) });
  await initThreadPool(threadCount);
  postMessage({ token: "ready", status: { engineRevision: engineVersion(), initializationMs: performance.now() - startedAt, threadCount } } satisfies EngineWorkerResponse);
})();
void ready.catch((error: unknown) => postMessage({ token: "ready", error: error instanceof Error ? error.message : "Thread pool initialization failed" } satisfies EngineWorkerResponse));
self.onmessage = (event: MessageEvent<EngineWorkerRequest>) => {
  void ready.then(() => {
    const result = analyze(event.data.request);
    postMessage({ token: event.data.token, result } satisfies EngineWorkerResponse);
  }).catch((error: unknown) => postMessage({ token: event.data.token, error: error instanceof Error ? error.message : "Threaded WASM analysis failed" } satisfies EngineWorkerResponse));
};
