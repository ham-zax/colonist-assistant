import {
  BUILD_COSTS,
  RESOURCE_LABELS,
  RESOURCE_ORDER,
  RESOURCE_STRATEGIC_WEIGHTS,
  cloneResources,
  emptyResources,
  hasResources,
  resourceTotal,
  type BuildKind,
  type Resource,
  type ResourceVector,
} from "./resources";
import { NUMBER_PIPS, type BoardSnapshot, type DiceMode } from "./placement";
import type {
  DiceHistoryProvenance,
  StochasticBeliefPolicyId,
  StochasticModelId,
} from "./dice-history";
import { getPlayerEstimate } from "./tracker";
import type { HandWorld, TrackerState } from "./types";
import {
  estimateBuildTempo,
  estimateDevelopmentDeck,
  playerBoardProfile,
  type PlayerBoardProfile,
} from "./strategy";

/** Decision policies supported by the engine transport. Production settings
 * pin Strategist; Weighted remains available for diagnostics and benchmarks. */
export type DecisionEngine = "deep-search" | "weighted";

export const isWasmDecisionEngine = (engine: DecisionEngine): boolean =>
  engine === "deep-search" || engine === "weighted";

export type DecisionRuntime =
  | "background-gpu"
  | "background-wasm"
  | "background-rollout";

export interface NativeGpuBuildIdentity {
  gitSha: string;
  dirty: boolean;
  builtAtUnixMs: number;
  ptxSha256: string;
}

export const isSearchDecisionRuntime = (
  runtime: DecisionRuntime | undefined,
): boolean => runtime === "background-gpu" || runtime === "background-wasm";

export interface DeepSearchAction {
  kind: string;
  tradeId?: string;
  targetId?: string;
  secondTargetId?: string;
  player?: string;
  resource?: Resource;
  otherResource?: Resource;
  ratio?: number;
  cards?: [number, number, number, number, number];
  receiveCards?: [number, number, number, number, number];
  recipients?: string[];
  accept?: boolean;
}

export interface DeepSearchExactActionDiagnostic {
  action: DeepSearchAction;
  value: number[];
  lowerBound: number[];
  legalWeight: number;
  decisionScore: number;
  lowerScore: number;
  comparatorScore: number;
}

export interface DeepSearchStageTimings {
  particlePreparationMs: number;
  rootScoringMs: number;
  exactFamiliesMs: number;
  threatSafetyMs: number;
  onePlyFloorMs: number;
  deepWavesMs: number;
  floorComplete: boolean;
  attemptedDepth: number;
  evidenceEscalationTriggered: boolean;
  evidenceEscalationCompleted: boolean;
  evidenceEscalationStrengthened: boolean;
  evidenceEscalationBaselineNodes: number;
  evidenceEscalationNodes: number;
  evidenceEscalationMs: number;
}

export type DeepSearchEffectiveEffort =
  | {
      backend: "cpu";
      timeBudgetMs: number;
      tacticalMaxDepth: number;
      tacticalNodeBudget: number;
      maxDepth: number;
      rootCap: number;
      nodesPerDepthWave: number;
      evidenceEscalationMs: number;
    }
  | {
      backend: "gpu";
      timeBudgetMs: number;
      tacticalMaxDepth: number;
      tacticalNodeBudget: number;
      rootCap: number;
      rolloutBudget: number;
      rolloutSteps: number;
    };

export interface DeepSearchActionStatistics {
  action: DeepSearchAction;
  visits: number;
  availability: number;
  availabilityWeight: number;
  legalWeight: number;
  prior: number;
  value: number[];
  lowerConfidenceValue: number[];
}

export type DecisionAuthority =
  | "exact-mandatory"
  | "tactical-proven"
  | "deep-maxn"
  | "gpu-root-rollout"
  | "weighted-policy"
  | "exact-family"
  | "safety-override";

export interface DeepSearchActionReplacement {
  from: DeepSearchAction;
  to: DeepSearchAction;
}

export interface DeepSearchRankedRoot {
  action: DeepSearchAction;
  rank: number;
  prior: number;
  plannerValue?: number;
  plannerCompletionMass?: number;
  plannerDecisiveCompletionMass?: number;
  plannerResponseWindows?: number;
}

export interface DeepSearchRetainedRoot {
  action: DeepSearchAction;
  preTruncationRank?: number;
  prior: number;
  nodeBudgetPerParticle: number;
  allocatedNodes: number;
  plannerValue?: number;
  plannerCompletionMass?: number;
  plannerDecisiveCompletionMass?: number;
  plannerResponseWindows?: number;
  finalRank?: number;
  finalEvaluationHorizon?: number;
  initialTerminalOutcome?: number;
  initialTerminalRate?: number;
  initialVictoryMargin?: number;
  initialStrategicMargin?: number;
  terminalOutcome?: number;
  terminalRate?: number;
  terminalLowerBound?: number;
  terminalUpperBound?: number;
  victoryMargin?: number;
  victoryMarginLowerBound?: number;
  victoryMarginUpperBound?: number;
  strategicMargin?: number;
  strategicMarginLowerBound?: number;
  strategicMarginUpperBound?: number;
  meanTurn?: number;
}

export interface DeepSearchPrunedRoot {
  action: DeepSearchAction;
  preTruncationRank?: number;
  reason:
    | "root-excluded"
    | "branch-truncated"
    | "trade-safety"
    | "exact-family-collapsed";
}

export type DeepSearchRootPromotionReason =
  | "road-award-protection"
  | "critical-expansion-protection"
  | "opponent-route-cut"
  | "closeout-compression";

export type DeepSearchDomesticTradeThreat =
  | "dirty-monopoly"
  | "immediate-win"
  | "award-swing"
  | "contested-settlement"
  | "material-build";

export interface DeepSearchIntroducedCriticalVertex {
  vertexId: string;
  roadLoss: number;
  additionalRoadLoss: number;
  awardLoss: boolean;
  awardLossIntroduced: boolean;
  awardVpExposure: number;
  expansionLoss: number;
  additionalExpansionLoss: number;
}

export interface DeepSearchIntroducedRoadFragility {
  criticalVertices: DeepSearchIntroducedCriticalVertex[];
  maximumAdditionalRoadLoss: number;
  awardVpExposure: number;
  maximumAdditionalExpansionLoss: number;
}

export interface DeepSearchRoadCutContinuationEvidence {
  vertexId: string;
  opponent: string;
  posterior: number;
  maritimeTradeRequiredPosterior: number;
  awardLossPosterior: number;
  maximumRoadLoss: number;
  approachEdgeIds: string[];
}

export interface DeepSearchRoadCutContinuationAssessment {
  posterior: number;
  awardLossPosterior: number;
  continuations: DeepSearchRoadCutContinuationEvidence[];
}

export interface DeepSearchRoadIntent {
  targetVertexId?: string;
  roadsRemaining: number;
  expectedRolls?: number;
  survivalProbability: number;
  targetValue: number;
  portfolioValue: number;
  frontierGain: number;
  orderingScore: number;
}

export interface DeepSearchRootCausalEvidence {
  action: DeepSearchAction;
  promotionReason?: DeepSearchRootPromotionReason;
  roadIntent?: DeepSearchRoadIntent;
  introducedRoadFragility?: DeepSearchIntroducedRoadFragility;
  roadCutContinuation?: DeepSearchRoadCutContinuationAssessment;
  admittedByPromotion: boolean;
  closeoutGain: number;
  responseWindows?: number;
  decisiveCompletionMass: number;
  tradeThreat?: DeepSearchDomesticTradeThreat;
  tradeRiskPosterior: number;
  dirtyMonopolyPosterior: number;
  tradeHardVetoPosterior: number;
  tradeHardVeto: boolean;
}

export interface DeepSearchHorizonEscalation {
  reason:
    | "fragile-award-low-terminal-completion"
    | "sparse-terminal-overlapping-strategic-cutoff";
  provisionalWinner: DeepSearchAction;
  initialHorizon: number;
  unresolvedCutMass: number;
  roots: DeepSearchAction[];
  attemptedHorizons: number[];
  completedHorizon?: number;
  finalWinner?: DeepSearchAction;
  deadlineLimited: boolean;
}

export interface DeepSearchRootProvenance {
  rankedRootCount: number;
  rankedRoots: DeepSearchRankedRoot[];
  retainedRoots: DeepSearchRetainedRoot[];
  prunedRootCount: number;
  prunedRoots: DeepSearchPrunedRoot[];
  rootEvidence?: DeepSearchRootCausalEvidence[];
  horizonEscalation?: DeepSearchHorizonEscalation;
  tradeHardVetoThreshold?: number;
  searchWinner?: DeepSearchAction;
  exactFamilyReplacement?: DeepSearchActionReplacement;
  safetyReplacement?: DeepSearchActionReplacement;
}

export interface DeepSearchAuthorityTrace {
  initialAuthority: DecisionAuthority;
  exactFamily?: string;
  exactFamilyReplacement?: DeepSearchActionReplacement;
  safetyReplacement?: DeepSearchActionReplacement;
}

export interface DomesticTradeState {
  give: ResourceVector;
  receive: ResourceVector;
}

export interface RootTradeActionExclusion extends DomesticTradeState {
  kind: "offer-trade" | "counter-trade";
}

export interface DecisionSearchConstraints {
  lastRejectedTrade?: DomesticTradeState;
  rootExclusions?: RootTradeActionExclusion[];
}

export interface DeepSearchResult {
  engineRevision: string;
  diceMode: DiceMode;
  /** Legacy spelling retained for additive evidence compatibility. */
  chanceModel: "fair-iid-2d6";
  requestedStochasticModel: StochasticModelId;
  stochasticModel: StochasticModelId;
  beliefPolicy?: StochasticBeliefPolicyId;
  diceHistoryProvenance?: DiceHistoryProvenance;
  publicHistoryDigest?: string;
  stochasticBeliefDigest?: string;
  stochasticBeliefParticleCount: number;
  rootIndex: number;
  learnedModelVersion?: string;
  tradeModelVersion?: string;
  algorithm: string;
  authority: DecisionAuthority;
  effectiveSearchEffort?: DeepSearchEffectiveEffort;
  chosen?: DeepSearchAction;
  rootValue: number[];
  tacticalWinProbability: number;
  tacticalLowerBound: number;
  tacticalProven: boolean;
  exactDecision: boolean;
  exactWorlds: number;
  exactActions?: DeepSearchExactActionDiagnostic[];
  tacticalLine: DeepSearchAction[];
  actions: DeepSearchActionStatistics[];
  iterations: number;
  nodes: number;
  deepestDecisionDepth: number;
  rollouts: number;
  /** Final joint particles sent across the TypeScript -> WASM boundary. */
  particles: number;
  sourceWorldCount: number;
  wasmParticleCount: number;
  rustPosteriorParticleCount: number;
  rustSearchParticleCount: number;
  searchStages?: DeepSearchStageTimings;
  rootProvenance: DeepSearchRootProvenance;
  authorityTrace: DeepSearchAuthorityTrace;
  mappingFailureReason?: string;
  effectiveParticleCount: number;
  deadlineReached?: boolean;
  elapsedMs: number;
  seed: number;
}

export interface DecisionRationale {
  summary: string;
  reasons: string[];
  evidence: string[];
}

const deepSearchActionKey = (action: DeepSearchAction | undefined): string =>
  JSON.stringify(
    action
      ? [
          action.kind,
          action.tradeId ?? null,
          action.targetId ?? null,
          action.secondTargetId ?? null,
          action.player ?? null,
          action.resource ?? null,
          action.otherResource ?? null,
          action.ratio ?? null,
          action.cards ?? null,
          action.receiveCards ?? null,
          action.recipients ?? null,
          action.accept ?? null,
        ]
      : null,
  );

const sameDeepSearchAction = (
  left: DeepSearchAction | undefined,
  right: DeepSearchAction | undefined,
): boolean => deepSearchActionKey(left) === deepSearchActionKey(right);

const fixedScore = (value: number | undefined): string =>
  Number.isFinite(value) ? (value as number).toFixed(3) : "n/a";

const percentWeight = (value: number | undefined): string =>
  `${Math.round(Math.max(0, Math.min(1, value ?? 0)) * 100)}%`;

const tupleLabel = (
  cards: [number, number, number, number, number] | undefined,
): string => {
  if (!cards) return "resources";
  const parts = RESOURCE_ORDER.flatMap((resource, index) => {
    const count = cards[index] ?? 0;
    return count > 0
      ? [`${count > 1 ? `${count}× ` : ""}${RESOURCE_LABELS[resource].toLowerCase()}`]
      : [];
  });
  return parts.length ? parts.join(" + ") : "nothing";
};

export const describeDeepSearchAction = (action: DeepSearchAction): string => {
  const target = action.targetId ? ` at ${action.targetId}` : "";
  switch (action.kind) {
    case "roll":
      return "roll the dice";
    case "end-turn":
      return "end the turn";
    case "place-settlement":
    case "build-settlement":
      return `build a settlement${target}`;
    case "build-city":
      return `build a city${target}`;
    case "place-road":
    case "build-road":
      return `build a road${target}`;
    case "buy-development":
      return "buy a development card";
    case "move-robber":
      return `move the robber${target}${action.player ? ` and steal from ${action.player}` : ""}`;
    case "discard":
      return `discard ${tupleLabel(action.cards)}`;
    case "maritime-trade":
      return `trade ${action.ratio ?? 4}× ${action.resource ? RESOURCE_LABELS[action.resource].toLowerCase() : "a resource"} for ${action.otherResource ? RESOURCE_LABELS[action.otherResource].toLowerCase() : "another resource"}`;
    case "offer-trade":
      return `offer ${tupleLabel(action.cards)} for ${tupleLabel(action.receiveCards)}`;
    case "counter-trade":
      return `counter with ${tupleLabel(action.cards)} for ${tupleLabel(action.receiveCards)}`;
    case "respond-trade":
      return action.accept ? "accept the trade" : "decline the trade";
    case "confirm-trade":
      return action.player ? `trade with ${action.player}` : "confirm the accepted trade";
    case "cancel-trade":
      return "cancel the trade";
    case "play-knight":
      return `play Knight${target}${action.player ? ` and steal from ${action.player}` : ""}`;
    case "play-monopoly":
      return `play Monopoly${action.resource ? ` on ${RESOURCE_LABELS[action.resource]}` : ""}`;
    case "play-road-building":
      return "play Road Building";
    case "play-year-of-plenty":
      return `play Year of Plenty${action.resource ? ` for ${RESOURCE_LABELS[action.resource]}` : ""}${action.otherResource ? ` + ${RESOURCE_LABELS[action.otherResource]}` : ""}`;
    default:
      return action.kind.replaceAll("-", " ");
  }
};

export const explainDeepSearchDecision = (
  search: DeepSearchResult,
): DecisionRationale | undefined => {
  const chosen = search.chosen;
  if (!chosen) return undefined;
  const chosenLabel = describeDeepSearchAction(chosen);
  const reasons: string[] = [];
  const evidence: string[] = [];
  let summary = `Strategist chose to ${chosenLabel}`;

  const replacement =
    search.authorityTrace.safetyReplacement ??
    search.rootProvenance.safetyReplacement;
  const exactReplacement =
    search.authorityTrace.exactFamilyReplacement ??
    search.rootProvenance.exactFamilyReplacement;
  const exactCandidates = [...(search.exactActions ?? [])].sort(
    (left, right) => right.comparatorScore - left.comparatorScore,
  );
  const chosenExact = exactCandidates.find((candidate) =>
    sameDeepSearchAction(candidate.action, chosen),
  );
  const exactRunnerUp = exactCandidates.find(
    (candidate) => !sameDeepSearchAction(candidate.action, chosen),
  );
  const chosenStats = search.actions.find((candidate) =>
    sameDeepSearchAction(candidate.action, chosen),
  );
  const strategicRunnerUp = [...search.actions]
    .filter((candidate) => !sameDeepSearchAction(candidate.action, chosen))
    .sort(
      (left, right) =>
        (right.value[search.rootIndex] ?? Number.NEGATIVE_INFINITY) -
        (left.value[search.rootIndex] ?? Number.NEGATIVE_INFINITY),
    )[0];

  if (search.authority === "exact-mandatory") {
    summary = `Exact rules arbitration chose to ${chosenLabel}`;
    reasons.push(
      search.exactWorlds > 1
        ? `The mandatory solver compared the legal choices across ${search.exactWorlds} weighted belief worlds`
        : "The mandatory solver resolved the legal choices for this exact state",
    );
  } else if (search.authority === "tactical-proven") {
    summary = `The current-turn tactical solver proved a line beginning with ${chosenLabel}`;
    reasons.push(
      search.tacticalLine.length > 1
        ? `It found a forced tactical line ${search.tacticalLine.length} actions long`
        : "It found a forced current-turn result, so deeper strategic ranking was not needed",
    );
  } else if (search.authority === "safety-override" && replacement) {
    summary = `Safety arbitration chose to ${chosenLabel} instead of ${describeDeepSearchAction(replacement.from)}`;
    reasons.push(
      "The raw search leader triggered the engine's forced-loss or unsafe end-turn guard, so the safer legal alternative became authoritative",
    );
  } else if (search.authority === "exact-family") {
    const family = search.authorityTrace.exactFamily?.replaceAll("-", " ");
    summary = `Exact ${family ? `${family} ` : ""}arbitration chose to ${chosenLabel}`;
    reasons.push(
      "Strategic search chose the action family first; exact arbitration then selected the strongest concrete parameterization across the current belief set",
    );
  } else if (search.authority === "gpu-root-rollout") {
    summary = `GPU root racing chose to ${chosenLabel}`;
    reasons.push(
      `The surviving root won the native GPU comparison after ${search.rollouts.toLocaleString()} rollout samples across ${search.rustSearchParticleCount.toLocaleString()} searched belief worlds`,
    );
    evidence.push(
      search.rootProvenance.horizonEscalation
        ? "Escalated roots are rerun at one common deeper horizon; terminal outcomes remain primary, while overlapping terminal evidence defers to the parity-tested strategic cutoff, raw victory-point margin, and then faster completion"
        : "GPU roots are ordered by terminal outcome, then the parity-tested strategic cutoff, raw victory-point margin, and faster completion, with strategic prior as the final tie-break",
    );
  } else if (search.authority === "weighted-policy") {
    summary = `The weighted policy chose to ${chosenLabel}`;
    reasons.push(
      "It had the highest one-step weighted value across the belief set, with strategic prior used only as a tie-break",
    );
  } else {
    summary = `Deep MaxN chose to ${chosenLabel}`;
  }

  if (chosenExact) {
    const comparison = exactRunnerUp
      ? ` versus ${fixedScore(exactRunnerUp.comparatorScore)} for ${describeDeepSearchAction(exactRunnerUp.action)}`
      : "";
    reasons.push(
      `Its exact comparator was ${fixedScore(chosenExact.comparatorScore)}${comparison}`,
    );
    evidence.push(
      `Exact decision score ${fixedScore(chosenExact.decisionScore)}, lower score ${fixedScore(chosenExact.lowerScore)}, legal weight ${percentWeight(chosenExact.legalWeight)}`,
    );
  } else if (
    chosenStats &&
    (search.authority === "deep-maxn" || search.authority === "weighted-policy")
  ) {
    const chosenValue = chosenStats.value[search.rootIndex];
    const runnerValue = strategicRunnerUp?.value[search.rootIndex];
    if (Number.isFinite(chosenValue)) {
      if (strategicRunnerUp && Number.isFinite(runnerValue)) {
        const delta = (chosenValue ?? 0) - (runnerValue ?? 0);
        reasons.push(
          `Its completed root value was ${fixedScore(chosenValue)}, ${fixedScore(Math.abs(delta))} ${delta >= 0 ? "ahead of" : "behind"} ${describeDeepSearchAction(strategicRunnerUp.action)}`,
        );
      } else {
        reasons.push(`Its completed root value was ${fixedScore(chosenValue)}`);
      }
    }
  }

  if (chosenStats) {
    evidence.push(
      `Chosen root legal weight ${percentWeight(chosenStats.legalWeight)}${Number.isFinite(chosenStats.lowerConfidenceValue[search.rootIndex]) ? `; lower-confidence value ${fixedScore(chosenStats.lowerConfidenceValue[search.rootIndex])}` : ""}`,
    );
  }

  const ranked = search.rootProvenance.rankedRoots.find((candidate) =>
    sameDeepSearchAction(candidate.action, chosen),
  );
  const retained = search.rootProvenance.retainedRoots.find((candidate) =>
    sameDeepSearchAction(candidate.action, chosen),
  );
  const causalEvidence = search.rootProvenance.rootEvidence?.find((candidate) =>
    sameDeepSearchAction(candidate.action, chosen),
  );
  const roadIntent = causalEvidence?.roadIntent;
  if (roadIntent?.targetVertexId) {
    const roadCount = roadIntent.roadsRemaining;
    const eta = Number.isFinite(roadIntent.expectedRolls)
      ? `; expected self-funded access in ${fixedScore(roadIntent.expectedRolls)} rolls`
      : "";
    reasons.push(
      `This road targets ${roadIntent.targetVertexId}, leaving ${roadCount} additional road${roadCount === 1 ? "" : "s"} before settlement access${eta}`,
    );
    evidence.push(
      `Road intent: frontier gain ${fixedScore(roadIntent.frontierGain)}, target value ${fixedScore(roadIntent.targetValue)}, fallback portfolio ${fixedScore(roadIntent.portfolioValue)}, survival ${percentWeight(roadIntent.survivalProbability)}, ordering score ${fixedScore(roadIntent.orderingScore)}`,
    );
  }
  const horizonEscalation = search.rootProvenance.horizonEscalation;
  if (horizonEscalation) {
    const escalatedRoots = horizonEscalation.roots
      .map(describeDeepSearchAction)
      .join(", ");
    reasons.push(
      horizonEscalation.reason === "fragile-award-low-terminal-completion"
        ? `Selective horizon arbitration reran ${escalatedRoots} because ${describeDeepSearchAction(horizonEscalation.provisionalWinner)} exposed an award-losing road cut with ${percentWeight(horizonEscalation.unresolvedCutMass)} unresolved continuation mass`
        : `Selective horizon arbitration reran ${escalatedRoots} because terminal completion was sparse and the strategic-cutoff confidence bands still overlapped (${percentWeight(horizonEscalation.unresolvedCutMass)} unresolved evidence mass)`,
    );
    evidence.push(
      `Adaptive horizons ${horizonEscalation.attemptedHorizons.join(" → ")}; deepest completed ${horizonEscalation.completedHorizon ?? "none"}; deeper winner ${horizonEscalation.finalWinner ? describeDeepSearchAction(horizonEscalation.finalWinner) : "unresolved"}${horizonEscalation.deadlineLimited ? "; decision deadline limited further escalation" : ""}`,
    );
  }
  if (retained?.finalRank !== undefined) {
    const gpuRunnerUp = search.rootProvenance.retainedRoots
      .filter(
        (candidate) =>
          candidate.finalRank !== undefined &&
          !sameDeepSearchAction(candidate.action, chosen),
      )
      .sort((left, right) => (left.finalRank ?? Infinity) - (right.finalRank ?? Infinity))[0];
    const runnerComparison = gpuRunnerUp
      ? `; runner-up ${describeDeepSearchAction(gpuRunnerUp.action)} at horizon ${gpuRunnerUp.finalEvaluationHorizon ?? "unknown"} had terminal score ${fixedScore(gpuRunnerUp.terminalOutcome)}, ${percentWeight(gpuRunnerUp.terminalRate)} terminal completion, strategic margin ${fixedScore(gpuRunnerUp.strategicMargin)}, and raw victory-point margin ${fixedScore(gpuRunnerUp.victoryMargin)}`
      : "";
    reasons.push(
      `Its final GPU comparator rank was #${retained.finalRank} at horizon ${retained.finalEvaluationHorizon ?? "unknown"}, with terminal score ${fixedScore(retained.terminalOutcome)}, ${percentWeight(retained.terminalRate)} terminal completion, strategic margin ${fixedScore(retained.strategicMargin)}, and raw victory-point margin ${fixedScore(retained.victoryMargin)}${runnerComparison}`,
    );
    evidence.push(
      `GPU confidence bands: terminal ${fixedScore(retained.terminalLowerBound)}..${fixedScore(retained.terminalUpperBound)}, terminal completion ${percentWeight(retained.terminalRate)}, strategic margin ${fixedScore(retained.strategicMarginLowerBound)}..${fixedScore(retained.strategicMarginUpperBound)}, raw victory-point margin ${fixedScore(retained.victoryMarginLowerBound)}..${fixedScore(retained.victoryMarginUpperBound)}, mean completion turn ${fixedScore(retained.meanTurn)}`,
    );
  }
  if (ranked?.rank) {
    evidence.push(
      `Planner pre-rank #${ranked.rank}${retained ? `; ${retained.allocatedNodes.toLocaleString()} search nodes allocated across belief worlds` : ""}`,
    );
  }
  const pruneCounts = search.rootProvenance.prunedRoots.reduce(
    (counts, candidate) => {
      counts[candidate.reason] = (counts[candidate.reason] ?? 0) + 1;
      return counts;
    },
    {} as Partial<Record<DeepSearchPrunedRoot["reason"], number>>,
  );
  const pruneSummary = Object.entries(pruneCounts)
    .filter(([, count]) => Boolean(count))
    .map(([reason, count]) => `${count} ${reason.replaceAll("-", " ")}`)
    .join(", ");
  if (pruneSummary) {
    evidence.push(`Root filtering removed ${pruneSummary}`);
  }
  if (exactReplacement && sameDeepSearchAction(exactReplacement.to, chosen)) {
    evidence.push(
      `Exact-family arbitration replaced ${describeDeepSearchAction(exactReplacement.from)} with this concrete action`,
    );
  }
  if (replacement && sameDeepSearchAction(replacement.to, chosen)) {
    evidence.push(
      `Safety arbitration replaced ${describeDeepSearchAction(replacement.from)} with the final action`,
    );
  }

  const effort = search.effectiveSearchEffort;
  if (effort?.backend === "cpu") {
    evidence.push(
      `CPU effort: ${effort.timeBudgetMs} ms base${effort.evidenceEscalationMs > 0 ? ` + up to ${effort.evidenceEscalationMs} ms evidence reserve` : ""}, depth cap ${effort.maxDepth}, root cap ${effort.rootCap}, ${effort.nodesPerDepthWave.toLocaleString()} nodes per depth wave`,
    );
  } else if (effort?.backend === "gpu") {
    evidence.push(
      `GPU effort: ${effort.timeBudgetMs} ms, root cap ${effort.rootCap}, rollout budget ${effort.rolloutBudget.toLocaleString()}, ${effort.rolloutSteps} rollout steps`,
    );
  }

  if (search.searchStages) {
    const stages = search.searchStages;
    if (search.deadlineReached && !stages.floorComplete) {
      evidence.push(
        "The deadline was reached before the complete one-ply floor finished, so this recommendation has weaker fallback evidence",
      );
    } else if (
      search.deadlineReached &&
      stages.attemptedDepth > search.deepestDecisionDepth
    ) {
      evidence.push(
        `The deadline arrived while attempting depth ${stages.attemptedDepth}; the returned recommendation uses the last completed decision depth ${search.deepestDecisionDepth}`,
      );
    }
    if (stages.evidenceEscalationTriggered) {
      const escalationOutcome = !stages.evidenceEscalationCompleted
        ? "stopped before completion"
        : stages.evidenceEscalationStrengthened
          ? "completed with stronger same-depth evidence"
          : "completed without stronger same-depth evidence; ordinary deepening resumed";
      evidence.push(
        `Evidence escalation ${escalationOutcome}: ${stages.evidenceEscalationBaselineNodes.toLocaleString()} baseline nodes, ${stages.evidenceEscalationNodes.toLocaleString()} rerun nodes in ${stages.evidenceEscalationMs} ms after the binary floor/wave winner disagreement`,
      );
    }
    evidence.push(
      `Stage time: particle prep ${stages.particlePreparationMs} ms, root scoring ${stages.rootScoringMs} ms, exact families ${stages.exactFamiliesMs} ms, threat/safety ${stages.threatSafetyMs} ms, one-ply ${stages.onePlyFloorMs} ms, deep waves ${stages.deepWavesMs} ms`,
    );
  } else if (search.deadlineReached) {
    evidence.push("The backend reached its decision deadline before returning");
  }

  if (search.tacticalProven) {
    evidence.push(
      `Tactical win estimate ${Math.round(search.tacticalWinProbability * 100)}% with ${Math.round(search.tacticalLowerBound * 100)}% lower bound`,
    );
  }

  return {
    summary,
    reasons: reasons.slice(0, 4),
    evidence: evidence.slice(0, 6),
  };
};

export interface PlayerWinEstimate {
  player: string;
  probability: number;
  etaTurns: number;
  samples: number;
  confidence: "high" | "medium" | "low";
  reasons: string[];
}

export interface DecisionAnalysis {
  engine: DecisionEngine;
  players: PlayerWinEstimate[];
  actionScores: Record<BuildKind, number>;
  simulations: number;
  model: string;
  runtime?: DecisionRuntime;
  runtimeReason?: string;
  nativeGpuBuild?: NativeGpuBuildIdentity;
  deepSearch?: DeepSearchResult;
}

interface SimPlayer {
  name: string;
  hand: ResourceVector;
  production: ResourceVector;
  tradeRatios: ResourceVector;
  cardDiscardLimit: number;
  points: number;
  settlements: number;
  cities: number;
  roads: number;
  remainingSettlements: number;
  remainingCities: number;
  remainingRoads: number;
  settlementCapacity: number;
  routeProgress: number;
  longestRoad: number;
  knights: number;
  heldKnights: number;
  heldProgress: number;
  hiddenVictoryPoints: number;
  hasLongestRoad: boolean;
  hasLargestArmy: boolean;
  settlementSites: Array<{
    production: ResourceVector;
    port?: Resource | "generic";
  }>;
  citySites: ResourceVector[];
  blockedResource?: Resource;
}

interface SimPosition {
  players: SimPlayer[];
  target: number;
  current: number;
  longestRoadHolder: number;
  largestArmyHolder: number;
  deck: {
    knight: number;
    progress: number;
    victoryPoint: number;
  };
}

const BUILD_KINDS = Object.keys(BUILD_COSTS) as BuildKind[];
const MAX_SIM_TURNS = 220;
const CACHE_LIMIT = 20;
const analysisCache = new Map<string, DecisionAnalysis>();

const hashString = (value: string): number => {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
};

const mulberry32 = (seed: number): (() => number) => {
  let value = seed >>> 0;
  return () => {
    value += 0x6d2b79f5;
    let result = value;
    result = Math.imul(result ^ (result >>> 15), result | 1);
    result ^= result + Math.imul(result ^ (result >>> 7), result | 61);
    return ((result ^ (result >>> 14)) >>> 0) / 4294967296;
  };
};

const weightedPick = <T>(
  values: Array<{ value: T; weight: number }>,
  random: () => number,
): T | undefined => {
  const total = values.reduce((sum, item) => sum + Math.max(0, item.weight), 0);
  if (total <= 0) return values[0]?.value;
  let cursor = random() * total;
  for (const item of values) {
    cursor -= Math.max(0, item.weight);
    if (cursor <= 0) return item.value;
  }
  return values.at(-1)?.value;
};

const sigmoid = (value: number): number => 1 / (1 + Math.exp(-value));

const boardSignature = (
  state: TrackerState,
  board: BoardSnapshot,
): string =>
  JSON.stringify({
    estimator: "strategist-public-prior",
    game: board.gameKey,
    turn: state.currentTurn.sequence,
    event: state.eventCount,
    action: board.action,
    hand: board.ownHand,
    bank: board.bankVisible ? board.bank : undefined,
    pieces: [
      ...board.vertices.flatMap((vertex) =>
        vertex.building
          ? [`${vertex.id}:${vertex.building.player}:${vertex.building.kind}`]
          : [],
      ),
      ...board.edges.flatMap((edge) =>
        edge.player ? [`${edge.id}:${edge.player}`] : [],
      ),
    ],
    public: board.players,
  });

const cacheResult = (key: string, value: DecisionAnalysis): DecisionAnalysis => {
  analysisCache.set(key, value);
  while (analysisCache.size > CACHE_LIMIT) {
    const oldest = analysisCache.keys().next().value;
    if (typeof oldest !== "string") break;
    analysisCache.delete(oldest);
  }
  return value;
};

const developmentEvidence = (state: TrackerState, board: BoardSnapshot) => {
  const players = state.playerOrder.map((player) => state.players[player]!);
  const playedCount = (
    player: (typeof players)[number],
    card: "knight" | "monopoly" | "road-building" | "year-of-plenty" | "victory-point",
  ): number =>
    Math.max(
      player.playedDevCards[card],
      board.players?.[player.name]?.playedDevelopmentCards?.[card] ?? 0,
    );
  const purchased = players.reduce((sum, player) => {
    const publicState = board.players?.[player.name];
    const trackedPlayed = Object.values(player.playedDevCards).reduce(
      (total, count) => total + count,
      0,
    );
    const mergedPlayed =
      playedCount(player, "knight") +
      playedCount(player, "monopoly") +
      playedCount(player, "road-building") +
      playedCount(player, "year-of-plenty") +
      playedCount(player, "victory-point");
    return (
      sum +
      Math.max(
        player.devCards.length + trackedPlayed,
        (publicState?.developmentCards ?? 0) + mergedPlayed,
      )
    );
  }, 0);
  return {
    purchased,
    playedKnights: players.reduce(
      (sum, player) => sum + playedCount(player, "knight"),
      0,
    ),
    playedMonopoly: players.reduce(
      (sum, player) => sum + playedCount(player, "monopoly"),
      0,
    ),
    playedRoadBuilding: players.reduce(
      (sum, player) => sum + playedCount(player, "road-building"),
      0,
    ),
    playedYearOfPlenty: players.reduce(
      (sum, player) => sum + playedCount(player, "year-of-plenty"),
      0,
    ),
    revealedVictoryPoints: players.reduce(
      (sum, player) => sum + playedCount(player, "victory-point"),
      0,
    ),
  };
};

const buildUtility = (
  kind: BuildKind,
  profile: PlayerBoardProfile,
): number => {
  if (kind === "city") {
    return profile.settlements > 0
      ? 1.15 + Math.min(0.8, profile.metrics.activeWeightedPips / 40)
      : 0;
  }
  if (kind === "settlement") {
    return profile.remaining.settlements > 0
      ? 1.05 + Math.min(0.65, profile.openSettlementSites * 0.16)
      : 0;
  }
  if (kind === "development") {
    return 0.28 + (profile.knightsToLargest <= 2 ? 0.5 : 0.14);
  }
  if (kind === "road") {
    if (profile.roadsToLongest <= 2) return 2 / Math.max(1, profile.roadsToLongest);
    return profile.openSettlementSites ? 0.08 : 0.3;
  }
  return 0;
};

const deterministicEta = (
  state: TrackerState,
  board: BoardSnapshot,
  player: string,
): { eta: number; actionScores: Record<BuildKind, number>; reasons: string[] } => {
  const hand = getPlayerEstimate(state, player).average;
  const profile = playerBoardProfile(board, player);
  const publicDevs = board.players?.[player]?.developmentCards ?? 0;
  const hiddenPointPrior = Math.min(1.8, publicDevs * 0.2);
  let pointsNeeded = Math.max(
    0.25,
    profile.victoryTarget - profile.visiblePoints - hiddenPointPrior,
  );
  const actionScores = Object.fromEntries(
    BUILD_KINDS.map((kind) => {
      const tempo = estimateBuildTempo(hand, kind, profile);
      const utility = buildUtility(kind, profile);
      const pieceBlocked =
        (kind === "city" &&
          (!profile.settlements || !profile.remaining.cities)) ||
        (kind === "settlement" && !profile.remaining.settlements) ||
        (kind === "road" && !profile.remaining.roads);
      const turns = tempo.rolls / Math.max(2, state.playerOrder.length);
      return [
        kind,
        pieceBlocked || utility <= 0
          ? -100
          : 100 * utility / Math.max(0.65, turns + (tempo.affordable ? 0 : 0.8)),
      ];
    }),
  ) as Record<BuildKind, number>;
  const options = BUILD_KINDS.map((kind) => ({
    kind,
    score: actionScores[kind],
    utility: Math.max(0.05, buildUtility(kind, profile)),
  })).filter((item) => item.score > -50);
  let eta = 0;
  let economyGrowth = 1;
  while (pointsNeeded > 0 && eta < 120) {
    const best = options
      .map((item) => ({
        ...item,
        cost: (100 / Math.max(0.1, item.score)) / economyGrowth,
      }))
      .sort((left, right) => left.cost / left.utility - right.cost / right.utility)[0];
    if (!best) {
      eta = 120;
      break;
    }
    eta += best.cost;
    pointsNeeded -= best.utility;
    if (best.kind === "settlement") economyGrowth *= 1.11;
    if (best.kind === "city") economyGrowth *= 1.08;
  }
  const portPower = RESOURCE_ORDER.reduce(
    (sum, resource) =>
      sum +
      Math.max(0, 4 - profile.tradeRatios[resource]) *
        profile.activeProduction[resource],
    0,
  );
  eta /= 1 + Math.min(0.24, portPower / 150);
  const bestAction = [...options].sort((a, b) => b.score - a.score)[0];
  return {
    eta: Math.max(0.2, eta),
    actionScores,
    reasons: [
      `${profile.visiblePoints}/${profile.victoryTarget} visible points`,
      `${Math.round(profile.metrics.activeWeightedPips)} weighted active pips`,
      bestAction
        ? `${bestAction.kind === "development" ? "Development card" : bestAction.kind} is the fastest modeled conversion`
        : "No clean public conversion path",
    ],
  };
};

const normalizeSoftmax = (
  players: Array<{ player: string; eta: number }>,
): Map<string, number> => {
  const temperature = Math.max(
    1.4,
    players.reduce((sum, player) => sum + player.eta, 0) /
      Math.max(1, players.length) /
      4.2,
  );
  const weights = players.map((player) => ({
    player: player.player,
    weight: Math.exp(-player.eta / temperature),
  }));
  const total = weights.reduce((sum, item) => sum + item.weight, 0) || 1;
  const uniform = 1 / Math.max(1, players.length);
  return new Map(
    weights.map((item) => [
      item.player,
      (item.weight / total) * 0.86 + uniform * 0.14,
    ]),
  );
};

const productionAtVertex = (
  board: BoardSnapshot,
  adjacentHexes: string[],
): ResourceVector => {
  const production = emptyResources();
  for (const id of adjacentHexes) {
    const hex = board.hexes.find((candidate) => candidate.id === id);
    if (hex?.resource && hex.number) {
      production[hex.resource] += NUMBER_PIPS[hex.number] ?? 0;
    }
  }
  return production;
};

const addProduction = (
  target: ResourceVector,
  source: ResourceVector,
): void => {
  for (const resource of RESOURCE_ORDER) {
    target[resource] += source[resource];
  }
};

const siteValue = (production: ResourceVector): number =>
  RESOURCE_ORDER.reduce(
    (sum, resource) =>
      sum + production[resource] * RESOURCE_STRATEGIC_WEIGHTS[resource],
    0,
  );

const sampleWorld = (
  state: TrackerState,
  board: BoardSnapshot,
  random: () => number,
): HandWorld => {
  const sampled =
    weightedPick(
      state.worlds.map((world) => ({ value: world, weight: world.weight })),
      random,
    ) ?? ({ hands: {}, weight: 1 } as HandWorld);
  const hands = Object.fromEntries(
    state.playerOrder.map((player) => [
      player,
      cloneResources(sampled.hands[player] ?? emptyResources()),
    ]),
  );
  if (board.myPlayer && board.ownHand) {
    hands[board.myPlayer] = cloneResources(board.ownHand);
  }
  return { hands, weight: sampled.weight };
};

const sampleDevelopmentHoldings = (
  count: number,
  remaining: { knight: number; progress: number; victoryPoint: number },
  random: () => number,
): { knight: number; progress: number; victoryPoint: number } => {
  const result = { knight: 0, progress: 0, victoryPoint: 0 };
  const pool = { ...remaining };
  for (let index = 0; index < count; index += 1) {
    const picked = weightedPick(
      [
        { value: "knight" as const, weight: pool.knight },
        { value: "progress" as const, weight: pool.progress },
        { value: "victoryPoint" as const, weight: pool.victoryPoint },
      ],
      random,
    );
    if (!picked || pool[picked] <= 0) continue;
    pool[picked] -= 1;
    result[picked] += 1;
  }
  return result;
};

const createSimPosition = (
  state: TrackerState,
  board: BoardSnapshot,
  random: () => number,
): SimPosition => {
  const world = sampleWorld(state, board, random);
  const evidence = developmentEvidence(state, board);
  const deckEstimate = estimateDevelopmentDeck(evidence);
  const remaining = {
    knight: Math.max(0, Math.round(deckEstimate.expectedComposition.knight)),
    progress: Math.max(0, Math.round(deckEstimate.expectedComposition.progress)),
    victoryPoint: Math.max(
      0,
      Math.round(deckEstimate.expectedComposition.victoryPoint),
    ),
  };
  const players = state.playerOrder.map((name) => {
    const profile = playerBoardProfile(board, name);
    const publicState = board.players?.[name];
    const heldCount = publicState?.developmentCards ?? state.players[name]?.devCards.length ?? 0;
    const sampled = sampleDevelopmentHoldings(heldCount, remaining, random);
    if (name === board.myPlayer && board.ownDevelopmentCards) {
      sampled.knight = board.ownDevelopmentCards.cards.knight;
      sampled.progress =
        board.ownDevelopmentCards.cards.monopoly +
        board.ownDevelopmentCards.cards["road-building"] +
        board.ownDevelopmentCards.cards["year-of-plenty"];
      sampled.victoryPoint =
        board.ownDevelopmentCards.cards["victory-point"];
    }
    const byVertex = new Map(
      board.vertices.map((vertex) => [vertex.id, vertex]),
    );
    const settlementSites = board.vertices
      .filter(
        (vertex) =>
          !vertex.building &&
          !vertex.adjacentVertices.some(
            (neighbor) => byVertex.get(neighbor)?.building,
          ) &&
          board.edges.some(
            (edge) =>
              edge.player === name && edge.vertices.includes(vertex.id),
          ),
      )
      .map((vertex) => ({
        production: productionAtVertex(board, vertex.adjacentHexes),
        ...(vertex.port ? { port: vertex.port } : {}),
      }))
      .sort(
        (left, right) =>
          siteValue(right.production) - siteValue(left.production),
      );
    const citySites = board.vertices
      .filter(
        (vertex) =>
          vertex.building?.player === name &&
          vertex.building.kind === "settlement",
      )
      .map((vertex) => productionAtVertex(board, vertex.adjacentHexes))
      .sort((left, right) => siteValue(right) - siteValue(left));
    return {
      name,
      hand: cloneResources(world.hands[name] ?? emptyResources()),
      production: cloneResources(profile.activeProduction),
      tradeRatios: cloneResources(profile.tradeRatios),
      cardDiscardLimit: profile.cardDiscardLimit,
      points: profile.visiblePoints,
      settlements: profile.settlements,
      cities: profile.cities,
      roads: profile.roads,
      remainingSettlements: profile.remaining.settlements,
      remainingCities: profile.remaining.cities,
      remainingRoads: profile.remaining.roads,
      settlementCapacity: Math.max(
        profile.openSettlementSites,
        profile.remaining.settlements > 0 ? 1 : 0,
      ),
      routeProgress: 0,
      longestRoad: profile.longestRoad,
      knights: profile.playedKnights,
      heldKnights: sampled.knight,
      heldProgress: sampled.progress,
      hiddenVictoryPoints: sampled.victoryPoint,
      hasLongestRoad: profile.hasLongestRoad,
      hasLargestArmy: profile.hasLargestArmy,
      settlementSites,
      citySites,
    };
  });
  const currentName = state.currentTurn.player;
  return {
    players,
    target: board.victoryTarget ?? 10,
    current: Math.max(
      0,
      players.findIndex((player) => player.name === currentName),
    ),
    longestRoadHolder: players.findIndex((player) => player.hasLongestRoad),
    largestArmyHolder: players.findIndex((player) => player.hasLargestArmy),
    deck: remaining,
  };
};

const spend = (hand: ResourceVector, kind: BuildKind): void => {
  const cost = BUILD_COSTS[kind] as Partial<ResourceVector>;
  for (const resource of RESOURCE_ORDER) {
    hand[resource] -= cost[resource] ?? 0;
  }
};

const maritimeTradeToward = (
  player: SimPlayer,
  kind: BuildKind,
): void => {
  const cost = BUILD_COSTS[kind] as Partial<ResourceVector>;
  for (let pass = 0; pass < 5; pass += 1) {
    const receive = RESOURCE_ORDER.filter(
      (resource) => player.hand[resource] < (cost[resource] ?? 0),
    ).sort(
      (left, right) =>
        ((cost[right] ?? 0) - player.hand[right]) *
          RESOURCE_STRATEGIC_WEIGHTS[right] -
        ((cost[left] ?? 0) - player.hand[left]) *
          RESOURCE_STRATEGIC_WEIGHTS[left],
    )[0];
    if (!receive) return;
    const give = RESOURCE_ORDER.filter(
      (resource) =>
        resource !== receive &&
        player.hand[resource] - (cost[resource] ?? 0) >=
          player.tradeRatios[resource],
    ).sort(
      (left, right) =>
        player.hand[right] / player.tradeRatios[right] -
        player.hand[left] / player.tradeRatios[left],
    )[0];
    if (!give) return;
    player.hand[give] -= player.tradeRatios[give];
    player.hand[receive] += 1;
  }
};

const preferredBuild = (
  player: SimPlayer,
  position: SimPosition,
  forced?: BuildKind,
): BuildKind => {
  if (forced) return forced;
  const remaining = position.target - (player.points + player.hiddenVictoryPoints);
  const candidates: Array<{ kind: BuildKind; score: number }> = [
    {
      kind: "city",
      score:
        (player.remainingCities && player.settlements ? 34 : -100) +
        (remaining <= 1 ? 28 : 0) +
        player.production.ore * 0.28 +
        player.production.grain * 0.22,
    },
    {
      kind: "settlement",
      score:
        (player.remainingSettlements && player.settlementCapacity ? 31 : -100) +
        (remaining <= 1 ? 25 : 0) +
        (player.settlements < 3 ? 9 : 0),
    },
    {
      kind: "development",
      score:
        (position.deck.knight +
          position.deck.progress +
          position.deck.victoryPoint >
        0
          ? 20
          : -100) +
        (remaining <= 2 ? 14 : 0) +
        (player.knights + player.heldKnights >= 2 ? 8 : 0),
    },
    {
      kind: "road",
      score:
        (player.remainingRoads ? 3 : -100) +
        (player.longestRoad >= 4 ? 21 : 0) +
        (!player.settlementCapacity && player.remainingSettlements ? 12 : 0),
    },
  ];
  return candidates.sort((left, right) => right.score - left.score)[0]!.kind;
};

const simulatedBuildDeficit = (
  player: SimPlayer,
  kind: BuildKind,
  hand: ResourceVector = player.hand,
): number => {
  const cost = BUILD_COSTS[kind] as Partial<ResourceVector>;
  return RESOURCE_ORDER.reduce((sum, resource) => {
    const missing = Math.max(0, (cost[resource] ?? 0) - hand[resource]);
    const scarcity =
      player.production[resource] > 0
        ? Math.min(1.1, 6 / player.production[resource])
        : 1.8;
    return (
      sum +
      missing *
        RESOURCE_STRATEGIC_WEIGHTS[resource] *
        scarcity
    );
  }, 0);
};

const applySimulatedTrade = (
  hand: ResourceVector,
  give: ResourceVector,
  receive: ResourceVector,
): ResourceVector => {
  const result = cloneResources(hand);
  for (const resource of RESOURCE_ORDER) {
    result[resource] += receive[resource] - give[resource];
  }
  return result;
};

const simulatedGiveVectors = (
  player: SimPlayer,
  kind: BuildKind,
  requested: Resource,
): ResourceVector[] => {
  const cost = BUILD_COSTS[kind] as Partial<ResourceVector>;
  const ranked = RESOURCE_ORDER.filter(
    (resource) => resource !== requested && player.hand[resource] > 0,
  ).sort((left, right) => {
    const leftReserve = player.hand[left] - (cost[left] ?? 0);
    const rightReserve = player.hand[right] - (cost[right] ?? 0);
    return (
      rightReserve - leftReserve ||
      RESOURCE_STRATEGIC_WEIGHTS[left] -
        RESOURCE_STRATEGIC_WEIGHTS[right]
    );
  });
  const offers: ResourceVector[] = [];
  const shortlist = ranked.slice(0, 3);
  for (let firstIndex = 0; firstIndex < shortlist.length; firstIndex += 1) {
    const first = shortlist[firstIndex]!;
    const single = emptyResources();
    single[first] = 1;
    offers.push(single);
    for (
      let secondIndex = firstIndex;
      secondIndex < shortlist.length;
      secondIndex += 1
    ) {
      const second = shortlist[secondIndex]!;
      const pair = cloneResources(single);
      pair[second] += 1;
      if (hasResources(player.hand, pair)) offers.push(pair);
    }
  }
  return offers.filter((offer, index, all) => {
    const key = RESOURCE_ORDER.map((resource) => offer[resource]).join(",");
    return (
      index ===
      all.findIndex(
        (candidate) =>
          RESOURCE_ORDER.map((resource) => candidate[resource]).join(",") ===
          key,
      )
    );
  });
};

/**
 * Models one bounded bargaining opportunity before maritime trade. This is a
 * strategic response model, not a claim that every human accepts the highest
 * utility offer: the logistic draw deliberately preserves negotiation noise.
 */
const attemptPlayerTrade = (
  position: SimPosition,
  actorIndex: number,
  kind: BuildKind,
  random: () => number,
): boolean => {
  const actor = position.players[actorIndex];
  if (!actor || hasResources(actor.hand, BUILD_COSTS[kind])) return false;
  const actorBeforeDeficit = simulatedBuildDeficit(actor, kind);
  if (actorBeforeDeficit > 4.5 || resourceTotal(actor.hand) < 2) return false;
  const requested = RESOURCE_ORDER.filter(
    (resource) =>
      actor.hand[resource] <
      ((BUILD_COSTS[kind] as Partial<ResourceVector>)[resource] ?? 0),
  ).sort(
    (left, right) =>
      RESOURCE_STRATEGIC_WEIGHTS[right] *
        Math.max(1, 5 - actor.production[right]) -
      RESOURCE_STRATEGIC_WEIGHTS[left] *
        Math.max(1, 5 - actor.production[left]),
  );
  const actorPointsRemaining =
    position.target - (actor.points + actor.hiddenVictoryPoints);
  const candidates: Array<{
    partner: SimPlayer;
    give: ResourceVector;
    receive: ResourceVector;
    acceptance: number;
    score: number;
  }> = [];

  for (const resource of requested.slice(0, 1)) {
    const receive = emptyResources();
    receive[resource] = 1;
    for (const give of simulatedGiveVectors(actor, kind, resource)) {
      const singleGiveResource = RESOURCE_ORDER.filter(
        (candidate) => give[candidate] > 0,
      );
      if (
        singleGiveResource.length === 1 &&
        give[singleGiveResource[0]!] >=
          actor.tradeRatios[singleGiveResource[0]!]
      ) {
        continue;
      }
      const actorAfter = applySimulatedTrade(actor.hand, give, receive);
      if (RESOURCE_ORDER.some((candidate) => actorAfter[candidate] < 0)) {
        continue;
      }
      const actorAfterDeficit = simulatedBuildDeficit(actor, kind, actorAfter);
      const completesActorBuild =
        !hasResources(actor.hand, BUILD_COSTS[kind]) &&
        hasResources(actorAfter, BUILD_COSTS[kind]);
      const actorTempoGain =
        actorBeforeDeficit -
        actorAfterDeficit +
        (completesActorBuild ? 4.2 : 0);
      if (actorTempoGain <= 0.1) continue;

      for (let partnerIndex = 0; partnerIndex < position.players.length; partnerIndex += 1) {
        if (partnerIndex === actorIndex) continue;
        const partner = position.players[partnerIndex]!;
        if (!hasResources(partner.hand, receive)) continue;
        const partnerPointsRemaining =
          position.target -
          (partner.points + partner.hiddenVictoryPoints);
        if (partnerPointsRemaining <= 1 && actorPointsRemaining > 1) continue;

        const partnerKind = preferredBuild(partner, position);
        const partnerBeforeDeficit = simulatedBuildDeficit(
          partner,
          partnerKind,
        );
        const partnerAfter = applySimulatedTrade(
          partner.hand,
          receive,
          give,
        );
        const partnerAfterDeficit = simulatedBuildDeficit(
          partner,
          partnerKind,
          partnerAfter,
        );
        const completesPartnerBuild =
          !hasResources(partner.hand, BUILD_COSTS[partnerKind]) &&
          hasResources(partnerAfter, BUILD_COSTS[partnerKind]);
        const partnerTempoGain =
          partnerBeforeDeficit -
          partnerAfterDeficit +
          (completesPartnerBuild ? 3.5 : 0);
        const receivedValue = RESOURCE_ORDER.reduce(
          (sum, candidate) =>
            sum +
            give[candidate] *
              RESOURCE_STRATEGIC_WEIGHTS[candidate],
          0,
        );
        const paidValue =
          RESOURCE_STRATEGIC_WEIGHTS[resource] *
          (partner.production[resource] > 0 ? 1 : 1.3);
        const leaderPenalty =
          actorPointsRemaining <= 1
            ? 6.5
            : actorPointsRemaining === 2
              ? 2.6
              : actorPointsRemaining === 3
                ? 0.8
                : 0;
        const sharedExpansionPenalty =
          kind === "settlement" &&
          partnerKind === "settlement" &&
          actor.settlementCapacity > 0 &&
          partner.settlementCapacity > 0
            ? 0.9
            : 0;
        const responseUtility =
          partnerTempoGain * 0.7 +
          (receivedValue - paidValue) * 0.9 -
          leaderPenalty -
          sharedExpansionPenalty -
          0.55;
        const acceptance = Math.max(
          0.01,
          Math.min(0.98, sigmoid(responseUtility) * 0.92 + 0.03),
        );
        const opponentThreat =
          partnerPointsRemaining <= 2
            ? 2.4
            : partnerPointsRemaining === 3
              ? 1.5
              : 0.65;
        const score =
          acceptance *
            (actorTempoGain * 2.6 -
              Math.max(0, partnerTempoGain) * opponentThreat) -
          (1 - acceptance) * 0.25;
        if (score > 0.2) {
          candidates.push({
            partner,
            give,
            receive,
            acceptance,
            score,
          });
        }
      }
    }
  }

  const best = candidates.sort(
    (left, right) =>
      right.score - left.score ||
      right.acceptance - left.acceptance,
  )[0];
  if (!best || random() > best.acceptance) return false;
  for (const resource of RESOURCE_ORDER) {
    actor.hand[resource] +=
      best.receive[resource] - best.give[resource];
    best.partner.hand[resource] +=
      best.give[resource] - best.receive[resource];
  }
  return true;
};

const sampleProduction = (player: SimPlayer, random: () => number): void => {
  for (const resource of RESOURCE_ORDER) {
    const robberMultiplier = player.blockedResource === resource ? 0.72 : 1;
    const expected =
      (Math.max(0, player.production[resource]) / 36) * robberMultiplier;
    const guaranteed = Math.floor(expected);
    player.hand[resource] +=
      guaranteed + (random() < expected - guaranteed ? 1 : 0);
  }
};

const applyRobber = (
  position: SimPosition,
  actorIndex: number,
  random: () => number,
): void => {
  for (const player of position.players) player.blockedResource = undefined;
  const actor = position.players[actorIndex];
  if (!actor) return;
  const target = weightedPick(
    position.players
      .map((player, index) => ({ player, index }))
      .filter(({ index }) => index !== actorIndex)
      .map(({ player, index }) => ({
        value: { player, index },
        weight:
          1 +
          (player.points + player.hiddenVictoryPoints) * 1.8 +
          resourceTotal(player.production) * 0.045 +
          resourceTotal(player.hand) * 0.15,
      })),
    random,
  );
  if (!target) return;
  const blocked = weightedPick(
    RESOURCE_ORDER.map((resource) => ({
      value: resource,
      weight:
        target.player.production[resource] *
        RESOURCE_STRATEGIC_WEIGHTS[resource],
    })),
    random,
  );
  if (blocked) target.player.blockedResource = blocked;
  const stolen = weightedPick(
    RESOURCE_ORDER.map((resource) => ({
      value: resource,
      weight: target.player.hand[resource],
    })),
    random,
  );
  if (stolen && target.player.hand[stolen] > 0) {
    target.player.hand[stolen] -= 1;
    actor.hand[stolen] += 1;
  }
};

const discardOnSeven = (player: SimPlayer): void => {
  let count = resourceTotal(player.hand);
  if (count <= player.cardDiscardLimit) return;
  let remove = Math.floor(count / 2);
  while (remove > 0) {
    const resource = [...RESOURCE_ORDER].sort(
      (left, right) =>
        player.hand[right] / RESOURCE_STRATEGIC_WEIGHTS[right] -
        player.hand[left] / RESOURCE_STRATEGIC_WEIGHTS[left],
    )[0]!;
    if (!player.hand[resource]) break;
    player.hand[resource] -= 1;
    remove -= 1;
    count -= 1;
  }
};

const updateAward = (
  position: SimPosition,
  award: "road" | "army",
): void => {
  const holderKey =
    award === "road" ? "longestRoadHolder" : "largestArmyHolder";
  const metric = (player: SimPlayer) =>
    award === "road" ? player.longestRoad : player.knights;
  const threshold = award === "road" ? 5 : 3;
  const ranked = position.players
    .map((player, index) => ({ index, value: metric(player) }))
    .sort((left, right) => right.value - left.value);
  const leader = ranked[0];
  const runnerUp = ranked[1];
  if (
    !leader ||
    leader.value < threshold ||
    (runnerUp && leader.value === runnerUp.value)
  ) {
    return;
  }
  const oldHolder = position[holderKey];
  if (oldHolder === leader.index) return;
  if (oldHolder >= 0) {
    position.players[oldHolder]!.points -= 2;
    if (award === "road") position.players[oldHolder]!.hasLongestRoad = false;
    else position.players[oldHolder]!.hasLargestArmy = false;
  }
  position.players[leader.index]!.points += 2;
  if (award === "road") position.players[leader.index]!.hasLongestRoad = true;
  else position.players[leader.index]!.hasLargestArmy = true;
  position[holderKey] = leader.index;
};

const drawDevelopmentCard = (
  position: SimPosition,
  player: SimPlayer,
  random: () => number,
): void => {
  const card = weightedPick(
    [
      { value: "knight" as const, weight: position.deck.knight },
      { value: "progress" as const, weight: position.deck.progress },
      { value: "victoryPoint" as const, weight: position.deck.victoryPoint },
    ],
    random,
  );
  if (!card || position.deck[card] <= 0) return;
  position.deck[card] -= 1;
  if (card === "knight") player.heldKnights += 1;
  if (card === "progress") player.heldProgress += 1;
  if (card === "victoryPoint") player.hiddenVictoryPoints += 1;
};

const executeBuild = (
  position: SimPosition,
  player: SimPlayer,
  kind: BuildKind,
  random: () => number,
): boolean => {
  maritimeTradeToward(player, kind);
  if (!hasResources(player.hand, BUILD_COSTS[kind])) return false;
  if (kind === "city" && (!player.remainingCities || !player.settlements)) {
    return false;
  }
  if (
    kind === "settlement" &&
    (!player.remainingSettlements || !player.settlementCapacity)
  ) {
    return false;
  }
  if (kind === "road" && !player.remainingRoads) return false;
  if (
    kind === "development" &&
    position.deck.knight + position.deck.progress + position.deck.victoryPoint <=
      0
  ) {
    return false;
  }
  spend(player.hand, kind);
  if (kind === "city") {
    player.points += 1;
    player.cities += 1;
    player.settlements -= 1;
    player.remainingCities -= 1;
    player.remainingSettlements += 1;
    const upgradedSite = player.citySites.shift();
    if (upgradedSite) addProduction(player.production, upgradedSite);
    else {
      const total = resourceTotal(player.production) || 1;
      for (const resource of RESOURCE_ORDER) {
        player.production[resource] += Math.max(
          0,
          Math.round((player.production[resource] / total) * 7),
        );
      }
    }
  } else if (kind === "settlement") {
    player.points += 1;
    player.settlements += 1;
    player.remainingSettlements -= 1;
    player.settlementCapacity -= 1;
    const site = player.settlementSites.shift();
    if (site) {
      addProduction(player.production, site.production);
      player.citySites.push(cloneResources(site.production));
      player.citySites.sort(
        (left, right) => siteValue(right) - siteValue(left),
      );
      if (site.port === "generic") {
        for (const resource of RESOURCE_ORDER) {
          player.tradeRatios[resource] = Math.min(
            player.tradeRatios[resource],
            3,
          );
        }
      } else if (site.port) {
        player.tradeRatios[site.port] = 2;
      }
    } else {
      const weakest = [...RESOURCE_ORDER].sort(
        (left, right) =>
          player.production[left] * RESOURCE_STRATEGIC_WEIGHTS[left] -
          player.production[right] * RESOURCE_STRATEGIC_WEIGHTS[right],
      );
      const synthetic = emptyResources();
      for (const resource of weakest.slice(0, 3)) {
        synthetic[resource] += resource === "grain" ? 4 : 3;
      }
      addProduction(player.production, synthetic);
      player.citySites.push(synthetic);
    }
  } else if (kind === "road") {
    player.roads += 1;
    player.remainingRoads -= 1;
    player.longestRoad += 1;
    player.routeProgress += 1;
    if (player.routeProgress >= 2 && player.remainingSettlements) {
      player.settlementCapacity += 1;
      player.routeProgress = 0;
    }
    updateAward(position, "road");
  } else {
    drawDevelopmentCard(position, player, random);
  }
  return true;
};

const simulateGame = (
  state: TrackerState,
  board: BoardSnapshot,
  rootPlayer: string,
  random: () => number,
  forcedRootAction?: BuildKind,
): string => {
  const position = createSimPosition(state, board, random);
  const rootIndex = position.players.findIndex(
    (player) => player.name === rootPlayer,
  );
  let forcedPending = rootIndex >= 0 && forcedRootAction ? forcedRootAction : undefined;
  for (let turn = 0; turn < MAX_SIM_TURNS; turn += 1) {
    const player = position.players[position.current]!;
    const displayedPoints = player.points + player.hiddenVictoryPoints;
    if (displayedPoints >= position.target) return player.name;

    if (random() < 1 / 6) {
      for (const candidate of position.players) discardOnSeven(candidate);
      applyRobber(position, position.current, random);
    } else {
      for (const candidate of position.players) sampleProduction(candidate, random);
    }

    if (player.heldKnights > 0) {
      const armyThreshold = Math.max(
        3,
        ...position.players
          .filter((candidate) => candidate !== player)
          .map((candidate) => candidate.knights + 1),
      );
      if (
        player.knights + 1 >= armyThreshold ||
        random() < 0.22 + Math.min(0.4, player.heldKnights * 0.08)
      ) {
        player.heldKnights -= 1;
        player.knights += 1;
        updateAward(position, "army");
        applyRobber(position, position.current, random);
      }
    } else if (player.heldProgress > 0 && random() < 0.28) {
      player.heldProgress -= 1;
      const needed = [...RESOURCE_ORDER].sort(
        (left, right) =>
          RESOURCE_STRATEGIC_WEIGHTS[right] - RESOURCE_STRATEGIC_WEIGHTS[left],
      );
      player.hand[needed[0]!] += 1;
      player.hand[needed[1]!] += 1;
    }

    const forced =
      forcedPending && position.current === rootIndex
        ? forcedPending
        : undefined;
    const preferred = preferredBuild(player, position, forced);
    attemptPlayerTrade(
      position,
      position.current,
      preferred,
      random,
    );
    let built = executeBuild(position, player, preferred, random);
    if (!built && !forced) {
      const fallbacks = BUILD_KINDS.filter((kind) => kind !== preferred).sort(
        (left, right) => {
          const leftAffordable = hasResources(player.hand, BUILD_COSTS[left]);
          const rightAffordable = hasResources(player.hand, BUILD_COSTS[right]);
          return Number(rightAffordable) - Number(leftAffordable);
        },
      );
      for (const fallback of fallbacks) {
        if (executeBuild(position, player, fallback, random)) {
          built = true;
          break;
        }
      }
    }
    if (forced && built) forcedPending = undefined;
    if (player.points + player.hiddenVictoryPoints >= position.target) {
      return player.name;
    }
    position.current = (position.current + 1) % position.players.length;
  }

  return position.players
    .map((player) => ({
      player: player.name,
      score:
        player.points +
        player.hiddenVictoryPoints +
        resourceTotal(player.hand) * 0.025 +
        resourceTotal(player.production) * 0.002,
    }))
    .sort((left, right) => right.score - left.score)[0]!.player;
};

const rolloutProbabilities = (
  state: TrackerState,
  board: BoardSnapshot,
  rootPlayer: string,
  samples: number,
  seed: number,
): Map<string, number> => {
  const wins = new Map(state.playerOrder.map((player) => [player, 0]));
  for (let sample = 0; sample < samples; sample += 1) {
    const random = mulberry32(seed + Math.imul(sample + 1, 0x9e3779b1));
    const winner = simulateGame(state, board, rootPlayer, random);
    wins.set(winner, (wins.get(winner) ?? 0) + 1);
  }
  return new Map(
    state.playerOrder.map((player) => [
      player,
      ((wins.get(player) ?? 0) + 2) /
        Math.max(1, samples + state.playerOrder.length * 2),
    ]),
  );
};

const rootBanditScores = (
  state: TrackerState,
  board: BoardSnapshot,
  rootPlayer: string,
  samples: number,
  seed: number,
): Record<BuildKind, number> => {
  const arms = Object.fromEntries(
    BUILD_KINDS.map((kind) => [kind, { visits: 0, wins: 0 }]),
  ) as Record<BuildKind, { visits: number; wins: number }>;
  for (let sample = 0; sample < samples; sample += 1) {
    const totalVisits = sample + 1;
    const arm = BUILD_KINDS.map((kind) => {
      const stats = arms[kind];
      const exploitation = stats.visits ? stats.wins / stats.visits : 1;
      const exploration = stats.visits
        ? Math.sqrt((1.35 * Math.log(totalVisits + 1)) / stats.visits)
        : Number.POSITIVE_INFINITY;
      return { kind, ucb: exploitation + exploration };
    }).sort((left, right) => right.ucb - left.ucb)[0]!.kind;
    const random = mulberry32(seed ^ Math.imul(sample + 11, 0x85ebca6b));
    const winner = simulateGame(state, board, rootPlayer, random, arm);
    arms[arm].visits += 1;
    if (winner === rootPlayer) arms[arm].wins += 1;
  }
  const raw = Object.fromEntries(
    BUILD_KINDS.map((kind) => [
      kind,
      arms[kind].visits ? arms[kind].wins / arms[kind].visits : 0,
    ]),
  ) as Record<BuildKind, number>;
  const minimum = Math.min(...Object.values(raw));
  const maximum = Math.max(...Object.values(raw));
  return Object.fromEntries(
    BUILD_KINDS.map((kind) => [
      kind,
      maximum > minimum ? ((raw[kind] - minimum) / (maximum - minimum)) * 100 : 50,
    ]),
  ) as Record<BuildKind, number>;
};

export const analyzePublicEstimate = (
  state: TrackerState,
  board: BoardSnapshot,
  rootPlayer: string,
): DecisionAnalysis => {
  const key = boardSignature(state, board);
  const cached = analysisCache.get(key);
  if (cached) return cached;
  const etas = state.playerOrder.map((player) => ({
    player,
    ...deterministicEta(state, board, player),
  }));
  const etaProbabilities = normalizeSoftmax(etas);
  const rootEta = etas.find((item) => item.player === rootPlayer);
  const simulationCount = 0;
  const blended = etaProbabilities;
  const probabilityTotal =
    [...blended.values()].reduce((sum, value) => sum + value, 0) || 1;
  const actionScores =
    rootEta?.actionScores ??
    ({ road: 0, settlement: 0, city: 0, development: 0 } as Record<
      BuildKind,
      number
    >);
  const playerEstimates = etas
    .map((item) => {
      const widthPenalty =
        getPlayerEstimate(state, item.player).totalMaximum -
        getPlayerEstimate(state, item.player).totalMinimum;
      const confidence =
        state.possibilitiesTruncated || widthPenalty > 5
          ? "low"
          : simulationCount >= 220 && widthPenalty <= 2
            ? "high"
            : "medium";
      return {
        player: item.player,
        probability: (blended.get(item.player) ?? 0) / probabilityTotal,
        etaTurns: Math.round(item.eta * 10) / 10,
        samples: simulationCount,
        confidence,
        reasons: item.reasons,
      } satisfies PlayerWinEstimate;
    })
    .sort((left, right) => right.probability - left.probability);
  return cacheResult(key, {
    engine: "deep-search",
    players: playerEstimates,
    actionScores,
    simulations: simulationCount,
    model: "Public build-time prior (display only; Strategist chooses actions)",
  });
};
