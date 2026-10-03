import { afterEach, describe, expect, it, vi } from "vitest";
import { OFFSCREEN_MESSAGE_TYPE, subtractQueueBudget, type OffscreenRequest } from "../src/offscreen/protocol";
import { DECISION_MESSAGE_TYPE, DECISION_STATUS_MESSAGE_TYPE, DECISION_CANCEL_MESSAGE_TYPE } from "../src/worker/protocol";
const analyze = vi.hoisted(() => vi.fn(async (_request: unknown, executor?: (request: unknown) => Promise<unknown>) => {
  if (executor) await executor({ timeBudgetMs: 2000 });
  return { deepSearch: {} };
}));
const warm = vi.hoisted(() => vi.fn(async () => ({ engineRevision: "sequential", initializationMs: 1 })));
vi.mock("../src/worker/analyze", () => ({ analyzeDecisionRequest: analyze }));
vi.mock("../src/worker/deep-search", () => ({ warmDeepSearchEngine: warm }));
type Receiver = (message: unknown, sender: unknown, respond: (response: unknown) => void) => unknown;
const setup = async (send?: (message: OffscreenRequest) => Promise<unknown>) => {
  vi.resetModules();
  let receive!: Receiver;
  const createDocument = vi.fn(async () => undefined);
  const closeDocument = vi.fn(async () => undefined);
  const getContexts = vi.fn(async () => [] as unknown[]);
  vi.stubGlobal("chrome", {
    offscreen: { createDocument, closeDocument, Reason: { WORKERS: "WORKERS" } },
    runtime: {
      ContextType: { OFFSCREEN_DOCUMENT: "OFFSCREEN_DOCUMENT" },
      getURL: (path: string) => `chrome-extension://test/${path}`,
      getContexts,
      sendMessage: vi.fn(send ?? (async (message: OffscreenRequest) => ({ token: message.token, status: { engineRevision: "parallel", threadCount: 4, initializationMs: 10 }, result: {} }))),
      onMessage: { addListener: (listener: Receiver) => { receive = listener; } },
    },
  });
  await import("../src/background/index");
  return { createDocument, closeDocument, getContexts, receive, dispatch: (message: unknown, sender: unknown = {}) => new Promise<unknown>((resolve) => receive(message, sender, resolve)) };
};
const decision = (id = 1) => ({ type: DECISION_MESSAGE_TYPE, id, state: {}, board: {}, rootPlayer: "P0", engine: "deep-search" });
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); analyze.mockClear(); warm.mockClear(); });

describe("threaded background routing", () => {
  it("falls back for a new decision after the previously ready worker fails", async () => {
    let failed = false;
    const { dispatch } = await setup(async (message) => failed
      ? { token: message.token, error: "worker disappeared" }
      : { token: message.token, status: { engineRevision: "parallel", threadCount: 2, initializationMs: 1 } });
    await expect(dispatch({ type: DECISION_STATUS_MESSAGE_TYPE, id: 1 })).resolves.toMatchObject({ runtime: "offscreen-wasm-threads" });
    failed = true;
    await expect(dispatch(decision(2))).resolves.toMatchObject({ analysis: { runtime: "background-wasm" } });
    expect(analyze.mock.calls[0]?.[1]).toBeUndefined();
  });
  it("serializes concurrent document creation and reports the actual pool", async () => {
    const { dispatch, createDocument } = await setup();
    const results = await Promise.all([1, 2, 3].map((id) => dispatch({ type: DECISION_STATUS_MESSAGE_TYPE, id, engine: "deep-search" })));
    expect(createDocument).toHaveBeenCalledOnce();
    for (const result of results) expect(result).toMatchObject({ runtime: "offscreen-wasm-threads", threadCount: 4, engineRevision: "parallel" });
    expect(warm).not.toHaveBeenCalled();
  });
  it("uses single-thread WASM when offscreen setup fails", async () => {
    const { dispatch, createDocument } = await setup();
    createDocument.mockRejectedValueOnce(new Error("offscreen unavailable"));
    await expect(dispatch(decision())).resolves.toMatchObject({ analysis: { runtime: "background-wasm" } });
    expect(analyze.mock.calls[0]?.[1]).toBeUndefined();
    await expect(dispatch({ type: DECISION_STATUS_MESSAGE_TYPE, id: 2 })).resolves.toMatchObject({ runtime: "background-wasm", runtimeReason: expect.stringContaining("offscreen unavailable") });
  });
  it("closes a newly created document when pool setup fails", async () => {
    const { dispatch, closeDocument } = await setup(async (message) => ({ token: message.token, error: "pool initialization failed" }));
    await expect(dispatch({ type: DECISION_STATUS_MESSAGE_TYPE, id: 1 })).resolves.toMatchObject({ runtime: "background-wasm" });
    expect(closeDocument).toHaveBeenCalledOnce();
  });
  it("closes a document whose creation finishes after the setup timeout", async () => {
    vi.useFakeTimers();
    const { dispatch, createDocument, closeDocument } = await setup();
    let completeCreate!: () => void;
    createDocument.mockImplementationOnce(() => new Promise<undefined>((resolve) => { completeCreate = () => resolve(undefined); }));
    const response = dispatch({ type: DECISION_STATUS_MESSAGE_TYPE, id: 1 });
    await vi.advanceTimersByTimeAsync(5_000);
    await expect(response).resolves.toMatchObject({ runtime: "background-wasm", threadCount: 1, runtimeReason: expect.stringContaining("timed out") });
    expect(closeDocument).not.toHaveBeenCalled();
    completeCreate();
    await vi.advanceTimersByTimeAsync(0);
    expect(closeDocument).toHaveBeenCalledOnce();
    expect(chrome.runtime.sendMessage).not.toHaveBeenCalled();
  });
  it("does not close an inherited offscreen document after a transport failure", async () => {
    const { dispatch, createDocument, closeDocument, getContexts } = await setup(async () => { throw new Error("transport unavailable"); });
    getContexts.mockResolvedValueOnce([{}]);
    await dispatch({ type: DECISION_STATUS_MESSAGE_TYPE, id: 1 });
    expect(createDocument).not.toHaveBeenCalled();
    expect(closeDocument).not.toHaveBeenCalled();
  });
  it("closes an inherited document when it reports a terminal worker failure", async () => {
    const { dispatch, createDocument, closeDocument, getContexts } = await setup(async (message) => ({ token: message.token, error: "pool failed" }));
    getContexts.mockResolvedValueOnce([{}]);
    await dispatch({ type: DECISION_STATUS_MESSAGE_TYPE, id: 1 });
    expect(createDocument).not.toHaveBeenCalled();
    expect(closeDocument).toHaveBeenCalledOnce();
  });
  it("keeps an active search alive when another tab's readiness probe times out", async () => {
    vi.useFakeTimers();
    let failProbe = false;
    let completeAnalysis!: (response: unknown) => void;
    let analysisToken = "";
    const { dispatch, closeDocument } = await setup(async (message) => {
      if (message.operation === "analyze") {
        analysisToken = message.token;
        return new Promise((resolve) => { completeAnalysis = resolve; });
      }
      if (message.operation === "status" && failProbe) return new Promise(() => undefined);
      return { token: message.token, status: { engineRevision: "parallel", threadCount: 2, initializationMs: 1 } };
    });
    const first = dispatch(decision(1), { tab: { id: 1 } });
    await vi.advanceTimersByTimeAsync(0);
    expect(analysisToken).not.toBe("");
    failProbe = true;
    const probe = dispatch({ type: DECISION_STATUS_MESSAGE_TYPE, id: 2 }, { tab: { id: 2 } });
    await vi.advanceTimersByTimeAsync(5_000);
    await expect(probe).resolves.toMatchObject({ runtime: "background-wasm" });
    expect(closeDocument).not.toHaveBeenCalled();
    completeAnalysis({ token: analysisToken, result: {} });
    await expect(first).resolves.toMatchObject({ analysis: { runtime: "offscreen-wasm-threads" } });
  });
  it("closes a document that fails readiness and retries the pool after the cooldown", async () => {
    let failed = false;
    const now = vi.spyOn(Date, "now").mockReturnValue(1_000);
    const { dispatch, createDocument, closeDocument, getContexts } = await setup(async (message) => failed
      ? { token: message.token, error: "worker disappeared" }
      : { token: message.token, status: { engineRevision: "parallel", threadCount: 2, initializationMs: 1 } });
    await dispatch({ type: DECISION_STATUS_MESSAGE_TYPE, id: 1 });
    failed = true;
    await expect(dispatch({ type: DECISION_STATUS_MESSAGE_TYPE, id: 2 })).resolves.toMatchObject({ runtime: "background-wasm" });
    expect(closeDocument).toHaveBeenCalledOnce();
    failed = false;
    await expect(dispatch({ type: DECISION_STATUS_MESSAGE_TYPE, id: 3 })).resolves.toMatchObject({ runtime: "background-wasm" });
    expect(createDocument).toHaveBeenCalledOnce();
    const { THREADED_RETRY_COOLDOWN_MS } = await import("../src/background/threaded-engine");
    now.mockReturnValue(1_000 + THREADED_RETRY_COOLDOWN_MS);
    getContexts.mockResolvedValue([]);
    await expect(dispatch({ type: DECISION_STATUS_MESSAGE_TYPE, id: 4 })).resolves.toMatchObject({ runtime: "offscreen-wasm-threads" });
    expect(createDocument).toHaveBeenCalledTimes(2);
  });
  it("routes decisions through the pool without starting a fallback after execution failure", async () => {
    const { dispatch } = await setup(async (message) => message.operation === "analyze"
      ? { token: message.token, error: "WASM execution failed" }
      : { token: message.token, status: { engineRevision: "parallel", threadCount: 2, initializationMs: 1 } });
    await expect(dispatch(decision())).resolves.toMatchObject({ error: "WASM execution failed" });
    expect(analyze).toHaveBeenCalledOnce();
    expect(warm).not.toHaveBeenCalled();
  });
  it("isolates identical IDs across senders and suppresses a cancelled response", async () => {
    const pending: Array<{ token: string; resolve: (response: unknown) => void }> = [];
    const { dispatch, receive } = await setup(async (message) => {
      if (message.operation === "analyze") return new Promise((resolve) => pending.push({ token: message.token, resolve }));
      return { token: message.token, status: { engineRevision: "parallel", threadCount: 2, initializationMs: 1 } };
    });
    const firstSender = { tab: { id: 1 }, documentId: "one", frameId: 0 };
    const secondSender = { tab: { id: 2 }, documentId: "two", frameId: 0 };
    const first = dispatch(decision(), firstSender);
    const second = dispatch(decision(), secondSender);
    await vi.waitFor(() => expect(pending).toHaveLength(2));
    expect(pending[0]!.token).not.toBe(pending[1]!.token);
    receive({ type: DECISION_CANCEL_MESSAGE_TYPE, id: 1 }, firstSender, vi.fn());
    for (const item of pending) item.resolve({ token: item.token, result: {} });
    await expect(first).resolves.toMatchObject({ error: expect.stringContaining("cancelled") });
    await expect(second).resolves.toMatchObject({ analysis: { runtime: "offscreen-wasm-threads", runtimeThreadCount: 2 } });
    expect(chrome.runtime.sendMessage).toHaveBeenCalledWith(expect.objectContaining({ type: OFFSCREEN_MESSAGE_TYPE, operation: "cancel", token: pending[0]!.token }));
  });
});
describe("offscreen queue budgets", () => {
  it("keeps unlimited fixed work and fails expired live work", () => {
    expect(subtractQueueBudget({ timeBudgetMs: 0 }, 5000)).toEqual({ timeBudgetMs: 0 });
    expect(() => subtractQueueBudget({ timeBudgetMs: 100 }, 100)).toThrow(/expired/u);
    expect(subtractQueueBudget({ timeBudgetMs: 100, effort: { decisionTimeMs: 100 } }, 20)).toEqual({ timeBudgetMs: 80, effort: { decisionTimeMs: 80 } });
  });
});
