import { describe, expect, it } from "vitest";

import type { DecisionTrace } from "../src/core/decision-trace";
import { CompactGameBuilder } from "../src/core/llm-record";
import type { BoardSnapshot } from "../src/core/placement";
import type { ResourceVector } from "../src/core/resources";

const resources = (
  lumber: number,
  brick: number,
  wool: number,
  grain: number,
  ore: number,
): ResourceVector => ({ lumber, brick, wool, grain, ore });

const trace = (stateHash: string, finalAction: unknown): DecisionTrace => ({
  stateHash,
  recordedAt: 1_000,
  turn: 7,
  phase: "none",
  hand: [0, 0, 0, 0, 0],
  publicVictoryPoints: [2, 3, 4, 5],
  beliefParticleCount: 0,
  sourceWorldCount: 0,
  deepStatus: "complete",
  deepTimedOut: false,
  executedBeforeDeepResult: false,
  finalAction,
  finalActionSource: "mandatory",
});

const captureBase = {
  scope: "game-1",
  sessionId: "session-1",
  gameKey: "game-1",
  startedAt: 1_000,
  partialHistory: false,
  unmatchedCount: 0,
  assistant: {
    engine: "weighted",
    disablePlayerTrades: false,
    autopilot: false,
  },
  events: [],
};

const encodedDisplay = (stateHash: string, finalAction: unknown): string => {
  const builder = new CompactGameBuilder();
  const record = builder.apply(
    {
      ...captureBase,
      decisions: [trace(stateHash, finalAction)],
    },
    false,
  );
  const displayColumn = record.contracts.decisionColumns.indexOf("display");
  return String(record.decisions[0]?.[displayColumn]);
};

describe("compact LLM game record", () => {
  it("preserves execution-significant live action payloads", () => {
    expect(
      encodedDisplay("discard", {
        kind: "discard",
        cards: resources(1, 0, 2, 3, 0),
        confidence: 1,
      }),
    ).toContain("cards=1,0,2,3,0");

    const playerTrade = encodedDisplay("player-trade", {
      kind: "trade-builder",
      mode: "player",
      give: resources(0, 2, 0, 0, 0),
      receive: resources(0, 0, 0, 1, 0),
      recipients: ["Rival A", "Rival B"],
      confidence: 0.91,
    });
    expect(playerTrade).toContain("mode=player");
    expect(playerTrade).toContain("give=0,2,0,0,0");
    expect(playerTrade).toContain("get=0,0,0,1,0");
    expect(playerTrade).toContain("to=P0,P1");

    const bankTrade = encodedDisplay("bank-trade", {
      kind: "trade-builder",
      mode: "bank",
      give: resources(4, 0, 0, 0, 0),
      receive: resources(0, 0, 0, 0, 1),
      confidence: 0.9,
    });
    expect(bankTrade).toContain("mode=bank");
    expect(bankTrade).toContain("give=4,0,0,0,0");
    expect(bankTrade).toContain("get=0,0,0,0,1");

    const counter = encodedDisplay("counter", {
      kind: "trade",
      offerIndex: 2,
      tradeId: "trade-17",
      verdict: "counter",
      counterGive: resources(0, 1, 0, 0, 1),
      counterReceive: resources(0, 0, 0, 2, 0),
      existingGive: resources(1, 0, 0, 0, 0),
      existingReceive: resources(0, 0, 1, 0, 0),
      confidence: 0.73,
    });
    expect(counter).toContain("oi=2");
    expect(counter).toContain("tid=trade-17");
    expect(counter).toContain("cg=0,1,0,0,1");
    expect(counter).toContain("cr=0,0,0,2,0");
    expect(counter).toContain("eg=1,0,0,0,0");
    expect(counter).toContain("er=0,0,1,0,0");

    const board = encodedDisplay("board", {
      kind: "board",
      boardAction: "robber",
      targetId: "h:-1,0",
      point: { x: 123.5, y: 456.25 },
      followupPlayer: "Rival",
      confidence: 0.96,
    });
    expect(board).toContain("ba=robber");
    expect(board).toContain("t=h:-1,0");
    expect(board).toContain("pt=123.5,456.25");
    expect(board).toContain("fp=P0");

    const development = encodedDisplay("development", {
      kind: "development",
      card: "year-of-plenty",
      followupResources: ["grain", "ore"],
      confidence: 0.95,
    });
    expect(development).toContain("card=year-of-plenty");
    expect(development).toContain("fr=grain,ore");
  });

  it("marks a previously known board value unavailable instead of unchanged", () => {
    const builder = new CompactGameBuilder();
    const observed: BoardSnapshot = {
      hexes: [],
      vertices: [],
      edges: [],
      observedAt: 1_000,
      ownHand: resources(2, 1, 0, 3, 0),
    };
    builder.apply(
      {
        ...captureBase,
        decisions: [],
        board: observed,
      },
      false,
    );
    const record = builder.apply(
      {
        ...captureBase,
        decisions: [],
        board: {
          ...observed,
          observedAt: 1_001,
          ownHand: undefined,
        },
      },
      false,
    );
    const handColumn = record.contracts.frameColumns.indexOf("hand");
    expect(record.frames[1]?.[handColumn]).toBe("~");
  });
});
