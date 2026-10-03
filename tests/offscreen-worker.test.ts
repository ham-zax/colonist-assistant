import { afterEach, describe, expect, it, vi } from "vitest";

const wasm = vi.hoisted(() => ({
  init: vi.fn(),
  analyze: vi.fn(),
  cancelWordAddress: vi.fn(() => 8),
  engineVersion: vi.fn(() => "test"),
  initThreadPool: vi.fn(async () => undefined),
}));
vi.mock("../src/generated/wasm-threads/colonist_search.js", () => ({
  default: wasm.init,
  analyze: wasm.analyze,
  cancel_word_address: wasm.cancelWordAddress,
  engine_version: wasm.engineVersion,
  initThreadPool: wasm.initThreadPool,
}));

const start = async (buffer: ArrayBuffer | SharedArrayBuffer, address = 8) => {
  vi.resetModules();
  wasm.init.mockResolvedValue({ memory: { buffer } });
  wasm.cancelWordAddress.mockReturnValue(address);
  const postMessage = vi.fn();
  vi.stubGlobal("crossOriginIsolated", true);
  vi.stubGlobal("postMessage", postMessage);
  vi.stubGlobal("self", { onmessage: undefined });
  await import("../src/offscreen/engine-worker");
  await vi.waitFor(() => expect(postMessage).toHaveBeenCalled());
  return postMessage;
};

afterEach(() => { vi.clearAllMocks(); vi.unstubAllGlobals(); });

describe("threaded engine worker cancellation memory", () => {
  it("advertises the actual shared cancellation word after initializing its pool", async () => {
    const buffer = new SharedArrayBuffer(16);
    const postMessage = await start(buffer);
    expect(wasm.initThreadPool).toHaveBeenCalledOnce();
    expect(postMessage).toHaveBeenCalledWith(expect.objectContaining({
      token: "ready", status: expect.objectContaining({ engineRevision: "test" }),
      cancelWord: { buffer, index: 2 },
    }));
  });

  it.each([
    [new ArrayBuffer(16), 8],
    [new SharedArrayBuffer(16), 3],
    [new SharedArrayBuffer(16), 16],
  ])("fails readiness for invalid cancellation memory %#", async (buffer, address) => {
    const postMessage = await start(buffer, address);
    expect(postMessage).toHaveBeenCalledWith({
      token: "ready", error: "Threaded WASM cancellation word is not in aligned shared memory",
    });
    expect(wasm.initThreadPool).not.toHaveBeenCalled();
  });
});
