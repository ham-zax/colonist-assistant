import { describe, expect, it } from "vitest";

import { explainDeepSearchDecision } from "../src/core/engine";
import { actionStats, makeDeepSearch } from "../scripts/ui-preview/fixtures";

describe("player-facing decision rationale", () => {
  const city = { kind: "build-city", targetId: "v2" } as const;
  const dev = { kind: "buy-development" } as const;

  it("describes a close search gap without raw values", () => {
    const rationale = explainDeepSearchDecision(
      makeDeepSearch(city, [actionStats(city, 0.412), actionStats(dev, 0.406)]),
    );
    expect(rationale?.plain).toBe("A close call over buy a development card");
    expect(rationale?.plain).not.toMatch(/\d\.\d{3}/);
    // Technical detail is kept for the "Why" section and records.
    expect(rationale?.summary).toContain("Deep MaxN");
  });

  it("describes a larger model-value gap without claiming the move is proven better", () => {
    const rationale = explainDeepSearchDecision(
      makeDeepSearch(city, [actionStats(city, 0.5), actionStats(dev, 0.3)]),
    );
    expect(rationale?.plain).toBe(
      "Higher modeled value than buy a development card",
    );
  });

  it.each([false, true])("keeps the time-limit caveat when diagnostic evidence is long (floor complete=%s)", (floorComplete) => {
    const monopoly = { kind: "play-monopoly", resource: "ore" as const };
    const search = makeDeepSearch(monopoly, [actionStats(monopoly, 0.5), actionStats(dev, 0.3)]);
    search.authority = "exact-family";
    search.authorityTrace.exactFamily = "monopoly";
    search.deadlineReached = true;
    search.exactActions = [{
      action: monopoly, value: [0.5, 0, 0, 0], lowerBound: [0.4, 0, 0, 0],
      legalWeight: 1, decisionScore: 0.5, lowerScore: 0.4, comparatorScore: 0.482,
    }];
    search.rootProvenance.rankedRoots = [{ action: monopoly, rank: 1, prior: 1 }];
    search.rootProvenance.prunedRoots = [{ action: dev, preTruncationRank: 2, reason: "branch-truncated" }];
    search.authorityTrace.exactFamilyReplacement = { from: { ...monopoly, resource: "grain" }, to: monopoly };
    search.effectiveSearchEffort = {
      backend: "cpu", timeBudgetMs: 2_000, tacticalMaxDepth: 2, tacticalNodeBudget: 900,
      maxDepth: 5, rootCap: 10, nodesPerDepthWave: 8_000, evidenceEscalationMs: 0,
    };
    search.searchStages = {
      particlePreparationMs: 1, rootScoringMs: 2, exactFamiliesMs: 3, threatSafetyMs: 4,
      onePlyFloorMs: 5, deepWavesMs: 1_985, floorComplete, attemptedDepth: 4,
      evidenceEscalationTriggered: false, evidenceEscalationCompleted: false, evidenceEscalationStrengthened: false,
      evidenceEscalationBaselineNodes: 0, evidenceEscalationNodes: 0, evidenceEscalationMs: 0,
    };
    const rationale = explainDeepSearchDecision(search);
    expect(rationale?.evidence[0]).toContain(floorComplete ? "last completed decision depth 3" : "weaker fallback evidence");
    expect(rationale?.evidence.length).toBeLessThanOrEqual(6);
  });
});
