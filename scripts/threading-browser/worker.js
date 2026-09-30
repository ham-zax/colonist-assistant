let engine;
self.onmessage = async ({ data }) => {
  try {
    if (data.type === "init") {
      let poolWorkers = 0;
      const BrowserWorker = Worker;
      self.Worker = class extends BrowserWorker {
        constructor(...arguments_) { super(...arguments_); poolWorkers += 1; }
      };
      engine = await import(data.threads ? "./wasm-threads/colonist_search.js" : "./wasm/colonist_search.js");
      const exports = await engine.default();
      if (data.threads) await engine.initThreadPool(data.threads);
      postMessage({
        type: "ready", isolated: crossOriginIsolated, poolWorkers,
        sharedMemory: typeof SharedArrayBuffer !== "undefined" && exports.memory.buffer instanceof SharedArrayBuffer,
      });
      return;
    }
    const started = performance.now();
    const result = engine.analyze(data.request);
    postMessage({ type: "result", ms: performance.now() - started, result });
  } catch (error) {
    postMessage({ type: "error", error: String(error), stack: error.stack });
  }
};
