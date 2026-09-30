let benchmark = { progress: "idle", runs: [] };
let documentReady;
chrome.runtime.onMessage.addListener((message, sender, reply) => {
  if (message.type === "threading-benchmark-document-ready") {
    documentReady?.();
    reply({ ready: true });
  } else if (message.type === "threading-benchmark-update") {
    benchmark = message.result;
    reply({ updated: true });
  } else if (message.type === "threading-benchmark-status") {
    reply(benchmark);
  } else if (message.type === "threading-benchmark-start") {
    if (!["idle", "done", "error"].includes(benchmark.progress)) {
      reply({ error: "Benchmark already running" });
      return;
    }
    benchmark = { progress: "starting", runs: [] };
    void (async () => {
      if (await chrome.offscreen.hasDocument()) await chrome.offscreen.closeDocument();
      let timer;
      const ready = new Promise((resolve) => {
        documentReady = resolve;
      });
      await chrome.offscreen.createDocument({
        url: "offscreen.html", reasons: ["WORKERS"],
        justification: "Benchmark local WASM search on a dedicated worker pool",
      });
      try {
        await Promise.race([ready, new Promise((resolve, reject) => {
          timer = setTimeout(() => reject(new Error("Offscreen document did not become ready")), 10000);
        })]);
      } finally {
        clearTimeout(timer); documentReady = undefined;
      }
      await chrome.runtime.sendMessage({ type: "threading-benchmark-run", options: message.options });
      reply({ started: true });
    })().catch((error) => {
      benchmark = { progress: "error", error: String(error), runs: [] };
      reply(benchmark);
    });
    return true;
  }
});
