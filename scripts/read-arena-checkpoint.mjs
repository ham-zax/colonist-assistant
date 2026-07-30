#!/usr/bin/env node

import { readFile } from "node:fs/promises";
import process from "node:process";

function usage() {
  console.error(
    "Usage: npm run benchmark:checkpoint -- CHECKPOINT.jsonl [...] [--json] [--split-lineup]",
  );
}

const arguments_ = process.argv.slice(2);
const checkpointPaths = arguments_.filter((argument) => !argument.startsWith("--"));
const outputJson = arguments_.includes("--json");
const splitLineup = arguments_.includes("--split-lineup");
if (checkpointPaths.length === 0) {
  usage();
  process.exit(2);
}
if (outputJson && checkpointPaths.length > 1) {
  throw new Error("--json accepts one checkpoint file");
}

const checkpoints = [];
const finalCheckpoints = [];
let invalidLines = 0;
for (const checkpointPath of checkpointPaths) {
  const contents = await readFile(checkpointPath, "utf8");
  let finalCheckpoint;
  for (const line of contents.split(/\r?\n/)) {
    if (!line.trim()) continue;
    try {
      const candidate = JSON.parse(line);
      if (candidate.kind === "colonist-native-arena-checkpoint") {
        finalCheckpoint = candidate;
        checkpoints.push(candidate);
      }
    } catch {
      invalidLines += 1;
    }
  }
  if (!finalCheckpoint) {
    throw new Error(`No valid arena checkpoint found in ${checkpointPath}`);
  }
  finalCheckpoints.push(finalCheckpoint);
}

const checkpoint = finalCheckpoints[finalCheckpoints.length - 1];
const combined = finalCheckpoints.length > 1;
const completedGames = finalCheckpoints.reduce(
  (sum, candidate) => sum + candidate.completedGames,
  0,
);
const completedMatchedBlocks = finalCheckpoints.reduce(
  (sum, candidate) => sum + candidate.completedMatchedBlocks,
  0,
);
const blocks = finalCheckpoints.reduce(
  (sum, candidate) => sum + candidate.blocks,
  0,
);
const cutoffs = finalCheckpoints.reduce(
  (sum, candidate) => sum + candidate.cutoffs,
  0,
);
if (
  combined &&
  finalCheckpoints.some(
    (candidate) =>
      JSON.stringify(candidate.lineup) !== JSON.stringify(checkpoint.lineup),
  )
) {
  throw new Error("Cannot combine checkpoints with different lineups");
}

if (outputJson) {
  console.log(JSON.stringify(checkpoint, null, 2));
  process.exit(0);
}

const percent = (value) => `${(value * 100).toFixed(1)}%`;
let standings = checkpoint.engineStandings;
if (combined && !splitLineup) {
  const engines = new Map();
  for (const candidate of finalCheckpoints) {
    for (const standing of candidate.engineStandings) {
      const aggregate = engines.get(standing.engine) ?? {
        engine: standing.engine,
        wins: 0,
        seatSamples: 0,
        rankSum: 0,
        pointSum: 0,
      };
      aggregate.wins += standing.wins;
      aggregate.seatSamples += standing.seatSamples;
      aggregate.rankSum += standing.meanRank * standing.seatSamples;
      aggregate.pointSum += standing.meanVictoryPoints * standing.seatSamples;
      engines.set(standing.engine, aggregate);
    }
  }
  standings = [...engines.values()].map((standing) => ({
    engine: standing.engine,
    wins: standing.wins,
    winShare: standing.wins / completedGames,
    seatSamples: standing.seatSamples,
    meanRank: standing.rankSum / standing.seatSamples,
    meanVictoryPoints: standing.pointSum / standing.seatSamples,
  }));
}
if (splitLineup) {
  if (!Array.isArray(checkpoint.lineup)) {
    throw new Error("--split-lineup requires a checkpoint created with --lineup");
  }
  const totals = new Map();
  for (const engine of checkpoint.lineup) {
    totals.set(engine, (totals.get(engine) ?? 0) + 1);
  }
  const occurrences = new Map();
  const participants = checkpoint.lineup.map((engine) => {
    const occurrence = (occurrences.get(engine) ?? 0) + 1;
    occurrences.set(engine, occurrence);
    return {
      engine:
        totals.get(engine) > 1 ? `${engine} #${occurrence}` : engine,
      wins: 0,
      seatSamples: 0,
      rankSum: 0,
      pointSum: 0,
    };
  });
  for (const snapshot of checkpoints) {
    const game = snapshot.lastGame;
    for (let player = 0; player < game.engines.length; player += 1) {
      const lineupSlot = (player + game.rotation) % participants.length;
      const participant = participants[lineupSlot];
      participant.seatSamples += 1;
      participant.rankSum += game.ranks[player];
      participant.pointSum += game.points[player];
      participant.wins += Number(game.winner === player);
    }
  }
  standings = participants.map((participant) => ({
    engine: participant.engine,
    wins: participant.wins,
    winShare: participant.wins / participant.seatSamples,
    seatSamples: participant.seatSamples,
    meanRank: participant.rankSum / participant.seatSamples,
    meanVictoryPoints: participant.pointSum / participant.seatSamples,
  }));
}
standings = standings
  .filter((standing) => standing.seatSamples > 0)
  .sort(
    (left, right) =>
      right.wins - left.wins ||
      left.meanRank - right.meanRank ||
      right.meanVictoryPoints - left.meanVictoryPoints,
  );
const rows = standings
  .map(
    (standing, index) =>
      `| ${index + 1} | ${standing.engine} | ${standing.wins} | ${percent(standing.winShare)} | ${standing.meanRank.toFixed(3)} | ${standing.meanVictoryPoints.toFixed(3)} | ${standing.seatSamples} |`,
  )
  .join("\n");

const status = combined
  ? `combined partial — ${completedGames} completed games from ${finalCheckpoints.length} checkpoints, ${completedMatchedBlocks} fully matched blocks, ${cutoffs} cutoffs`
  : `${checkpoint.complete ? "complete" : "partial"} — ${checkpoint.completedGames}/${checkpoint.scheduledGames} games (${percent(checkpoint.completionShare)}), ${checkpoint.completedMatchedBlocks}/${blocks} fully matched blocks, ${cutoffs} cutoffs`;

console.log(`# Arena checkpoint

Status: ${status}.

| Place | Engine | Wins | Win rate | Avg place | Avg points | Samples |
|---:|---|---:|---:|---:|---:|---:|
${rows}`);

if (invalidLines > 0) {
  console.error(
    `Ignored ${invalidLines} invalid checkpoint line${invalidLines === 1 ? "" : "s"}; the latest valid snapshot is shown.`,
  );
}
