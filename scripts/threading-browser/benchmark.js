window.benchmark = { progress: "idle", runs: [] };
window.startBenchmark = (options) => chrome.runtime.sendMessage({ type: "threading-benchmark-start", options });
setInterval(async () => {
  try {
    window.benchmark = await chrome.runtime.sendMessage({ type: "threading-benchmark-status" });
    document.getElementById("progress").textContent = benchmark.progress;
  } catch (error) {
    document.getElementById("progress").textContent = String(error);
  }
}, 500);
