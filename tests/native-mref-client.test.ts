import { afterEach, describe, expect, it, vi } from "vitest";
import { NativeGpuClient } from "../src/background/native-gpu";
import type { DecisionMessage } from "../src/worker/protocol";

const mockHost = (stateSchemaVersion: number) => {
  const listeners: Array<(message: unknown) => void> = [];
  const postMessage = vi.fn((message: Record<string, unknown>) => {
    queueMicrotask(() => {
      const response = message.type === "hello"
        ? {
            id: message.id, runtime: "gpu-native", protocolVersion: 6,
            stateSchemaVersion, engineRevision: "deep-maxn-v12",
            device: { backend: "cuda-resident-sim", ordinal: 0, name: "test CUDA", computeCapability: [8, 6] },
          }
        : { id: message.id, response: { stochasticModel: "mref-colonist-linked-2024-v1" } };
      for (const listener of listeners) listener(response);
    });
  });
  const port = {
    postMessage, disconnect: vi.fn(),
    onMessage: { addListener: (listener: (message: unknown) => void) => listeners.push(listener) },
    onDisconnect: { addListener: vi.fn() },
  };
  vi.stubGlobal("chrome", { runtime: { connectNative: vi.fn(() => port), onMessage: { addListener: vi.fn() } } });
  return port;
};

afterEach(() => vi.unstubAllGlobals());

describe("Mref native compatibility boundary", () => {
  it("routes eligible Mref decisions to native but leaves opening and pondering on CPU", async () => {
    mockHost(4);
    const { shouldUseNativeGpu } = await import("../src/background/index");
    const message = {
      engine: "deep-search",
      stochastic: { model: "mref-colonist-linked-2024-v1" },
      board: { initialPlacement: false, isMyTurn: true },
    } as DecisionMessage;
    expect(shouldUseNativeGpu(message)).toBe(true);
    expect(shouldUseNativeGpu({ ...message, board: { ...message.board, initialPlacement: true } })).toBe(false);
    expect(shouldUseNativeGpu({ ...message, board: { ...message.board, isMyTurn: false } })).toBe(false);
  });

  it("rejects an old M0-only companion before sending an analysis", async () => {
    const port = mockHost(3);
    const client = new NativeGpuClient();
    try {
      await expect(client.status()).rejects.toThrow(/incompatible/);
      expect(port.postMessage).toHaveBeenCalledTimes(1);
      expect(port.postMessage.mock.calls[0]?.[0]).toMatchObject({ type: "hello", stateSchemaVersion: 4 });
    } finally { client.release(); }
  });

  it("sends Mref unchanged after the stateful-model handshake", async () => {
    const port = mockHost(4);
    const client = new NativeGpuClient();
    const request = { stochastic: { model: "mref-colonist-linked-2024-v1", playerMapping: ["You", "Rival"] } };
    try {
      await expect(client.analyze(request)).resolves.toMatchObject({ stochasticModel: "mref-colonist-linked-2024-v1" });
      expect(port.postMessage.mock.calls[1]?.[0]).toMatchObject({ type: "analyze", request });
    } finally { client.release(); }
  });
});
