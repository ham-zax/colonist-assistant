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
