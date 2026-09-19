#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

function parseArgs(argv) {
  const config = { manifest: null, direct: [], dimensions: [], output: null };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    if (flag === "--manifest") config.manifest = argv[++index];
    else if (flag === "--direct") config.direct.push(argv[++index]);
    else if (flag === "--dimensions") config.dimensions.push(argv[++index]);
    else if (flag === "--output") config.output = argv[++index];
    else if (flag === "--help" || flag === "-h") {
      console.log("node scripts/assemble-jev-opening-calibration.mjs --manifest CASES.json --direct DIRECT.jsonl [--direct ...] --dimensions DIMENSIONS.jsonl [--dimensions ...] --output DATASET.jsonl");
      process.exit(0);
    } else throw new Error(`Unknown argument: ${flag}`);
  }
  if (!config.manifest || !config.output || !config.direct.length || !config.dimensions.length) {
    throw new Error("--manifest, --output, at least one --direct, and at least one --dimensions are required");
  }
  return config;
}

function readJsonl(file) {
  return fs.readFileSync(file, "utf8")
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line, index) => {
      try {
        return JSON.parse(line);
      } catch (error) {
        throw new Error(`${file}:${index + 1}: ${error.message}`);
      }
    });
}

function sourceKey(source) {
  const fields = [
    source.game,
    source.boardSeed,
    source.chanceSeed,
    source.decisionIndex,
    source.stateHash,
    source.phase,
    source.actor,
    source.playerTradesEnabled,
  ];
  if (fields.some((value) => value === undefined || value === null)) {
    throw new Error("source identity requires game, boardSeed, chanceSeed, decisionIndex, stateHash, phase, actor, and playerTradesEnabled");
  }
  return fields.join("|");
}

function candidateMaps(calibrationCase, evaluation) {
  const candidateByAction = new Map(calibrationCase.candidates.map((candidate) => [candidate.action, candidate.candidateId]));
  const candidateByOption = new Map();
  for (const [option, candidate] of Object.entries(evaluation.candidates ?? {})) {
    const candidateId = candidateByAction.get(candidate.action);
    if (!candidateId) {
      throw new Error(`${calibrationCase.caseId}: evaluation contains unmanifested action ${candidate.action}`);
    }
    candidateByOption.set(option, candidateId);
  }
  if (candidateByOption.size !== calibrationCase.candidates.length) {
    throw new Error(`${calibrationCase.caseId}: evaluation candidate count differs from manifest`);
  }
  return { candidateByAction, candidateByOption };
}

function normalizedOrder(calibrationCase, evaluation, candidateByAction) {
  const actions = evaluation.candidateOrder;
  if (!Array.isArray(actions)) throw new Error(`${calibrationCase.caseId}: evaluation lacks candidateOrder`);
  return actions.map((action) => {
    const candidateId = candidateByAction.get(action);
    if (!candidateId) throw new Error(`${calibrationCase.caseId}: candidateOrder contains unknown action ${action}`);
    return candidateId;
  });
}

function requireCacheKey(calibrationCase, evaluation) {
  if (!/^[0-9a-f]{64}$/.test(evaluation.rawCacheKey ?? "")) {
    throw new Error(`${calibrationCase.caseId}: evaluation lacks a valid rawCacheKey`);
  }
  return evaluation.rawCacheKey;
}

function directRun(calibrationCase, evaluation) {
  if (evaluation.pass !== "direct") throw new Error(`${calibrationCase.caseId}: direct input contains pass ${evaluation.pass}`);
  const { candidateByAction, candidateByOption } = candidateMaps(calibrationCase, evaluation);
  const answer = evaluation.answers?.direct_best_candidate;
  if (answer?.type !== "choice" || !answer.probabilities || typeof answer.confidence !== "number") {
    throw new Error(`${calibrationCase.caseId}: direct answer is incomplete`);
  }
  const probabilities = {};
  for (const [option, probability] of Object.entries(answer.probabilities)) {
    const candidateId = candidateByOption.get(option);
    if (!candidateId) throw new Error(`${calibrationCase.caseId}: direct answer names unknown option ${option}`);
    probabilities[candidateId] = probability;
  }
  return {
    rawCacheKey: requireCacheKey(calibrationCase, evaluation),
    candidateOrder: normalizedOrder(calibrationCase, evaluation, candidateByAction),
    wordingVariant: evaluation.wordingVariant,
    probabilities,
    confidence: answer.confidence,
  };
}

function dimensionRun(calibrationCase, evaluation) {
  if (!["pass3", "pass4", "pass5"].includes(evaluation.pass)) {
    throw new Error(`${calibrationCase.caseId}: dimension input contains pass ${evaluation.pass}`);
  }
  const { candidateByAction, candidateByOption } = candidateMaps(calibrationCase, evaluation);
  const scores = Object.fromEntries(calibrationCase.candidates.map((candidate) => [candidate.candidateId, {}]));
  for (const [questionId, answer] of Object.entries(evaluation.answers ?? {})) {
    const split = questionId.indexOf("__");
    if (split < 1 || answer?.type !== "score") continue;
    const option = questionId.slice(0, split);
    const dimension = questionId.slice(split + 2);
    const candidateId = candidateByOption.get(option);
    if (!candidateId) throw new Error(`${calibrationCase.caseId}: dimension answer names unknown option ${option}`);
    scores[candidateId][dimension] = { score: answer.score, confidence: answer.confidence };
  }
  return {
    rawCacheKey: requireCacheKey(calibrationCase, evaluation),
    candidateOrder: normalizedOrder(calibrationCase, evaluation, candidateByAction),
    wordingVariant: evaluation.wordingVariant,
    scores,
  };
}

function main() {
  const config = parseArgs(process.argv.slice(2));
  const manifest = JSON.parse(fs.readFileSync(config.manifest, "utf8"));
  if (manifest.schemaVersion !== 1 || manifest.kind !== "jev-opening-calibration-manifest" || !Array.isArray(manifest.cases)) {
    throw new Error("manifest must be jev-opening-calibration-manifest schemaVersion 1");
  }
  const casesBySource = new Map();
  for (const calibrationCase of manifest.cases) {
    const key = sourceKey(calibrationCase.source);
    if (casesBySource.has(key)) throw new Error(`manifest source identity is duplicated: ${key}`);
    casesBySource.set(key, calibrationCase);
    calibrationCase.recordType = "openingCalibrationCase";
    calibrationCase.schemaVersion = 1;
    calibrationCase.judgments = { direct: { runs: [] }, dimensions: { runs: [] } };
  }
  const ingest = (files, kind) => {
    for (const file of files) {
      for (const evaluation of readJsonl(file)) {
        if (evaluation.recordType !== "jevEvaluation") continue;
        const calibrationCase = casesBySource.get(sourceKey(evaluation.source));
        if (!calibrationCase) throw new Error(`${file}: no manifest case matches ${sourceKey(evaluation.source)}`);
        if (kind === "direct") calibrationCase.judgments.direct.runs.push(directRun(calibrationCase, evaluation));
        else calibrationCase.judgments.dimensions.runs.push(dimensionRun(calibrationCase, evaluation));
      }
    }
  };
  ingest(config.direct, "direct");
  ingest(config.dimensions, "dimensions");
  for (const calibrationCase of manifest.cases) {
    if (!calibrationCase.judgments.direct.runs.length || !calibrationCase.judgments.dimensions.runs.length) {
      throw new Error(`${calibrationCase.caseId}: direct and dimension runs are both required`);
    }
  }
  fs.mkdirSync(path.dirname(config.output), { recursive: true });
  fs.writeFileSync(config.output, `${manifest.cases.map((calibrationCase) => JSON.stringify(calibrationCase)).join("\n")}\n`);
}

main();
