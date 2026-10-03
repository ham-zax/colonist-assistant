import { describe, expect, it } from "vitest";

import {
  explainDeepSearchDecision,
  roadTargetReachableBefore,
} from "../src/core/engine";
import { actionStats, makeDeepSearch } from "../scripts/ui-preview/fixtures";

describe("player-facing decision rationale", () => {
  const city = { kind: "build-city", targetId: "v2" } as const;
  const dev = { kind: "buy-development" } as const;

  it("describes a close search gap without raw values", () => {
    const rationale = explainDeepSearchDecision(
      makeDeepSearch(city, [actionStats(city, 0.412), actionStats(dev, 0.406)]),
    );
    expect(rationale?.plain).toBe(
      "A close call on modeled value over buy a development card",
    );
    expect(rationale?.plain).not.toMatch(/\d\.\d{3}/);
    expect(rationale?.plain).not.toMatch(/win/i);
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

  it("never claims a new settlement spot when the road target was already reachable (crop6309 D20/D63/D66)", () => {
    const road = { kind: "build-road", targetId: "e:0,1,2" } as const;
    const otherRoad = { kind: "build-road", targetId: "e:-1,2,0" } as const;
    const makeRoadSearch = (roadIntent: Record<string, unknown>) => {
      const search = makeDeepSearch(road, [
        actionStats(road, 0.742),
        actionStats(otherRoad, 0.725),
      ]);
      search.rootProvenance.rootEvidence = [
        {
          action: { ...road },
          roadIntent: {
            targetVertexId: "v:-1,2,0",
            roadsRemaining: 0,
            expectedRolls: 12,
            survivalProbability: 1,
            targetValue: 13.282,
            portfolioValue: 8.239,
            frontierGain: -0.609,
            orderingScore: 2.636,
            ...roadIntent,
          },
          admittedByPromotion: false,
          closeoutGain: 0,
          decisiveCompletionMass: 0,
          tradeRiskPosterior: 0,
          dirtyMonopolyPosterior: 0,
          tradeHardVetoPosterior: 0,
          tradeHardVeto: false,
        },
      ];
      return search;
    };

    // Captured mapping metadata is preferred: no board arg needed in
    // production renderers.
    const captured = explainDeepSearchDecision(
      makeRoadSearch({ targetAlreadyReachable: true }),
    );
    expect(captured?.reasons.join(" ")).toContain(
      "was already reachable before this road",
    );
    expect(captured?.reasons.join(" ")).not.toContain("leaving 0");
    expect(captured?.plain).toContain(
      "The highlighted expansion was already reachable",
    );
    expect(captured?.plain).not.toContain("new settlement spot");

    // Before-board fallback agrees: the D20 pattern where v:-1,2,0 was
    // already in the build-settlement set.
    const alreadyReachable = explainDeepSearchDecision(makeRoadSearch({}), {
      vertices: [],
      edges: [],
      buildableSettlementIds: ["v:-1,2,0"],
      myPlayer: "P0",
    });
    expect(alreadyReachable?.reasons.join(" ")).toContain(
      "was already reachable before this road",
    );

    // Insufficient coverage stays unknown: empty gated lists alone never
    // prove unreachable, so wording stays factual post-move, never "new".
    const unknownBoard = explainDeepSearchDecision(makeRoadSearch({}), {
      vertices: [],
      edges: [],
      buildableSettlementIds: ["v:9,9,9"],
      legalVertexIds: [],
      myPlayer: "P0",
    });
    expect(unknownBoard?.reasons.join(" ")).toContain(
      "best remaining expansion is v:-1,2,0",
    );
    expect(unknownBoard?.plain).not.toContain("new settlement spot");
    expect(unknownBoard?.plain).toContain("after this road");

    // Proven new access via complete geometry still reads as new.
    const newlyOpened = explainDeepSearchDecision(makeRoadSearch({}), {
      vertices: [
        { id: "v:-1,2,0", adjacentHexes: [], adjacentVertices: [] },
        { id: "v:other", adjacentHexes: [], adjacentVertices: [] },
      ],
      edges: [{ id: "e:9", vertices: ["v:other", "v:other2"] }],
      buildableSettlementIds: [],
      myPlayer: "P0",
    });
    expect(newlyOpened?.reasons.join(" ")).toContain(
      "opens settlement access at v:-1,2,0",
    );
    expect(newlyOpened?.plain).toContain("Reaches a new settlement spot");
  });

  it("stays unknown when target-local geometry coverage is incomplete", () => {
    const base = {
      edges: [{ id: "e:t-a", vertices: ["v:t", "v:a"] as [string, string] }],
      buildableSettlementIds: [],
      myPlayer: "P0",
    };
    // A missing referenced neighbor would otherwise read as open.
    expect(
      roadTargetReachableBefore(
        {
          ...base,
          vertices: [
            { id: "v:t", adjacentHexes: [], adjacentVertices: ["v:a", "v:gone"] },
            { id: "v:a", adjacentHexes: [], adjacentVertices: ["v:t"] },
          ],
        },
        "v:t",
      ),
    ).toBeUndefined();
    // A missing incident edge could be the own-road touch.
    expect(
      roadTargetReachableBefore(
        {
          ...base,
          vertices: [
            { id: "v:t", adjacentHexes: [], adjacentVertices: ["v:a", "v:b"] },
            { id: "v:a", adjacentHexes: [], adjacentVertices: ["v:t"] },
            { id: "v:b", adjacentHexes: [], adjacentVertices: ["v:t"] },
          ],
        },
        "v:t",
      ),
    ).toBeUndefined();
  });

  it("proves reachability from complete target-local geometry plus an own-road touch", () => {
    expect(
      roadTargetReachableBefore(
        {
          vertices: [
            { id: "v:t", adjacentHexes: [], adjacentVertices: ["v:a"] },
            { id: "v:a", adjacentHexes: [], adjacentVertices: ["v:t"] },
          ],
          edges: [{ id: "e:t-a", vertices: ["v:t", "v:a"], player: "P0" }],
          buildableSettlementIds: [],
          myPlayer: "P0",
        },
        "v:t",
      ),
    ).toBe(true);
  });

  it("keeps tiny model values distinct instead of flattening to an exact zero", () => {
    const road = { kind: "build-road", targetId: "e:1,1,1" } as const;
    const other = { kind: "build-road", targetId: "e:0,0,1" } as const;
    const search = makeDeepSearch(road, [
      actionStats(road, 0.0028),
      actionStats(other, 0.002),
    ]);
    const rationale = explainDeepSearchDecision(search);
    expect(rationale?.reasons.join(" ")).toContain("0.003");
    expect(rationale?.reasons.join(" ")).toContain("modeled value");
    expect(rationale?.reasons.join(" ")).not.toMatch(/0\.000 ahead/);
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
