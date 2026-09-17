import { afterEach, describe, expect, it, vi } from "vitest";
import {
  NATIVE_GPU_EXACT_ALGORITHM,
  NATIVE_GPU_HANDSHAKE_TIMEOUT_MS,
  NATIVE_GPU_ROLLOUT_ALGORITHM,
  NativeGpuClient,
  nativeGpuSupportsExactMaxn,
  nativeGpuSupportsProductionExactMaxn,
} from "../src/background/native-gpu";
import { M0_FAIR_IID_2D6_V1 as M0, MREF_COLONIST_LINKED_2024_V1 as MREF } from "../src/core/dice-history";

const clients: NativeGpuClient[] = [];
afterEach(() => {
  for (const client of clients.splice(0)) client.release();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

interface CompanionOptions {
  protocolVersion?: number;
  models?: string[];
  responseModel?: string;
  exactAvailable?: boolean;
  exactFixedWorkOnly?: boolean;
  includeCapabilities?: boolean;
}

const companion = ({
  protocolVersion = 7,
  models = [M0, MREF],
  responseModel = M0,
  exactAvailable = true,
      exactFixedWorkOnly = false,
  includeCapabilities = true,
}: CompanionOptions = {}) => {
  let receive: (message: unknown) => void = () => undefined;
  let disconnectHost: () => void = () => undefined;
  const disconnect = vi.fn();
  const capabilities = {
    algorithms: [
      NATIVE_GPU_ROLLOUT_ALGORITHM,
      ...(exactAvailable ? [NATIVE_GPU_EXACT_ALGORITHM] : []),
    ],
    exactMaxn: {
      algorithm: NATIVE_GPU_EXACT_ALGORITHM,
      available: exactAvailable,
      supportsDeadline: exactAvailable,
      supportsCancellation: exactAvailable,
      supportsOpening: false,
      fixedWorkParityOnly: exactFixedWorkOnly,
      ...(!exactAvailable ? { unavailableReason: "exact evaluator unavailable" } : {}),
    },
  };
  const postMessage = vi.fn((request: { id: number; type: string }) => {
    const reply =
      request.type === "hello"
        ? {
            id: request.id,
            runtime: "gpu-native",
            protocolVersion,
            stateSchemaVersion: 3,
            engineRevision: "deep-maxn-v14",
            stochasticModels: models,
            ...(includeCapabilities ? { capabilities } : {}),
            device: {
              backend: "cuda-resident-sim",
              ordinal: 0,
              name: "parity-fixture",
              computeCapability: [8, 6],
            },
          }
        : {
            id: request.id,
            response: {
              engineRevision: "deep-maxn-v14",
              algorithm:
                request.type === "analyze-exact"
                  ? NATIVE_GPU_EXACT_ALGORITHM
                  : NATIVE_GPU_ROLLOUT_ALGORITHM,
              stochasticModel: responseModel,
            },
          };
    queueMicrotask(() => receive(reply));
  });
  vi.stubGlobal("chrome", {
    runtime: {
      connectNative: () => ({
        postMessage,
        disconnect,
        onMessage: { addListener: (listener: typeof receive) => { receive = listener; } },
        onDisconnect: { addListener: (listener: typeof disconnectHost) => { disconnectHost = listener; } },
      }),
    },
  });
  const client = new NativeGpuClient();
  clients.push(client);
  return { client, postMessage, disconnect, disconnectHost: () => disconnectHost() };
};

describe("native Mref capability and returned authority", () => {
  it("bounds a silent hello and leaves later decisions free to use WASM", async () => {
    vi.useFakeTimers();
    const { client, postMessage, disconnect } = companion();
    postMessage.mockImplementationOnce(() => {});
    const first = client.status();
    const second = client.status();
    await vi.advanceTimersByTimeAsync(NATIVE_GPU_HANDSHAKE_TIMEOUT_MS);
    await expect(first).resolves.toBeUndefined();
    await expect(second).resolves.toBeUndefined();
    await expect(client.status()).resolves.toBeUndefined();
    expect(postMessage).toHaveBeenCalledOnce();
    expect(disconnect).toHaveBeenCalledOnce();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("closes a silent reconnect after a previously healthy companion", async () => {
    vi.useFakeTimers();
    const { client, postMessage, disconnect, disconnectHost } = companion();
    await client.status();
    disconnectHost();
    postMessage.mockImplementationOnce(() => {});
    const reconnect = expect(client.status()).rejects.toThrow(/handshake timed out/u);
    await vi.advanceTimersByTimeAsync(NATIVE_GPU_HANDSHAKE_TIMEOUT_MS);
    await reconnect;
    expect(disconnect).toHaveBeenCalledOnce();
    await expect(client.status()).resolves.toMatchObject({ runtime: "gpu-native" });
  });

  it("does not let a released handshake poison its replacement", async () => {
    vi.useFakeTimers();
    const { client, postMessage } = companion();
    postMessage.mockImplementationOnce(() => {});
    const released = expect(client.status()).rejects.toThrow(/released/u);
    client.release();
    const replacement = await client.status();
    await released;
    await expect(client.status()).resolves.toBe(replacement);
    expect(postMessage).toHaveBeenCalledTimes(2);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("honors cancellation that arrives while the hello is pending", async () => {
    vi.useFakeTimers();
    const { client, postMessage } = companion();
    const reply = postMessage.getMockImplementation()!;
    postMessage.mockImplementationOnce(() => {});
    const controller = new AbortController();
    const cancelled = expect(client.analyzeExact({}, 1, controller.signal)).rejects.toThrow(/stale/u);
    controller.abort(new Error("Decision cancelled as stale"));
    reply({ type: "hello", id: 1 });
    await cancelled;
    expect(postMessage).toHaveBeenCalledOnce();
  });

  it("does not publish readiness if the port closes just after the hello reply", async () => {
    const { client, postMessage, disconnectHost } = companion();
    const reply = postMessage.getMockImplementation()!;
    postMessage.mockImplementationOnce((request) => {
      reply(request);
      queueMicrotask(disconnectHost);
    });
    await expect(client.status()).resolves.toBeUndefined();
    await expect(client.status()).resolves.toBeUndefined();
  });

  it("caches a missing companion without reconnecting on every decision", async () => {
    const { client, postMessage, disconnectHost } = companion();
    postMessage.mockImplementationOnce(() => queueMicrotask(disconnectHost));
    await expect(client.status()).resolves.toBeUndefined();
    await expect(client.status()).resolves.toBeUndefined();
    expect(postMessage).toHaveBeenCalledOnce();
  });

  it("rejects protocol-6 companions instead of guessing algorithm capability", async () => {
    const { client } = companion({ protocolVersion: 6 });
    await expect(client.status()).rejects.toThrow(/incompatible/u);
  });

  it("requires protocol-7 algorithm capability evidence", async () => {
    const { client } = companion({ includeCapabilities: false });
    await expect(client.status()).rejects.toThrow(/algorithm capabilities/u);
  });

  it("promotes deadline- and cancellation-capable exact MaxN", async () => {
    const { client } = companion();
    const status = await client.status();
    expect(status).toBeDefined();
    expect(nativeGpuSupportsExactMaxn(status!)).toBe(true);
    expect(nativeGpuSupportsProductionExactMaxn(status!)).toBe(true);
  });

  it("keeps a parity-only companion out of production routing", async () => {
    const { client } = companion({ exactFixedWorkOnly: true });
    const status = await client.status();
    expect(status).toBeDefined();
    expect(nativeGpuSupportsExactMaxn(status!)).toBe(true);
    expect(nativeGpuSupportsProductionExactMaxn(status!)).toBe(false);
  });

  it("executes Mref on exact MaxN only when it is advertised and returned as Mref", async () => {
    const { client, postMessage } = companion({ responseModel: MREF });
    await expect(
      client.analyzeExact({ stochastic: { model: MREF } }),
    ).resolves.toMatchObject({
      stochasticModel: MREF,
      algorithm: NATIVE_GPU_EXACT_ALGORITHM,
    });
    expect(postMessage.mock.calls.map(([message]) => message.type)).toEqual([
      "hello",
      "analyze-exact",
    ]);
  });

  it.each([undefined, M0])(
    "rejects missing or downgraded returned Mref identity (%s)",
    async (responseModel) => {
      const { client } = companion({
        responseModel: responseModel ?? "",
      });
      await expect(
        client.analyzeExact({ stochastic: { model: MREF } }),
      ).rejects.toThrow(/mismatched Mref/u);
    },
  );

  it("does not trust an advertised but unsupported stochastic model", async () => {
    const { client, postMessage } = companion({
      models: [M0, "future-model"],
      responseModel: "future-model",
    });
    await expect(
      client.analyzeExact({ stochastic: { model: "future-model" } }),
    ).rejects.toThrow(/does not support/u);
    expect(postMessage).toHaveBeenCalledTimes(1);
  });

  it("keeps rollout strategy-policy rejection separate from exact MaxN", async () => {
    const { client, postMessage } = companion({ responseModel: M0 });
    await expect(
      client.analyzeRollout({ strategyPolicy: "adaptive-candidate-admission-v1" }),
    ).rejects.toThrow(/GPU rollout does not support strategy policy/u);
    expect(postMessage).not.toHaveBeenCalled();

    await expect(
      client.analyzeExact({
        strategyPolicy: "adaptive-candidate-admission-v1",
        stochastic: { model: M0 },
      }),
    ).resolves.toMatchObject({ algorithm: NATIVE_GPU_EXACT_ALGORITHM });
    expect(postMessage.mock.calls.map(([message]) => message.type)).toEqual([
      "hello",
      "analyze-exact",
    ]);
  });

  it("rejects exact requests when the host exposes rollout only", async () => {
    const { client, postMessage } = companion({ exactAvailable: false });
    await expect(
      client.analyzeExact({ stochastic: { model: M0 } }),
    ).rejects.toThrow(/exact evaluator unavailable/u);
    expect(postMessage.mock.calls.map(([message]) => message.type)).toEqual(["hello"]);
  });
});
