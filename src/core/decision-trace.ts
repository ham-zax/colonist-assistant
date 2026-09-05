import type { BoardSnapshot } from "./placement";
import { DECISION_TRACE_STORAGE_KEY } from "./local-data";
import { RESOURCE_ORDER } from "./resources";
import type { ResourceVector } from "./resources";
import { explainDeepSearchDecision } from "./engine";
import type {
  DeepSearchAction,
  DeepSearchAuthorityTrace,
  DeepSearchEffectiveEffort,
  DeepSearchRootProvenance,
  DecisionRationale,
  DecisionAnalysis,
  DecisionAuthority,
  DecisionSearchConstraints,
  NativeGpuBuildIdentity,
} from "./engine";
import type { TrackerState } from "./types";
import type {
  DiceHistoryProvenance,
  StochasticBeliefPolicyId,
  StochasticModelId,
} from "./dice-history";

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
  source?: "strategic" | "exact";
  decisionScore?: number;
  lowerScore?: number;
  comparatorScore?: number;
}

export interface DecisionTraceSettings {
  engine: string;
  disablePlayerTrades: boolean;
  autopilot: boolean;
}

export interface DecisionTraceSearchConstraints {
  lastRejectedTrade?: {
    give: [number, number, number, number, number];
    receive: [number, number, number, number, number];
  };
  rootExclusions: Array<{
    kind: "offer-trade" | "counter-trade";
    give: [number, number, number, number, number];
    receive: [number, number, number, number, number];
  }>;
}

export interface DecisionTraceSearchStages {
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

export interface DecisionExecutionDiagnostic {
  actionKind: string;
  tradeId?: string;
  offerIndex?: number;
  boardTradeIds?: string[];
  boardTradeAtIndex?: string;
  visibleTradeCount?: number;
  visibleTradeFingerprints?: string[];
  visibleTradeControlFingerprints?: string[];
}

export type DecisionSearchStatus =
  | "pending"
  | "complete"
  | "failed"
  | "superseded";

export type DecisionLifecycleStatus =
  | "search-pending"
  | "search-complete"
  | "search-failed"
  | "superseded"
  | "action-selected"
  | "execution-pending"
  | "execution-complete"
  | "execution-failed";

export interface DecisionSearchAttempt {
  startedAt: number;
  finishedAt?: number;
  latencyMs?: number;
  slowWarningAtMs?: number;
  status: DecisionSearchStatus;
  failureReason?: string;
  timedOut: boolean;
}

export type DecisionResourceTuple = [number, number, number, number, number];

export interface DecisionBeliefPlayerSummary {
  player: string;
  expected: DecisionResourceTuple;
  pAtLeastOne: DecisionResourceTuple;
  minimum: DecisionResourceTuple;
  maximum: DecisionResourceTuple;
}

export interface DecisionBeliefSummary {
  worldCount: number;
  possibilitiesTruncated: boolean;
  players: DecisionBeliefPlayerSummary[];
}

export interface DecisionTrace {
  stateHash: string;
  recordedAt: number;
  turn: number;
  phase: string;
  hand: [number, number, number, number, number];
  publicVictoryPoints: number[];
  settings?: DecisionTraceSettings;
  searchConstraints?: DecisionTraceSearchConstraints;
  beliefParticleCount: number;
  sourceWorldCount: number;
  beliefSummary?: DecisionBeliefSummary;
  wasmParticleCount?: number;
  rustPosteriorParticleCount?: number;
  rustSearchParticleCount?: number;
  effectiveParticleCount?: number;
  deepRequestStartedAt?: number;
  deepRequestFinishedAt?: number;
  deepLatencyMs?: number;
  deepSlowWarningAtMs?: number;
  deepStatus: DecisionSearchStatus;
  lifecycleStatus?: DecisionLifecycleStatus;
  deepFailureReason?: string;
  deepTimedOut: boolean;
  deepAttempts?: DecisionSearchAttempt[];
  decisionModel?: string;
  runtimeReason?: string;
  engineRevision?: string;
  diceMode?: BoardSnapshot["diceMode"];
  chanceModel?: "fair-iid-2d6";
  requestedStochasticModel?: StochasticModelId;
  stochasticModel?: StochasticModelId;
  beliefPolicy?: StochasticBeliefPolicyId;
  diceHistoryProvenance?: DiceHistoryProvenance;
  publicHistoryDigest?: string;
  stochasticBeliefDigest?: string;
  stochasticBeliefParticleCount?: number;
  nativeGpuBuild?: NativeGpuBuildIdentity;
  algorithm?: string;
  effectiveSearchEffort?: DeepSearchEffectiveEffort;
  decisionRationale?: DecisionRationale;
  searchResultId?: string;
  searchResultOriginStateHash?: string;
  searchResultReused?: boolean;
  searchElapsedMs?: number;
  searchStages?: DecisionTraceSearchStages;
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
  executionStartedAt?: number;
  executionFinishedAt?: number;
  executionSucceeded?: boolean;
  executionFailureReason?: string;
  executionDiagnostic?: DecisionExecutionDiagnostic;
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
const MAX_SEARCH_ATTEMPTS = 8;

const resourceTuple = (
  resources?: Partial<ResourceVector>,
): DecisionResourceTuple =>
  RESOURCE_ORDER.map((resource) => resources?.[resource] ?? 0) as DecisionResourceTuple;

const tuple = (board: BoardSnapshot): DecisionResourceTuple =>
  resourceTuple(board.ownHand);

const searchConstraintSnapshot = (
  constraints?: DecisionSearchConstraints,
): DecisionTraceSearchConstraints | undefined => {
  if (!constraints) return undefined;
  return {
    ...(constraints.lastRejectedTrade
      ? {
          lastRejectedTrade: {
            give: resourceTuple(constraints.lastRejectedTrade.give),
            receive: resourceTuple(constraints.lastRejectedTrade.receive),
          },
        }
      : {}),
    rootExclusions: (constraints.rootExclusions ?? []).map((exclusion) => ({
      kind: exclusion.kind,
      give: resourceTuple(exclusion.give),
      receive: resourceTuple(exclusion.receive),
    })),
  };
};

const summarizeBeliefs = (
  state: TrackerState,
  rootPlayer?: string,
): DecisionBeliefSummary => {
  const players = state.playerOrder.filter((player) => player !== rootPlayer);
  const totalWeight = state.worlds.reduce(
    (total, world) => total + Math.max(0, world.weight ?? 0),
    0,
  );
  const uniformWeight = state.worlds.length ? 1 / state.worlds.length : 0;
  const normalizedWeight = (weight: number): number =>
    totalWeight > Number.EPSILON ? Math.max(0, weight) / totalWeight : uniformWeight;

  return {
    worldCount: state.worlds.length,
    possibilitiesTruncated: state.possibilitiesTruncated,
    players: players.map((player) => {
      const expected = [0, 0, 0, 0, 0] as DecisionResourceTuple;
      const pAtLeastOne = [0, 0, 0, 0, 0] as DecisionResourceTuple;
      const minimum = [Infinity, Infinity, Infinity, Infinity, Infinity] as DecisionResourceTuple;
      const maximum = [0, 0, 0, 0, 0] as DecisionResourceTuple;
      for (const world of state.worlds) {
        const hand = resourceTuple(world.hands[player]);
        const weight = normalizedWeight(world.weight ?? 0);
        for (let index = 0; index < hand.length; index += 1) {
          const count = hand[index] ?? 0;
          expected[index] = (expected[index] ?? 0) + count * weight;
          if (count > 0) {
            pAtLeastOne[index] = (pAtLeastOne[index] ?? 0) + weight;
          }
          minimum[index] = Math.min(minimum[index] ?? Infinity, count);
          maximum[index] = Math.max(maximum[index] ?? 0, count);
        }
      }
      if (!state.worlds.length) minimum.fill(0);
      return { player, expected, pAtLeastOne, minimum, maximum };
    }),
  };
};

const refreshLifecycleStatus = (trace: DecisionTrace): void => {
  if (trace.executionFinishedAt !== undefined) {
    trace.lifecycleStatus = trace.executionSucceeded
      ? "execution-complete"
      : "execution-failed";
  } else if (trace.deepStatus === "superseded") {
    trace.lifecycleStatus = "superseded";
  } else if (trace.executionStartedAt !== undefined) {
    trace.lifecycleStatus = "execution-pending";
  } else if (trace.finalAction !== undefined) {
    trace.lifecycleStatus = "action-selected";
  } else if (trace.deepStatus === "complete") {
    trace.lifecycleStatus = "search-complete";
  } else if (trace.deepStatus === "failed") {
    trace.lifecycleStatus = "search-failed";
  } else {
    trace.lifecycleStatus = "search-pending";
  }
};

export class DecisionTraceRecorder {
  private readonly traces = new Map<string, DecisionTrace>();
  private persistTimer?: ReturnType<typeof globalThis.setTimeout>;
  private storageOperations: Promise<void> = Promise.resolve();
  private legacyStorageEnabled = true;
  private readonly recordSignatures = new Map<string, string>();
  private readonly recordBeliefCaptured = new Set<string>();
  private analysisSearchResults = new WeakMap<DecisionAnalysis, { id: string; originStateHash: string }>();
  private nextSearchResultId = 1;

  constructor(private readonly onChange?: () => void) {}

  setLegacyStorageEnabled(enabled: boolean): void {
    if (enabled === this.legacyStorageEnabled) return;
    this.legacyStorageEnabled = enabled;
    if (!enabled) {
      if (this.persistTimer !== undefined) {
        globalThis.clearTimeout(this.persistTimer);
        this.persistTimer = undefined;
      }
      void this.enqueueStorage(() =>
        chrome.storage.local.remove(DECISION_TRACE_STORAGE_KEY),
      ).catch(() => undefined);
      return;
    }
    this.scheduleLegacyPersist();
  }

  begin(
    stateHash: string,
    state: TrackerState,
    board: BoardSnapshot,
    startedAt = performance.now(),
    context?: {
      settings?: DecisionTraceSettings;
      searchConstraints?: DecisionSearchConstraints;
    },
  ): void {
    this.supersedePending(stateHash, startedAt);
    const existing = this.traces.get(stateHash);
    if (existing) {
      if (existing.deepStatus === "pending") return;
      existing.deepRequestStartedAt = startedAt;
      existing.deepRequestFinishedAt = undefined;
      existing.deepLatencyMs = undefined;
      existing.deepSlowWarningAtMs = undefined;
      existing.deepStatus = "pending";
      existing.lifecycleStatus = "search-pending";
      existing.deepFailureReason = undefined;
      existing.deepTimedOut = false;
      existing.settings = context?.settings
        ? { ...context.settings }
        : existing.settings;
      existing.searchConstraints = context?.searchConstraints
        ? searchConstraintSnapshot(context.searchConstraints)
        : existing.searchConstraints;
      existing.deepAttempts ??= [];
      existing.deepAttempts.push({
        startedAt,
        status: "pending",
        timedOut: false,
      });
      if (existing.deepAttempts.length > MAX_SEARCH_ATTEMPTS) {
        existing.deepAttempts.splice(
          0,
          existing.deepAttempts.length - MAX_SEARCH_ATTEMPTS,
        );
      }
      this.schedulePersist();
      return;
    }
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
      ...(context?.settings ? { settings: { ...context.settings } } : {}),
      ...(context?.searchConstraints
        ? { searchConstraints: searchConstraintSnapshot(context.searchConstraints) }
        : {}),
      beliefParticleCount: state.worlds.length,
      sourceWorldCount: state.worlds.length,
      beliefSummary: summarizeBeliefs(state, board.myPlayer),
      deepRequestStartedAt: startedAt,
      deepStatus: "pending",
      lifecycleStatus: "search-pending",
      deepTimedOut: false,
      deepAttempts: [{ startedAt, status: "pending", timedOut: false }],
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
    const attempt = trace.deepAttempts?.at(-1);
    // Once a position is superseded its late worker response is obsolete
    // evidence. Never revive the top-level status or attach a search result
    // after the recorder has declared the active attempt stale.
    if (trace.deepStatus === "superseded" || attempt?.status === "superseded") {
      return;
    }
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
    if (attempt?.status === "pending") {
      attempt.finishedAt = finishedAt;
      attempt.latencyMs = trace.deepLatencyMs;
      attempt.status = "complete";
      attempt.timedOut = trace.deepTimedOut;
    }
    trace.decisionModel = analysis.model;
    trace.runtimeReason = analysis.runtimeReason;
    trace.engine = analysis.engine;
    trace.runtime = analysis.runtime;
    trace.engineRevision = analysis.deepSearch?.engineRevision;
    trace.diceMode = analysis.deepSearch?.diceMode;
    trace.chanceModel = analysis.deepSearch?.chanceModel;
    trace.requestedStochasticModel = analysis.deepSearch?.requestedStochasticModel;
    trace.stochasticModel = analysis.deepSearch?.stochasticModel;
    trace.beliefPolicy = analysis.deepSearch?.beliefPolicy;
    trace.diceHistoryProvenance = analysis.deepSearch?.diceHistoryProvenance;
    trace.publicHistoryDigest = analysis.deepSearch?.publicHistoryDigest;
    trace.stochasticBeliefDigest = analysis.deepSearch?.stochasticBeliefDigest;
    trace.stochasticBeliefParticleCount =
      analysis.deepSearch?.stochasticBeliefParticleCount;
    trace.nativeGpuBuild = analysis.nativeGpuBuild
      ? structuredClone(analysis.nativeGpuBuild)
      : undefined;
    trace.algorithm = analysis.deepSearch?.algorithm;
    trace.effectiveSearchEffort = analysis.deepSearch?.effectiveSearchEffort
      ? structuredClone(analysis.deepSearch.effectiveSearchEffort)
      : undefined;
    trace.decisionRationale = analysis.deepSearch
      ? explainDeepSearchDecision(analysis.deepSearch)
      : undefined;
    let searchResult = this.analysisSearchResults.get(analysis);
    if (!searchResult) {
      searchResult = {
        id: `S${this.nextSearchResultId++}`,
        originStateHash: stateHash,
      };
      this.analysisSearchResults.set(analysis, searchResult);
    }
    trace.searchResultId = searchResult.id;
    trace.searchResultOriginStateHash = searchResult.originStateHash;
    trace.searchResultReused = searchResult.originStateHash !== stateHash;
    trace.searchElapsedMs = analysis.deepSearch?.elapsedMs;
    trace.searchStages = analysis.deepSearch?.searchStages;
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
    const strategicCandidates =
      analysis.deepSearch?.authority === "exact-mandatory"
        ? []
        : analysis.deepSearch?.actions.map((candidate) => ({
            action: candidate.action,
            value: candidate.value[rootIndex] ?? candidate.value[0] ?? 0,
            visits: candidate.visits,
            availabilityWeight: candidate.availabilityWeight,
            legalWeight: candidate.legalWeight,
            lowerConfidenceValue:
              candidate.lowerConfidenceValue[rootIndex] ??
              candidate.lowerConfidenceValue[0],
            prior: candidate.prior,
            source: "strategic" as const,
          })) ?? [];
    const exactCandidates = analysis.deepSearch?.exactActions?.map((candidate) => ({
      action: candidate.action,
      value: candidate.value[rootIndex] ?? candidate.value[0] ?? 0,
      legalWeight: candidate.legalWeight,
      lowerConfidenceValue:
        candidate.lowerBound[rootIndex] ?? candidate.lowerBound[0],
      source: "exact" as const,
      decisionScore: candidate.decisionScore,
      lowerScore: candidate.lowerScore,
      comparatorScore: candidate.comparatorScore,
    })) ?? [];
    trace.deepCandidates = [...strategicCandidates, ...exactCandidates];
    refreshLifecycleStatus(trace);
    this.schedulePersist();
  }

  slow(stateHash: string): void {
    const trace = this.traces.get(stateHash);
    if (!trace || trace.deepSlowWarningAtMs !== undefined) return;
    trace.deepSlowWarningAtMs =
      trace.deepRequestStartedAt === undefined
        ? undefined
        : performance.now() - trace.deepRequestStartedAt;
    const attempt = trace.deepAttempts?.at(-1);
    if (attempt?.status === "pending") {
      attempt.slowWarningAtMs = trace.deepSlowWarningAtMs;
    }
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
    const attempt = trace.deepAttempts?.at(-1);
    if (attempt?.status === "pending") {
      attempt.finishedAt = finishedAt;
      attempt.latencyMs = trace.deepLatencyMs;
      attempt.status = "failed";
      attempt.failureReason = reason;
      attempt.timedOut = false;
    }
    refreshLifecycleStatus(trace);
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
    refreshLifecycleStatus(trace);
    this.schedulePersist();
  }

  executionStarted(stateHash: string): void {
    const trace = this.traces.get(stateHash);
    if (!trace || trace.executionStartedAt !== undefined) return;
    trace.executionStartedAt = Date.now();
    trace.executedBeforeDeepResult = trace.deepRequestFinishedAt === undefined;
    refreshLifecycleStatus(trace);
    this.schedulePersist();
  }

  execution(
    stateHash: string,
    succeeded: boolean,
    failureReason?: string,
    diagnostic?: DecisionExecutionDiagnostic,
  ): void {
    const trace = this.traces.get(stateHash);
    if (!trace) return;
    trace.executionFinishedAt = Date.now();
    trace.executionSucceeded = succeeded;
    trace.executionFailureReason = failureReason;
    trace.executionDiagnostic = diagnostic
      ? structuredClone(diagnostic)
      : undefined;
    refreshLifecycleStatus(trace);
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

  /**
   * Record Mode only needs traces whose analytical evidence changed since the
   * previous capture. Compare the compact trace surface first, then clone the
   * bounded replay state only for changed decisions.
   */
  snapshotForRecord(): DecisionTrace[] {
    const changed: DecisionTrace[] = [];
    for (const [stateHash, trace] of this.traces) {
      const {
        replayState: _replayState,
        replayBoard: _replayBoard,
        ...compact
      } = trace;
      const signature = JSON.stringify(compact);
      if (this.recordSignatures.get(stateHash) === signature) continue;
      this.recordSignatures.set(stateHash, signature);
      if (this.recordBeliefCaptured.has(stateHash)) {
        changed.push(structuredClone(compact));
      } else {
        // Raw replay evidence is transfer-only in Record Mode. Hand one detached
        // copy to the compact encoder, then release the resident heavy state.
        const firstCapture = structuredClone(trace);
        this.recordBeliefCaptured.add(stateHash);
        delete trace.replayState;
        delete trace.replayBoard;
        changed.push(firstCapture);
      }
    }
    return changed;
  }

  async reset(): Promise<void> {
    if (this.persistTimer !== undefined) {
      globalThis.clearTimeout(this.persistTimer);
      this.persistTimer = undefined;
    }
    this.traces.clear();
    this.recordSignatures.clear();
    this.recordBeliefCaptured.clear();
    this.analysisSearchResults = new WeakMap();
    this.nextSearchResultId = 1;
    await this.enqueueStorage(() =>
      chrome.storage.local.remove(DECISION_TRACE_STORAGE_KEY),
    );
  }

  supersedePending(
    activeStateHash?: string,
    finishedAt = performance.now(),
  ): void {
    let changed = false;
    for (const [stateHash, trace] of this.traces) {
      if (stateHash === activeStateHash || trace.deepStatus !== "pending") continue;
      trace.deepRequestFinishedAt = finishedAt;
      trace.deepLatencyMs =
        trace.deepRequestStartedAt === undefined
          ? undefined
          : Math.max(0, finishedAt - trace.deepRequestStartedAt);
      trace.deepStatus = "superseded";
      const attempt = trace.deepAttempts?.at(-1);
      if (attempt?.status === "pending") {
        attempt.finishedAt = finishedAt;
        attempt.latencyMs = trace.deepLatencyMs;
        attempt.status = "superseded";
        attempt.timedOut = false;
      }
      refreshLifecycleStatus(trace);
      changed = true;
    }
    if (changed) this.schedulePersist();
  }

  private schedulePersist(): void {
    this.onChange?.();
    this.scheduleLegacyPersist();
  }

  private scheduleLegacyPersist(): void {
    if (!this.legacyStorageEnabled || this.persistTimer !== undefined) return;
    const storage = chrome.storage.local;
    this.persistTimer = globalThis.setTimeout(() => {
      this.persistTimer = undefined;
      if (!this.legacyStorageEnabled) return;
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
