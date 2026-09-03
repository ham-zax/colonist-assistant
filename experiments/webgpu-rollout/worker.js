const WORKGROUP_SIZE = 128;
const ROOT_STATS_WORDS = 10;
const DEV_STATS_WORDS = 8;
const DEV_CARD_NAMES = ["knight", "roadBuilding", "yearOfPlenty", "monopoly"];
const PARAM_WORDS = 12;
const U64_MASK = (1n << 64n) - 1n;

self.addEventListener("install", (event) => event.waitUntil(self.skipWaiting()));
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));
self.addEventListener("message", (event) => {
  if (event.data?.type !== "run") return;
  const port = event.ports[0];
  const task = runFeasibility(event.data.casePath ?? "./case.json");
  event.waitUntil(task);
  task.then(
    (result) => port.postMessage({ ok: true, result }),
    (error) => port.postMessage({
      ok: false,
      error: error instanceof Error ? `${error.name}: ${error.message}\n${error.stack ?? ""}` : String(error),
    }),
  );
});

const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
};

const limits = (source) => Object.fromEntries([
  "maxStorageBuffersPerShaderStage",
  "maxStorageBufferBindingSize",
  "maxBufferSize",
  "maxComputeWorkgroupSizeX",
  "maxComputeInvocationsPerWorkgroup",
  "maxComputeWorkgroupsPerDimension",
  "maxComputeWorkgroupStorageSize",
].map((name) => [name, Number(source.limits[name])]));

const createBuffer = (device, data, usage) => {
  const source = data instanceof Uint32Array ? data : new Uint32Array(data);
  const buffer = device.createBuffer({
    size: Math.max(4, source.byteLength),
    usage: usage | GPUBufferUsage.COPY_DST,
  });
  if (source.byteLength) device.queue.writeBuffer(buffer, 0, source);
  return buffer;
};

const writeParams = (device, buffer, values) => {
  const words = new Uint32Array(PARAM_WORDS);
  words[0] = values.laneCount;
  words[1] = values.rootCount;
  words[2] = values.chunkRollouts;
  words[3] = values.totalRollouts;
  words[4] = 0;
  words[5] = 1;
  words[6] = values.stepCount;
  words[7] = values.seedLo;
  words[8] = values.seedHi;
  device.queue.writeBuffer(buffer, 0, words);
};

const readStats = async (device, statsBuffer, byteLength) => {
  const readback = device.createBuffer({
    size: byteLength,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });
  const started = performance.now();
  const encoder = device.createCommandEncoder();
  encoder.copyBufferToBuffer(statsBuffer, 0, readback, 0, byteLength);
  device.queue.submit([encoder.finish()]);
  await readback.mapAsync(GPUMapMode.READ);
  const data = new Uint32Array(readback.getMappedRange().slice(0));
  readback.unmap();
  readback.destroy();
  return { data, ms: performance.now() - started };
};

const summarizeRoots = (words, rootCount, labels) => {
  const field = (index, root) => words[index * rootCount + root];
  return Array.from({ length: rootCount }, (_, root) => {
    const samples = field(0, root);
    const errors = field(1, root);
    const valid = Math.max(1, samples - errors);
    const actor = field(5, root) / valid;
    const opponent = field(6, root) / valid;
    return {
      root,
      label: labels[root],
      samples,
      errors,
      terminalSamples: field(2, root),
      wins: field(3, root),
      meanTurn: field(4, root) / valid,
      meanVictoryPoints: actor,
      meanBestOpponentVictoryPoints: opponent,
      meanVictoryMargin: actor - opponent,
    };
  });
};

const summarizeDevelopment = (words, rootCount) => {
  const base = ROOT_STATS_WORDS * rootCount;
  return DEV_CARD_NAMES.map((card, index) => {
    const opportunities = words[base + index];
    const selections = words[base + 4 + index];
    return {
      card,
      opportunities,
      selections,
      selectionRate: opportunities === 0 ? 0 : selections / opportunities,
    };
  });
};

const mix64 = (value) => value & U64_MASK;
const splitLoHi = (value) => [Number(value & 0xffff_ffffn), Number((value >> 32n) & 0xffff_ffffn)];
const cpuMixStreamSeed = (baseSeed, globalIndex, domain) => {
  let value = mix64(baseSeed ^ domain ^ mix64(globalIndex * 0xd1342543de82ef95n));
  value = mix64((value ^ (value >> 30n)) * 0xbf58476d1ce4e5b9n);
  value = mix64((value ^ (value >> 27n)) * 0x94d049bb133111ebn);
  return mix64(value ^ (value >> 31n));
};
const cpuSplitmixNext = (state) => {
  state = mix64(state + 0x9e3779b97f4a7c15n);
  let value = state;
  value = mix64((value ^ (value >> 30n)) * 0xbf58476d1ce4e5b9n);
  value = mix64((value ^ (value >> 27n)) * 0x94d049bb133111ebn);
  return { state, value: mix64(value ^ (value >> 31n)) };
};

async function runFeasibility(casePath) {
  if (!self.navigator.gpu) throw new Error("navigator.gpu is unavailable in this ServiceWorker");
  const [caseResponse, shaderResponse] = await Promise.all([
    fetch(casePath, { cache: "no-store" }),
    fetch("./rollout.wgsl", { cache: "no-store" }),
  ]);
  if (!caseResponse.ok || !shaderResponse.ok) throw new Error("failed to load feasibility inputs");
  const data = await caseResponse.json();
  const shader = await shaderResponse.text();
  const adapterStarted = performance.now();
  const adapter = await self.navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
  const adapterMs = performance.now() - adapterStarted;
  if (!adapter) throw new Error("high-performance WebGPU adapter unavailable");
  const adapterInfo = {
    vendor: adapter.info?.vendor ?? "",
    architecture: adapter.info?.architecture ?? "",
    device: adapter.info?.device ?? "",
    description: adapter.info?.description ?? "",
  };
  const adapterLimits = limits(adapter);
  const deviceStarted = performance.now();
  const device = await adapter.requestDevice();
  const deviceMs = performance.now() - deviceStarted;
  const deviceLimits = limits(device);

  const shaderStarted = performance.now();
  const module = device.createShaderModule({ code: shader, label: "real-state rollout feasibility" });
  const compilation = await module.getCompilationInfo();
  const shaderCompileMs = performance.now() - shaderStarted;
  const compilationMessages = compilation.messages.map((message) => ({
    type: message.type,
    lineNum: message.lineNum,
    linePos: message.linePos,
    message: message.message,
  }));
  const errors = compilationMessages.filter((message) => message.type === "error");
  if (errors.length) throw new Error(`WGSL compilation failed: ${JSON.stringify(errors.slice(0, 8))}`);

  const bindGroupLayout = device.createBindGroupLayout({
    entries: [
      { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "uniform" } },
      { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
      { binding: 2, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
      { binding: 3, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
      { binding: 4, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
      { binding: 5, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
    ],
  });
  const pipelineLayout = device.createPipelineLayout({ bindGroupLayouts: [bindGroupLayout] });
  const pipelineStarted = performance.now();
  const [rngPipeline, expandPipeline, rolloutPipeline, reducePipeline] = await Promise.all([
    device.createComputePipelineAsync({ layout: pipelineLayout, compute: { module, entryPoint: "rng_probe" } }),
    device.createComputePipelineAsync({ layout: pipelineLayout, compute: { module, entryPoint: "expand_root_rollouts" } }),
    device.createComputePipelineAsync({ layout: pipelineLayout, compute: { module, entryPoint: "run_rollout_steps" } }),
    device.createComputePipelineAsync({ layout: pipelineLayout, compute: { module, entryPoint: "reduce_root_rollouts" } }),
  ]);
  const pipelineCreateMs = performance.now() - pipelineStarted;

  const rootCount = data.rootLabels.length;
  const baseBuffer = createBuffer(device, new Uint32Array(data.baseStateWords), GPUBufferUsage.STORAGE);
  const topologyBuffer = createBuffer(device, new Uint32Array(data.topologyWordsData), GPUBufferUsage.STORAGE);
  const rootWords = new Uint32Array([...data.rootActionWords, ...data.rootBaseIndices]);
  const rootBuffer = createBuffer(device, rootWords, GPUBufferUsage.STORAGE);
  const paramsBuffer = device.createBuffer({ size: 48, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  const statsWords = ROOT_STATS_WORDS * rootCount + DEV_STATS_WORDS;
  const statsBytes = statsWords * 4;
  const zeroStats = new Uint32Array(statsWords);

  const createScenario = (laneCount) => {
    if (laneCount % rootCount !== 0) throw new Error(`lane count ${laneCount} is not divisible by ${rootCount} roots`);
    const laneBytes = data.layout.laneWords * laneCount * 4;
    if (laneBytes > device.limits.maxStorageBufferBindingSize) {
      throw new Error(`lane buffer ${laneBytes} exceeds default binding limit ${device.limits.maxStorageBufferBindingSize}`);
    }
    if (laneBytes > device.limits.maxBufferSize) {
      throw new Error(`lane buffer ${laneBytes} exceeds default buffer limit ${device.limits.maxBufferSize}`);
    }
    const started = performance.now();
    const laneBuffer = device.createBuffer({
      size: laneBytes,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
    });
    const statsBuffer = device.createBuffer({
      size: statsBytes,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST,
    });
    const combinedReadback = device.createBuffer({
      size: statsBytes,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
    const bindGroup = device.createBindGroup({
      layout: bindGroupLayout,
      entries: [
        { binding: 0, resource: { buffer: paramsBuffer } },
        { binding: 1, resource: { buffer: baseBuffer } },
        { binding: 2, resource: { buffer: topologyBuffer } },
        { binding: 3, resource: { buffer: rootBuffer } },
        { binding: 4, resource: { buffer: laneBuffer } },
        { binding: 5, resource: { buffer: statsBuffer } },
      ],
    });
    return {
      laneCount,
      rolloutsPerRoot: laneCount / rootCount,
      laneBytes,
      totalResidentBytes: laneBytes + statsBytes * 2 + data.baseStateWords.length * 4 + data.topologyWordsData.length * 4 + rootWords.byteLength + 48,
      allocateMs: performance.now() - started,
      laneBuffer,
      statsBuffer,
      combinedReadback,
      bindGroup,
      destroy() {
        laneBuffer.destroy();
        statsBuffer.destroy();
        combinedReadback.destroy();
      },
    };
  };

  const bindAndDispatch = (pass, pipeline, bindGroup, laneCount) => {
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, bindGroup);
    pass.dispatchWorkgroups(Math.ceil(laneCount / WORKGROUP_SIZE));
  };

  const runScenario = async (scenario, steps) => {
    if (steps % 16 !== 0) throw new Error("feasibility measurements use 16-step chunks");
    device.queue.writeBuffer(scenario.statsBuffer, 0, zeroStats);
    writeParams(device, paramsBuffer, {
      laneCount: scenario.laneCount,
      rootCount,
      chunkRollouts: scenario.rolloutsPerRoot,
      totalRollouts: scenario.rolloutsPerRoot,
      stepCount: 16,
      seedLo: data.seed.lo,
      seedHi: data.seed.hi,
    });
    const totalStarted = performance.now();
    const computeStarted = performance.now();
    const computeEncoder = device.createCommandEncoder();
    let pass = computeEncoder.beginComputePass();
    bindAndDispatch(pass, expandPipeline, scenario.bindGroup, scenario.laneCount);
    pass.end();
    for (let completed = 0; completed < steps; completed += 16) {
      pass = computeEncoder.beginComputePass();
      bindAndDispatch(pass, rolloutPipeline, scenario.bindGroup, scenario.laneCount);
      pass.end();
    }
    device.queue.submit([computeEncoder.finish()]);
    await device.queue.onSubmittedWorkDone();
    const computeMs = performance.now() - computeStarted;

    const reductionStarted = performance.now();
    const reductionEncoder = device.createCommandEncoder();
    pass = reductionEncoder.beginComputePass();
    bindAndDispatch(pass, reducePipeline, scenario.bindGroup, scenario.laneCount);
    pass.end();
    device.queue.submit([reductionEncoder.finish()]);
    await device.queue.onSubmittedWorkDone();
    const reductionMs = performance.now() - reductionStarted;
    const readback = await readStats(device, scenario.statsBuffer, statsBytes);
    const totalMs = performance.now() - totalStarted;
    return {
      steps,
      laneCount: scenario.laneCount,
      rolloutsPerRoot: scenario.rolloutsPerRoot,
      laneBytes: scenario.laneBytes,
      totalResidentBytes: scenario.totalResidentBytes,
      computeMs,
      reductionMs,
      readbackMs: readback.ms,
      totalMs,
      rolloutStepsPerSecond: scenario.laneCount * steps / (totalMs / 1000),
      roots: summarizeRoots(readback.data, rootCount, data.rootLabels),
      developmentActions: summarizeDevelopment(readback.data, rootCount),
    };
  };

  const runCombinedScenario = async (scenario, steps) => {
    if (steps % 16 !== 0) throw new Error("feasibility measurements use 16-step chunks");
    device.queue.writeBuffer(scenario.statsBuffer, 0, zeroStats);
    writeParams(device, paramsBuffer, {
      laneCount: scenario.laneCount,
      rootCount,
      chunkRollouts: scenario.rolloutsPerRoot,
      totalRollouts: scenario.rolloutsPerRoot,
      stepCount: 16,
      seedLo: data.seed.lo,
      seedHi: data.seed.hi,
    });
    const started = performance.now();
    const encoder = device.createCommandEncoder();
    let pass = encoder.beginComputePass();
    bindAndDispatch(pass, expandPipeline, scenario.bindGroup, scenario.laneCount);
    pass.end();
    for (let completed = 0; completed < steps; completed += 16) {
      pass = encoder.beginComputePass();
      bindAndDispatch(pass, rolloutPipeline, scenario.bindGroup, scenario.laneCount);
      pass.end();
    }
    pass = encoder.beginComputePass();
    bindAndDispatch(pass, reducePipeline, scenario.bindGroup, scenario.laneCount);
    pass.end();
    encoder.copyBufferToBuffer(scenario.statsBuffer, 0, scenario.combinedReadback, 0, statsBytes);
    device.queue.submit([encoder.finish()]);
    await scenario.combinedReadback.mapAsync(GPUMapMode.READ);
    const words = new Uint32Array(scenario.combinedReadback.getMappedRange().slice(0));
    scenario.combinedReadback.unmap();
    const totalMs = performance.now() - started;
    return {
      steps,
      totalMs,
      rolloutStepsPerSecond: scenario.laneCount * steps / (totalMs / 1000),
      roots: summarizeRoots(words, rootCount, data.rootLabels),
      developmentActions: summarizeDevelopment(words, rootCount),
    };
  };

  const primary = createScenario(rootCount * data.cuda.rolloutsPerRoot);
  writeParams(device, paramsBuffer, {
    laneCount: primary.laneCount,
    rootCount,
    chunkRollouts: primary.rolloutsPerRoot,
    totalRollouts: primary.rolloutsPerRoot,
    stepCount: 16,
    seedLo: data.seed.lo,
    seedHi: data.seed.hi,
  });
  device.queue.writeBuffer(primary.statsBuffer, 0, zeroStats);
  const rngEncoder = device.createCommandEncoder();
  let rngPass = rngEncoder.beginComputePass();
  rngPass.setPipeline(rngPipeline);
  rngPass.setBindGroup(0, primary.bindGroup);
  rngPass.dispatchWorkgroups(1);
  rngPass.end();
  device.queue.submit([rngEncoder.finish()]);
  await device.queue.onSubmittedWorkDone();
  const rngReadback = await readStats(device, primary.statsBuffer, statsBytes);
  const seed = (BigInt(data.seed.hi) << 32n) | BigInt(data.seed.lo);
  const cpuMixed = cpuMixStreamSeed(seed, 0n, 0xa4093822299f31d0n);
  const cpuNext = cpuSplitmixNext(cpuMixed).value;
  const [mixedLo, mixedHi] = splitLoHi(cpuMixed);
  const [nextLo, nextHi] = splitLoHi(cpuNext);
  const expectedRng = [mixedLo, mixedHi, nextLo, nextHi, Number(cpuNext % 6n)];
  const actualRng = Array.from(rngReadback.data.slice(0, 5));
  const rngParity = expectedRng.every((value, index) => value === actualRng[index]);
  if (!rngParity) throw new Error(`paired-u32 SplitMix64 parity failed: expected=${expectedRng} actual=${actualRng}`);

  const warmup = await runScenario(primary, 16);
  const measure = async (steps, repetitions = 3) => {
    const runs = [];
    for (let repetition = 0; repetition < repetitions; repetition += 1) runs.push(await runScenario(primary, steps));
    const medianTotal = median(runs.map((run) => run.totalMs));
    const selected = runs.reduce((best, run) => Math.abs(run.totalMs - medianTotal) < Math.abs(best.totalMs - medianTotal) ? run : best, runs[0]);
    return {
      repetitions,
      medianMs: medianTotal,
      medianComputeMs: median(runs.map((run) => run.computeMs)),
      medianReductionMs: median(runs.map((run) => run.reductionMs)),
      medianReadbackMs: median(runs.map((run) => run.readbackMs)),
      rolloutStepsPerSecond: primary.laneCount * steps / (medianTotal / 1000),
      representativeRoots: selected.roots,
      developmentActions: selected.developmentActions,
      runTimesMs: runs.map((run) => run.totalMs),
    };
  };
  const h48 = await measure(48);
  const h96 = await measure(96);
  const measureCombined = async (steps) => {
    const runs = [];
    for (let repetition = 0; repetition < 3; repetition += 1) runs.push(await runCombinedScenario(primary, steps));
    const medianTotal = median(runs.map((run) => run.totalMs));
    const selected = runs.reduce((best, run) => Math.abs(run.totalMs - medianTotal) < Math.abs(best.totalMs - medianTotal) ? run : best, runs[0]);
    return {
      repetitions: runs.length,
      medianMs: medianTotal,
      rolloutStepsPerSecond: primary.laneCount * steps / (medianTotal / 1000),
      representativeRoots: selected.roots,
      developmentActions: selected.developmentActions,
      runTimesMs: runs.map((run) => run.totalMs),
    };
  };
  const combined48 = await measureCombined(48);
  const combined96 = await measureCombined(96);

  const scaling = [];
  for (const laneCount of [8192, 16384, 32768, 65536]) {
    if (laneCount === primary.laneCount) {
      scaling.push({
        laneCount,
        rolloutsPerRoot: laneCount / rootCount,
        laneBytes: primary.laneBytes,
        totalResidentBytes: primary.totalResidentBytes,
        totalMs: h48.medianMs,
        rolloutStepsPerSecond: h48.rolloutStepsPerSecond,
        errors: h48.representativeRoots.reduce((sum, root) => sum + root.errors, 0),
        source: "h48-median",
      });
      continue;
    }
    const scenario = createScenario(laneCount);
    await runScenario(scenario, 16);
    const measured = await runScenario(scenario, 48);
    scaling.push({
      laneCount,
      rolloutsPerRoot: scenario.rolloutsPerRoot,
      laneBytes: scenario.laneBytes,
      totalResidentBytes: scenario.totalResidentBytes,
      allocateMs: scenario.allocateMs,
      totalMs: measured.totalMs,
      rolloutStepsPerSecond: measured.rolloutStepsPerSecond,
      errors: measured.roots.reduce((sum, root) => sum + root.errors, 0),
      source: "single-steady-after-warmup",
    });
    scenario.destroy();
  }

  primary.destroy();
  baseBuffer.destroy();
  topologyBuffer.destroy();
  rootBuffer.destroy();
  paramsBuffer.destroy();
  device.destroy();

  return {
    kind: "browser-webgpu-rollout-feasibility",
    caseProfile: data.generator.profile ?? "baseline",
    executionSurface: "Chrome ServiceWorker on http://localhost",
    adapter: {
      requestMs: adapterMs,
      info: adapterInfo,
      limits: adapterLimits,
      features: [...adapter.features].sort(),
    },
    device: {
      requestMs: deviceMs,
      limits: deviceLimits,
    },
    shader: {
      sourceBytes: new TextEncoder().encode(shader).byteLength,
      compileMs: shaderCompileMs,
      pipelineCreateMs,
      compilationMessages,
      entryPoints: ["rng_probe", "expand_root_rollouts", "run_rollout_steps", "reduce_root_rollouts"],
    },
    layout: {
      storageBufferCount: 5,
      rootCount,
      stateWords: data.layout.stateWords,
      stateBytesPerLane: data.layout.stateBytesPerLane,
      laneWords: data.layout.laneWords,
      laneBytesPerLane: data.layout.laneBytesPerLane,
      defaultDeviceMaxStorageBufferBindingSize: deviceLimits.maxStorageBufferBindingSize,
      maxLanesAtCurrentPackingUnderBindingLimit: Math.floor(deviceLimits.maxStorageBufferBindingSize / data.layout.laneBytesPerLane),
    },
    rng: {
      approach: "paired-u32 exact SplitMix64 arithmetic",
      expectedProbe: expectedRng,
      actualProbe: actualRng,
      parity: rngParity,
    },
    warmup,
    h48,
    h96,
    combined48,
    combined96,
    scaling,
    cudaReference: {
      device: data.cuda.device,
      initMs: data.cuda.initMs,
      uploadMs: data.cuda.uploadMs,
      warmupMs: data.cuda.warmupMs,
      h0ExpandReduceUpperBound: data.cuda.h0ExpandReduceUpperBound,
      h48: data.cuda.h48,
      h96: data.cuda.h96,
    },
    rootLabels: data.rootLabels,
    policyScope: {
      productionPackedStateLayout: true,
      includedTransitionSemanticsMatchProduction: true,
      fullNoPlayerTradeRolloutPolicyParity: true,
      fullProductionRolloutPolicyParity: false,
      playerTradesEnabled: false,
      includedRolloutFamilies: [
        "roll/chance",
        "discard",
        "robber/steal",
        "road",
        "settlement",
        "city",
        "buy-development/draw",
        "knight",
        "road-building",
        "year-of-plenty",
        "monopoly",
        "maritime-trade",
        "end-turn",
      ],
      omittedRolloutFamilies: ["domestic player trades", "setup phases"],
      note: "H3 closes the reachable no-player-trade development-card policy against the production CUDA weights and transition semantics; domestic trades and unreachable setup remain outside the gate.",
    },
  };
}
