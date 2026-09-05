import { afterEach, describe, expect, it, vi } from "vitest";
import { DECISION_MESSAGE_TYPE } from "../src/worker/protocol";
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

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  analyze.mockClear();
});

describe("background Mref dispatch", () => {
  it.each([
    { name: "capable companion", models: [M0, MREF], opening: false, ownTurn: true, native: true },
    { name: "legacy companion", models: [M0], opening: false, ownTurn: true, native: false },
    { name: "opening", models: [M0, MREF], opening: true, ownTurn: true, native: false },
    { name: "opponent pondering", models: [M0, MREF], opening: false, ownTurn: false, native: false },
  ])("preserves Mref through $name", async (scenario) => {
    vi.resetModules();
    let receive: (message: unknown, sender: unknown, sendResponse: (response: unknown) => void) => unknown;
    vi.stubGlobal("chrome", { runtime: { onMessage: { addListener: (listener: typeof receive) => { receive = listener; } } } });
    const { NativeGpuClient } = await import("../src/background/native-gpu");
    vi.spyOn(NativeGpuClient.prototype, "status").mockResolvedValue({
      runtime: "gpu-native", engineRevision: "deep-maxn-v12", stochasticModels: scenario.models,
      device: { backend: "cuda-resident-sim", ordinal: 0, name: "routing-fixture", computeCapability: [8, 6] },
    });
    const native = vi.spyOn(NativeGpuClient.prototype, "analyze").mockResolvedValue({ stochasticModel: MREF } as never);
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
});
