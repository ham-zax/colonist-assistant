#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const QUESTION_FAMILIES = new Set(["first_settlement", "second_settlement", "setup_road"]);
const PARTITIONS = new Set(["calibration", "holdout"]);
const CONCLUSIVE_LABELS = new Set(["left", "right"]);
const SECRET_KEY_PATTERN = /api.?key|authorization|bearer|credential|secret|token/i;

function parseArgs(argv) {
  const config = { input: null, output: null };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    if (flag === "--input") config.input = argv[++index];
    else if (flag === "--output") config.output = argv[++index];
    else if (flag === "--help" || flag === "-h") {
      console.log("node scripts/analyze-jev-opening-calibration.mjs --input DATASET.jsonl [--output REPORT.json]");
      process.exit(0);
    } else throw new Error(`Unknown argument: ${flag}`);
  }
  if (!config.input) throw new Error("--input is required");
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

function isFiniteNumber(value) {
  return typeof value === "number" && Number.isFinite(value);
}

function scanSecretKeys(value, location, errors) {
  if (Array.isArray(value)) {
    value.forEach((entry, index) => scanSecretKeys(entry, `${location}[${index}]`, errors));
    return;
  }
  if (!value || typeof value !== "object") return;
  for (const [key, child] of Object.entries(value)) {
    if (SECRET_KEY_PATTERN.test(key) && key !== "rawCacheKey") {
      errors.push(`${location}.${key}: secret-shaped fields are forbidden`);
    }
    scanSecretKeys(child, `${location}.${key}`, errors);
  }
}

function validateCandidateOrder(order, candidateIds, location, errors) {
  if (!Array.isArray(order) || order.length !== candidateIds.length) {
    errors.push(`${location}: must contain every candidate exactly once`);
    return;
  }
  const actual = [...order].sort();
  const expected = [...candidateIds].sort();
  if (actual.some((value, index) => value !== expected[index])) {
    errors.push(`${location}: must be a permutation of candidate ids`);
  }
}

function validateProbabilityMap(probabilities, candidateIds, location, errors) {
  if (!probabilities || typeof probabilities !== "object") {
    errors.push(`${location}: probabilities object is required`);
    return;
  }
  const keys = Object.keys(probabilities).sort();
  const expected = [...candidateIds].sort();
  if (keys.length !== expected.length || keys.some((key, index) => key !== expected[index])) {
    errors.push(`${location}: probabilities must name every candidate exactly once`);
    return;
  }
  let total = 0;
  for (const candidateId of candidateIds) {
    const value = probabilities[candidateId];
    if (!isFiniteNumber(value) || value < 0 || value > 1) {
      errors.push(`${location}.${candidateId}: must be in [0, 1]`);
    } else total += value;
  }
  if (Math.abs(total - 1) > 0.02) errors.push(`${location}: probabilities must sum to 1 (±0.02)`);
}

function validateRecord(record, index, splitPartitions, caseIds) {
  const location = `record ${index + 1}`;
  const errors = [];
  scanSecretKeys(record, location, errors);
  if (record.recordType !== "openingCalibrationCase" || record.schemaVersion !== 1) {
    errors.push(`${location}: recordType/schemaVersion must be openingCalibrationCase/1`);
  }
  if (typeof record.caseId !== "string" || !record.caseId) errors.push(`${location}.caseId is required`);
  else if (caseIds.has(record.caseId)) errors.push(`${location}.caseId is duplicated`);
  else caseIds.add(record.caseId);
  if (!QUESTION_FAMILIES.has(record.questionFamily)) errors.push(`${location}.questionFamily is invalid`);
  if (typeof record.splitGroup !== "string" || !record.splitGroup) errors.push(`${location}.splitGroup is required`);
  if (!PARTITIONS.has(record.partition)) errors.push(`${location}.partition is invalid`);
  if (record.splitGroup && PARTITIONS.has(record.partition)) {
    const previous = splitPartitions.get(record.splitGroup);
    if (previous && previous !== record.partition) {
      errors.push(`${location}: splitGroup ${record.splitGroup} leaks across ${previous}/${record.partition}`);
    } else splitPartitions.set(record.splitGroup, record.partition);
  }
  if (!["draft", "validated"].includes(record.featureSemantics?.status)
      || typeof record.featureSemantics?.version !== "string") {
    errors.push(`${location}.featureSemantics must contain version and draft|validated status`);
  }
  const source = record.source;
  if (!source || typeof source !== "object") errors.push(`${location}.source is required`);
  else {
    for (const field of ["boardTemplateId", "stateHash", "phase", "questionVersion"]) {
      if (typeof source[field] !== "string" || !source[field]) errors.push(`${location}.source.${field} is required`);
    }
    for (const field of ["game", "boardSeed", "chanceSeed", "decisionIndex", "actor", "playerCount"]) {
      if (!Number.isSafeInteger(source[field]) || source[field] < 0) errors.push(`${location}.source.${field} must be a non-negative safe integer`);
    }
    if (![2, 3, 4].includes(source.playerCount)) errors.push(`${location}.source.playerCount must be 2, 3, or 4`);
    if (typeof source.playerTradesEnabled !== "boolean") errors.push(`${location}.source.playerTradesEnabled must be boolean`);
  }
  if (!Array.isArray(record.candidates) || record.candidates.length < 2) {
    errors.push(`${location}.candidates must contain at least two candidates`);
    return errors;
  }
  const candidateIds = record.candidates.map((candidate) => candidate.candidateId);
  if (candidateIds.some((id) => typeof id !== "string" || !id)
      || new Set(candidateIds).size !== candidateIds.length) {
    errors.push(`${location}.candidates require unique non-empty candidateId values`);
  }
  const incumbentCount = record.candidates.filter((candidate) => candidate.incumbent === true).length;
  if (incumbentCount !== 1) errors.push(`${location}.candidates require exactly one incumbent`);
  for (const candidate of record.candidates) {
    if (candidate.deterministicScore !== undefined && !isFiniteNumber(candidate.deterministicScore)) {
      errors.push(`${location}.${candidate.candidateId}.deterministicScore must be finite`);
    }
    if (!candidate.deterministicFeatures || typeof candidate.deterministicFeatures !== "object") {
      errors.push(`${location}.${candidate.candidateId}.deterministicFeatures is required`);
    } else if (Object.values(candidate.deterministicFeatures).some((value) => !isFiniteNumber(value))) {
      errors.push(`${location}.${candidate.candidateId}.deterministicFeatures must be numeric`);
    }
  }
  const directRuns = record.judgments?.direct?.runs;
  if (!Array.isArray(directRuns) || directRuns.length === 0) errors.push(`${location}.judgments.direct.runs requires at least one run`);
  else directRuns.forEach((run, runIndex) => {
    const runLocation = `${location}.judgments.direct.runs[${runIndex}]`;
    if (!/^[0-9a-f]{64}$/.test(run.rawCacheKey ?? "")) errors.push(`${runLocation}.rawCacheKey is invalid`);
    if (typeof run.wordingVariant !== "string" || !run.wordingVariant) errors.push(`${runLocation}.wordingVariant is required`);
    if (!isFiniteNumber(run.confidence) || run.confidence < 0 || run.confidence > 1) errors.push(`${runLocation}.confidence must be in [0, 1]`);
    validateCandidateOrder(run.candidateOrder, candidateIds, `${runLocation}.candidateOrder`, errors);
    validateProbabilityMap(run.probabilities, candidateIds, `${runLocation}.probabilities`, errors);
  });
  const dimensionRuns = record.judgments?.dimensions?.runs;
  if (!Array.isArray(dimensionRuns) || dimensionRuns.length === 0) errors.push(`${location}.judgments.dimensions.runs requires at least one run`);
  else dimensionRuns.forEach((run, runIndex) => {
    const runLocation = `${location}.judgments.dimensions.runs[${runIndex}]`;
    if (!/^[0-9a-f]{64}$/.test(run.rawCacheKey ?? "")) errors.push(`${runLocation}.rawCacheKey is invalid`);
    if (typeof run.wordingVariant !== "string" || !run.wordingVariant) errors.push(`${runLocation}.wordingVariant is required`);
    validateCandidateOrder(run.candidateOrder, candidateIds, `${runLocation}.candidateOrder`, errors);
    for (const candidateId of candidateIds) {
      const dimensions = run.scores?.[candidateId];
      if (!dimensions || typeof dimensions !== "object" || Object.keys(dimensions).length === 0) {
        errors.push(`${runLocation}.scores.${candidateId}: at least one dimension is required`);
        continue;
      }
      for (const [dimension, judgment] of Object.entries(dimensions)) {
        if (!isFiniteNumber(judgment?.score) || judgment.score < 0 || judgment.score > 4) {
          errors.push(`${runLocation}.scores.${candidateId}.${dimension}.score must be in [0, 4]`);
        }
        if (!isFiniteNumber(judgment?.confidence) || judgment.confidence < 0 || judgment.confidence > 1) {
          errors.push(`${runLocation}.scores.${candidateId}.${dimension}.confidence must be in [0, 1]`);
        }
      }
    }
  });
  if (!Array.isArray(record.matchedComparisons) || record.matchedComparisons.length === 0) {
    errors.push(`${location}.matchedComparisons requires at least one comparison`);
  } else {
    const comparisonPairs = new Set();
    record.matchedComparisons.forEach((comparison, comparisonIndex) => {
      const comparisonLocation = `${location}.matchedComparisons[${comparisonIndex}]`;
      if (!candidateIds.includes(comparison.leftCandidateId) || !candidateIds.includes(comparison.rightCandidateId)
          || comparison.leftCandidateId === comparison.rightCandidateId) {
        errors.push(`${comparisonLocation}: candidate ids must name two distinct candidates`);
      }
      const pairKey = [comparison.leftCandidateId, comparison.rightCandidateId].sort().join("\u0000");
      if (comparisonPairs.has(pairKey)) errors.push(`${comparisonLocation}: candidate pair is duplicated`);
      else comparisonPairs.add(pairKey);
      if (!["left", "right", "tie", "inconclusive"].includes(comparison.label)) errors.push(`${comparisonLocation}.label is invalid`);
      if (comparison.authority !== "matched_terminal_simulation" || comparison.terminal !== true) {
        errors.push(`${comparisonLocation}: only matched terminal simulation is authoritative`);
      }
      if (!Number.isInteger(comparison.pairedSamples) || comparison.pairedSamples < 1) errors.push(`${comparisonLocation}.pairedSamples must be positive`);
      if (!Array.isArray(comparison.continuationSeeds)
          || comparison.continuationSeeds.length !== comparison.pairedSamples
          || comparison.continuationSeeds.some((seed) => !Number.isSafeInteger(seed) || seed < 0)) {
        errors.push(`${comparisonLocation}.continuationSeeds must contain one non-negative safe integer per paired sample`);
      }
      if (comparison.meanDelta !== null && !isFiniteNumber(comparison.meanDelta)) errors.push(`${comparisonLocation}.meanDelta must be finite or null`);
      if (!Array.isArray(comparison.ci95) || comparison.ci95.length !== 2
          || comparison.ci95.some((value) => value !== null && !isFiniteNumber(value))) {
        errors.push(`${comparisonLocation}.ci95 must contain two numbers or nulls`);
      }
    });
  }
  return errors;
}

function mean(values) {
  return values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : null;
}

function candidateScores(record) {
  const ids = record.candidates.map((candidate) => candidate.candidateId);
  const direct = Object.fromEntries(ids.map((id) => [id, mean(record.judgments.direct.runs.map((run) => run.probabilities[id]))]));
  const dimensions = {};
  const dimensionsConfidenceWeighted = {};
  for (const id of ids) {
    const judgments = record.judgments.dimensions.runs.flatMap((run) => Object.values(run.scores[id]));
    dimensions[id] = mean(judgments.map((judgment) => judgment.score / 4));
    const confidenceWeight = judgments.reduce((sum, judgment) => sum + judgment.confidence, 0);
    dimensionsConfidenceWeighted[id] = confidenceWeight
      ? judgments.reduce((sum, judgment) => sum + (judgment.score / 4) * judgment.confidence, 0) / confidenceWeight
      : dimensions[id];
  }
  const deterministic = Object.fromEntries(record.candidates.map((candidate) => [candidate.candidateId, candidate.deterministicScore ?? null]));
  return { direct, dimensions, dimensionsConfidenceWeighted, deterministic };
}

function buildExamples(records) {
  const examples = [];
  for (const record of records) {
    const scores = candidateScores(record);
    const dimensionConfidences = record.judgments.dimensions.runs.flatMap((run) =>
      Object.values(run.scores).flatMap((dimensions) =>
        Object.values(dimensions).map((judgment) => judgment.confidence)
      )
    );
    const methodConfidence = {
      direct: mean(record.judgments.direct.runs.map((run) => run.confidence)),
      dimensions: mean(dimensionConfidences),
      dimensionsConfidenceWeighted: mean(dimensionConfidences),
      deterministic: null,
    };
    const incumbent = record.candidates.find((candidate) => candidate.incumbent).candidateId;
    for (const comparison of record.matchedComparisons) {
      if (!CONCLUSIVE_LABELS.has(comparison.label)) continue;
      const expected = comparison.label === "left" ? 1 : -1;
      for (const [method, byCandidate] of Object.entries(scores)) {
        const left = byCandidate[comparison.leftCandidateId];
        const right = byCandidate[comparison.rightCandidateId];
        if (!isFiniteNumber(left) || !isFiniteNumber(right)) continue;
        examples.push({
          caseId: record.caseId,
          family: record.questionFamily,
          partition: record.partition,
          method,
          confidence: methodConfidence[method],
          delta: left - right,
          expected,
          leftCandidateId: comparison.leftCandidateId,
          rightCandidateId: comparison.rightCandidateId,
          incumbent,
          corpusTags: record.corpusTags,
          screeningStatus: record.screening.status,
        });
      }
    }
  }
  return examples;
}

function confidenceDiagnostics(examples) {
  const bins = [
    { label: "low", minimum: 0, maximum: 0.5 },
    { label: "medium", minimum: 0.5, maximum: 0.8 },
    { label: "high", minimum: 0.8, maximum: 1.0000001 },
  ];
  return bins.map((bin) => {
    const rows = examples.filter((example) =>
      isFiniteNumber(example.confidence)
      && example.confidence >= bin.minimum
      && example.confidence < bin.maximum
    );
    return {
      bin: bin.label,
      comparisons: rows.length,
      meanReportedConfidence: mean(rows.map((row) => row.confidence)),
      directionalAccuracy: rows.length ? evaluate(rows, 0).accuracy : null,
    };
  });
}

function evaluate(examples, threshold) {
  let predicted = 0;
  let correct = 0;
  let falsePositives = 0;
  let falsePositiveOpportunities = 0;
  for (const example of examples) {
    const expectedCandidate = example.expected === 1 ? example.leftCandidateId : example.rightCandidateId;
    if (expectedCandidate === example.incumbent) falsePositiveOpportunities += 1;
    const prediction = example.delta > threshold ? 1 : example.delta < -threshold ? -1 : 0;
    if (prediction === 0) continue;
    predicted += 1;
    if (prediction === example.expected) correct += 1;
    const predictedCandidate = prediction === 1 ? example.leftCandidateId : example.rightCandidateId;
    if (expectedCandidate === example.incumbent && predictedCandidate !== example.incumbent) falsePositives += 1;
  }
  return {
    comparisons: examples.length,
    predicted,
    coverage: examples.length ? predicted / examples.length : null,
    abstentionRate: examples.length ? 1 - (predicted / examples.length) : null,
    accuracy: predicted ? correct / predicted : null,
    falsePositiveRate: falsePositiveOpportunities ? falsePositives / falsePositiveOpportunities : null,
  };
}

function selectThreshold(examples) {
  if (!examples.length) return null;
  const minimumPredictions = Math.min(examples.length, Math.max(2, Math.ceil(examples.length / 2)));
  const candidates = [...new Set([0, ...examples.map((example) => Math.abs(example.delta))])].sort((a, b) => a - b);
  const results = candidates
    .map((threshold) => ({ threshold, metrics: evaluate(examples, threshold) }))
    .filter((entry) => entry.metrics.predicted >= minimumPredictions)
    .sort((left, right) =>
      (right.metrics.accuracy ?? -1) - (left.metrics.accuracy ?? -1)
      || (left.metrics.falsePositiveRate ?? 1) - (right.metrics.falsePositiveRate ?? 1)
      || (right.metrics.coverage ?? -1) - (left.metrics.coverage ?? -1)
      || left.threshold - right.threshold
    );
  return results[0] ?? null;
}

function topCandidate(scoreMap) {
  return Object.entries(scoreMap).sort((left, right) => right[1] - left[1] || left[0].localeCompare(right[0]))[0]?.[0] ?? null;
}

function runTop(run, method) {
  if (method === "direct") return topCandidate(run.probabilities);
  const scores = Object.fromEntries(Object.entries(run.scores).map(([candidateId, dimensions]) => [
    candidateId,
    mean(Object.values(dimensions).map((judgment) => judgment.score)),
  ]));
  return topCandidate(scores);
}

function pairConsistency(groups) {
  let comparisons = 0;
  let agreements = 0;
  for (const values of groups.values()) {
    for (let left = 0; left < values.length; left += 1) {
      for (let right = left + 1; right < values.length; right += 1) {
        comparisons += 1;
        if (values[left] === values[right]) agreements += 1;
      }
    }
  }
  return { comparisons, consistency: comparisons ? agreements / comparisons : null };
}

function invariance(records, method) {
  const orderGroups = new Map();
  const wordingGroups = new Map();
  for (const record of records) {
    const runs = method === "direct" ? record.judgments.direct.runs : record.judgments.dimensions.runs;
    for (const run of runs) {
      const top = runTop(run, method);
      const orderKey = `${record.caseId}\u0000${run.wordingVariant}`;
      const wordingKey = `${record.caseId}\u0000${JSON.stringify(run.candidateOrder)}`;
      if (!orderGroups.has(orderKey)) orderGroups.set(orderKey, []);
      if (!wordingGroups.has(wordingKey)) wordingGroups.set(wordingKey, []);
      orderGroups.get(orderKey).push(top);
      wordingGroups.get(wordingKey).push(top);
    }
  }
  return { candidateOrder: pairConsistency(orderGroups), wording: pairConsistency(wordingGroups) };
}

function marginOrdinality(examples) {
  if (examples.length < 3) return { buckets: [], monotone: null };
  const sorted = [...examples].sort((left, right) => Math.abs(left.delta) - Math.abs(right.delta));
  const bucketSize = Math.ceil(sorted.length / 3);
  const buckets = [];
  for (let start = 0; start < sorted.length; start += bucketSize) {
    const bucket = sorted.slice(start, start + bucketSize);
    buckets.push({
      count: bucket.length,
      minMargin: Math.abs(bucket[0].delta),
      maxMargin: Math.abs(bucket[bucket.length - 1].delta),
      accuracy: evaluate(bucket, 0).accuracy,
    });
  }
  const monotone = buckets.every((bucket, index) => index === 0 || bucket.accuracy + 0.02 >= buckets[index - 1].accuracy);
  return { buckets, monotone };
}

function binomialTwoSided(successes, trials) {
  if (!trials) return 1;
  const probability = (k) => {
    let combination = 1;
    for (let i = 1; i <= k; i += 1) combination *= (trials - i + 1) / i;
    return combination * (0.5 ** trials);
  };
  const observed = probability(successes);
  let result = 0;
  for (let k = 0; k <= trials; k += 1) if (probability(k) <= observed + 1e-12) result += probability(k);
  return Math.min(1, result);
}

function compareDirectAndDimensions(examples) {
  const byKey = new Map();
  for (const example of examples.filter((entry) => ["direct", "dimensions"].includes(entry.method))) {
    const key = `${example.caseId}\u0000${example.leftCandidateId}\u0000${example.rightCandidateId}`;
    if (!byKey.has(key)) byKey.set(key, {});
    byKey.get(key)[example.method] = Math.sign(example.delta) === example.expected;
  }
  let directOnly = 0;
  let dimensionsOnly = 0;
  let bothCorrect = 0;
  let bothWrong = 0;
  for (const result of byKey.values()) {
    if (result.direct === undefined || result.dimensions === undefined) continue;
    if (result.direct && result.dimensions) bothCorrect += 1;
    else if (result.direct) directOnly += 1;
    else if (result.dimensions) dimensionsOnly += 1;
    else bothWrong += 1;
  }
  const discordant = directOnly + dimensionsOnly;
  const pValue = binomialTwoSided(Math.min(directOnly, dimensionsOnly), discordant);
  const benefit = pValue < 0.05
    ? dimensionsOnly > directOnly ? "dimensions" : directOnly > dimensionsOnly ? "direct" : "not_demonstrated"
    : "not_demonstrated";
  return { bothCorrect, bothWrong, directOnly, dimensionsOnly, discordant, twoSidedExactP: pValue, benefit };
}

function corpusDiagnostics(examples) {
  const tags = [...new Set(examples.flatMap((example) => example.corpusTags))].sort();
  return Object.fromEntries(tags.map((tag) => {
    const tagged = examples.filter((example) => example.corpusTags.includes(tag));
    const caseCount = new Set(tagged.map((example) => example.caseId)).size;
    const methods = {};
    for (const method of ["direct", "dimensions", "dimensionsConfidenceWeighted", "deterministic"]) {
      const rows = tagged.filter((example) => example.method === method);
      if (rows.length) methods[method] = evaluate(rows, 0);
    }
    return [tag, { caseCount, methods }];
  }));
}

function screeningDiagnostics(examples) {
  const statuses = [...new Set(examples.map((example) => example.screeningStatus))].sort();
  return Object.fromEntries(statuses.map((status) => {
    const screened = examples.filter((example) => example.screeningStatus === status);
    const methods = {};
    for (const method of ["direct", "dimensions", "dimensionsConfidenceWeighted", "deterministic"]) {
      const rows = screened.filter((example) => example.method === method);
      if (rows.length) methods[method] = evaluate(rows, 0);
    }
    return [status, { caseCount: new Set(screened.map((example) => example.caseId)).size, methods }];
  }));
}

function buildReport(records) {
  const examples = buildExamples(records);
  const semanticsValidated = records.length > 0 && records.every((record) => record.featureSemantics.status === "validated");
  const partitions = new Set(records.map((record) => record.partition));
  const calibrationEligible = semanticsValidated && partitions.has("calibration") && partitions.has("holdout");
  const families = {};
  for (const family of QUESTION_FAMILIES) {
    const familyExamples = examples.filter((example) => example.family === family);
    if (!familyExamples.length) continue;
    const methods = {};
    for (const method of ["direct", "dimensions", "dimensionsConfidenceWeighted", "deterministic"]) {
      const methodExamples = familyExamples.filter((example) => example.method === method);
      if (!methodExamples.length) continue;
      const calibration = methodExamples.filter((example) => example.partition === "calibration");
      const holdout = methodExamples.filter((example) => example.partition === "holdout");
      const selected = calibrationEligible ? selectThreshold(calibration) : null;
      const holdoutFixed = evaluate(holdout, 0);
      const methodInvariance = ["direct", "dimensions"].includes(method)
        ? invariance(records.filter((record) => record.questionFamily === family && record.partition === "holdout"), method)
        : null;
      const ordinality = marginOrdinality(holdout);
      const orderingConsistency = methodInvariance?.candidateOrder.consistency;
      const wordingConsistency = methodInvariance?.wording.consistency;
      const rankerEligible = holdoutFixed.comparisons >= 20
        && (holdoutFixed.accuracy ?? 0) >= 0.65
        && (holdoutFixed.coverage ?? 0) >= 0.8
        && ordinality.monotone === true
        && (orderingConsistency ?? 0) >= 0.8
        && (wordingConsistency ?? 0) >= 0.8;
      methods[method] = {
        fixedZeroThreshold: { calibration: evaluate(calibration, 0), holdout: holdoutFixed },
        selectedThreshold: selected?.threshold ?? null,
        calibrationAtSelectedThreshold: selected?.metrics ?? null,
        holdoutAtSelectedThreshold: selected ? evaluate(holdout, selected.threshold) : null,
        ordinality,
        invariance: methodInvariance,
        confidenceDiagnostics: confidenceDiagnostics(holdout),
        rankerEligible,
      };
    }
    families[family] = {
      methods,
      directVsDimensions: compareDirectAndDimensions(
        familyExamples.filter((example) => example.partition === "holdout"),
      ),
      holdoutCorpusDiagnostics: corpusDiagnostics(
        familyExamples.filter((example) => example.partition === "holdout"),
      ),
      holdoutScreeningDiagnostics: screeningDiagnostics(
        familyExamples.filter((example) => example.partition === "holdout"),
      ),
    };
  }
  return {
    schemaVersion: 1,
    kind: "jev-opening-calibration-report",
    caseCount: records.length,
    conclusiveComparisonCount: new Set(examples.map((example) => `${example.caseId}\u0000${example.leftCandidateId}\u0000${example.rightCandidateId}`)).size,
    calibration: {
      eligible: calibrationEligible,
      blocker: calibrationEligible
        ? null
        : !semanticsValidated
          ? "feature semantics are not validated for every case"
          : "both calibration and holdout partitions are required",
      rule: "thresholds are selected per question family on the calibration partition only and evaluated once on holdout",
      minimumCalibrationCoverage: 0.5,
    },
    confidencePolicy: "Jev confidence is recorded and used only in the explicitly named diagnostic variant; it is not treated as calibrated correctness probability.",
    rankerAdmission: {
      minimumHoldoutComparisons: 20,
      minimumPairwiseAccuracy: 0.65,
      minimumCoverage: 0.8,
      minimumOrderAndWordingConsistency: 0.8,
      requiresMonotoneMarginAccuracy: true,
    },
    families,
  };
}

function main() {
  const config = parseArgs(process.argv.slice(2));
  const records = readJsonl(config.input);
  const splitPartitions = new Map();
  const caseIds = new Set();
  const errors = records.flatMap((record, index) => validateRecord(record, index, splitPartitions, caseIds));
  if (errors.length) throw new Error(`Dataset validation failed:\n- ${errors.join("\n- ")}`);
  const report = buildReport(records);
  const serialized = `${JSON.stringify(report, null, 2)}\n`;
  if (config.output) {
    fs.mkdirSync(path.dirname(config.output), { recursive: true });
    fs.writeFileSync(config.output, serialized);
  } else process.stdout.write(serialized);
}

main();
