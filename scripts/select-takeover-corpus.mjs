#!/usr/bin/env node

import { readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { mkdir } from "node:fs/promises";
import process from "node:process";

function readOptions(argv) {
  const options = {
    inputs: [],
    output: null,
    threePlayer: 50,
    fourPlayer: 100,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const name = argv[index];
    const value = argv[index + 1];
    switch (name) {
      case "--input":
        options.inputs.push(resolve(value));
        index += 1;
        break;
      case "--output":
        options.output = resolve(value);
        index += 1;
        break;
      case "--three-player":
        options.threePlayer = Number(value);
        index += 1;
        break;
      case "--four-player":
        options.fourPlayer = Number(value);
        index += 1;
        break;
      case "--help":
      case "-h":
        console.log(`Usage: node scripts/select-takeover-corpus.mjs [options]\n\n  --input PATH       Challenge JSONL input; repeat for multiple matchups\n  --output PATH      Frozen JSONL corpus output\n  --three-player N   Number of 3-player challenges (default: 50)\n  --four-player N    Number of 4-player challenges (default: 100)\n`);
        process.exit(0);
      default:
        throw new Error(`Unknown option: ${name}`);
    }
  }
  if (!options.output || options.inputs.length === 0) {
    throw new Error("At least one --input and exactly one --output are required.");
  }
  for (const value of [options.threePlayer, options.fourPlayer]) {
    if (!Number.isInteger(value) || value < 0) {
      throw new Error("Corpus target sizes must be non-negative integers.");
    }
  }
  return options;
}

function parseJsonl(text, source) {
  return text
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line, index) => {
      try {
        return JSON.parse(line);
      } catch (error) {
        throw new Error(`${source}:${index + 1}: invalid JSON: ${error.message}`);
      }
    });
}

function assertChallenge(snapshot) {
  if (snapshot.kind !== "colonist-native-takeover-challenge") {
    throw new Error(`Unexpected challenge kind for ${snapshot.snapshotId ?? "unknown"}.`);
  }
  if (![3, 4].includes(snapshot.players)) {
    throw new Error(`Unsupported player count in ${snapshot.snapshotId}.`);
  }
  if (snapshot.gameState?.phase?.kind !== "preRoll") {
    throw new Error(`Challenge ${snapshot.snapshotId} is not a stable PreRoll state.`);
  }
  if (snapshot.turn < snapshot.players * 5) {
    throw new Error(`Challenge ${snapshot.snapshotId} violates the minimum-turn criterion.`);
  }
  if (!(snapshot.targetEvaluatorWinValue <= 0.25)) {
    throw new Error(`Challenge ${snapshot.snapshotId} violates the evaluator criterion.`);
  }
  const publicPoints = snapshot.gameState.players.map(
    (player) => player.publicVictoryPoints,
  );
  const targetPoints = publicPoints[snapshot.targetSeat];
  const minimum = Math.min(...publicPoints);
  const leader = Math.max(...publicPoints);
  if (!(targetPoints === minimum || leader - targetPoints >= 2)) {
    throw new Error(`Challenge ${snapshot.snapshotId} violates the behind criterion.`);
  }
}

function stableCompare(left, right) {
  return (
    left.sourceBlock - right.sourceBlock ||
    left.sourceRotation - right.sourceRotation ||
    left.targetSeat - right.targetSeat ||
    left.turn - right.turn ||
    left.boardSeed - right.boardSeed ||
    String(left.stateHash).localeCompare(String(right.stateHash)) ||
    String(left.snapshotId).localeCompare(String(right.snapshotId))
  );
}

function seatBalancedSelection(candidates, players, target) {
  const bySeat = Array.from({ length: players }, () => []);
  for (const snapshot of candidates.filter((entry) => entry.players === players)) {
    bySeat[snapshot.targetSeat].push(snapshot);
  }
  for (const seat of bySeat) seat.sort(stableCompare);

  const selected = [];
  let seatCursor = 0;
  while (selected.length < target) {
    let picked = false;
    for (let attempt = 0; attempt < players; attempt += 1) {
      const seat = (seatCursor + attempt) % players;
      const next = bySeat[seat].shift();
      if (!next) continue;
      selected.push(next);
      seatCursor = (seat + 1) % players;
      picked = true;
      break;
    }
    if (!picked) break;
  }
  if (selected.length !== target) {
    const available = candidates.filter((entry) => entry.players === players).length;
    throw new Error(
      `Need ${target} ${players}-player challenge states but only ${available} qualifying states were available. Extend the deterministic seed range; do not weaken the criteria.`,
    );
  }
  return selected;
}

const options = readOptions(process.argv.slice(2));
const all = [];
for (const input of options.inputs) {
  const parsed = parseJsonl(await readFile(input, "utf8"), input);
  for (const snapshot of parsed) {
    assertChallenge(snapshot);
    all.push(snapshot);
  }
}

const identities = new Map();
const sourceTargetKeys = new Set();
const sourceHashKeys = new Set();
const uniqueSnapshots = new Map();
for (const snapshot of all) {
  identities.set(
    `${snapshot.sourceGitSha}:${snapshot.sourceBuildDirty}:${snapshot.engineRevision}`,
    {
      sourceGitSha: snapshot.sourceGitSha,
      sourceBuildDirty: snapshot.sourceBuildDirty,
      engineRevision: snapshot.engineRevision,
    },
  );
  const sourceGame = `${snapshot.players}:${snapshot.boardSeed}:${snapshot.chanceSeed}:${snapshot.sourceBlock}:${snapshot.sourceRotation}`;
  const sourceTarget = `${sourceGame}:${snapshot.targetSeat}`;
  if (sourceTargetKeys.has(sourceTarget)) {
    throw new Error(`More than one challenge exists for target seat/source game: ${sourceTarget}`);
  }
  sourceTargetKeys.add(sourceTarget);
  const sourceHash = `${sourceGame}:${snapshot.stateHash}`;
  if (sourceHashKeys.has(sourceHash)) {
    throw new Error(`Duplicate state hash within source game: ${sourceHash}`);
  }
  sourceHashKeys.add(sourceHash);
  if (uniqueSnapshots.has(snapshot.snapshotId)) {
    throw new Error(`Duplicate snapshot id: ${snapshot.snapshotId}`);
  }
  uniqueSnapshots.set(snapshot.snapshotId, snapshot);
}
if (identities.size !== 1) {
  throw new Error("Challenge inputs contain multiple source build identities.");
}

const candidates = [...uniqueSnapshots.values()].sort(stableCompare);
const selected = [
  ...seatBalancedSelection(candidates, 3, options.threePlayer),
  ...seatBalancedSelection(candidates, 4, options.fourPlayer),
];
const seatCounts = Object.fromEntries(
  [3, 4].map((players) => [
    `${players}p`,
    Array.from({ length: players }, (_, seat) =>
      selected.filter(
        (snapshot) => snapshot.players === players && snapshot.targetSeat === seat,
      ).length,
    ),
  ]),
);
const identity = [...identities.values()][0];
const manifest = {
  schemaVersion: 1,
  kind: "colonist-native-frozen-takeover-corpus",
  selectionRule: {
    boundary: "PreRoll start of turn",
    nonterminal: true,
    minimumTurnFormula: "turn >= players * 5",
    behindRule: "target last by public VP OR >=2 public VP behind leader",
    maximumEvaluatorWinValue: 0.25,
    uniqueStateHashPerSourceGame: true,
    maximumPerTargetSeatPerSourceGame: 1,
    selectionOrder: "deterministic source block/rotation/seat/turn/board/hash round-robin by target seat",
  },
  sourceIdentity: identity,
  inputFiles: options.inputs,
  targets: { threePlayer: options.threePlayer, fourPlayer: options.fourPlayer },
  selected: selected.length,
  seatCounts,
  snapshotIds: selected.map((snapshot) => snapshot.snapshotId),
};

await mkdir(dirname(options.output), { recursive: true });
await Promise.all([
  writeFile(
    options.output,
    `${selected.map((snapshot) => JSON.stringify(snapshot)).join("\n")}\n`,
  ),
  writeFile(
    `${options.output}.manifest.json`,
    `${JSON.stringify(manifest, null, 2)}\n`,
  ),
]);
console.log(JSON.stringify({ output: options.output, manifest: `${options.output}.manifest.json`, selected: selected.length, seatCounts, sourceIdentity: identity }, null, 2));
