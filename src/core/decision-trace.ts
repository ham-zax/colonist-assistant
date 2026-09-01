import type { BoardSnapshot } from "./placement";
import { DECISION_TRACE_STORAGE_KEY } from "./local-data";
import { RESOURCE_ORDER } from "./resources";
import type {
  DeepSearchAction,
  DeepSearchAuthorityTrace,
  DeepSearchRootProvenance,
  DecisionAnalysis,
  DecisionAuthority,
} from "./engine";
import type { TrackerState } from "./types";

export type DecisionActionSource =
  | DecisionAuthority
  | "incoming-trade-evaluator"
  | "road-plan"
  | "placement-heuristic"
  | "coach-goal"
  | "end-turn-fallback"
  | "mandatory";

export interface DecisionTraceCandidate {
  action: DeepSearchAction;
  value: number;
  visits?: number;
  availabilityWeight?: number;
  legalWeight?: number;
  lowerConfidenceValue?: number;
  prior?: number;
}

export interface DecisionTrace {
  stateHash: string;
  recordedAt: number;
  turn: number;
  phase: string;
  hand: [number, number, number, number, number];
  publicVictoryPoints: number[];
  beliefParticleCount: number;
  sourceWorldCount: number;
  wasmParticleCount?: number;
  rustPosteriorParticleCount?: number;
  rustSearchParticleCount?: number;
  effectiveParticleCount?: number;
  deepRequestStartedAt?: number;
  deepRequestFinishedAt?: number;
  deepLatencyMs?: number;
  deepSlowWarningAtMs?: number;
  deepStatus: "pending" | "complete" | "failed";
  deepFailureReason?: string;
  deepTimedOut: boolean;
  decisionModel?: string;
  runtimeReason?: string;
  engineRevision?: string;
  algorithm?: string;
  searchElapsedMs?: number;
  iterations?: number;
  nodes?: number;
  deepestDecisionDepth?: number;
  rollouts?: number;
  tacticalWinProbability?: number;
  tacticalProven?: boolean;
  exactDecision?: boolean;
  deepChosenAction?: DeepSearchAction;
  deepCandidates?: DecisionTraceCandidate[];
  rustAuthority?: DecisionAuthority;
  authorityTrace?: DeepSearchAuthorityTrace;
  rootProvenance?: DeepSearchRootProvenance;
  mappingFailureReason?: string;
  finalAction?: unknown;
  finalActionSource?: DecisionActionSource;
  finalActionSelectedAt?: number;
  executedBeforeDeepResult: boolean;
  executionFinishedAt?: number;
  executionSucceeded?: boolean;
  executionFailureReason?: string;
  engine?: string;
  runtime?: string;
  learnedModelVersion?: string;
  tradeModelVersion?: string;
  seed?: number;
  rootPlayer?: string;
  replayState?: TrackerState;
  replayBoard?: BoardSnapshot;
}

const MAX_TRACES = 120;

const tuple = (
  board: BoardSnapshot,
): [number, number, number, number, number] =>
  RESOURCE_ORDER.map(
    (resource) => board.ownHand?.[resource] ?? 0,
  ) as [number, number, number, number, number];

export class DecisionTraceRecorder {
  private readonly traces = new Map<string, DecisionTrace>();
  private persistTimer?: ReturnType<typeof globalThis.setTimeout>;
  private storageOperations: Promise<void> = Promise.resolve();

  constructor(private readonly onChange?: () => void) {}

  begin(
    stateHash: string,
    state: TrackerState,
    board: BoardSnapshot,
    startedAt = performance.now(),
  ): void {
    if (this.traces.has(stateHash)) return;
    const replayState: TrackerState = {
      ...structuredClone(state),
      worlds: [...state.worlds]
        .sort(
          (left, right) =>
            (right.weight ?? 0) - (left.weight ?? 0),
        )
        .slice(0, 128),
      recentEvents: state.recentEvents.slice(-120),
    };
    const replayBoard = structuredClone(board);
    delete replayBoard.assets;
    replayBoard.hexes = replayBoard.hexes.map(({ screen: _screen, ...hex }) => hex);
    replayBoard.vertices = replayBoard.vertices.map(
      ({ screen: _screen, ...vertex }) => vertex,
    );
    replayBoard.edges = replayBoard.edges.map(({ screen: _screen, ...edge }) => edge);
    this.traces.set(stateHash, {
      stateHash,
      recordedAt: Date.now(),
      turn: board.turn ?? state.currentTurn.sequence,
      phase: board.action ?? "none",
      hand: tuple(board),
      publicVictoryPoints: (board.playerOrder ?? Object.keys(board.players ?? {}))
        .map((player) => board.players?.[player]?.visiblePoints ?? 0),
      beliefParticleCount: state.worlds.length,
      sourceWorldCount: state.worlds.length,
      deepRequestStartedAt: startedAt,
      deepStatus: "pending",
      deepTimedOut: false,
      executedBeforeDeepResult: false,
      rootPlayer: board.myPlayer,
      replayState,
      replayBoard,
    });
    // Persist starts as well as completions. If the selected engine transport
    // or executor stalls, the live benchmark must retain the exact pending
    // state instead of losing the most important trace when Chromium closes.
    this.schedulePersist();
  }

  complete(stateHash: string, analysis: DecisionAnalysis): void {
    const trace = this.traces.get(stateHash);
    if (!trace) return;
    const rootIndex =
      analysis.deepSearch?.rootIndex ??
      Math.max(
        0,
        analysis.players.findIndex(
          (player) => player.player === trace.rootPlayer,
        ),
      );
    const finishedAt = performance.now();
    trace.deepRequestFinishedAt = finishedAt;
    trace.deepLatencyMs =
      trace.deepRequestStartedAt === undefined
        ? undefined
        : finishedAt - trace.deepRequestStartedAt;
    // A cooperative search deadline returns the best fully bounded report; it
    // is diagnostic evidence, not permission to substitute another policy.
    trace.deepStatus = "complete";
    trace.deepFailureReason = undefined;
    trace.deepTimedOut = analysis.deepSearch?.deadlineReached ?? false;
    trace.decisionModel = analysis.model;
    trace.runtimeReason = analysis.runtimeReason;
    trace.engine = analysis.engine;
    trace.runtime = analysis.runtime;
    trace.engineRevision = analysis.deepSearch?.engineRevision;
    trace.algorithm = analysis.deepSearch?.algorithm;
    trace.searchElapsedMs = analysis.deepSearch?.elapsedMs;
    trace.iterations = analysis.deepSearch?.iterations;
    trace.nodes = analysis.deepSearch?.nodes;
    trace.deepestDecisionDepth = analysis.deepSearch?.deepestDecisionDepth;
    trace.rollouts = analysis.deepSearch?.rollouts;
    trace.tacticalWinProbability = analysis.deepSearch?.tacticalWinProbability;
    trace.tacticalProven = analysis.deepSearch?.tacticalProven;
    trace.exactDecision = analysis.deepSearch?.exactDecision;
    trace.learnedModelVersion =
      analysis.deepSearch?.learnedModelVersion;
    trace.tradeModelVersion =
      analysis.deepSearch?.tradeModelVersion;
    trace.sourceWorldCount =
      analysis.deepSearch?.sourceWorldCount ?? trace.sourceWorldCount;
    trace.wasmParticleCount = analysis.deepSearch?.wasmParticleCount;
    trace.rustPosteriorParticleCount =
      analysis.deepSearch?.rustPosteriorParticleCount;
    trace.rustSearchParticleCount =
      analysis.deepSearch?.rustSearchParticleCount;
    trace.effectiveParticleCount =
      analysis.deepSearch?.effectiveParticleCount;
    trace.seed = analysis.deepSearch?.seed;
    trace.rustAuthority = analysis.deepSearch?.authority;
    trace.authorityTrace = analysis.deepSearch?.authorityTrace;
    trace.rootProvenance = analysis.deepSearch?.rootProvenance;
    trace.mappingFailureReason = analysis.deepSearch?.mappingFailureReason;
    trace.deepChosenAction = analysis.deepSearch?.chosen;
    trace.deepCandidates = analysis.deepSearch?.actions.map((candidate) => ({
      action: candidate.action,
      value: candidate.value[rootIndex] ?? candidate.value[0] ?? 0,
      visits: candidate.visits,
      availabilityWeight: candidate.availabilityWeight,
      legalWeight: candidate.legalWeight,
      lowerConfidenceValue:
        candidate.lowerConfidenceValue[rootIndex] ??
        candidate.lowerConfidenceValue[0],
      prior: candidate.prior,
    }));
    this.schedulePersist();
  }

  slow(stateHash: string): void {
    const trace = this.traces.get(stateHash);
    if (!trace || trace.deepSlowWarningAtMs !== undefined) return;
    trace.deepSlowWarningAtMs =
      trace.deepRequestStartedAt === undefined
        ? undefined
        : performance.now() - trace.deepRequestStartedAt;
    this.schedulePersist();
  }

  failure(stateHash: string, reason: string): void {
    const trace = this.traces.get(stateHash);
    if (!trace) return;
    const finishedAt = performance.now();
    trace.deepRequestFinishedAt = finishedAt;
    trace.deepLatencyMs =
      trace.deepRequestStartedAt === undefined
        ? undefined
        : finishedAt - trace.deepRequestStartedAt;
    trace.deepStatus = "failed";
    trace.deepFailureReason = reason;
    this.schedulePersist();
  }

  mappingFailure(stateHash: string, reason: string): void {
    const trace = this.traces.get(stateHash);
    if (!trace || trace.mappingFailureReason === reason) return;
    trace.mappingFailureReason = reason;
    this.schedulePersist();
  }

  final(
    stateHash: string,
    action: unknown,
    source: DecisionActionSource,
  ): void {
    const trace = this.traces.get(stateHash);
    if (!trace) return;
    if (
      trace.finalActionSource === source &&
      JSON.stringify(trace.finalAction) === JSON.stringify(action)
    ) {
      return;
    }
    trace.finalAction = action;
    trace.finalActionSource = source;
    trace.finalActionSelectedAt ??= Date.now();
    trace.executedBeforeDeepResult =
      trace.deepRequestFinishedAt === undefined;
    this.schedulePersist();
  }

  execution(
    stateHash: string,
    succeeded: boolean,
    failureReason?: string,
  ): void {
    const trace = this.traces.get(stateHash);
    if (!trace) return;
    trace.executionFinishedAt = Date.now();
    trace.executionSucceeded = succeeded;
    trace.executionFailureReason = failureReason;
    this.schedulePersist();
  }

  snapshot(includeReplayState = true): DecisionTrace[] {
    if (includeReplayState) {
      return structuredClone([...this.traces.values()]);
    }
    return [...this.traces.values()].map((trace) => {
      const {
        replayState: _replayState,
        replayBoard: _replayBoard,
        ...compact
      } = trace;
      return structuredClone(compact);
    });
  }

  async reset(): Promise<void> {
    if (this.persistTimer !== undefined) {
      globalThis.clearTimeout(this.persistTimer);
      this.persistTimer = undefined;
    }
    this.traces.clear();
    await this.enqueueStorage(() =>
      chrome.storage.local.remove(DECISION_TRACE_STORAGE_KEY),
    );
  }

  private schedulePersist(): void {
    this.onChange?.();
    if (this.persistTimer !== undefined) return;
    const storage = chrome.storage.local;
    this.persistTimer = globalThis.setTimeout(() => {
      this.persistTimer = undefined;
      const traces = [...this.traces.values()].slice(-MAX_TRACES);
      void this.enqueueStorage(() =>
        storage.set({ [DECISION_TRACE_STORAGE_KEY]: traces }),
      );
    }, 200);
  }

  private enqueueStorage(operation: () => Promise<void>): Promise<void> {
    const next = this.storageOperations.then(operation, operation);
    this.storageOperations = next.catch(() => undefined);
    return next;
  }
}

export const readDecisionTraces = async (): Promise<DecisionTrace[]> => {
  const result = await chrome.storage.local.get(DECISION_TRACE_STORAGE_KEY);
  return Array.isArray(result[DECISION_TRACE_STORAGE_KEY])
    ? (result[DECISION_TRACE_STORAGE_KEY] as DecisionTrace[])
    : [];
};
