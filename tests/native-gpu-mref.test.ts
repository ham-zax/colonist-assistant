import { afterEach, describe, expect, it, vi } from "vitest";
import { NativeGpuClient } from "../src/background/native-gpu";
import { M0_FAIR_IID_2D6_V1 as M0, MREF_COLONIST_LINKED_2024_V1 as MREF } from "../src/core/dice-history";

const clients: NativeGpuClient[] = [];
afterEach(() => {
  for (const client of clients.splice(0)) client.release();
  vi.unstubAllGlobals();
});

const companion = (models: string[] | undefined, responseModel: string | undefined) => {
  let receive: (message: unknown) => void = () => undefined;
  const postMessage = vi.fn((request: { id: number; type: string }) => {
    const reply = request.type === "hello" ? {
      id: request.id,
      runtime: "gpu-native",
      protocolVersion: 6,
      stateSchemaVersion: 3,
      engineRevision: "deep-maxn-v12",
      device: { backend: "cuda-resident-sim", ordinal: 0, name: "parity-fixture", computeCapability: [8, 6] },
      ...(models ? { stochasticModels: models } : {}),
    } : {
      id: request.id,
      response: { engineRevision: "deep-maxn-v12", ...(responseModel ? { stochasticModel: responseModel } : {}) },
    };
    queueMicrotask(() => receive(reply));
  });
  vi.stubGlobal("chrome", {
    runtime: {
      connectNative: () => ({
        postMessage,
        disconnect: vi.fn(),
        onMessage: { addListener: (listener: typeof receive) => { receive = listener; } },
        onDisconnect: { addListener: vi.fn() },
      }),
    },
  });
  const client = new NativeGpuClient();
  clients.push(client);
  return { client, postMessage };
};

describe("native Mref capability and returned authority", () => {
  it("keeps pre-Mref protocol-6 companions compatible with M0", async () => {
    const { client } = companion(undefined, undefined);
    expect((await client.status())?.stochasticModels).toEqual([M0]);
    await expect(client.analyze({ stochastic: { model: M0 } })).resolves.toMatchObject({ engineRevision: "deep-maxn-v12" });
  });

  it("never sends Mref to a legacy companion despite a compatible handshake", async () => {
    const { client, postMessage } = companion(undefined, undefined);
    await expect(client.analyze({ stochastic: { model: MREF } })).rejects.toThrow(/does not support/);
    expect(postMessage.mock.calls.map(([message]) => message.type)).toEqual(["hello"]);
  });

  it("executes Mref only when it is advertised and returned as Mref", async () => {
    const { client, postMessage } = companion([M0, MREF], MREF);
    await expect(client.analyze({ stochastic: { model: MREF } })).resolves.toMatchObject({ stochasticModel: MREF });
    expect(postMessage.mock.calls.map(([message]) => message.type)).toEqual(["hello", "analyze"]);
  });

  it.each([undefined, M0])("rejects missing or downgraded returned Mref identity (%s)", async (responseModel) => {
    const { client } = companion([M0, MREF], responseModel);
    await expect(client.analyze({ stochastic: { model: MREF } })).rejects.toThrow(/mismatched Mref/);
  });

  it("does not trust an advertised but unimplemented model", async () => {
    const { client, postMessage } = companion([M0, "future-model"], "future-model");
    await expect(client.analyze({ stochastic: { model: "future-model" } })).rejects.toThrow(/does not support/);
    expect(postMessage).toHaveBeenCalledTimes(1);
  });
});
