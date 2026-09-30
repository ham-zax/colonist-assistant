const exchange = (worker, message) => new Promise((resolve, reject) => {
  const timer = setTimeout(() => reject(new Error("Engine worker timed out")), 45000);
  worker.onmessage = ({ data }) => {
    clearTimeout(timer);
    if (data.type === "error") reject(new Error(data.error));
    else resolve(data);
  };
  worker.onerror = (event) => { clearTimeout(timer); reject(new Error(event.message)); };
  worker.postMessage(message);
});

const publish = (result) => chrome.runtime.sendMessage({ type: "threading-benchmark-update", result });
const signature = (result) => ({
  chosen: result.chosen ?? null, actions: result.actions, nodes: result.nodes,
  depth: result.deepestDecisionDepth, authority: result.authority, deadlineReached: result.deadlineReached,
});

async function run(options = {}) {
  const mode = options.mode ?? "fixed";
  const repetitions = options.reps ?? 1;
  const threadCounts = options.threads ?? [0, 1, 2, 4, 8];
  const result = { progress: "starting", mode, runs: [], offscreenIsolated: crossOriginIsolated };
  let worker;
  try {
    if (!["fixed", "live-current", "live-original"].includes(mode)) throw new Error("Unknown benchmark mode");
    if (!Number.isInteger(repetitions) || repetitions < 1) throw new Error("reps must be a positive integer");
    if (!Array.isArray(threadCounts) || !threadCounts.length || threadCounts.some((count) => !Number.isInteger(count) || count < 0 || count > 8)) throw new Error("threads must contain integer counts 0 through 8");
    if (new Set(threadCounts).size !== threadCounts.length) throw new Error("threads must not contain duplicates");
    let fixtures = await (await fetch("requests.json")).json();
    if (options.ids) fixtures = fixtures.filter((item) => options.ids.includes(item.id));
    else if (mode === "live-current") fixtures = fixtures.filter((item) => item.id.endsWith("#s0") && item.request.state.players.length === 4);
    if (!fixtures.length) throw new Error("No fixtures selected");
    const prepare = (request, selectedMode) => {
      const copy = structuredClone(request);
      if (selectedMode === "fixed") {
        copy.depth = 2; copy.effort.cpu.maxDepth = 2;
        copy.timeBudgetMs = 0; copy.effort.decisionTimeMs = 0;
        copy.effort.cpu.evidenceEscalationMs = 0;
      } else if (selectedMode === "live-current") {
        copy.timeBudgetMs = 10000; copy.effort.decisionTimeMs = 10000;
        copy.effort.cpu.nodesPerDepthWave = 48000;
      }
      return copy;
    };
    for (const threads of threadCounts) {
      worker = new Worker("worker.js", { type: "module" });
      const status = await exchange(worker, { type: "init", threads });
      const rows = [];
      await exchange(worker, { type: "analyze", request: prepare(fixtures[0].request, "fixed") });
      for (let rep = 0; rep < repetitions; rep += 1) {
        let sumMs = 0;
        for (const fixture of fixtures) {
          result.progress = `${mode} threads=${threads} rep=${rep} ${fixture.id}`;
          await publish(result);
          const response = await exchange(worker, { type: "analyze", request: prepare(fixture.request, mode) });
          sumMs += response.ms;
          rows.push({ rep, id: fixture.id, ms: response.ms, sig: signature(response.result), stages: response.result.searchStages });
        }
        rows.push({ rep, sumMs });
      }
      result.runs.push({ threads, status, rows });
      worker.terminate(); worker = undefined;
      await publish(result);
    }
    result.progress = "done";
  } catch (error) {
    result.progress = "error"; result.error = String(error); result.stack = error.stack;
  } finally {
    worker?.terminate();
    await publish(result);
  }
}

chrome.runtime.onMessage.addListener((message, sender, reply) => {
  if (message.type !== "threading-benchmark-run") return;
  reply({ accepted: true });
  void run(message.options);
});
void chrome.runtime.sendMessage({ type: "threading-benchmark-document-ready" });
