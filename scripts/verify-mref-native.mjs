#!/usr/bin/env node
// Local-only integration gate: real TypeScript adapter -> native messaging -> CUDA.
// The D68 board supplies topology/resources; dice evidence here is synthetic,
// not a claim about that recording or current Colonist server tuning.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { build } from "esbuild";

const root = resolve(import.meta.dirname, "..");
const temporary = await mkdtemp(join(tmpdir(), "colonist-native-mref-"));
let child;
const pending = new Map();
let nextId = 1;
let buffer = Buffer.alloc(0);
let stderr = "";

function rejectPending(error) {
  for (const { reject, timer } of pending.values()) {
    clearTimeout(timer);
    reject(error);
  }
  pending.clear();
}
function send(payload, timeoutMs = 120_000) {
  const id = nextId++;
  return new Promise((resolveReply, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`Native request ${id} timed out: ${stderr.slice(-4000)}`));
    }, timeoutMs);
    pending.set(id, { resolve: resolveReply, reject, timer });
    const json = Buffer.from(JSON.stringify({ ...payload, id }));
    const size = Buffer.alloc(4);
    size.writeUInt32LE(json.length);
    child.stdin.write(Buffer.concat([size, json]));
  });
}

try {
  await build({
    entryPoints: [join(root, "src/worker/deep-search.ts"), join(root, "src/core/dice-history.ts")],
    outdir: temporary, outbase: join(root, "src"), outExtension: { ".js": ".mjs" },
    platform: "node", format: "esm", target: "node22", bundle: true, logLevel: "error",
  });
  const { buildDeepSearchRequest } = await import(pathToFileURL(join(temporary, "worker/deep-search.mjs")));
  const dice = await import(pathToFileURL(join(temporary, "core/dice-history.mjs")));
  const wasm = await import(pathToFileURL(join(root, "src/generated/wasm/colonist_search.js")));
  await wasm.default({ module_or_path: await readFile(join(root, "src/generated/wasm/colonist_search_bg.wasm")) });
  const fixture = JSON.parse(await readFile(join(root, "tests/fixtures/gpu-strategic-strength-d68.json"), "utf8"));
  const empty = () => ({ lumber: 0, brick: 0, wool: 0, grain: 0, ore: 0 });
  const players = Object.fromEntries(fixture.tracker.playerOrder.map((name) => {
    const source = fixture.tracker.players[name];
    return [name, {
      name, color: "", devCards: source.devCards, playedDevCards: source.playedDevCards,
      builds: { road: 0, settlement: 0, city: 0, development: 0 },
      resourcesGained: empty(), productionGained: empty(), resourcesSpent: empty(),
      opponentModel: { tradeAccepts: 0, tradeRejects: 0, offersMade: 0, countersMade: 0, policyPosterior: source.policyPosterior },
    }];
  }));
  const tracker = {
    ...fixture.tracker, players, diceRolls: {}, uncertaintyEvents: 0,
    possibilitiesTruncated: false, warnings: [], pendingTradeBehaviour: {}, tradeEmbargoes: {},
  };
  const board = { ...fixture.board, diceMode: "balanced", victoryTarget: 15, friendlyRobber: true };
  for (const player of Object.values(board.players)) player.cardDiscardLimit = 9;
  const started = performance.now();
  child = spawn(join(root, "engine/target/release/colonist-assistant-gpu"), [], { stdio: ["pipe", "pipe", "pipe"] });
  child.once("error", rejectPending);
  child.once("exit", (code) => rejectPending(new Error(`Native host exited ${code}: ${stderr.slice(-4000)}`)));
  child.stderr.on("data", (chunk) => { stderr = (stderr + chunk.toString()).slice(-12000); });
  child.stdout.on("data", (chunk) => {
    buffer = Buffer.concat([buffer, chunk]);
    while (buffer.length >= 4) {
      const length = buffer.readUInt32LE();
      if (length > 1024 * 1024) { rejectPending(new Error("Oversized native response")); child.kill(); return; }
      if (buffer.length < 4 + length) break;
      let response;
      try { response = JSON.parse(buffer.subarray(4, 4 + length)); }
      catch (error) { rejectPending(error); child.kill(); return; }
      buffer = buffer.subarray(4 + length);
      const waiter = pending.get(response.id);
      if (waiter) {
        pending.delete(response.id);
        clearTimeout(waiter.timer);
        waiter.resolve(response);
      }
    }
  });
  const hello = await send({ type: "hello", protocolVersion: 6, stateSchemaVersion: 3 });
  assert.equal(hello.error, undefined);
  assert.equal(hello.engineRevision, "deep-maxn-v12");
  assert(hello.stochasticModels.includes(dice.MREF_COLONIST_LINKED_2024_V1));
  console.log(JSON.stringify({ stage: "hello", elapsedMs: Math.round(performance.now() - started), device: hello.device.name, stochasticModels: hello.stochasticModels }));
  const results = [];
  let lastRequest;
  for (const model of ["m0", "mref-complete", "mref-suffix"]) {
    const history = dice.createDiceHistoryState();
    dice.observeLogCoverage(history, model === "mref-suffix" ? [10] : [0]);
    if (model === "mref-suffix") {
      history.missingPrefixRolls = 8;
      dice.appendPublicDiceRoll(history, { actor: board.playerOrder[0], total: 8, eventId: "synthetic-suffix-8", logIndex: 10 });
    }
    const stochastic = model === "m0" ? { model: dice.M0_FAIR_IID_2D6_V1 }
      : dice.buildLiveDecisionStochasticInput("balanced", history, board.playerOrder);
    const request = buildDeepSearchRequest(tracker, board, fixture.rootPlayer, {}, false, 4, stochastic).request;
    request.effort = {
      decisionTimeMs: 4000,
      tactical: { maxDepth: 1, nodeBudget: 1000 },
      cpu: { maxDepth: 2, rootCap: 12, nodesPerDepthWave: 8000 },
      gpu: { rootCap: 12, rolloutBudget: 384, rolloutSteps: 96 },
    };
    lastRequest = request;
    const before = performance.now();
    const result = await send({ type: "analyze", request }, 20_000);
    assert.equal(result.error, undefined, result.error);
    const response = result.response;
    assert.equal(response.engineRevision, "deep-maxn-v12");
    assert.equal(response.stochasticModel, stochastic.model);
    assert(response.chosen, "native decision must return an action");
    assert.equal(response.algorithm, "gpu-root-rollout", "fixture must exercise GPU strategy, not only an exact CPU fast path");
    assert(response.rollouts > 0, "native search must realize CUDA rollout evidence");
    if (model !== "m0") {
      const cpu = wasm.inspect_stochastic({ numPlayers: board.playerOrder.length, stochastic });
      assert.equal(response.stochasticBeliefDigest, cpu.stochasticBeliefDigest);
      assert.equal(response.stochasticBeliefParticleCount, cpu.stochasticBeliefParticleCount);
      assert.equal(response.publicHistoryDigest, cpu.publicHistoryDigest);
      if (model === "mref-suffix") assert(response.stochasticBeliefParticleCount > 1);
    }
    results.push({ model, elapsedMs: Math.round(performance.now() - before), authority: response.authority, rollouts: response.rollouts, particles: response.stochasticBeliefParticleCount, chosen: response.chosen });
    console.log(JSON.stringify(results.at(-1)));
  }
  const invalid = await send({ type: "analyze", request: {
    ...lastRequest, stochastic: { ...lastRequest.stochastic, provenance: "unknown" },
  } });
  assert.match(invalid.error, /unavailable|unknown/i);
  assert.equal(invalid.response, undefined, "invalid Mref evidence must not become an M0 result");

  const cancelledId = nextId;
  const cancelStarted = performance.now();
  const cancelled = send({ type: "analyze", request: lastRequest });
  const cancelTimer = setTimeout(() => {
    const body = Buffer.from(JSON.stringify({ type: "cancel", id: cancelledId }));
    const length = Buffer.alloc(4);
    length.writeUInt32LE(body.length);
    child.stdin.write(Buffer.concat([length, body]));
  }, 1500);
  let cancellation;
  try { cancellation = await cancelled; }
  finally { clearTimeout(cancelTimer); }
  assert.match(cancellation.error, /cancelled/i);
  assert.equal(cancellation.response, undefined);
  const recovered = await send({ type: "hello", protocolVersion: 6, stateSchemaVersion: 3 });
  assert.equal(recovered.error, undefined);
  console.log(JSON.stringify({ status: "PASS", scope: "adapter-native-CUDA and CPU/WASM posterior identity", invalidEvidenceRejected: true, cancellationMs: Math.round(performance.now() - cancelStarted), results }));
} finally {
  rejectPending(new Error("Native verification stopped"));
  child?.kill();
  await rm(temporary, { recursive: true, force: true });
}
