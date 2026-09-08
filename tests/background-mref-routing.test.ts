import { afterEach, describe, expect, it, vi } from "vitest";
import {
  DECISION_MESSAGE_TYPE,
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

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  analyze.mockClear();
});

describe("background Mref dispatch", () => {
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
    if (scenario.native) expect(native).toHaveBeenCalledWith({ stochastic: { model: MREF } }, 41);
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
      runtimeReason: expect.stringMatching(/Strategy policy adaptive-candidate-admission-v1 remains on CPU\/WASM/u),
    } });
    expect(status).not.toHaveBeenCalled();
    expect(native).not.toHaveBeenCalled();
    expect(analyze).toHaveBeenCalledOnce();
    expect(analyze.mock.calls[0]?.[0]).toMatchObject({
      strategyPolicy: "adaptive-candidate-admission-v1",
    });
  });

  it("does not consult a disconnected rollout companion for production MaxN", async () => {
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
      runtimeReason: expect.stringMatching(/Mref preserved on CPU\/WASM Deep MaxN.*fixed-work parity backend/u),
    } });
    expect(status).not.toHaveBeenCalled();
    expect(native).not.toHaveBeenCalled();
    expect(release).not.toHaveBeenCalled();
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
      capabilities: fixedWorkExactCapabilities,
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
      runtime: "background-wasm",
      engineRevision: "deep-maxn-v14",
    });
    expect(status).not.toHaveBeenCalled();
  });
});
