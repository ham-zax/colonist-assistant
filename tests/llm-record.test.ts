import { describe, expect, it } from "vitest";

import type { DecisionTrace } from "../src/core/decision-trace";
import {
  CompactGameBuilder,
  normalizeCompactRecordIntegrity,
} from "../src/core/llm-record";
import type { BoardSnapshot, DiceMode } from "../src/core/placement";
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

const diceBoard = (
  diceMode: DiceMode,
  observedAt: number,
  diceModeRaw?: number,
): BoardSnapshot => ({
  hexes: [],
  vertices: [],
  edges: [],
  diceMode,
  ...(diceModeRaw !== undefined ? { diceModeRaw } : {}),
  observedAt,
});

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
  it("records canonical dice mode and forensic unsupported raw evidence", () => {
    for (const [diceMode, raw] of [
      ["random", undefined],
      ["balanced", undefined],
      ["unsupported", 2],
    ] as const) {
      const builder = new CompactGameBuilder();
      const record = builder.apply(
        {
          ...captureBase,
          decisions: [],
          board: diceBoard(diceMode, 1_100, raw),
        },
        false,
      );
      expect(record.meta.diceMode).toBe(diceMode);
      expect(record.meta.diceModeRaw).toBe(raw);
    }
  });

  it("normalizes legacy evidence without dice mode to Unknown", () => {
    const builder = new CompactGameBuilder();
    const current = builder.apply(
      {
        ...captureBase,
        decisions: [],
        board: diceBoard("unknown", 1_100),
      },
      false,
    );
    const legacy = structuredClone(current);
    delete (legacy.meta as Partial<typeof legacy.meta>).diceMode;

    expect(normalizeCompactRecordIntegrity(legacy).meta.diceMode).toBe("unknown");
  });

  it("upgrades Unknown dice evidence when a later active observation becomes known", () => {
    const builder = new CompactGameBuilder();
    builder.apply(
      {
        ...captureBase,
        decisions: [],
        board: diceBoard("unknown", 1_100),
      },
      false,
    );
    const record = builder.apply(
      {
        ...captureBase,
        decisions: [],
        board: diceBoard("balanced", 1_200),
      },
      false,
    );

    expect(record.meta.diceMode).toBe("balanced");
    expect(record.meta.diceModeConflicts).toBeUndefined();
  });

  it("preserves conflicting known dice observations without overwriting established evidence", () => {
    const builder = new CompactGameBuilder();
    builder.apply(
      {
        ...captureBase,
        decisions: [],
        board: diceBoard("random", 1_100),
      },
      false,
    );
    const record = builder.apply(
      {
        ...captureBase,
        decisions: [],
        board: diceBoard("balanced", 1_200),
      },
      false,
    );

    expect(record.meta.diceMode).toBe("random");
    expect(record.meta.diceModeConflicts).toEqual([
      expect.objectContaining({
        established: "random",
        observed: "balanced",
        observedAt: 1_200,
      }),
    ]);
    expect(record.meta.integrityIssues).toContain("dice-mode-conflict");
  });

  it("treats different raw Unsupported values as contradictory evidence", () => {
    const builder = new CompactGameBuilder();
    builder.apply(
      {
        ...captureBase,
        decisions: [],
        board: diceBoard("unsupported", 1_100, 2),
      },
      false,
    );
    const record = builder.apply(
      {
        ...captureBase,
        decisions: [],
        board: diceBoard("unsupported", 1_200, 3),
      },
      false,
    );

    expect(record.meta.diceMode).toBe("unsupported");
    expect(record.meta.diceModeRaw).toBe(2);
    expect(record.meta.diceModeConflicts).toEqual([
      expect.objectContaining({
        established: "unsupported",
        observed: "unsupported",
        observedAt: 1_200,
        establishedRawSetting: 2,
        rawObservedSetting: 3,
      }),
    ]);
    expect(record.meta.integrityIssues).toContain("dice-mode-conflict");
  });

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
      diceMode: "unknown",
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

  it("migrates resumed v2 decision and root rows by declared column names", () => {
    const rootedTrace = (stateHash: string): DecisionTrace => ({
      ...trace(stateHash, { kind: "turn-control", control: "end" }),
      rootProvenance: {
        rankedRootCount: 1,
        rankedRoots: [],
        retainedRoots: [
          {
            action: { kind: "end-turn" },
            preTruncationRank: 1,
            prior: 0.7,
            nodeBudgetPerParticle: 12,
            allocatedNodes: 24,
          },
        ],
        prunedRootCount: 0,
        prunedRoots: [],
      },
    });
    const lifecycleTraces: DecisionTrace[] = [
      {
        ...trace("selected-state", { kind: "turn-control", control: "end" }),
        finalActionSelectedAt: 1_100,
      },
      {
        ...trace("execution-pending-state", { kind: "turn-control", control: "end" }),
        finalActionSelectedAt: 1_100,
        executionStartedAt: 1_200,
      },
      {
        ...trace("execution-complete-state", { kind: "turn-control", control: "end" }),
        finalActionSelectedAt: 1_100,
        executionStartedAt: 1_200,
        executionFinishedAt: 1_300,
        executionSucceeded: true,
      },
      {
        ...trace("execution-failed-state", { kind: "turn-control", control: "end" }),
        finalActionSelectedAt: 1_100,
        executionStartedAt: 1_200,
        executionFinishedAt: 1_300,
        executionSucceeded: false,
      },
    ];
    const initial = new CompactGameBuilder().apply(
      {
        ...captureBase,
        decisions: [rootedTrace("legacy-state"), ...lifecycleTraces],
      },
      false,
    );
    const legacy = structuredClone(initial);
    const currentDecisionColumns = [...legacy.contracts.decisionColumns];
    const currentRootColumns = [...legacy.contracts.rootColumns];
    const removedDecisionColumns = new Set([
      "lifecycle",
      "searchResult",
      "reusedFrom",
    ]);
    const removedRootColumns = new Set([
      "finalRank",
      "terminalOutcome",
      "terminalLcb",
      "terminalUcb",
      "victoryMargin",
      "marginLcb",
      "marginUcb",
      "meanTurn",
    ]);
    const legacyDecisionColumns = currentDecisionColumns.filter(
      (column) => !removedDecisionColumns.has(column),
    );
    const legacyRootColumns = currentRootColumns.filter(
      (column) => !removedRootColumns.has(column),
    );
    legacy.decisions = legacy.decisions.map((row) =>
      legacyDecisionColumns.map(
        (column) => row[currentDecisionColumns.indexOf(column)]!,
      ),
    );
    legacy.roots = legacy.roots.map((row) =>
      legacyRootColumns.map(
        (column) => row[currentRootColumns.indexOf(column)]!,
      ),
    );
    legacy.contracts = {
      ...legacy.contracts,
      decisionColumns: legacyDecisionColumns,
      rootColumns: legacyRootColumns,
    };

    const resumed = new CompactGameBuilder(legacy).apply(
      {
        ...captureBase,
        decisions: [rootedTrace("fresh-state")],
      },
      false,
    );

    expect(
      resumed.decisions.every(
        (row) => row.length === resumed.contracts.decisionColumns.length,
      ),
    ).toBe(true);
    expect(
      resumed.roots.every(
        (row) => row.length === resumed.contracts.rootColumns.length,
      ),
    ).toBe(true);
    const stateColumn = resumed.contracts.decisionColumns.indexOf("state");
    const displayColumn = resumed.contracts.decisionColumns.indexOf("display");
    const statusColumn = resumed.contracts.decisionColumns.indexOf("status");
    const lifecycleColumn = resumed.contracts.decisionColumns.indexOf("lifecycle");
    const legacyDecision = resumed.decisions.find(
      (row) => row[stateColumn] === "legacy-state",
    );
    expect(legacyDecision?.[displayColumn]).toBe("turn-control|ctl=end");
    expect(legacyDecision?.[statusColumn]).toBe("complete");
    expect(legacyDecision?.[lifecycleColumn]).toBe("search-complete");
    const lifecycleByState = Object.fromEntries(
      resumed.decisions.map((row) => [String(row[stateColumn]), row[lifecycleColumn]]),
    );
    expect(lifecycleByState).toMatchObject({
      "selected-state": "action-selected",
      "execution-pending-state": "execution-pending",
      "execution-complete-state": "execution-complete",
      "execution-failed-state": "execution-failed",
    });
    const rootActionColumn = resumed.contracts.rootColumns.indexOf("action");
    const finalRankColumn = resumed.contracts.rootColumns.indexOf("finalRank");
    const legacyRoot = resumed.roots.find((row) => row[0] === "D1");
    expect(legacyRoot?.[rootActionColumn]).toBe("end-turn");
    expect(legacyRoot?.[finalRankColumn]).toBeNull();
  });
});
