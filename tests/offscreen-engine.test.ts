import { afterEach, describe, expect, it, vi } from "vitest";
import { OFFSCREEN_MESSAGE_TYPE, type OffscreenRequest, type EngineWorkerResponse } from "../src/offscreen/protocol";
type Receive = (message: OffscreenRequest, sender: unknown, respond: (response: unknown) => void) => unknown;
const setup = async (isolated = true) => {
  vi.resetModules();
  let receive!: Receive;
  let worker!: FakeWorker;
  class FakeWorker {
    onmessage?: (event: { data: EngineWorkerResponse }) => void;
    onerror?: (event: { message: string }) => void;
    onmessageerror?: () => void;
    postMessage = vi.fn();
    constructor() { worker = this; }
  }
  vi.stubGlobal("crossOriginIsolated", isolated);
  vi.stubGlobal("Worker", FakeWorker);
  vi.stubGlobal("chrome", { runtime: { id: "test", getURL: (path: string) => path, onMessage: { addListener: (callback: Receive) => { receive = callback; } } } });
  await import("../src/offscreen/index");
  const dispatch = (message: Omit<OffscreenRequest, "type">) => {
    const respond = vi.fn();
    receive({ ...message, type: OFFSCREEN_MESSAGE_TYPE } as OffscreenRequest, { id: "test" }, respond);
    return respond;
  };
  const ready = () => worker.onmessage?.({ data: { token: "ready", status: { threadCount: 4, engineRevision: "test", initializationMs: 3 } } });
  return { dispatch, ready, worker };
};
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
describe("offscreen engine lifecycle", () => {
  it("reports isolation failure without constructing a worker", async () => {
    const { dispatch, worker } = await setup(false);
    const respond = dispatch({ token: "status", operation: "status" });
    await vi.waitFor(() => expect(respond).toHaveBeenCalledWith(expect.objectContaining({ error: expect.stringContaining("not cross-origin isolated") })));
    expect(worker).toBeUndefined();
  });
  it("settles queued work once when pool initialization fails", async () => {
    const { dispatch, worker } = await setup();
    const result = dispatch({ token: "one", operation: "analyze", request: {} } as OffscreenRequest);
    worker.onerror?.({ message: "pool failed" });
    await vi.waitFor(() => expect(result).toHaveBeenCalledOnce());
    expect(result).toHaveBeenCalledWith({ token: "one", error: "pool failed" });
    expect(worker.postMessage).not.toHaveBeenCalled();
  });
  it("cancels work queued before readiness and never dispatches it", async () => {
    const { dispatch, ready, worker } = await setup();
    const result = dispatch({ token: "one", operation: "analyze", request: { timeBudgetMs: 2000 } } as OffscreenRequest);
    dispatch({ token: "one", operation: "cancel" });
    ready();
    await vi.waitFor(() => expect(result).toHaveBeenCalledWith({ token: "one", error: "Decision cancelled as stale" }));
    expect(worker.postMessage).not.toHaveBeenCalled();
  });
  it("suppresses an active stale result and serializes the next request", async () => {
    const { dispatch, ready, worker } = await setup();
    ready();
    const first = dispatch({ token: "one", operation: "analyze", request: { timeBudgetMs: 2000 } } as OffscreenRequest);
    const second = dispatch({ token: "two", operation: "analyze", request: { timeBudgetMs: 2000 } } as OffscreenRequest);
    await vi.waitFor(() => expect(worker.postMessage).toHaveBeenCalledOnce());
    dispatch({ token: "one", operation: "cancel" });
    expect(first).toHaveBeenCalledWith({ token: "one", error: "Decision cancelled as stale" });
    expect(worker.postMessage).toHaveBeenCalledOnce();
    worker.onmessage?.({ data: { token: "one", result: {} as never } });
    expect(first).toHaveBeenCalledOnce();
    expect(worker.postMessage).toHaveBeenCalledTimes(2);
    worker.onmessage?.({ data: { token: "two", result: {} as never } });
    expect(second).toHaveBeenCalledWith({ token: "two", result: {} });
  });
  it("fails readiness, active work, and queued work on worker failure", async () => {
    const { dispatch, ready, worker } = await setup();
    ready();
    const first = dispatch({ token: "one", operation: "analyze", request: {} } as OffscreenRequest);
    const second = dispatch({ token: "two", operation: "analyze", request: {} } as OffscreenRequest);
    await vi.waitFor(() => expect(worker.postMessage).toHaveBeenCalledOnce());
    worker.onerror?.({ message: "worker failed" });
    expect(first).toHaveBeenCalledWith({ token: "one", error: "worker failed" });
    expect(second).toHaveBeenCalledWith({ token: "two", error: "worker failed" });
    const status = dispatch({ token: "status", operation: "status" });
    await vi.waitFor(() => expect(status).toHaveBeenCalledWith({ token: "status", error: "worker failed" }));
  });
});
