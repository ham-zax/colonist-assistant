import { afterEach, describe, expect, it, vi } from "vitest";
import {
  DECISION_MESSAGE_TYPE,
  DECISION_CANCEL_MESSAGE_TYPE,
  DECISION_STATUS_MESSAGE_TYPE,
} from "../src/worker/protocol";
import { M0_FAIR_IID_2D6_V1 as M0, MREF_COLONIST_LINKED_2024_V1 as MREF } from "../src/core/dice-history";

const analyze = vi.hoisted(() => vi.fn(async (
  message: { stochastic?: { model?: string } },
  execute?: (request: unknown) => Promise<unknown>,
) => {
  if (execute) await execute({ stochastic: message.stochastic });
  return { deepSearch: {}, requestedModel: message.stochastic?.model };
}));
vi.mock("../src/worker/analyze", () => ({ analyzeDecisionRequest: analyze }));
vi.mock("../src/worker/deep-search", () => ({ warmDeepSearchEngine: vi.fn() }));

const fixedWorkExactCapabilities = {
  algorithms: ["gpu-root-rollout", "deep-maxn-cuda-exact-fixed-work-v1"],
  exactMaxn: {
    algorithm: "deep-maxn-cuda-exact-fixed-work-v1",
    available: true,
    supportsDeadline: true,
    supportsCancellation: true,
    supportsOpening: false,
    fixedWorkParityOnly: true,
  },
} as const;

const productionExactCapabilities = {
  ...fixedWorkExactCapabilities,
  exactMaxn: {
    ...fixedWorkExactCapabilities.exactMaxn,
    fixedWorkParityOnly: false,
  },
} as const;

const gpuStatus = {
  runtime: "gpu-native" as const,
  engineRevision: "deep-maxn-v14",
  stochasticModels: [M0, MREF],
  capabilities: productionExactCapabilities,
  device: { backend: "cuda", ordinal: 0, name: "routing-fixture", computeCapability: [8, 6] as [number, number] },
};

const setupRouter = async () => {
  vi.resetModules();
  let receive!: (message: unknown, sender: unknown, sendResponse: (response: unknown) => void) => unknown;
  vi.stubGlobal("chrome", { runtime: { onMessage: { addListener: (listener: typeof receive) => { receive = listener; } } } });
  const { NativeGpuClient } = await import("../src/background/native-gpu");
  const status = vi.spyOn(NativeGpuClient.prototype, "status").mockResolvedValue(gpuStatus);
  const native = vi.spyOn(NativeGpuClient.prototype, "analyzeExact").mockResolvedValue({ stochasticModel: MREF } as never);
  const cancel = vi.spyOn(NativeGpuClient.prototype, "cancelDecision");
  const release = vi.spyOn(NativeGpuClient.prototype, "release");
  await import("../src/background/index");
  const dispatch = (message: unknown, sender: unknown) =>
    new Promise<unknown>((resolve) => receive(message, sender, resolve));
  return { dispatch, receive, status, native, cancel, release };
};

const liveDecision = (id = 1) => ({
  type: DECISION_MESSAGE_TYPE, id, state: {}, rootPlayer: "P0", engine: "deep-search",
  board: { initialPlacement: false, isMyTurn: true }, stochastic: { model: MREF },
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  analyze.mockClear();
});

describe("background Mref dispatch", () => {
  it.each([
    { tab: { id: 2 }, documentId: "second", frameId: 0 },
    { tab: { id: 1 }, documentId: "replacement", frameId: 0 },
    { tab: { id: 1 }, documentId: "first", frameId: 1 },
  ])("isolates identical decision IDs from another tab/document/frame (%j)", async (otherSender) => {
    const { dispatch, receive, native, cancel, release } = await setupRouter();
    let complete!: () => void;
    native.mockImplementationOnce(() => new Promise((resolve) => {
      complete = () => resolve({ stochasticModel: MREF } as never);
    }));
    const owner = { tab: { id: 1 }, documentId: "first", frameId: 0 };
    const first = dispatch(liveDecision(), owner);
    await vi.waitFor(() => expect(native).toHaveBeenCalledOnce());
    const second = await dispatch(liveDecision(), otherSender);
    expect(second).toMatchObject({ analysis: {
      runtime: "background-wasm", requestedModel: MREF,
      runtimeReason: expect.stringMatching(/busy/u),
    } });
    receive({ type: DECISION_CANCEL_MESSAGE_TYPE, id: 1 }, otherSender, vi.fn());
    await dispatch({ ...liveDecision(2), engine: "weighted" }, otherSender);
    expect(cancel).not.toHaveBeenCalled();
    expect(release).not.toHaveBeenCalled();
    complete();
    await expect(first).resolves.toMatchObject({ analysis: { runtime: "background-gpu" } });
    expect(native).toHaveBeenCalledOnce();
  });

  it("remembers cancellation during initialization and permits a fresh decision", async () => {
    const { dispatch, receive, status, native, cancel } = await setupRouter();
    let ready!: (status: typeof gpuStatus) => void;
    status.mockImplementationOnce(() => new Promise((resolve) => { ready = resolve; }));
    const sender = { tab: { id: 1 }, documentId: "first", frameId: 0 };
    const pending = dispatch(liveDecision(9), sender);
    receive({ type: DECISION_CANCEL_MESSAGE_TYPE, id: 9 }, sender, vi.fn());
    ready(gpuStatus);
    await expect(pending).resolves.toMatchObject({ id: 9, error: expect.stringMatching(/cancelled/u) });
    expect(native).not.toHaveBeenCalled();
    expect(analyze).not.toHaveBeenCalled();
    await expect(dispatch(liveDecision(10), sender)).resolves.toMatchObject({ analysis: { runtime: "background-gpu" } });
    expect(native).toHaveBeenCalledOnce();
    expect(native.mock.calls[0]?.[1]).not.toBe(cancel.mock.calls[0]?.[0]);
  });

  it("cancels an active native search using its private request identity", async () => {
    const { dispatch, receive, native, cancel } = await setupRouter();
    native.mockImplementationOnce((_request, _id, signal) => new Promise((_resolve, reject) => {
      signal!.addEventListener("abort", () => reject(signal!.reason), { once: true });
    }));
    const sender = { tab: { id: 1 }, documentId: "first" };
    const pending = dispatch(liveDecision(99), sender);
    await vi.waitFor(() => expect(native).toHaveBeenCalledOnce());
    receive({ type: DECISION_CANCEL_MESSAGE_TYPE, id: 99 }, sender, vi.fn());
    await expect(pending).resolves.toMatchObject({ error: expect.stringMatching(/cancelled/u) });
    expect(cancel).toHaveBeenCalledWith(native.mock.calls[0]?.[1]);
    expect(cancel).not.toHaveBeenCalledWith(99);
    expect(analyze).toHaveBeenCalledOnce();
  });

  it("uses WASM status after a native reconnect transport failure", async () => {
    const { dispatch, status } = await setupRouter();
    status.mockRejectedValueOnce(new Error("GPU companion handshake timed out"));
    const { warmDeepSearchEngine } = await import("../src/worker/deep-search");
    vi.mocked(warmDeepSearchEngine).mockResolvedValue({ engineRevision: "deep-maxn-v14", initializationMs: 2 });
    await expect(dispatch({ type: DECISION_STATUS_MESSAGE_TYPE, id: 1, engine: "deep-search" }, {}))
      .resolves.toMatchObject({ runtime: "background-wasm", engineRevision: "deep-maxn-v14" });
  });

  it("subtracts only background-local elapsed time from the remaining decision allowance", async () => {
    vi.resetModules();
    vi.stubGlobal("chrome", {
      runtime: { onMessage: { addListener: vi.fn() } },
    });
    const now = vi.spyOn(performance, "now").mockReturnValue(350);
    const { withRemainingDecisionBudget } = await import("../src/background/index");
    const adjusted = withRemainingDecisionBudget(
      {
        type: DECISION_MESSAGE_TYPE,
        id: 40,
        state: {} as never,
        rootPlayer: "P0",
        engine: "deep-search",
        board: { initialPlacement: false, isMyTurn: true } as never,
        decisionBudget: {
          contract: "client-end-to-end-v1",
          totalMs: 12_000,
          remainingEngineMs: 5_000,
          transportReserveMs: 500,
          finalizationReserveMs: 500,
        },
      },
      100,
    );
    expect(adjusted.decisionBudget?.remainingEngineMs).toBe(4_750);
    now.mockRestore();
  });

  it.each([
    { name: "capable rollout companion", models: [M0, MREF], opening: false, ownTurn: true, native: false },
    { name: "legacy companion", models: [M0], opening: false, ownTurn: true, native: false },
    { name: "opening", models: [M0, MREF], opening: true, ownTurn: true, native: false },
    { name: "opponent pondering", models: [M0, MREF], opening: false, ownTurn: false, native: false },
  ])("preserves Mref through $name", async (scenario) => {
    vi.resetModules();
    let receive: (message: unknown, sender: unknown, sendResponse: (response: unknown) => void) => unknown;
    vi.stubGlobal("chrome", { runtime: { onMessage: { addListener: (listener: typeof receive) => { receive = listener; } } } });
    const { NativeGpuClient } = await import("../src/background/native-gpu");
    vi.spyOn(NativeGpuClient.prototype, "status").mockResolvedValue({
      runtime: "gpu-native", engineRevision: "deep-maxn-v14", stochasticModels: scenario.models,
      capabilities: fixedWorkExactCapabilities,
      device: { backend: "cuda-resident-sim", ordinal: 0, name: "routing-fixture", computeCapability: [8, 6] },
    });
    const native = vi.spyOn(NativeGpuClient.prototype, "analyzeExact").mockResolvedValue({ stochasticModel: MREF } as never);
    await import("../src/background/index");
    const message = {
      type: DECISION_MESSAGE_TYPE, id: 41, state: {}, rootPlayer: "P0", engine: "deep-search",
      board: { initialPlacement: scenario.opening, isMyTurn: scenario.ownTurn },
      stochastic: { model: MREF },
    };
    const response = await new Promise<unknown>((resolve) => receive(message, {}, resolve));
    expect(response).toMatchObject({ analysis: {
      requestedModel: MREF,
      runtime: scenario.native ? "background-gpu" : "background-wasm",
    } });
    expect(native).toHaveBeenCalledTimes(scenario.native ? 1 : 0);
    if (scenario.native) expect(native).toHaveBeenCalledWith({ stochastic: { model: MREF } }, expect.any(Number), expect.any(AbortSignal));
    expect(analyze.mock.calls[0]?.[0]).toMatchObject({ stochastic: { model: MREF } });
  });

  it("routes explicit strategy admission to CPU/WASM even when native GPU is available", async () => {
    vi.resetModules();
    let receive: (message: unknown, sender: unknown, sendResponse: (response: unknown) => void) => unknown;
    vi.stubGlobal("chrome", { runtime: { onMessage: { addListener: (listener: typeof receive) => { receive = listener; } } } });
    const { NativeGpuClient } = await import("../src/background/native-gpu");
    const status = vi.spyOn(NativeGpuClient.prototype, "status").mockResolvedValue({
      runtime: "gpu-native", engineRevision: "deep-maxn-v14", stochasticModels: [M0, MREF],
      capabilities: fixedWorkExactCapabilities,
      device: { backend: "cuda-resident-sim", ordinal: 0, name: "routing-fixture", computeCapability: [8, 6] },
    });
    const native = vi.spyOn(NativeGpuClient.prototype, "analyzeExact");
    await import("../src/background/index");
    const message = {
      type: DECISION_MESSAGE_TYPE, id: 45, state: {}, rootPlayer: "P0", engine: "deep-search",
      board: { initialPlacement: false, isMyTurn: true },
      stochastic: { model: M0 },
      strategyPolicy: "adaptive-candidate-admission-v1",
    };
    const response = await new Promise<unknown>((resolve) => receive(message, {}, resolve));
    expect(response).toMatchObject({ analysis: {
      runtime: "background-wasm",
      runtimeReason: expect.stringMatching(/Strategy policy adaptive-candidate-admission-v1 remains on its CPU\/WASM owner/u),
    } });
    expect(status).not.toHaveBeenCalled();
    expect(native).not.toHaveBeenCalled();
    expect(analyze).toHaveBeenCalledOnce();
    expect(analyze.mock.calls[0]?.[0]).toMatchObject({
      strategyPolicy: "adaptive-candidate-admission-v1",
    });
  });

  it("routes baseline non-opening Deep MaxN to exact CUDA when the companion is production-capable", async () => {
    vi.resetModules();
    let receive: (message: unknown, sender: unknown, sendResponse: (response: unknown) => void) => unknown;
    vi.stubGlobal("chrome", { runtime: { onMessage: { addListener: (listener: typeof receive) => { receive = listener; } } } });
    const { NativeGpuClient } = await import("../src/background/native-gpu");
    const status = vi.spyOn(NativeGpuClient.prototype, "status").mockResolvedValue({
      runtime: "gpu-native", engineRevision: "deep-maxn-v14", stochasticModels: [M0, MREF],
      capabilities: productionExactCapabilities,
      device: { backend: "cuda-resident-sim", ordinal: 0, name: "routing-fixture", computeCapability: [8, 6] },
    });
    const native = vi.spyOn(NativeGpuClient.prototype, "analyzeExact").mockResolvedValue({ stochasticModel: M0 } as never);
    await import("../src/background/index");
    const response = await new Promise<unknown>((resolve) => receive({
      type: DECISION_MESSAGE_TYPE, id: 46, state: {}, rootPlayer: "P0", engine: "deep-search",
      board: { initialPlacement: false, isMyTurn: true }, stochastic: { model: M0 },
    }, {}, resolve));
    expect(response).toMatchObject({ analysis: { runtime: "background-gpu", runtimeReason: "Exact CUDA MaxN on routing-fixture" } });
    expect(status).toHaveBeenCalledOnce();
    expect(native).toHaveBeenCalledWith({ stochastic: { model: M0 } }, expect.any(Number), expect.any(AbortSignal));
    expect(analyze).toHaveBeenCalledOnce();
  });

  it("preserves Mref on WASM when the production companion disconnects", async () => {
    vi.resetModules();
    let receive: (message: unknown, sender: unknown, sendResponse: (response: unknown) => void) => unknown;
    vi.stubGlobal("chrome", { runtime: { onMessage: { addListener: (listener: typeof receive) => { receive = listener; } } } });
    const { NativeGpuClient } = await import("../src/background/native-gpu");
    const status = vi.spyOn(NativeGpuClient.prototype, "status").mockRejectedValue(new Error("Native host has exited."));
    const native = vi.spyOn(NativeGpuClient.prototype, "analyzeExact");
    const release = vi.spyOn(NativeGpuClient.prototype, "release").mockImplementation(() => undefined);
    await import("../src/background/index");
    const message = {
      type: DECISION_MESSAGE_TYPE, id: 42, state: {}, rootPlayer: "P0", engine: "deep-search",
      board: { initialPlacement: false, isMyTurn: true },
      stochastic: { model: MREF },
    };
    const response = await new Promise<unknown>((resolve) => receive(message, {}, resolve));
    expect(response).toMatchObject({ analysis: {
      requestedModel: MREF,
      runtime: "background-wasm",
      runtimeReason: expect.stringMatching(/Native GPU transport failed.*Mref preserved on CPU\/WASM Deep MaxN/u),
    } });
    expect(status).toHaveBeenCalledOnce();
    expect(native).not.toHaveBeenCalled();
    expect(release).toHaveBeenCalledOnce();
    expect(analyze).toHaveBeenCalledOnce();
    expect(analyze.mock.calls[0]?.[0]).toMatchObject({ stochastic: { model: MREF } });
    expect(analyze.mock.calls[0]?.[1]).toBeUndefined();
  });

  it("reports the production MaxN runtime instead of rollout GPU availability", async () => {
    vi.resetModules();
    let receive: (message: unknown, sender: unknown, sendResponse: (response: unknown) => void) => unknown;
    vi.stubGlobal("chrome", { runtime: { onMessage: { addListener: (listener: typeof receive) => { receive = listener; } } } });
    const { NativeGpuClient } = await import("../src/background/native-gpu");
    const status = vi.spyOn(NativeGpuClient.prototype, "status").mockResolvedValue({
      runtime: "gpu-native", engineRevision: "deep-maxn-v14", stochasticModels: [M0, MREF],
      capabilities: productionExactCapabilities,
      device: { backend: "cuda-resident-sim", ordinal: 0, name: "routing-fixture", computeCapability: [8, 6] },
    });
    const { warmDeepSearchEngine } = await import("../src/worker/deep-search");
    vi.mocked(warmDeepSearchEngine).mockResolvedValue({
      engineRevision: "deep-maxn-v14",
      initializationMs: 2,
    } as never);
    await import("../src/background/index");
    const response = await new Promise<unknown>((resolve) =>
      receive(
        { type: DECISION_STATUS_MESSAGE_TYPE, id: 43, engine: "deep-search" },
        {},
        resolve,
      ));
    expect(response).toMatchObject({
      id: 43,
      runtime: "background-gpu",
      engineRevision: "deep-maxn-v14",
      deviceName: "routing-fixture",
    });
    expect(status).toHaveBeenCalledOnce();
  });
});
