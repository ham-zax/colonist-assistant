import { afterEach, describe, expect, it, vi } from "vitest";

import type { DecisionAnalysis } from "../src/core/engine";
import type { BoardSnapshot } from "../src/core/placement";
import type { TrackerState } from "../src/core/types";
import { DecisionWorkerClient } from "../src/content/decision-worker";
import { EXTENSION_CONTEXT_RELOAD_MESSAGE } from "../src/content/extension-context";

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("decision service client", () => {
  it("warms the packaged WASM service before the first board decision", async () => {
    const sendMessage = vi.fn(async (message: { id: number }) => ({
      id: message.id,
      runtime: "background-wasm",
      engineRevision: "test-revision",
      initializationMs: 14,
    }));
    vi.stubGlobal("chrome", { runtime: { sendMessage } });
    const callback = vi.fn();
    const client = new DecisionWorkerClient();

    client.warm(callback);

    await vi.waitFor(() =>
      expect(callback).toHaveBeenCalledWith({
        runtime: "background-wasm",
        detail: "test-revision ready",
        initializationMs: 14,
      }),
    );
    expect(sendMessage).toHaveBeenCalledOnce();
    client.destroy();
  });

  it("uses extension messaging without constructing a page-origin Worker", async () => {
    const analysis: DecisionAnalysis = {
      engine: "deep-search",
      players: [],
      actionScores: {
        road: 0,
        settlement: 0,
        city: 0,
        development: 0,
      },
      simulations: 1,
      model: "test",
    };
    const sendMessage = vi.fn(async (message: { id: number }) => ({
      id: message.id,
      analysis,
    }));
    vi.stubGlobal("chrome", { runtime: { sendMessage } });
    vi.stubGlobal(
      "Worker",
      class {
        constructor() {
          throw new Error("A page-origin worker must never be constructed");
        }
      },
    );
    const callback = vi.fn();
    const client = new DecisionWorkerClient();
    client.request(
      "position-1",
      {} as TrackerState,
      {} as BoardSnapshot,
      "You",
      "deep-search",
      callback,
    );

    await vi.waitFor(() =>
      expect(callback).toHaveBeenCalledWith({
        ...analysis,
        runtime: "background-rollout",
      }),
    );
    expect(sendMessage).toHaveBeenCalledOnce();
    client.destroy();
  });

  it("sends the player-trade legality setting to the background decision service", async () => {
    const analysis: DecisionAnalysis = {
      engine: "deep-search",
      players: [],
      actionScores: {
        road: 0,
        settlement: 0,
        city: 0,
        development: 0,
      },
      simulations: 1,
      model: "test",
    };
    const sendMessage = vi.fn(async (message: { id: number }) => ({
      id: message.id,
      analysis,
    }));
    vi.stubGlobal("chrome", { runtime: { sendMessage } });
    const client = new DecisionWorkerClient();

    client.request(
      "player-trades-disabled",
      {} as TrackerState,
      {} as BoardSnapshot,
      "You",
      "deep-search",
      vi.fn(),
      undefined,
      undefined,
      undefined,
      false,
    );

    await vi.waitFor(() => expect(sendMessage).toHaveBeenCalledOnce());
    expect(sendMessage).toHaveBeenCalledWith(
      expect.objectContaining({
        type: "colonist-assistant:decision",
        playerTradesEnabled: false,
      }),
    );
    client.destroy();
  });

  it("does not block a position on the independent warm-up status request", async () => {
    const analysis: DecisionAnalysis = {
      engine: "deep-search",
      players: [],
      actionScores: {
        road: 0,
        settlement: 0,
        city: 0,
        development: 0,
      },
      simulations: 1,
      model: "warm-independent",
    };
    const sendMessage = vi.fn(
      (message: { id: number; type: string }) => {
        if (message.type === "colonist-assistant:decision-status") {
          return new Promise(() => undefined);
        }
        return Promise.resolve({ id: message.id, analysis });
      },
    );
    vi.stubGlobal("chrome", { runtime: { sendMessage } });
    const callback = vi.fn();
    const client = new DecisionWorkerClient();

    client.warm(vi.fn());
    client.request(
      "position-during-warmup",
      {} as TrackerState,
      {} as BoardSnapshot,
      "You",
      "deep-search",
      callback,
    );

    await vi.waitFor(() => expect(callback).toHaveBeenCalledOnce());
    expect(sendMessage).toHaveBeenCalledTimes(2);
    client.destroy();
  });

  it("reports a background failure without substituting another algorithm", async () => {
    const sendMessage = vi.fn(async () => {
      throw new Error("Extension context invalidated");
    });
    vi.stubGlobal("chrome", { runtime: { sendMessage } });
    const callback = vi.fn();
    const failure = vi.fn();
    const client = new DecisionWorkerClient();
    client.request(
      "position-error",
      {} as TrackerState,
      {} as BoardSnapshot,
      "You",
      "deep-search",
      callback,
      undefined,
      failure,
    );

    await vi.waitFor(() =>
      expect(failure).toHaveBeenCalledWith(
        EXTENSION_CONTEXT_RELOAD_MESSAGE,
      ),
    );
    expect(callback).not.toHaveBeenCalled();
    expect(sendMessage).toHaveBeenCalledOnce();

    expect(
      client.request(
        "position-after-context-loss",
        {} as TrackerState,
        {} as BoardSnapshot,
        "You",
        "deep-search",
        callback,
      ),
    ).toBe("context-invalidated");
    expect(sendMessage).toHaveBeenCalledOnce();
    client.destroy();
  });

  it("keeps the selected request alive after the one-second warning", async () => {
    vi.useFakeTimers();
    const analysis: DecisionAnalysis = {
      engine: "deep-search",
      players: [],
      actionScores: {
        road: 0,
        settlement: 0,
        city: 0,
        development: 0,
      },
      simulations: 1,
      model: "eventual-wasm-result",
    };
    const sendMessage = vi.fn(
      (message: { id: number }) =>
        new Promise<{ id: number; analysis: DecisionAnalysis }>((resolve) => {
          globalThis.setTimeout(
            () => resolve({ id: message.id, analysis }),
            1_500,
          );
        }),
    );
    vi.stubGlobal("chrome", { runtime: { sendMessage } });
    const warning = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    const callback = vi.fn();
    const slow = vi.fn();
    const failure = vi.fn();
    const client = new DecisionWorkerClient();

    client.request(
      "long-position",
      {} as TrackerState,
      {} as BoardSnapshot,
      "You",
      "deep-search",
      callback,
      slow,
      failure,
    );
    await vi.advanceTimersByTimeAsync(1_000);

    expect(slow).toHaveBeenCalledWith(1_000);
    expect(callback).not.toHaveBeenCalled();
    expect(failure).not.toHaveBeenCalled();
    expect(sendMessage).toHaveBeenCalledOnce();
    expect(warning).toHaveBeenCalledWith(
      expect.stringMatching(
        /^\[Colonist Assistant\] Decision still running \(deep-search, \d+ ms\)$/u,
      ),
      expect.objectContaining({
        engine: "deep-search",
        policy: "selected-engine-only",
        fallbackStarted: false,
      }),
    );

    await vi.advanceTimersByTimeAsync(500);
    expect(callback).toHaveBeenCalledWith(
      expect.objectContaining({
        model: "eventual-wasm-result",
        runtime: "background-rollout",
      }),
    );
    expect(sendMessage).toHaveBeenCalledOnce();
    client.destroy();
  });

  it("ends a silent transport at the hard limit without duplicating search", async () => {
    vi.useFakeTimers();
    const analysis: DecisionAnalysis = {
      engine: "deep-search",
      players: [],
      actionScores: {
        road: 0,
        settlement: 0,
        city: 0,
        development: 0,
      },
      simulations: 1,
      model: "next-position-result",
    };
    const sendMessage = vi.fn((message: { id: number }) => {
      if (sendMessage.mock.calls.length === 1) {
        return new Promise(() => undefined);
      }
      return Promise.resolve({ id: message.id, analysis });
    });
    vi.stubGlobal("chrome", { runtime: { sendMessage } });
    const callback = vi.fn();
    const failure = vi.fn();
    const client = new DecisionWorkerClient();

    client.request(
      "lost-transport-position",
      {} as TrackerState,
      {} as BoardSnapshot,
      "You",
      "deep-search",
      callback,
      undefined,
      failure,
    );
    await vi.advanceTimersByTimeAsync(12_000);
    await Promise.resolve();
    await Promise.resolve();

    expect(sendMessage).toHaveBeenCalledOnce();
    expect(callback).not.toHaveBeenCalled();
    expect(failure).toHaveBeenCalledWith(
      "Strategist did not return before the 12-second safety limit",
    );

    const nextCallback = vi.fn();
    client.request(
      "next-position",
      {} as TrackerState,
      {} as BoardSnapshot,
      "You",
      "deep-search",
      nextCallback,
    );
    await vi.advanceTimersByTimeAsync(0);
    await Promise.resolve();

    expect(sendMessage).toHaveBeenCalledTimes(2);
    expect(sendMessage.mock.calls[1]?.[0]).toEqual(
      expect.objectContaining({ engine: "deep-search", id: 2 }),
    );
    expect(nextCallback).toHaveBeenCalledWith(
      expect.objectContaining({
        model: "next-position-result",
        runtime: "background-rollout",
      }),
    );
    client.destroy();
  });

  it("ignores a stale deep result after a newer position is requested", async () => {
    const resolvers: Array<
      (value: {
        id: number;
        analysis: DecisionAnalysis;
      }) => void
    > = [];
    const sendMessage = vi.fn(
      (message: { id: number }) =>
        new Promise<{
          id: number;
          analysis: DecisionAnalysis;
        }>((resolve) => {
          resolvers.push(resolve);
        }),
    );
    vi.stubGlobal("chrome", { runtime: { sendMessage } });
    const firstCallback = vi.fn();
    const secondCallback = vi.fn();
    const client = new DecisionWorkerClient();
    const analysis = (model: string): DecisionAnalysis => ({
      engine: "deep-search",
      players: [],
      actionScores: {
        road: 0,
        settlement: 0,
        city: 0,
        development: 0,
      },
      simulations: 1,
      model,
    });

    client.request(
      "old-position",
      {} as TrackerState,
      {} as BoardSnapshot,
      "You",
      "deep-search",
      firstCallback,
    );
    client.request(
      "new-position",
      {} as TrackerState,
      {} as BoardSnapshot,
      "You",
      "deep-search",
      secondCallback,
    );

    resolvers[0]?.({ id: 1, analysis: analysis("stale") });
    await vi.waitFor(() => expect(sendMessage).toHaveBeenCalledTimes(2));
    expect(firstCallback).not.toHaveBeenCalled();

    resolvers[1]?.({ id: 2, analysis: analysis("current") });
    await vi.waitFor(() =>
      expect(secondCallback).toHaveBeenCalledWith(
        expect.objectContaining({
          model: "current",
          runtime: "background-rollout",
        }),
      ),
    );
    expect(firstCallback).not.toHaveBeenCalled();
    client.destroy();
  });

  it("invalidates an in-flight result when the session resets", async () => {
    let resolveRequest:
      | ((value: { id: number; analysis: DecisionAnalysis }) => void)
      | undefined;
    const sendMessage = vi.fn(
      (message: { id: number }) =>
        new Promise<{ id: number; analysis: DecisionAnalysis }>((resolve) => {
          resolveRequest = resolve;
        }),
    );
    vi.stubGlobal("chrome", { runtime: { sendMessage } });
    const callback = vi.fn();
    const client = new DecisionWorkerClient();
    client.request(
      "old-session",
      {} as TrackerState,
      {} as BoardSnapshot,
      "You",
      "deep-search",
      callback,
    );
    client.reset();
    resolveRequest?.({
      id: 1,
      analysis: {
        engine: "deep-search",
        players: [],
        actionScores: {
          road: 0,
          settlement: 0,
          city: 0,
          development: 0,
        },
        simulations: 1,
        model: "invalidated",
      },
    });
    await Promise.resolve();
    await Promise.resolve();
    expect(callback).not.toHaveBeenCalled();
    client.destroy();
  });

  it("prints structured diagnostics when a decision exceeds one second", async () => {
    vi.useFakeTimers();
    const analysis: DecisionAnalysis = {
      engine: "deep-search",
      players: [],
      actionScores: {
        road: 0,
        settlement: 0,
        city: 0,
        development: 0,
      },
      simulations: 1,
      model: "slow-test",
    };
    const sendMessage = vi.fn(
      (message: { id: number }) =>
        new Promise<{ id: number; analysis: DecisionAnalysis }>((resolve) => {
          globalThis.setTimeout(
            () => resolve({ id: message.id, analysis }),
            1_100,
          );
        }),
    );
    vi.stubGlobal("chrome", { runtime: { sendMessage } });
    const warning = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    const callback = vi.fn();
    const client = new DecisionWorkerClient();

    client.request(
      "slow-position",
      {} as TrackerState,
      {
        hexes: [],
        vertices: [],
        edges: [],
        gameKey: "test-game",
        turn: 7,
        isMyTurn: true,
        action: "none",
      },
      "You",
      "deep-search",
      callback,
    );
    await vi.advanceTimersByTimeAsync(1_100);

    expect(warning).toHaveBeenCalledWith(
      expect.stringMatching(
        /^\[Colonist Assistant\] Slow decision \(deep-search, \d+ ms total\)$/u,
      ),
      expect.objectContaining({
        key: "slow-position",
        engine: "deep-search",
        totalMs: 1_100,
        serviceMs: 1_100,
        gameKey: "test-game",
        turn: 7,
      }),
    );
    expect(callback).toHaveBeenCalledOnce();
    client.destroy();
  });
});
