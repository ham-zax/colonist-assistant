import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  DECISION_CANCEL_MESSAGE_TYPE,
  DECISION_MESSAGE_TYPE,
  DECISION_STATUS_MESSAGE_TYPE,
} from "../src/worker/protocol";
import { MREF_COLONIST_LINKED_2024_V1 as MREF } from "../src/core/dice-history";

const analyze = vi.hoisted(() => vi.fn(async (message: { stochastic?: { model?: string } }) => ({
  deepSearch: {},
  requestedModel: message.stochastic?.model,
})));
vi.mock("../src/worker/analyze", () => ({ analyzeDecisionRequest: analyze }));
const warm = vi.hoisted(() => vi.fn(async () => ({
  engineRevision: "deep-maxn-test",
  initializationMs: 7,
})));
vi.mock("../src/worker/deep-search", () => ({ warmDeepSearchEngine: warm }));

type Receive = (
  message: unknown,
  sender: unknown,
  sendResponse: (response: unknown) => void,
) => unknown;

const setupRouter = async () => {
  vi.resetModules();
  let receive!: Receive;
  vi.stubGlobal("chrome", {
    runtime: { onMessage: { addListener: (listener: Receive) => { receive = listener; } } },
  });
  await import("../src/background/index");
  const dispatch = (message: unknown, sender: unknown) =>
    new Promise<unknown>((resolveResponse) => receive(message, sender, resolveResponse));
  return { dispatch, receive };
};

const liveDecision = (id = 1, engine = "deep-search") => ({
  type: DECISION_MESSAGE_TYPE,
  id,
  state: {},
  rootPlayer: "P0",
  engine,
  board: { initialPlacement: false, isMyTurn: true },
  stochastic: { model: MREF },
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  analyze.mockClear();
  warm.mockClear();
});

describe("extension runs only on the packaged WASM engine", () => {
  it("declares no native messaging permission", () => {
    const manifest = JSON.parse(
      readFileSync(resolve(__dirname, "../static/manifest.json"), "utf8"),
    ) as { permissions?: string[] };
    expect(manifest.permissions).not.toContain("nativeMessaging");
  });

  it("reports background-wasm status for every engine", async () => {
    const { dispatch } = await setupRouter();
    for (const engine of ["deep-search", "weighted"]) {
      await expect(
        dispatch({ type: DECISION_STATUS_MESSAGE_TYPE, id: 1, engine }, {}),
      ).resolves.toEqual({
        id: 1,
        runtime: "background-wasm",
        engineRevision: "deep-maxn-test",
        initializationMs: 7,
      });
    }
    expect(warm).toHaveBeenCalledTimes(2);
  });

  it("routes live decisions to WASM and preserves the requested stochastic model", async () => {
    const { dispatch } = await setupRouter();
    await expect(dispatch(liveDecision(), { tab: { id: 1 } })).resolves.toMatchObject({
      analysis: { runtime: "background-wasm", requestedModel: MREF },
      execution: "background",
    });
    expect(analyze.mock.calls[0]?.[0]).toMatchObject({ stochastic: { model: MREF } });
  });

  it("cancels a stale decision and permits a fresh one with the same sender", async () => {
    const { dispatch, receive } = await setupRouter();
    const sender = { tab: { id: 1 }, documentId: "first", frameId: 0 };
    const pending = dispatch(liveDecision(9), sender);
    receive({ type: DECISION_CANCEL_MESSAGE_TYPE, id: 9 }, sender, vi.fn());
    await expect(pending).resolves.toMatchObject({
      id: 9,
      error: expect.stringMatching(/cancelled/u),
    });
    await expect(dispatch(liveDecision(10), sender)).resolves.toMatchObject({
      analysis: { runtime: "background-wasm" },
    });
  });

  it("isolates identical decision IDs from another tab, document, or frame", async () => {
    const { dispatch, receive } = await setupRouter();
    let complete!: () => void;
    analyze.mockImplementationOnce(() => new Promise((resolveAnalysis) => {
      complete = () => resolveAnalysis({ deepSearch: {}, requestedModel: MREF });
    }));
    const owner = { tab: { id: 1 }, documentId: "first", frameId: 0 };
    const first = dispatch(liveDecision(), owner);
    await vi.waitFor(() => expect(analyze).toHaveBeenCalledOnce());
    for (const other of [
      { tab: { id: 2 }, documentId: "second", frameId: 0 },
      { tab: { id: 1 }, documentId: "replacement", frameId: 0 },
      { tab: { id: 1 }, documentId: "first", frameId: 1 },
    ]) {
      receive({ type: DECISION_CANCEL_MESSAGE_TYPE, id: 1 }, other, vi.fn());
    }
    complete();
    await expect(first).resolves.toMatchObject({ analysis: { runtime: "background-wasm" } });
  });

  it("subtracts only background-local elapsed time from the remaining decision allowance", async () => {
    vi.resetModules();
    vi.stubGlobal("chrome", { runtime: { onMessage: { addListener: vi.fn() } } });
    vi.spyOn(performance, "now").mockReturnValue(350);
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
  });
});
