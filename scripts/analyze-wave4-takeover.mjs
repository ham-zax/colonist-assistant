#!/usr/bin/env node

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import process from "node:process";

const MECHANISM_FIELDS = [
  "roadCuts",
  "awardTransfers",
  "expansionDenialEvents",
  "expansionPortfolioDenied",
  "expansionProtectionEvents",
  "monopolyCardsTransferred",
  "dirtyMonopolySequences",
  "oneTurnCloseouts",
];

function readOptions(argv) {
  const options = {
    corpus: null,
    baseline: null,
    candidate: null,
    output: null,
    selectOutput: null,
    selectManifest: null,
    selectMax: 24,
    selectMinTurn: 0,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const name = argv[index];
    const value = argv[index + 1];
    switch (name) {
      case "--corpus":
        options.corpus = resolve(value);
        index += 1;
        break;
      case "--baseline":
        options.baseline = resolve(value);
        index += 1;
        break;
      case "--candidate":
        options.candidate = resolve(value);
        index += 1;
        break;
      case "--output":
        options.output = resolve(value);
        index += 1;
        break;
      case "--select-output":
        options.selectOutput = resolve(value);
        index += 1;
        break;
      case "--select-manifest":
        options.selectManifest = resolve(value);
        index += 1;
        break;
      case "--select-max":
        options.selectMax = Number(value);
        index += 1;
        break;
      case "--select-min-turn":
        options.selectMinTurn = Number(value);
        index += 1;
        break;
      case "--help":
      case "-h":
        console.log(
          "Usage: node scripts/analyze-wave4-takeover.mjs --corpus snapshots.jsonl --baseline baseline.jsonl --candidate candidate.jsonl --output summary.json [--select-output frozen.jsonl --select-manifest frozen.manifest.json --select-max 24 --select-min-turn 0]",
        );
        process.exit(0);
      default:
        throw new Error(`Unknown option: ${name}`);
    }
  }
  for (const required of ["corpus", "baseline", "candidate", "output"]) {
    if (!options[required]) throw new Error(`Missing --${required}.`);
  }
  if (!Number.isInteger(options.selectMax) || options.selectMax < 1) {
    throw new Error("--select-max must be a positive integer.");
  }
  if (!Number.isInteger(options.selectMinTurn) || options.selectMinTurn < 0) {
    throw new Error("--select-min-turn must be a non-negative integer.");
  }
  if ((options.selectOutput === null) !== (options.selectManifest === null)) {
    throw new Error("--select-output and --select-manifest must be provided together.");
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

function indexUnique(records, source) {
  const byId = new Map();
  for (const record of records) {
    if (!record.snapshotId) throw new Error(`${source}: record is missing snapshotId.`);
    if (byId.has(record.snapshotId)) {
      throw new Error(`${source}: duplicate snapshotId ${record.snapshotId}.`);
    }
    byId.set(record.snapshotId, record);
  }
  return byId;
}

function sameArray(left, right) {
  return (
    Array.isArray(left) &&
    Array.isArray(right) &&
    left.length === right.length &&
    left.every((value, index) => value === right[index])
  );
}

function assertOutcomeMatchesSnapshot(snapshot, outcome, arm) {
  const scalarFields = [
    "snapshotId",
    "stateHash",
    "boardSeed",
    "chanceSeed",
    "players",
    "targetSeat",
    "sourceBlock",
    "sourceRotation",
  ];
  for (const field of scalarFields) {
    if (outcome[field] !== snapshot[field]) {
      throw new Error(
        `${arm} ${snapshot.snapshotId}: ${field} mismatch (${outcome[field]} != ${snapshot[field]}).`,
      );
    }
  }
  if (outcome.chanceRngState !== snapshot.chanceRngState) {
    throw new Error(`${arm} ${snapshot.snapshotId}: chance RNG state mismatch.`);
  }
  if (!sameArray(outcome.policyRngStates, snapshot.policyRngStates)) {
    throw new Error(`${arm} ${snapshot.snapshotId}: policy RNG states mismatch.`);
  }
}

function mean(values) {
  if (values.length === 0) return null;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function sum(values) {
  return values.reduce((total, value) => total + value, 0);
}

function percentile(sorted, fraction) {
  if (sorted.length === 0) return null;
  const index = Math.min(sorted.length - 1, Math.max(0, Math.floor(fraction * sorted.length)));
  return sorted[index];
}

function makeRng(seed) {
  let state = BigInt.asUintN(64, BigInt(seed));
  return () => {
    state ^= state << 13n;
    state ^= state >> 7n;
    state ^= state << 17n;
    state = BigInt.asUintN(64, state);
    return Number(state & 0xffffffffn) / 0x100000000;
  };
}

function bootstrapInterval(blockValues, seed) {
  if (blockValues.length < 8) return null;
  const rng = makeRng(seed);
  const samples = [];
  for (let sample = 0; sample < 5000; sample += 1) {
    let total = 0;
    for (let index = 0; index < blockValues.length; index += 1) {
      total += blockValues[Math.floor(rng() * blockValues.length)];
    }
    samples.push(total / blockValues.length);
  }
  samples.sort((left, right) => left - right);
  return [percentile(samples, 0.025), percentile(samples, 0.975)];
}

function sourceGameId(snapshot) {
  return `${snapshot.players}p:${snapshot.boardSeed}:${snapshot.chanceSeed}:b${snapshot.sourceBlock}:r${snapshot.sourceRotation}`;
}

function blockId(snapshot) {
  return `${snapshot.players}p:${snapshot.boardSeed}:${snapshot.chanceSeed}:b${snapshot.sourceBlock}`;
}

function progressArray(outcome, field) {
  const value = outcome[field];
  return Array.isArray(value) && value.length === 4 ? value : [0, 0, 0, 0];
}

function mechanismRecord(outcome) {
  const record = Object.fromEntries(
    MECHANISM_FIELDS.map((field) => [
      field,
      outcome[field] == null ? null : Number(outcome[field]),
    ]),
  );
  record.progressCardPlays = progressArray(outcome, "progressCardPlays");
  record.progressCardConversions = progressArray(outcome, "progressCardConversions");
  record.longestRoadAcquired = Boolean(outcome.longestRoadAcquired);
  record.largestArmyAcquired = Boolean(outcome.largestArmyAcquired);
  return record;
}

function mechanismLabels(outcome) {
  const source = outcome.mechanisms ?? outcome;
  const labels = [];
  for (const field of MECHANISM_FIELDS) {
    if (Number(source[field] ?? 0) > 0) labels.push(field);
  }
  const plays = progressArray(source, "progressCardPlays");
  const playNames = ["knightPlay", "roadBuildingPlay", "yearOfPlentyPlay", "monopolyPlay"];
  plays.forEach((value, index) => {
    if (value > 0) labels.push(playNames[index]);
  });
  const conversions = progressArray(source, "progressCardConversions");
  const conversionNames = ["knightConversion", "roadBuildingConversion", "yearOfPlentyConversion", "monopolyConversion"];
  conversions.forEach((value, index) => {
    if (value > 0) labels.push(conversionNames[index]);
  });
  if (source.longestRoadAcquired ?? outcome.longestRoadAcquired) labels.push("longestRoadAcquired");
  if (source.largestArmyAcquired ?? outcome.largestArmyAcquired) labels.push("largestArmyAcquired");
  return labels;
}

function mechanismDelta(baseline, candidate) {
  const delta = Object.fromEntries(
    MECHANISM_FIELDS.map((field) => {
      if (baseline[field] == null || candidate[field] == null) return [field, null];
      return [field, Number(candidate[field]) - Number(baseline[field])];
    }),
  );
  delta.progressCardPlays = progressArray(candidate, "progressCardPlays").map(
    (value, index) => value - progressArray(baseline, "progressCardPlays")[index],
  );
  delta.progressCardConversions = progressArray(candidate, "progressCardConversions").map(
    (value, index) => value - progressArray(baseline, "progressCardConversions")[index],
  );
  delta.longestRoadAcquired = Number(Boolean(candidate.longestRoadAcquired)) - Number(Boolean(baseline.longestRoadAcquired));
  delta.largestArmyAcquired = Number(Boolean(candidate.largestArmyAcquired)) - Number(Boolean(baseline.largestArmyAcquired));
  return delta;
}

function rootAction(outcome) {
  return outcome.initialRoot?.action ?? null;
}

function decisionIdentity(outcome) {
  const native = outcome.nativeGpuIdentity;
  if (native?.runtime === "gpu-native") {
    return {
      backend: "native-gpu",
      buildGitSha: native.build?.gitSha ?? outcome.decisionRevision ?? null,
      buildDirty: native.build?.dirty ?? null,
      engineRevision: native.engineRevision ?? null,
      protocolVersion: native.protocolVersion ?? null,
      stateSchemaVersion: native.stateSchemaVersion ?? null,
      device: native.device ?? null,
    };
  }
  return {
    backend: outcome.decisionBackend ?? "arena",
    buildGitSha: outcome.decisionRevision ?? outcome.buildGitSha ?? null,
    buildDirty: outcome.buildDirty ?? null,
    engineRevision: outcome.engineRevision ?? null,
    protocolVersion: null,
    stateSchemaVersion: null,
    device: null,
  };
}

function disagreementProvenance(baselineRoot, candidateRoot, baseline, candidate) {
  if (baselineRoot === candidateRoot) return { category: "concordant", detail: null };
  const provenance = candidate.initialRoot?.provenance;
  if (!provenance) return { category: "unavailable", detail: null };

  const diagnostics = candidate.nativeGpuInitialDiagnostics;
  const authority = diagnostics?.authority ?? null;
  const authorityTrace = diagnostics?.authorityTrace ?? null;
  const selectedEvidence = provenance.rootEvidence?.find(
    (entry) => entry.action === candidateRoot,
  );
  const baselineEvidence = provenance.rootEvidence?.find(
    (entry) => entry.action === baselineRoot,
  );
  const prunedBaseline = provenance.prunedRoots?.find(
    (entry) => entry.action === baselineRoot,
  );
  const baselineProvenance = baseline.initialRoot?.provenance;
  const prunedCandidateInBaseline = baselineProvenance?.prunedRoots?.find(
    (entry) => entry.action === candidateRoot,
  );

  if (
    authority === "safety-override" ||
    authorityTrace?.safetyReplacement ||
    provenance.safetyReplacement ||
    prunedBaseline?.reason === "trade-safety" ||
    baselineEvidence?.tradeHardVeto
  ) {
    return {
      category: "trade-safety-arbitration",
      detail: prunedBaseline?.reason ?? "safety-override",
    };
  }
  if (
    authority === "exact-family" ||
    authority === "exact-mandatory" ||
    authority === "tactical-proven" ||
    authorityTrace?.exactFamilyReplacement ||
    provenance.exactFamilyReplacement ||
    (provenance.searchWinner && provenance.searchWinner !== candidateRoot)
  ) {
    return {
      category: "final-arbitration",
      detail: authority ?? "post-rollout-replacement",
    };
  }
  if (selectedEvidence?.admittedByPromotion) {
    return {
      category: "root-promotion-admission",
      detail: selectedEvidence.promotionReason ?? "promotion-admission",
    };
  }
  if (
    prunedBaseline?.reason === "root-excluded" ||
    prunedCandidateInBaseline?.reason === "root-excluded"
  ) {
    return { category: "root-promotion-admission", detail: "root-excluded" };
  }
  if (
    provenance.retainedRoots?.includes(baselineRoot) &&
    provenance.retainedRoots?.includes(candidateRoot)
  ) {
    return {
      category: "rollout-policy-value-comparison",
      detail: authority === "gpu-root-rollout" ? "gpu-both-roots-retained" : "both-roots-retained",
    };
  }
  return { category: "unavailable", detail: null };
}

function compareContinuationOutcome(baseline, candidate) {
  if (candidate.targetWin !== baseline.targetWin) {
    return candidate.targetWin ? 1 : -1;
  }
  if (candidate.finalRank !== baseline.finalRank) {
    return candidate.finalRank < baseline.finalRank ? 1 : -1;
  }
  if (candidate.victoryPointMargin !== baseline.victoryPointMargin) {
    return candidate.victoryPointMargin > baseline.victoryPointMargin ? 1 : -1;
  }
  if (candidate.finalVictoryPoints !== baseline.finalVictoryPoints) {
    return candidate.finalVictoryPoints > baseline.finalVictoryPoints ? 1 : -1;
  }
  return 0;
}

function pairClassification(baselineRoot, candidateRoot, baseline, candidate) {
  if (baselineRoot === candidateRoot) return "concordant";
  const comparison = compareContinuationOutcome(baseline, candidate);
  if (comparison > 0) return "candidate-rescue";
  if (comparison < 0) return "candidate-regression";
  return "outcome-neutral-disagreement";
}

function aggregateMechanismDeltas(pairs) {
  const totals = Object.fromEntries(MECHANISM_FIELDS.map((field) => [field, 0]));
  const comparablePairs = Object.fromEntries(MECHANISM_FIELDS.map((field) => [field, 0]));
  totals.progressCardPlays = [0, 0, 0, 0];
  totals.progressCardConversions = [0, 0, 0, 0];
  totals.longestRoadAcquired = 0;
  totals.largestArmyAcquired = 0;
  comparablePairs.progressCardPlays = pairs.length;
  comparablePairs.progressCardConversions = pairs.length;
  comparablePairs.longestRoadAcquired = pairs.length;
  comparablePairs.largestArmyAcquired = pairs.length;
  for (const pair of pairs) {
    for (const field of MECHANISM_FIELDS) {
      const value = pair.deltas.mechanisms[field];
      if (Number.isFinite(value)) {
        totals[field] += value;
        comparablePairs[field] += 1;
      }
    }
    pair.deltas.mechanisms.progressCardPlays.forEach((value, index) => {
      totals.progressCardPlays[index] += value;
    });
    pair.deltas.mechanisms.progressCardConversions.forEach((value, index) => {
      totals.progressCardConversions[index] += value;
    });
    totals.longestRoadAcquired += pair.deltas.mechanisms.longestRoadAcquired;
    totals.largestArmyAcquired += pair.deltas.mechanisms.largestArmyAcquired;
  }
  return { totals, comparablePairs };
}

function pairedBlockMeans(pairs, selector) {
  const grouped = new Map();
  for (const pair of pairs) {
    const value = selector(pair);
    if (!Number.isFinite(value)) continue;
    const values = grouped.get(pair.sourceBlockId) ?? [];
    values.push(value);
    grouped.set(pair.sourceBlockId, values);
  }
  return [...grouped.entries()].map(([id, values]) => ({ id, value: mean(values) }));
}

function actionFamily(action) {
  if (!action) return "none";
  return String(action).split(/[ {]/u, 1)[0];
}

function selectFrozenCorpus(snapshots, pairs, maximum, minimumTurn) {
  const byId = new Map(pairs.map((pair) => [pair.snapshotId, pair]));
  const eligible = snapshots
    .map((snapshot) => {
      const pair = byId.get(snapshot.snapshotId);
      const baselineLabels = mechanismLabels(pair.baseline);
      const candidateLabels = mechanismLabels(pair.candidate);
      const labels = [...new Set([...baselineLabels, ...candidateLabels])].sort();
      const disagreement = !pair.rootChoiceConcordance;
      const baselineFamily = actionFamily(pair.baselineRoot);
      const candidateFamily = actionFamily(pair.candidateRoot);
      const tradeOnlyDisagreement =
        disagreement && baselineFamily === "OfferTrade" && candidateFamily === "OfferTrade";
      const tier = labels.length > 0 ? 3 : disagreement && !tradeOnlyDisagreement ? 2 : disagreement ? 1 : 0;
      return {
        snapshot,
        pair,
        disagreement,
        labels,
        eligible: tier > 0,
        tier,
        baselineFamily,
        candidateFamily,
        score: tier * 10_000 + labels.length * 1_000 + snapshot.turn,
      };
    })
    .filter((entry) => entry.eligible && entry.snapshot.turn >= minimumTurn);

  const bySourceGame = new Map();
  for (const entry of eligible) {
    const values = bySourceGame.get(entry.pair.sourceGameId) ?? [];
    values.push(entry);
    bySourceGame.set(entry.pair.sourceGameId, values);
  }
  const representatives = [];
  for (const values of bySourceGame.values()) {
    values.sort(
      (left, right) =>
        right.score - left.score ||
        left.snapshot.sourceRotation - right.snapshot.sourceRotation ||
        left.snapshot.targetSeat - right.snapshot.targetSeat ||
        String(left.snapshot.snapshotId).localeCompare(String(right.snapshot.snapshotId)),
    );
    representatives.push(values[0]);
  }

  const byPlayers = new Map();
  for (const entry of representatives) {
    const values = byPlayers.get(entry.snapshot.players) ?? [];
    values.push(entry);
    byPlayers.set(entry.snapshot.players, values);
  }
  for (const values of byPlayers.values()) {
    values.sort(
      (left, right) =>
        right.score - left.score ||
        String(left.pair.sourceBlockId).localeCompare(String(right.pair.sourceBlockId)),
    );
  }

  const selected = [];
  const seenSourceBlocks = new Set();
  const playerCounts = [...byPlayers.keys()].sort((left, right) => left - right);
  while (selected.length < maximum) {
    let added = false;
    for (const players of playerCounts) {
      const candidates = byPlayers.get(players) ?? [];
      let chosen = null;
      while (candidates.length > 0) {
        const candidate = candidates.shift();
        if (!seenSourceBlocks.has(candidate.pair.sourceBlockId)) {
          chosen = candidate;
          break;
        }
      }
      if (!chosen) continue;
      seenSourceBlocks.add(chosen.pair.sourceBlockId);
      selected.push(chosen);
      added = true;
      if (selected.length >= maximum) break;
    }
    if (!added) break;
  }
  return selected;
}

const options = readOptions(process.argv.slice(2));
const [corpusText, baselineText, candidateText] = await Promise.all([
  readFile(options.corpus, "utf8"),
  readFile(options.baseline, "utf8"),
  readFile(options.candidate, "utf8"),
]);
const snapshots = parseJsonl(corpusText, options.corpus);
const baselineOutcomes = parseJsonl(baselineText, options.baseline);
const candidateOutcomes = parseJsonl(candidateText, options.candidate);
if (baselineOutcomes.length !== snapshots.length || candidateOutcomes.length !== snapshots.length) {
  throw new Error(
    `Outcome cardinality mismatch: corpus=${snapshots.length}, baseline=${baselineOutcomes.length}, candidate=${candidateOutcomes.length}.`,
  );
}
const baselineById = indexUnique(baselineOutcomes, options.baseline);
const candidateById = indexUnique(candidateOutcomes, options.candidate);

const pairs = snapshots.map((snapshot, index) => {
  const baseline = baselineById.get(snapshot.snapshotId);
  const candidate = candidateById.get(snapshot.snapshotId);
  if (!baseline || !candidate) {
    throw new Error(`Missing paired outcome for ${snapshot.snapshotId}.`);
  }
  if (baselineOutcomes[index].snapshotId !== snapshot.snapshotId) {
    throw new Error(`Baseline output order diverges at ${snapshot.snapshotId}.`);
  }
  if (candidateOutcomes[index].snapshotId !== snapshot.snapshotId) {
    throw new Error(`Candidate output order diverges at ${snapshot.snapshotId}.`);
  }
  assertOutcomeMatchesSnapshot(snapshot, baseline, "baseline");
  assertOutcomeMatchesSnapshot(snapshot, candidate, "candidate");

  const baselineRoot = rootAction(baseline);
  const candidateRoot = rootAction(candidate);
  const rootChoiceConcordance = baselineRoot === candidateRoot;
  const classification = pairClassification(
    baselineRoot,
    candidateRoot,
    baseline,
    candidate,
  );
  const rootRegretValid =
    Number.isFinite(baseline.initialRoot?.regret) &&
    Number.isFinite(candidate.initialRoot?.regret);
  const provenance = disagreementProvenance(baselineRoot, candidateRoot, baseline, candidate);
  const baselineDecision = decisionIdentity(baseline);
  const candidateDecision = decisionIdentity(candidate);
  return {
    snapshotId: snapshot.snapshotId,
    stateHash: snapshot.stateHash,
    sourceGameId: sourceGameId(snapshot),
    sourceBlockId: blockId(snapshot),
    sourceBlock: snapshot.sourceBlock,
    sourceRotation: snapshot.sourceRotation,
    targetSeat: snapshot.targetSeat,
    baselineRoot,
    candidateRoot,
    rootChoiceConcordance,
    classification,
    provenance,
    baseline: {
      decision: baselineDecision,
      buildGitSha: baselineDecision.buildGitSha,
      engineRevision: baselineDecision.engineRevision,
      targetWin: baseline.targetWin,
      finalRank: baseline.finalRank,
      finalVictoryPoints: baseline.finalVictoryPoints,
      bestOpponentVictoryPoints: baseline.bestOpponentVictoryPoints,
      victoryPointMargin: baseline.victoryPointMargin,
      longestRoadAcquired: baseline.longestRoadAcquired,
      largestArmyAcquired: baseline.largestArmyAcquired,
      mechanisms: mechanismRecord(baseline),
      rootRegret: baseline.initialRoot?.regret ?? null,
      authority: baseline.nativeGpuInitialDiagnostics?.authority ?? null,
      algorithm: baseline.nativeGpuInitialDiagnostics?.algorithm ?? null,
      deadlineReached: baseline.nativeGpuInitialDiagnostics?.deadlineReached ?? false,
      cutoff: baseline.cutoff,
      illegalActionFailures: baseline.illegalActionFailures,
      protocolFailures: baseline.protocolFailures,
    },
    candidate: {
      decision: candidateDecision,
      buildGitSha: candidateDecision.buildGitSha,
      engineRevision: candidateDecision.engineRevision,
      targetWin: candidate.targetWin,
      finalRank: candidate.finalRank,
      finalVictoryPoints: candidate.finalVictoryPoints,
      bestOpponentVictoryPoints: candidate.bestOpponentVictoryPoints,
      victoryPointMargin: candidate.victoryPointMargin,
      longestRoadAcquired: candidate.longestRoadAcquired,
      largestArmyAcquired: candidate.largestArmyAcquired,
      mechanisms: mechanismRecord(candidate),
      rootRegret: candidate.initialRoot?.regret ?? null,
      authority: candidate.nativeGpuInitialDiagnostics?.authority ?? null,
      algorithm: candidate.nativeGpuInitialDiagnostics?.algorithm ?? null,
      deadlineReached: candidate.nativeGpuInitialDiagnostics?.deadlineReached ?? false,
      cutoff: candidate.cutoff,
      illegalActionFailures: candidate.illegalActionFailures,
      protocolFailures: candidate.protocolFailures,
    },
    deltas: {
      targetVictoryPoints: candidate.finalVictoryPoints - baseline.finalVictoryPoints,
      victoryPointMargin: candidate.victoryPointMargin - baseline.victoryPointMargin,
      finalRank: candidate.finalRank - baseline.finalRank,
      rankImprovement: baseline.finalRank - candidate.finalRank,
      rootRegret: rootRegretValid
        ? candidate.initialRoot.regret - baseline.initialRoot.regret
        : null,
      mechanisms: mechanismDelta(baseline, candidate),
    },
  };
});

const classificationCounts = Object.fromEntries(
  ["concordant", "candidate-rescue", "candidate-regression", "outcome-neutral-disagreement"].map(
    (name) => [name, pairs.filter((pair) => pair.classification === name).length],
  ),
);
const disagreements = pairs.filter((pair) => !pair.rootChoiceConcordance);
const concordantOutcomeDivergences = pairs.filter(
  (pair) =>
    pair.rootChoiceConcordance &&
    (pair.baseline.targetWin !== pair.candidate.targetWin ||
      pair.baseline.finalRank !== pair.candidate.finalRank ||
      pair.baseline.finalVictoryPoints !== pair.candidate.finalVictoryPoints ||
      pair.baseline.victoryPointMargin !== pair.candidate.victoryPointMargin),
);
const provenanceBreakdown = {};
for (const pair of disagreements) {
  provenanceBreakdown[pair.provenance.category] =
    (provenanceBreakdown[pair.provenance.category] ?? 0) + 1;
}

const marginBlocks = pairedBlockMeans(pairs, (pair) => pair.deltas.victoryPointMargin);
const vpBlocks = pairedBlockMeans(pairs, (pair) => pair.deltas.targetVictoryPoints);
const rankBlocks = pairedBlockMeans(pairs, (pair) => pair.deltas.finalRank);
const regretBlocks = pairedBlockMeans(pairs, (pair) => pair.deltas.rootRegret);
const validRegretPairs = pairs.filter((pair) => Number.isFinite(pair.deltas.rootRegret));
const sourceGames = new Set(pairs.map((pair) => pair.sourceGameId));
const sourceBlocks = new Set(pairs.map((pair) => pair.sourceBlockId));

const failures = {
  baselineCutoffs: sum(pairs.map((pair) => Number(Boolean(pair.baseline.cutoff)))),
  candidateCutoffs: sum(pairs.map((pair) => Number(Boolean(pair.candidate.cutoff)))),
  baselineIllegalActionFailures: sum(pairs.map((pair) => pair.baseline.illegalActionFailures ?? 0)),
  candidateIllegalActionFailures: sum(pairs.map((pair) => pair.candidate.illegalActionFailures ?? 0)),
  baselineProtocolFailures: sum(pairs.map((pair) => pair.baseline.protocolFailures ?? 0)),
  candidateProtocolFailures: sum(pairs.map((pair) => pair.candidate.protocolFailures ?? 0)),
  baselineNativeGpuDeadlines: sum(pairs.map((pair) => Number(Boolean(pair.baseline.deadlineReached)))),
  candidateNativeGpuDeadlines: sum(pairs.map((pair) => Number(Boolean(pair.candidate.deadlineReached)))),
};
const regressions = pairs.filter((pair) => pair.classification === "candidate-regression");
const netRescues = classificationCounts["candidate-rescue"] - classificationCounts["candidate-regression"];
const regressionsCausallyAttributed = regressions.every(
  (pair) => !pair.rootChoiceConcordance && pair.provenance.category !== "unavailable",
);

const summary = {
  schemaVersion: 2,
  kind: "wave4-matched-takeover-evidence",
  inputs: options,
  revisions: {
    baselineBuilds: [...new Set(pairs.map((pair) => pair.baseline.buildGitSha))],
    candidateBuilds: [...new Set(pairs.map((pair) => pair.candidate.buildGitSha))],
    baselineEngineRevisions: [...new Set(pairs.map((pair) => pair.baseline.engineRevision))],
    candidateEngineRevisions: [...new Set(pairs.map((pair) => pair.candidate.engineRevision))],
    baselineDecisionBackends: [...new Set(pairs.map((pair) => pair.baseline.decision.backend))],
    candidateDecisionBackends: [...new Set(pairs.map((pair) => pair.candidate.decision.backend))],
    baselineDevices: [...new Set(pairs.map((pair) => pair.baseline.decision.device?.name).filter(Boolean))],
    candidateDevices: [...new Set(pairs.map((pair) => pair.candidate.decision.device?.name).filter(Boolean))],
    baselineAuthorities: [...new Set(pairs.map((pair) => pair.baseline.authority).filter(Boolean))],
    candidateAuthorities: [...new Set(pairs.map((pair) => pair.candidate.authority).filter(Boolean))],
  },
  corpus: {
    snapshots: pairs.length,
    sourceGames: sourceGames.size,
    sourceBlocks: sourceBlocks.size,
  },
  stateRngMatching: {
    exact: true,
    verifiedSnapshots: pairs.length,
    fields: ["snapshotId", "stateHash", "boardSeed", "chanceSeed", "chanceRngState", "policyRngStates"],
  },
  rootChoice: {
    concordant: pairs.length - disagreements.length,
    disagreements: disagreements.length,
    concordanceRate: pairs.length === 0 ? null : (pairs.length - disagreements.length) / pairs.length,
    concordantOutcomeDivergences: concordantOutcomeDivergences.length,
  },
  classifications: classificationCounts,
  netRescues,
  pairedDeltas: {
    meanTargetVictoryPoints: mean(pairs.map((pair) => pair.deltas.targetVictoryPoints)),
    meanVictoryPointMargin: mean(pairs.map((pair) => pair.deltas.victoryPointMargin)),
    meanFinalRank: mean(pairs.map((pair) => pair.deltas.finalRank)),
    meanRankImprovement: mean(pairs.map((pair) => pair.deltas.rankImprovement)),
    blockBootstrap95Ci: {
      targetVictoryPoints: bootstrapInterval(vpBlocks.map((entry) => entry.value), 0x51425650),
      victoryPointMargin: bootstrapInterval(marginBlocks.map((entry) => entry.value), 0x514d4152),
      finalRank: bootstrapInterval(rankBlocks.map((entry) => entry.value), 0x5152414e),
    },
  },
  mechanismEventDeltas: aggregateMechanismDeltas(pairs),
  rootRegret: {
    validPairs: validRegretPairs.length,
    unavailablePairs: pairs.length - validRegretPairs.length,
    meanBaseline: mean(validRegretPairs.map((pair) => pair.baseline.rootRegret)),
    meanCandidate: mean(validRegretPairs.map((pair) => pair.candidate.rootRegret)),
    meanDelta: mean(validRegretPairs.map((pair) => pair.deltas.rootRegret)),
    blockBootstrap95Ci: bootstrapInterval(regretBlocks.map((entry) => entry.value), 0x51524547),
  },
  provenanceBreakdown,
  failures,
  regressions: regressions.map((pair) => ({
    snapshotId: pair.snapshotId,
    sourceGameId: pair.sourceGameId,
    sourceBlockId: pair.sourceBlockId,
    baselineRoot: pair.baselineRoot,
    candidateRoot: pair.candidateRoot,
    provenance: pair.provenance,
    deltas: pair.deltas,
  })),
  gate: {
    commonRandomReplayValid: concordantOutcomeDivergences.length === 0,
    noIllegalProtocolOrCutoffFailures: Object.values(failures).every((value) => value === 0),
    nonNegativeNetRescueEvidence: netRescues >= 0,
    regressionSetCausallyAttributed: regressionsCausallyAttributed,
  },
  pairs,
};

await mkdir(dirname(options.output), { recursive: true });
await writeFile(options.output, `${JSON.stringify(summary, null, 2)}\n`);

let selectedCount = null;
if (options.selectOutput) {
  const selected = selectFrozenCorpus(
    snapshots,
    pairs,
    options.selectMax,
    options.selectMinTurn,
  );
  selectedCount = selected.length;
  if (selected.length === 0) {
    throw new Error("No disagreement/mechanism snapshots were eligible for the frozen corpus.");
  }
  await mkdir(dirname(options.selectOutput), { recursive: true });
  await Promise.all([
    writeFile(
      options.selectOutput,
      `${selected.map((entry) => JSON.stringify(entry.snapshot)).join("\n")}\n`,
    ),
    writeFile(
      options.selectManifest,
      `${JSON.stringify(
        {
          schemaVersion: 1,
          kind: "wave4-matched-takeover-frozen-corpus",
          sourceCorpus: options.corpus,
          screeningBaseline: options.baseline,
          screeningCandidate: options.candidate,
          maximumSnapshots: options.selectMax,
          minimumTurn: options.selectMinTurn,
          selected: selected.length,
          sourceGames: new Set(selected.map((entry) => entry.pair.sourceGameId)).size,
          sourceBlocks: new Set(selected.map((entry) => entry.pair.sourceBlockId)).size,
          selectionRule: {
            eligible: "baseline/candidate first nontrivial target-root disagreement OR targeted Task 9 mechanism event in either arm",
            sourceGameCap: 1,
            sourceBlockCap: 1,
            playerCountRoundRobin: true,
            minimumTurn: options.selectMinTurn,
            priority: "mechanism event, then non-trade root disagreement, then trade-only root disagreement; later turn breaks ties",
          },
          records: selected.map((entry) => ({
            snapshotId: entry.snapshot.snapshotId,
            sourceGameId: entry.pair.sourceGameId,
            sourceBlockId: entry.pair.sourceBlockId,
            rootDisagreement: entry.disagreement,
            mechanismLabels: entry.labels,
            baselineRoot: entry.pair.baselineRoot,
            candidateRoot: entry.pair.candidateRoot,
            baselineRootFamily: entry.baselineFamily,
            candidateRootFamily: entry.candidateFamily,
          })),
        },
        null,
        2,
      )}\n`,
    ),
  ]);
}

console.log(
  JSON.stringify(
    {
      output: options.output,
      snapshots: pairs.length,
      sourceGames: sourceGames.size,
      sourceBlocks: sourceBlocks.size,
      rootDisagreements: disagreements.length,
      classifications: classificationCounts,
      netRescues,
      failures,
      selected: selectedCount,
    },
    null,
    2,
  ),
);
