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
}

export interface WasmRetainedRoot {
  action: WasmAction;
  preTruncationRank?: number;
  prior: number;
  nodeBudgetPerParticle: number;
  allocatedNodes: number;
  plannerValue?: number;
  plannerCompletionMass?: number;
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

export interface WasmRootProvenance {
  rankedRootCount: number;
  rankedRoots: WasmRankedRoot[];
  retainedRoots: WasmRetainedRoot[];
  prunedRootCount: number;
  prunedRoots: WasmPrunedRoot[];
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
  actions: WasmActionStatistics[];
  iterations: number;
  nodes: number;
  deepestDecisionDepth: number;
  rollouts: number;
  particles: number;
  wasmParticles: number;
  rustPosteriorParticles: number;
  rustSearchParticles: number;
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
