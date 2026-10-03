import initWasm, { analyze, cancel_word_address as cancelWordAddress, engine_version as engineVersion, initThreadPool } from "../generated/wasm-threads/colonist_search.js";
import type { EngineWorkerRequest, EngineWorkerResponse } from "./protocol";

const startedAt = performance.now();
const threadCount = Math.min(Math.max((navigator.hardwareConcurrency || 2) - 1, 1), 8);
const ready = (async () => {
  if (!crossOriginIsolated || typeof SharedArrayBuffer === "undefined") throw new Error("Threaded WASM worker is not cross-origin isolated");
  const exports = await initWasm({ module_or_path: new URL("./wasm-threads/colonist_search_bg.wasm", import.meta.url) }) as unknown as { memory: WebAssembly.Memory };
  // TypeScript's WebAssembly declarations omit the shared-memory buffer case.
  const buffer = exports.memory.buffer as ArrayBuffer | SharedArrayBuffer;
  const address = cancelWordAddress();
  if (!(buffer instanceof SharedArrayBuffer) || address % 4 !== 0 || address + 4 > buffer.byteLength) {
    throw new Error("Threaded WASM cancellation word is not in aligned shared memory");
  }
  await initThreadPool(threadCount);
  // The word is a static; later memory growth keeps its original bytes valid.
  const cancelWord = { buffer, index: address / 4 };
  postMessage({ token: "ready", status: { engineRevision: engineVersion(), initializationMs: performance.now() - startedAt, threadCount }, cancelWord } satisfies EngineWorkerResponse);
})();
void ready.catch((error: unknown) => postMessage({ token: "ready", error: error instanceof Error ? error.message : "Thread pool initialization failed" } satisfies EngineWorkerResponse));
self.onmessage = (event: MessageEvent<EngineWorkerRequest>) => {
  void ready.then(() => {
    const result = analyze(event.data.request);
    postMessage({ token: event.data.token, result } satisfies EngineWorkerResponse);
  }).catch((error: unknown) => postMessage({ token: event.data.token, error: error instanceof Error ? error.message : "Threaded WASM analysis failed" } satisfies EngineWorkerResponse));
};
