#!/usr/bin/env node

import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const reportPath = process.argv[2];
if (!reportPath) {
  throw new Error(
    "Usage: node scripts/audit-colonist-benchmark.mjs <benchmark.json>",
  );
}

const report = JSON.parse(await readFile(reportPath, "utf8"));
const resourceOrder = ["lumber", "brick", "wool", "grain", "ore"];

const vectorKey = (vector = {}) =>
  resourceOrder.map((resource) => Number(vector[resource] ?? 0)).join(",");

const cardTotal = (vector = {}) =>
  resourceOrder.reduce(
    (total, resource) => total + Number(vector[resource] ?? 0),
    0,
  );

const canAfford = (hand = {}, cost = {}) =>
  resourceOrder.every(
    (resource) =>
      Number(hand[resource] ?? 0) >= Number(cost[resource] ?? 0),
  );

const remainingPieces = (trace, piece, supply) => {
  const player =
    trace.replayState?.players?.[
      trace.rootPlayer ?? trace.replayBoard?.myPlayer
    ];
  const built = Number(player?.builds?.[piece]);
  return Number.isFinite(built) ? Math.max(0, supply - built) : undefined;
};

const hasDeterministicHandReduction = (trace) => {
  const board = trace.replayBoard;
  const hand = board?.ownHand;
  if (!board || !hand) return false;
  if (
    (remainingPieces(trace, "road", 15) ?? 0) > 0 &&
    board.buildableRoadIds?.length > 0 &&
    canAfford(hand, { lumber: 1, brick: 1 })
  ) {
    return true;
  }
  if (
    (remainingPieces(trace, "settlement", 5) ?? 0) > 0 &&
    board.buildableSettlementIds?.length > 0 &&
    canAfford(hand, { lumber: 1, brick: 1, wool: 1, grain: 1 })
  ) {
    return true;
  }
  if (
    (remainingPieces(trace, "city", 4) ?? 0) > 0 &&
    board.buildableCityIds?.length > 0 &&
    canAfford(hand, { grain: 2, ore: 3 })
  ) {
    return true;
  }
  const ratios =
    board.players?.[board.myPlayer]?.tradeRatios;
  return resourceOrder.some((given) => {
    const ratio = Number(ratios?.[given] ?? 4);
    if (ratio <= 1 || Number(hand[given] ?? 0) < ratio) return false;
    return resourceOrder.some(
      (received) =>
        received !== given &&
        (!board.bankVisible || Number(board.bank?.[received] ?? 0) > 0),
    );
  });
};

const impossibleFromVisibleBank = (board, requested) => {
  if (!board?.bankVisible || !board.bank || !board.ownHand) return false;
  const supply = Object.keys(board.players ?? {}).length > 4 ? 24 : 19;
  return resourceOrder.some(
    (resource) =>
      supply -
        Number(board.bank[resource] ?? 0) -
        Number(board.ownHand[resource] ?? 0) <
      Number(requested?.[resource] ?? 0),
  );
};

const wilson = (wins, games, z = 1.959963984540054) => {
  if (!games) return undefined;
  const rate = wins / games;
  const denominator = 1 + (z * z) / games;
  const center = (rate + (z * z) / (2 * games)) / denominator;
  const margin =
    (z *
      Math.sqrt(
        (rate * (1 - rate)) / games + (z * z) / (4 * games * games),
      )) /
    denominator;
  return {
    lower: Math.max(0, center - margin),
    upper: Math.min(1, center + margin),
  };
};

const auditTrace = (traces) => {
  const final = traces.filter((trace) => trace.finalAction);
  const protocolFailures = traces.filter(
    (trace) =>
      trace.executionSucceeded === false &&
      !/State signature or legal target set changed before execution/i.test(
        trace.executionFailureReason ?? "",
      ),
  );
  const validationCancellations = traces.filter(
    (trace) =>
      trace.executionSucceeded === false &&
      /State signature or legal target set changed before execution/i.test(
        trace.executionFailureReason ?? "",
      ),
  );
  const repeatedOffers = [];
  const offers = new Set();
  const impossibleTrades = [];
  const staleRolls = [];
  const postGameActions = [];
  const riskyEndTurns = [];
  const forcedRiskyEndTurns = [];
  const robberOnCurrentHex = [];
  for (const trace of final) {
    const action = trace.finalAction;
    const board = trace.replayBoard;
    if (board?.gameOver) postGameActions.push(trace.stateHash);
    if (
      action.kind === "turn-control" &&
      action.control === "roll" &&
      trace.phase &&
      trace.phase !== "none"
    ) {
      staleRolls.push(trace.stateHash);
    }
    if (
      action.kind === "turn-control" &&
      action.control === "end" &&
      cardTotal(board?.ownHand) > 7
    ) {
      if (hasDeterministicHandReduction(trace)) {
        riskyEndTurns.push(trace.stateHash);
      } else {
        forcedRiskyEndTurns.push(trace.stateHash);
      }
    }
    if (
      trace.deepChosenAction?.kind === "move-robber" &&
      board?.hexes?.find((hex) => hex.id === trace.deepChosenAction.targetId)
        ?.blocked
    ) {
      robberOnCurrentHex.push(trace.stateHash);
    }
    const outgoing =
      action.kind === "trade-builder" &&
      action.mode === "player" &&
      action.give &&
      action.receive;
    if (outgoing) {
      const key = `${board?.turn ?? trace.turn}|${vectorKey(action.give)}>${vectorKey(action.receive)}`;
      if (offers.has(key)) repeatedOffers.push(key);
      offers.add(key);
      if (impossibleFromVisibleBank(board, action.receive)) {
        impossibleTrades.push(key);
      }
    }
    if (
      action.kind === "trade" &&
      action.verdict === "counter" &&
      action.counterReceive &&
      impossibleFromVisibleBank(board, action.counterReceive)
    ) {
      impossibleTrades.push(
        `${board?.turn ?? trace.turn}|counter>${vectorKey(action.counterReceive)}`,
      );
    }
  }
  const sourceCounts = {};
  const runtimeCounts = {};
  const actionCounts = {};
  for (const trace of traces) {
    const source = trace.finalActionSource ?? "no-final-action";
    sourceCounts[source] = (sourceCounts[source] ?? 0) + 1;
    const runtime = trace.runtime ?? "fast-path/no-search";
    runtimeCounts[runtime] = (runtimeCounts[runtime] ?? 0) + 1;
    if (trace.finalAction?.kind) {
      actionCounts[trace.finalAction.kind] =
        (actionCounts[trace.finalAction.kind] ?? 0) + 1;
    }
  }
  return {
    traces: traces.length,
    finalActions: final.length,
    sourceCounts,
    runtimeCounts,
    actionCounts,
    validationCancellations: validationCancellations.length,
    protocolFailures: protocolFailures.map((trace) => ({
      turn: trace.turn,
      phase: trace.phase,
      reason: trace.executionFailureReason,
    })),
    repeatedOffers,
    impossibleTrades,
    staleRolls: staleRolls.length,
    robberOnCurrentHex: robberOnCurrentHex.length,
    riskyEndTurns: riskyEndTurns.length,
    forcedRiskyEndTurns: forcedRiskyEndTurns.length,
    postGameActions: postGameActions.length,
  };
};

const games = [];
for (const result of report.results ?? []) {
  let traceAudit;
  let traceReadError;
  if (result.tracePath) {
    try {
      const parsed = JSON.parse(await readFile(result.tracePath, "utf8"));
      traceAudit = auditTrace(parsed.traces ?? []);
    } catch (error) {
      traceReadError = error instanceof Error ? error.message : String(error);
    }
  }
  games.push({
    difficulty: result.difficulty,
    game: result.game,
    status: result.status,
    rank: result.rank,
    won: result.won,
    durationMs: result.durationMs,
    automatedClicks: result.automatedClicks,
    assistantRuntime: result.assistantRuntime,
    decisionServiceSummary: result.decisionServiceSummary,
    traceAudit,
    traceReadError,
    harnessError: result.error,
    artifact: result.artifact,
  });
}

const completed = games.filter((game) => game.status === "completed");
const byDifficulty = [...new Set(games.map((game) => game.difficulty))].map(
  (difficulty) => {
    const selected = completed.filter(
      (game) => game.difficulty === difficulty,
    );
    const wins = selected.filter((game) => game.won).length;
    return {
      difficulty,
      games: selected.length,
      wins,
      winRate: selected.length ? wins / selected.length : undefined,
      confidence95: wilson(wins, selected.length),
    };
  },
);

const aggregate = {
  completedGames: completed.length,
  wins: completed.filter((game) => game.won).length,
  protocolFailures: games.reduce(
    (total, game) =>
      total + (game.traceAudit?.protocolFailures.length ?? 0),
    0,
  ),
  repeatedOffers: games.reduce(
    (total, game) => total + (game.traceAudit?.repeatedOffers.length ?? 0),
    0,
  ),
  impossibleTrades: games.reduce(
    (total, game) => total + (game.traceAudit?.impossibleTrades.length ?? 0),
    0,
  ),
  staleRolls: games.reduce(
    (total, game) => total + (game.traceAudit?.staleRolls ?? 0),
    0,
  ),
  robberOnCurrentHex: games.reduce(
    (total, game) => total + (game.traceAudit?.robberOnCurrentHex ?? 0),
    0,
  ),
  riskyEndTurns: games.reduce(
    (total, game) => total + (game.traceAudit?.riskyEndTurns ?? 0),
    0,
  ),
  forcedRiskyEndTurns: games.reduce(
    (total, game) =>
      total + (game.traceAudit?.forcedRiskyEndTurns ?? 0),
    0,
  ),
  postGameActions: games.reduce(
    (total, game) => total + (game.traceAudit?.postGameActions ?? 0),
    0,
  ),
  localFallbackTraces: games.reduce(
    (total, game) =>
      total + (game.traceAudit?.runtimeCounts["local-fallback"] ?? 0),
    0,
  ),
};

const audit = {
  schemaVersion: 1,
  kind: "colonist-live-benchmark-audit",
  generatedAt: new Date().toISOString(),
  source: resolve(reportPath),
  byDifficulty,
  aggregate,
  games,
};
const output = reportPath.replace(/\.json$/i, "-audit.json");
await writeFile(output, `${JSON.stringify(audit, null, 2)}\n`);
console.log(JSON.stringify({ output: resolve(output), ...aggregate }, null, 2));
