export interface WasmAction {
  kind: string;
  first?: number;
  second?: number;
  player?: number;
  resource?: number;
  otherResource?: number;
  cards?: [number, number, number, number, number];
  receiveCards?: [number, number, number, number, number];
  accept?: boolean;
}

export interface WasmExactActionDiagnostic {
  action: WasmAction;
  value: [number, number, number, number];
  lowerBound: [number, number, number, number];
  legalWeight: number;
  decisionScore: number;
  lowerScore: number;
  comparatorScore: number;
}

export interface WasmSearchStages {
  particlePreparationMs: number;
  rootScoringMs: number;
  exactFamiliesMs: number;
  threatSafetyMs: number;
  onePlyFloorMs: number;
  deepWavesMs: number;
  floorComplete: boolean;
  attemptedDepth: number;
}

export interface WasmSearchEffort {
  decisionTimeMs: number;
  tactical: { maxDepth: number; nodeBudget: number };
  cpu: { maxDepth: number; rootCap: number; nodesPerDepthWave: number };
  gpu: { rootCap: number; rolloutBudget: number; rolloutSteps: number };
}

export interface WasmActionStatistics {
  action: WasmAction;
  visits: number;
  availability: number;
  availabilityWeight: number;
  legalWeight: number;
  prior: number;
  value: [number, number, number, number];
  lowerConfidenceValue: [number, number, number, number];
}

export type WasmDecisionAuthority =
  | "exact-mandatory"
  | "tactical-proven"
  | "deep-maxn"
  | "gpu-root-rollout"
  | "weighted-policy"
  | "exact-family"
  | "safety-override";

export interface WasmActionReplacement {
  from: WasmAction;
  to: WasmAction;
}

export interface WasmRankedRoot {
  action: WasmAction;
  rank: number;
  prior: number;
  plannerValue?: number;
  plannerCompletionMass?: number;
  plannerDecisiveCompletionMass?: number;
  plannerResponseWindows?: number;
}

export interface WasmRetainedRoot {
  action: WasmAction;
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
  terminalOutcome?: number;
  terminalRate?: number;
  terminalLowerBound?: number;
  terminalUpperBound?: number;
  victoryMargin?: number;
  victoryMarginLowerBound?: number;
  victoryMarginUpperBound?: number;
  meanTurn?: number;
}

export interface WasmPrunedRoot {
  action: WasmAction;
  preTruncationRank?: number;
  reason:
    | "root-excluded"
    | "branch-truncated"
    | "trade-safety"
    | "exact-family-collapsed";
}

export type WasmRootPromotionReason =
  | "road-award-protection"
  | "critical-expansion-protection"
  | "opponent-route-cut"
  | "closeout-compression";

export type WasmDomesticTradeThreat =
  | "dirty-monopoly"
  | "immediate-win"
  | "award-swing"
  | "contested-settlement"
  | "material-build";

export interface WasmIntroducedCriticalVertex {
  vertex: number;
  roadLoss: number;
  additionalRoadLoss: number;
  awardLoss: boolean;
  awardLossIntroduced: boolean;
  awardVpExposure: number;
  expansionLoss: number;
  additionalExpansionLoss: number;
}

export interface WasmIntroducedRoadFragility {
  criticalVertices: WasmIntroducedCriticalVertex[];
  maximumAdditionalRoadLoss: number;
  awardVpExposure: number;
  maximumAdditionalExpansionLoss: number;
}

export interface WasmRoadCutContinuationEvidence {
  vertex: number;
  opponent: number;
  posterior: number;
  maritimeTradeRequiredPosterior: number;
  awardLossPosterior: number;
  maximumRoadLoss: number;
  approachEdges: number[];
}

export interface WasmRoadCutContinuationAssessment {
  posterior: number;
  awardLossPosterior: number;
  continuations: WasmRoadCutContinuationEvidence[];
}

export interface WasmRootCausalEvidence {
  action: WasmAction;
  promotionReason?: WasmRootPromotionReason;
  introducedRoadFragility?: WasmIntroducedRoadFragility;
  roadCutContinuation?: WasmRoadCutContinuationAssessment;
  admittedByPromotion: boolean;
  closeoutGain: number;
  responseWindows?: number;
  decisiveCompletionMass: number;
  tradeThreat?: WasmDomesticTradeThreat;
  tradeRiskPosterior: number;
  dirtyMonopolyPosterior: number;
  tradeHardVetoPosterior: number;
  tradeHardVeto: boolean;
}

export interface WasmHorizonEscalation {
  reason: "fragile-award-low-terminal-completion";
  provisionalWinner: WasmAction;
  initialHorizon: number;
  unresolvedCutMass: number;
  roots: WasmAction[];
  attemptedHorizons: number[];
  completedHorizon?: number;
  finalWinner?: WasmAction;
  deadlineLimited: boolean;
}

export interface WasmRootProvenance {
  rankedRootCount: number;
  rankedRoots: WasmRankedRoot[];
  retainedRoots: WasmRetainedRoot[];
  prunedRootCount: number;
  prunedRoots: WasmPrunedRoot[];
  rootEvidence: WasmRootCausalEvidence[];
  horizonEscalation?: WasmHorizonEscalation;
  tradeHardVetoThreshold: number;
  searchWinner?: WasmAction;
  exactFamilyReplacement?: WasmActionReplacement;
  safetyReplacement?: WasmActionReplacement;
}

export interface WasmAuthorityTrace {
  initialAuthority: WasmDecisionAuthority;
  exactFamily?: string;
  exactFamilyReplacement?: WasmActionReplacement;
  safetyReplacement?: WasmActionReplacement;
}

export interface WasmSearchResponse {
  engineRevision: string;
  authority: WasmDecisionAuthority;
  learnedModelVersion: string;
  tradeModelVersion: string;
  algorithm: string;
  chosen?: WasmAction;
  rootValue: [number, number, number, number];
  tacticalWinProbability: number;
  tacticalLowerBound: number;
  tacticalProven: boolean;
  tacticalLine: WasmAction[];
  exactDecision: boolean;
  exactWorlds: number;
  exactActions: WasmExactActionDiagnostic[];
  actions: WasmActionStatistics[];
  iterations: number;
  nodes: number;
  deepestDecisionDepth: number;
  rollouts: number;
  particles: number;
  wasmParticles: number;
  rustPosteriorParticles: number;
  rustSearchParticles: number;
  effectiveEffort: WasmSearchEffort;
  searchStages?: WasmSearchStages;
  rootProvenance: WasmRootProvenance;
  authorityTrace: WasmAuthorityTrace;
  effectiveParticleCount: number;
  deadlineReached: boolean;
}

export function analyze(request: unknown): WasmSearchResponse;
export function engine_version(): string;
export default function init(
  module:
    | string
    | URL
    | Request
    | Response
    | BufferSource
    | WebAssembly.Module
    | {
        module_or_path:
          | string
          | URL
          | Request
          | Response
          | BufferSource
          | WebAssembly.Module;
      },
): Promise<WebAssembly.Exports>;
