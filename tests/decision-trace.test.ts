import { afterEach, describe, expect, it, vi } from "vitest";

import { DecisionTraceRecorder } from "../src/core/decision-trace";
import type { DecisionAnalysis } from "../src/core/engine";
import type { BoardSnapshot } from "../src/core/placement";
import { emptyResources } from "../src/core/resources";
import { createTrackerState, reduceTracker } from "../src/core/tracker";

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const board = (): BoardSnapshot => ({
  hexes: [],
  vertices: [],
  edges: [],
  myPlayer: "You",
  playerOrder: ["Rival", "You", "Third"],
  players: {
    Rival: {
      handSize: 0,
      tradeRatios: emptyResources(),
      cardDiscardLimit: 7,
      visiblePoints: 6,
    },
    You: {
      handSize: 4,
      tradeRatios: emptyResources(),
      cardDiscardLimit: 7,
      visiblePoints: 4,
    },
    Third: {
      handSize: 0,
      tradeRatios: emptyResources(),
      cardDiscardLimit: 7,
      visiblePoints: 5,
    },
  },
  ownHand: {
    lumber: 1,
    brick: 1,
    wool: 1,
    grain: 1,
    ore: 0,
  },
  action: "none",
  isMyTurn: true,
  turn: 17,
});

describe("decision trace recorder", () => {
  it("records the actual root player's candidate value and execution source", async () => {
    vi.useFakeTimers();
    const set = vi.fn(async (_value: Record<string, unknown>) => undefined);
    vi.stubGlobal("chrome", {
      storage: {
        local: {
          set,
          get: vi.fn(async () => ({})),
        },
      },
    });
    let state = createTrackerState();
    for (const player of ["Rival", "You", "Third"]) {
      state = reduceTracker(state, { type: "discover", player });
    }
    const recorder = new DecisionTraceRecorder();
    recorder.begin("state-17", state, board(), 100);
    const analysis: DecisionAnalysis = {
      engine: "deep-puct",
      players: [
        {
          player: "Rival",
          probability: 0.5,
          etaTurns: 4,
          samples: 1,
          confidence: "low",
          reasons: [],
        },
        {
          player: "You",
          probability: 0.3,
          etaTurns: 6,
          samples: 1,
          confidence: "low",
          reasons: [],
        },
        {
          player: "Third",
          probability: 0.2,
          etaTurns: 7,
          samples: 1,
          confidence: "low",
          reasons: [],
        },
      ],
      actionScores: {
        road: 0,
        settlement: 0,
        city: 0,
        development: 0,
      },
      simulations: 12,
      model: "test",
      runtime: "background-wasm",
      deepSearch: {
        engineRevision: "trace-test",
        algorithm: "puct",
        chosen: { kind: "build-city", targetId: "v1" },
        rootValue: [0.5, 0.3, 0.2],
        tacticalWinProbability: 0,
        tacticalLowerBound: 0,
        tacticalProven: false,
        exactDecision: false,
        exactWorlds: 3,
        tacticalLine: [],
        actions: [
          {
            action: { kind: "build-city", targetId: "v1" },
            visits: 12,
            availability: 12,
            availabilityWeight: 1,
            legalWeight: 1,
            prior: 0.5,
            value: [0.61, 0.27, 0.12],
            lowerConfidenceValue: [0.5, 0.2, 0.08],
          },
        ],
        iterations: 12,
        nodes: 30,
        deepestDecisionDepth: 3,
        rollouts: 12,
        particles: 3,
        effectiveParticleCount: 2.8,
        elapsedMs: 4,
        seed: 17,
      },
    };
    recorder.complete("state-17", analysis);
    recorder.final(
      "state-17",
      { kind: "build", build: "city" },
      "deep",
    );
    recorder.execution("state-17", true);
    await vi.advanceTimersByTimeAsync(250);

    const payload = set.mock.calls[0]?.[0] as
      | Record<string, unknown>
      | undefined;
    const traces = Object.values(payload ?? {})[0] as
      | Array<Record<string, unknown>>
      | undefined;
    expect(traces?.[0]).toMatchObject({
      stateHash: "state-17",
      turn: 17,
      deepTimedOut: false,
      finalActionSource: "deep",
      executedBeforeDeepResult: false,
      executionSucceeded: true,
    });
    expect(
      (traces?.[0]?.deepCandidates as Array<{ value: number }> | undefined)?.[0]
        ?.value,
    ).toBeCloseTo(0.27);
  });

  it("marks a fallback executed before deep completion as an override", async () => {
    vi.useFakeTimers();
    const set = vi.fn(async (_value: Record<string, unknown>) => undefined);
    vi.stubGlobal("chrome", {
      storage: {
        local: {
          set,
          get: vi.fn(async () => ({})),
        },
      },
    });
    let state = createTrackerState();
    state = reduceTracker(state, { type: "discover", player: "You" });
    state = reduceTracker(state, { type: "discover", player: "Rival" });
    const recorder = new DecisionTraceRecorder();
    recorder.begin("state-pending", state, {
      ...board(),
      playerOrder: ["You", "Rival"],
      players: {
        You: board().players!.You!,
        Rival: board().players!.Rival!,
      },
    });
    recorder.final(
      "state-pending",
      { kind: "turn-control", control: "end" },
      "end-turn-fallback",
    );
    await vi.advanceTimersByTimeAsync(250);
    const payload = set.mock.calls[0]?.[0] as
      | Record<string, unknown>
      | undefined;
    expect(payload).toBeDefined();
    const traces = Object.values(payload ?? {})[0] as Array<
      Record<string, unknown>
    >;
    expect(traces[0]).toMatchObject({
      executedBeforeDeepResult: true,
      finalActionSource: "end-turn-fallback",
    });
  });
});
