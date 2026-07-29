import type { BoardSnapshot } from "./placement";
import { RESOURCE_ORDER } from "./resources";
import type { DeepSearchAction, DecisionAnalysis } from "./engine";
import type { TrackerState } from "./types";

export type DecisionActionSource =
  | "tactical"
  | "deep"
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
}

export interface DecisionTrace {
  stateHash: string;
  turn: number;
  phase: string;
  hand: [number, number, number, number, number];
  publicVictoryPoints: number[];
  beliefParticleCount: number;
  effectiveParticleCount?: number;
  deepRequestStartedAt?: number;
  deepRequestFinishedAt?: number;
  deepLatencyMs?: number;
  deepTimedOut: boolean;
  deepChosenAction?: DeepSearchAction;
  deepCandidates?: DecisionTraceCandidate[];
  finalAction?: unknown;
  finalActionSource?: DecisionActionSource;
  executedBeforeDeepResult: boolean;
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

const STORAGE_KEY = "colonist-assistant-decision-traces-v1";
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
      turn: board.turn ?? state.currentTurn.sequence,
      phase: board.action ?? "none",
      hand: tuple(board),
      publicVictoryPoints: (board.playerOrder ?? Object.keys(board.players ?? {}))
        .map((player) => board.players?.[player]?.visiblePoints ?? 0),
      beliefParticleCount: state.worlds.length,
      deepRequestStartedAt: startedAt,
      deepTimedOut: false,
      executedBeforeDeepResult: false,
      rootPlayer: board.myPlayer,
      replayState,
      replayBoard,
    });
  }

  complete(stateHash: string, analysis: DecisionAnalysis): void {
    const trace = this.traces.get(stateHash);
    if (!trace) return;
    const rootIndex = Math.max(
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
    // Interactive deadlines are diagnostic-only. The selected engine is
    // never replaced by a timeout policy.
    trace.deepTimedOut = false;
    trace.engine = analysis.engine;
    trace.runtime = analysis.runtime;
    trace.learnedModelVersion =
      analysis.deepSearch?.learnedModelVersion;
    trace.tradeModelVersion =
      analysis.deepSearch?.tradeModelVersion;
    trace.effectiveParticleCount =
      analysis.deepSearch?.effectiveParticleCount;
    trace.seed = analysis.deepSearch?.seed;
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
    }));
    this.schedulePersist();
  }

  final(
    stateHash: string,
    action: unknown,
    source: DecisionActionSource,
  ): void {
    const trace = this.traces.get(stateHash);
    if (!trace) return;
    trace.finalAction = action;
    trace.finalActionSource = source;
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
    trace.executionSucceeded = succeeded;
    trace.executionFailureReason = failureReason;
    this.schedulePersist();
  }

  reset(): void {
    this.traces.clear();
  }

  private schedulePersist(): void {
    if (this.persistTimer !== undefined) return;
    this.persistTimer = globalThis.setTimeout(() => {
      this.persistTimer = undefined;
      const traces = [...this.traces.values()].slice(-MAX_TRACES);
      void chrome.storage.local.set({ [STORAGE_KEY]: traces });
    }, 200);
  }
}

export const readDecisionTraces = async (): Promise<DecisionTrace[]> => {
  const result = await chrome.storage.local.get(STORAGE_KEY);
  return Array.isArray(result[STORAGE_KEY])
    ? (result[STORAGE_KEY] as DecisionTrace[])
    : [];
};
