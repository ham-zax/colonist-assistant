import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

const ANALYZER = resolve("scripts/analyze-jev-opening-calibration.mjs");
const ASSEMBLER = resolve("scripts/assemble-jev-opening-calibration.mjs");
const LAB = resolve("scripts/jev-strategy-lab.mjs");
const temporaryDirectories: string[] = [];

function temporaryDirectory(): string {
  const directory = mkdtempSync(join(tmpdir(), "colonist-jev-calibration-"));
  temporaryDirectories.push(directory);
  return directory;
}

function cacheKey(index: number): string {
  return index.toString(16).padStart(64, "0");
}

function run(candidateOrder: string[], wordingVariant: string, leftProbability: number, index: number) {
  return {
    rawCacheKey: cacheKey(index),
    candidateOrder,
    wordingVariant,
    probabilities: { engine: leftProbability, alternative: 1 - leftProbability },
    confidence: 0.9,
  };
}

function dimensionRun(
  candidateOrder: string[],
  wordingVariant: string,
  engineScore: number,
  alternativeScore: number,
  index: number,
) {
  return {
    rawCacheKey: cacheKey(index),
    candidateOrder,
    wordingVariant,
    scores: {
      engine: { portfolio: { score: engineScore, confidence: 0.9 } },
      alternative: { portfolio: { score: alternativeScore, confidence: 0.9 } },
    },
  };
}

function calibrationCase(caseId: string, splitGroup: string, partition: "calibration" | "holdout") {
  return {
    recordType: "openingCalibrationCase",
    schemaVersion: 1,
    caseId,
    questionFamily: "second_settlement",
    splitGroup,
    partition,
    featureSemantics: { version: "opening-a1-v1", status: "validated" },
    source: {
      boardTemplateId: splitGroup,
      game: 0,
      boardSeed: 42,
      chanceSeed: 99,
      decisionIndex: partition === "calibration" ? 1 : 2,
      stateHash: caseId,
      phase: "SetupSettlement",
      actor: 0,
      playerCount: 4,
      playerTradesEnabled: false,
      questionVersion: "opening-direct-dimensions-v1",
    },
    candidates: [
      {
        candidateId: "engine",
        action: "PlaceSettlement { vertex: 1 }",
        incumbent: true,
        deterministicScore: 0.7,
        deterministicFeatures: { production: 18, selfFunding: 0.8 },
      },
      {
        candidateId: "alternative",
        action: "PlaceSettlement { vertex: 2 }",
        incumbent: false,
        deterministicScore: 0.4,
        deterministicFeatures: { production: 15, selfFunding: 0.5 },
      },
    ],
    judgments: {
      direct: {
        runs: [
          run(["engine", "alternative"], "v1", 0.8, 1),
          run(["alternative", "engine"], "v1", 0.75, 2),
          run(["engine", "alternative"], "v2", 0.7, 3),
        ],
      },
      dimensions: {
        runs: [
          dimensionRun(["engine", "alternative"], "v1", 2, 4, 4),
          dimensionRun(["alternative", "engine"], "v1", 2, 3, 5),
          dimensionRun(["engine", "alternative"], "v2", 1, 4, 6),
        ],
      },
    },
    screening: { status: "passed", reasons: [] },
    matchedComparisons: [
      {
        leftCandidateId: "engine",
        rightCandidateId: "alternative",
        label: "left",
        authority: "matched_terminal_simulation",
        terminal: true,
        pairedSamples: 16,
        continuationSeeds: Array.from({ length: 16 }, (_, index) => 1000 + index),
        meanDelta: 0.2,
        ci95: [0.05, 0.35],
      },
    ],
    corpusTags: ["hard-negative"],
  };
}

function writeDataset(records: object[]): { input: string; output: string } {
  const directory = temporaryDirectory();
  const input = join(directory, "dataset.jsonl");
  const output = join(directory, "report.json");
  writeFileSync(input, `${records.map((record) => JSON.stringify(record)).join("\n")}\n`);
  return { input, output };
}

afterEach(() => {
  while (temporaryDirectories.length) rmSync(temporaryDirectories.pop()!, { recursive: true, force: true });
});

describe("Jev opening calibration research tooling", () => {
  it("content-addresses and replays raw Jev responses without credentials", async () => {
    // @ts-ignore Research-only JavaScript module intentionally has no production declaration file.
    const cache = await import("../scripts/jev-raw-cache.mjs");
    const directory = temporaryDirectory();
    const firstRequest = { model: "jev-test", state: { b: 2, a: 1 }, questions: { q: { type: "choice" } } };
    const reorderedRequest = { questions: { q: { type: "choice" } }, state: { a: 1, b: 2 }, model: "jev-test" };

    const key = cache.jevRequestCacheKey(firstRequest);
    expect(cache.jevRequestCacheKey(reorderedRequest)).toBe(key);
    cache.writeCachedJevResponse(directory, key, firstRequest, { answers: { q: { type: "choice" } } });

    expect(cache.readCachedJevResponse(directory, key).record).toMatchObject({
      schemaVersion: 1,
      cacheKey: key,
      response: { answers: { q: { type: "choice" } } },
    });
  });

  it("calibrates per family and keeps direct and dimensions as separate comparators", () => {
    const files = writeDataset([
      calibrationCase("calibration-case", "board-a", "calibration"),
      calibrationCase("holdout-case", "board-b", "holdout"),
    ]);

    execFileSync(process.execPath, [ANALYZER, "--input", files.input, "--output", files.output]);
    const report = JSON.parse(readFileSync(files.output, "utf8"));

    expect(report.calibration.eligible).toBe(true);
    expect(report.families.second_settlement.methods.direct.holdoutAtSelectedThreshold.accuracy).toBe(1);
    expect(report.families.second_settlement.methods.dimensions.holdoutAtSelectedThreshold.accuracy).toBe(0);
    expect(report.families.second_settlement.methods.direct.invariance.candidateOrder.consistency).toBe(1);
    expect(report.families.second_settlement.directVsDimensions).toMatchObject({
      directOnly: 1,
      dimensionsOnly: 0,
      benefit: "not_demonstrated",
    });
  });

  it("assembles cached direct and dimension runs by exact source and action identity", () => {
    const directory = temporaryDirectory();
    const completeCase = calibrationCase("assembled-case", "board-a", "calibration");
    const { recordType: _recordType, schemaVersion: _schemaVersion, judgments: _judgments, ...manifestCase } = completeCase;
    const manifest = join(directory, "manifest.json");
    const direct = join(directory, "direct.jsonl");
    const dimensions = join(directory, "dimensions.jsonl");
    const output = join(directory, "dataset.jsonl");
    writeFileSync(manifest, JSON.stringify({
      schemaVersion: 1,
      kind: "jev-opening-calibration-manifest",
      cases: [manifestCase],
    }));
    const source = {
      game: completeCase.source.game,
      boardSeed: completeCase.source.boardSeed,
      chanceSeed: completeCase.source.chanceSeed,
      decisionIndex: completeCase.source.decisionIndex,
      stateHash: completeCase.source.stateHash,
      phase: completeCase.source.phase,
      actor: completeCase.source.actor,
      playerTradesEnabled: completeCase.source.playerTradesEnabled,
    };
    const engineCandidate = completeCase.candidates[0]!;
    const alternativeCandidate = completeCase.candidates[1]!;
    const candidates = {
      c0: { action: engineCandidate.action },
      c1: { action: alternativeCandidate.action },
    };
    writeFileSync(direct, `${JSON.stringify({
      recordType: "jevEvaluation",
      pass: "direct",
      source,
      candidates,
      candidateOrder: [engineCandidate.action, alternativeCandidate.action],
      wordingVariant: "standard",
      rawCacheKey: cacheKey(20),
      answers: {
        direct_best_candidate: {
          type: "choice",
          probabilities: { c0: 0.8, c1: 0.2 },
          confidence: 0.9,
        },
      },
    })}\n`);
    writeFileSync(dimensions, `${JSON.stringify({
      recordType: "jevEvaluation",
      pass: "pass3",
      source,
      candidates,
      candidateOrder: [alternativeCandidate.action, engineCandidate.action],
      wordingVariant: "alternate",
      rawCacheKey: cacheKey(21),
      answers: {
        c0__portfolio: { type: "score", score: 4, confidence: 0.8 },
        c1__portfolio: { type: "score", score: 2, confidence: 0.7 },
      },
    })}\n`);

    execFileSync(process.execPath, [
      ASSEMBLER,
      "--manifest", manifest,
      "--direct", direct,
      "--dimensions", dimensions,
      "--output", output,
    ]);
    const assembled = JSON.parse(readFileSync(output, "utf8").trim());

    expect(assembled).toMatchObject({
      recordType: "openingCalibrationCase",
      schemaVersion: 1,
      judgments: {
        direct: { runs: [{ probabilities: { engine: 0.8, alternative: 0.2 } }] },
        dimensions: {
          runs: [{
            candidateOrder: ["alternative", "engine"],
            scores: { engine: { portfolio: { score: 4, confidence: 0.8 } } },
          }],
        },
      },
    });
  });

  it("blocks calibration while feature semantics are draft", () => {
    const calibration = calibrationCase("calibration-case", "board-a", "calibration");
    calibration.featureSemantics.status = "draft";
    const files = writeDataset([calibration, calibrationCase("holdout-case", "board-b", "holdout")]);

    execFileSync(process.execPath, [ANALYZER, "--input", files.input, "--output", files.output]);
    const report = JSON.parse(readFileSync(files.output, "utf8"));

    expect(report.calibration).toMatchObject({
      eligible: false,
      blocker: "feature semantics are not validated for every case",
    });
    expect(report.families.second_settlement.methods.direct.selectedThreshold).toBeNull();
  });

  it("rejects grouped split leakage", () => {
    const files = writeDataset([
      calibrationCase("calibration-case", "same-board", "calibration"),
      calibrationCase("holdout-case", "same-board", "holdout"),
    ]);

    expect(() => execFileSync(
      process.execPath,
      [ANALYZER, "--input", files.input, "--output", files.output],
      { stdio: "pipe" },
    )).toThrow(/leaks across calibration\/holdout/);
  });

  it("exposes the one-question direct pass and raw-cache option", () => {
    const help = execFileSync(process.execPath, [LAB, "--help"], { encoding: "utf8" });
    expect(help).toContain("direct|pass1");
    expect(help).toContain("--cache-dir PATH");
  });
});
