import type {
  DecisionAnalysis,
  DecisionEngine,
  DecisionSearchConstraints,
  NativeGpuBuildIdentity,
} from "../core/engine";
import type { BoardSnapshot } from "../core/placement";
import type { TrackerState } from "../core/types";
import {
  M0_FAIR_IID_2D6_V1,
  type PublicStochasticInput,
} from "../core/dice-history";
import {
  DECISION_CANCEL_MESSAGE_TYPE,
  DECISION_MESSAGE_TYPE,
  DECISION_STATUS_MESSAGE_TYPE,
  type DecisionCancelMessage,
  type DecisionMessage,
  type DecisionMessageResponse,
  type DecisionStatusMessage,
  type DecisionStatusMessageResponse,
} from "../worker/protocol";
import type { DecisionRequest } from "../worker/analyze";
import {
  EXTENSION_CONTEXT_RELOAD_MESSAGE,
  isExtensionContextInvalidatedError,
} from "./extension-context";

const SLOW_DECISION_MS = 10_000;
const HARD_DECISION_MS = 12_000;
const HARD_DECISION_ERROR =
  "Strategist did not return before the 12-second safety limit";

export interface DecisionServiceStatus {
  runtime: "background-gpu" | "background-wasm" | "engine-error";
  detail: string;
  nativeGpuBuild?: NativeGpuBuildIdentity;
  initializationMs?: number;
}

export type DecisionRequestDisposition =
  | "started"
  | "queued"
  | "duplicate"
  | "completed"
  | "context-invalidated"
  | "destroyed";

interface PendingDecision extends DecisionRequest {
  id: number;
  key: string;
  generation: number;
  enqueuedAt: number;
  callback: (analysis: DecisionAnalysis) => void;
  slowCallback?: (elapsedMs: number) => void;
  failureCallback?: (detail: string) => void;
  startCallback?: () => void;
  waitedForActive: boolean;
}

export class DecisionWorkerClient {
  private nextId = 1;
  // Cancellation is advisory across extension/background boundaries: an old
  // response can still arrive after reset(). Generation tags make every such
  // late result stale locally, so a previous board/reload state can never
  // overwrite the strategist result for the current position.
  private generation = 1;
  private active?: PendingDecision;
  private queued?: PendingDecision;
  private completedKey = "";
  private desiredKey = "";
  private readiness?: {
    engine: DecisionEngine;
    promise: Promise<DecisionStatusMessageResponse>;
  };
  private destroyed = false;
  private contextInvalidated = false;

  warm(callback: (status: DecisionServiceStatus) => void): void;
  warm(
    engine: DecisionEngine,
    callback: (status: DecisionServiceStatus) => void,
  ): void;
  warm(
    engineOrCallback: DecisionEngine | ((status: DecisionServiceStatus) => void),
    maybeCallback?: (status: DecisionServiceStatus) => void,
  ): void {
    const engine =
      typeof engineOrCallback === "function" ? "deep-search" : engineOrCallback;
    const callback =
      typeof engineOrCallback === "function" ? engineOrCallback : maybeCallback!;
    if (!this.readiness || this.readiness.engine !== engine) {
      this.readiness = {
        engine,
        promise: this.queryStatus(engine),
      };
    }
    const readiness = this.readiness;
    void readiness.promise
      .then((response) => {
        if (this.readiness !== readiness) return;
        if (!response.runtime || !response.engineRevision) {
          throw new Error(response.error ?? "Decision service returned no status");
        }
        callback({
          runtime: response.runtime,
          detail:
            response.runtime === "background-gpu"
              ? `${response.engineRevision} · ${response.deviceName ?? "CUDA GPU"} ready`
              : `${response.engineRevision} ready`,
          ...(response.nativeGpuBuild
            ? { nativeGpuBuild: response.nativeGpuBuild }
            : {}),
          ...(response.initializationMs !== undefined
            ? { initializationMs: response.initializationMs }
            : {}),
        });
      })
      .catch((error: unknown) => {
        if (this.readiness !== readiness) return;
        if (isExtensionContextInvalidatedError(error)) {
          this.contextInvalidated = true;
        }
        if (this.readiness === readiness) this.readiness = undefined;
        callback({
          runtime: "engine-error",
          detail:
            this.contextInvalidated
              ? EXTENSION_CONTEXT_RELOAD_MESSAGE
              : error instanceof Error
              ? error.message
              : "The background decision service did not respond",
        });
      });
  }

  request(
    key: string,
    state: TrackerState,
    board: BoardSnapshot,
    rootPlayer: string,
    engine: DecisionEngine,
    callback: (analysis: DecisionAnalysis) => void,
    slowCallback?: (elapsedMs: number) => void,
    failureCallback?: (detail: string) => void,
    searchConstraints?: DecisionSearchConstraints,
    playerTradesEnabled = true,
    startCallback?: () => void,
    stochastic: PublicStochasticInput = { model: M0_FAIR_IID_2D6_V1 },
  ): DecisionRequestDisposition {
    if (this.destroyed) return "destroyed";
    if (this.contextInvalidated) return "context-invalidated";
    if (
      (
        this.active?.key === key &&
        this.active.generation === this.generation
      ) ||
      (
        this.queued?.key === key &&
        this.queued.generation === this.generation
      )
    ) {
      return "duplicate";
    }
    if (key === this.completedKey) return "completed";
    if (this.active) this.cancelDecision(this.active.id);
    const waitedForActive = Boolean(this.active);
    this.queued = {
      id: this.nextId++,
      key,
      generation: this.generation,
      enqueuedAt: performance.now(),
      state,
      board,
      rootPlayer,
      engine,
      callback,
      ...(slowCallback ? { slowCallback } : {}),
      ...(failureCallback ? { failureCallback } : {}),
      ...(startCallback ? { startCallback } : {}),
      ...(searchConstraints ? { searchConstraints } : {}),
      playerTradesEnabled,
      stochastic,
      waitedForActive,
    };
    this.desiredKey = key;
    this.pump();
    return waitedForActive ? "queued" : "started";
  }

  private pump(): void {
    if (this.active || !this.queued) return;
    const request = this.queued;
    this.queued = undefined;
    this.active = request;
    if (request.waitedForActive) request.startCallback?.();
    const startedAt = performance.now();
    const slowDecisionMs = SLOW_DECISION_MS;
    const message: DecisionMessage = {
      type: DECISION_MESSAGE_TYPE,
      id: request.id,
      state: request.state,
      board: request.board,
      rootPlayer: request.rootPlayer,
      engine: request.engine,
      ...(request.searchConstraints
        ? { searchConstraints: request.searchConstraints }
        : {}),
      playerTradesEnabled: request.playerTradesEnabled ?? true,
      stochastic: request.stochastic ?? { model: M0_FAIR_IID_2D6_V1 },
    };
    const slowTimer = globalThis.setTimeout(() => {
      const elapsedMs = performance.now() - startedAt;
      const stale =
        request.generation !== this.generation ||
        request.key !== this.desiredKey;
      console.warn(
        `[Colonist Assistant] Decision still running (${request.engine}, ${Math.round(elapsedMs)} ms)`,
        {
          key: request.key,
          engine: request.engine,
          elapsedMs: Math.round(elapsedMs),
          gameKey: request.board.gameKey,
          turn: request.board.turn,
          phase: request.board.action ?? "none",
          isMyTurn: Boolean(request.board.isMyTurn),
          currentPlayer: request.board.currentPlayer,
          activeTrades: request.board.activeTrades?.length ?? 0,
          stale,
          policy: "selected-engine-only",
          fallbackStarted: false,
        },
      );
      if (!stale) request.slowCallback?.(elapsedMs);
    }, slowDecisionMs);
    let recoveryTimer: ReturnType<typeof globalThis.setTimeout> | undefined;
    let settledFailure: string | undefined;
    const response = Promise.race([
      this.send(message),
      new Promise<DecisionMessageResponse>((resolve) => {
        recoveryTimer = globalThis.setTimeout(() => {
          this.cancelDecision(request.id);
          resolve({
            id: request.id,
            error: HARD_DECISION_ERROR,
          });
        }, HARD_DECISION_MS);
      }),
    ]);
    void response
      .then((response) => {
        const finishedAt = performance.now();
        const totalMs = finishedAt - request.enqueuedAt;
        if (totalMs >= slowDecisionMs) {
          const search = response.analysis?.deepSearch;
          console.warn(
            `[Colonist Assistant] Slow decision (${request.engine}, ${Math.round(totalMs)} ms total)`,
            {
              key: request.key,
              engine: request.engine,
              totalMs: Math.round(totalMs),
              queueWaitMs: Math.round(startedAt - request.enqueuedAt),
              serviceMs: Math.round(finishedAt - startedAt),
              searchMs:
                search?.elapsedMs === undefined
                  ? undefined
                  : Math.round(search.elapsedMs),
              runtime: response.analysis?.runtime ?? "no-analysis",
              runtimeReason: response.analysis?.runtimeReason,
              selectedAction: search?.chosen?.kind ?? "none",
              nodes: search?.nodes,
              iterations: search?.iterations,
              particles: search?.particles,
              deepestDecisionDepth: search?.deepestDecisionDepth,
              gameKey: request.board.gameKey,
              turn: request.board.turn,
              phase: request.board.action ?? "none",
              isMyTurn: Boolean(request.board.isMyTurn),
              currentPlayer: request.board.currentPlayer,
              activeTrades: request.board.activeTrades?.length ?? 0,
              stale:
                request.generation !== this.generation ||
                request.key !== this.desiredKey,
              error: response.error,
            },
          );
        }
        const stale =
          request.generation !== this.generation ||
          request.key !== this.desiredKey ||
          response.id !== request.id;
        if (stale) {
          return;
        }
        if (!response.analysis) {
          settledFailure =
            response.error ?? "Decision service returned no analysis";
          return;
        }
        this.completedKey = request.key;
        request.callback(response.analysis);
      })
      .finally(() => {
        globalThis.clearTimeout(slowTimer);
        if (recoveryTimer !== undefined) {
          globalThis.clearTimeout(recoveryTimer);
        }
        if (this.active?.id === request.id) this.active = undefined;
        if (
          settledFailure &&
          request.generation === this.generation &&
          request.key === this.desiredKey
        ) {
          request.failureCallback?.(settledFailure);
        }
        this.pump();
      });
  }

  private async send(
    message: DecisionMessage,
  ): Promise<DecisionMessageResponse> {
    try {
      const response =
        await chrome.runtime.sendMessage<DecisionMessageResponse>(message);
      if (response?.id === message.id && response.analysis) {
        return response;
      }
      const detail =
        response?.error ??
        "Decision service returned no response";
      if (isExtensionContextInvalidatedError(detail)) {
        this.contextInvalidated = true;
      }
      return {
        id: message.id,
        error: this.contextInvalidated
          ? EXTENSION_CONTEXT_RELOAD_MESSAGE
          : detail,
      };
    } catch (error) {
      if (isExtensionContextInvalidatedError(error)) {
        this.contextInvalidated = true;
      }
      return {
        id: message.id,
        error:
          this.contextInvalidated
            ? EXTENSION_CONTEXT_RELOAD_MESSAGE
            : error instanceof Error
            ? error.message
            : "The extension background service did not respond",
      };
    }
  }

  private cancelDecision(id: number): void {
    const message: DecisionCancelMessage = {
      type: DECISION_CANCEL_MESSAGE_TYPE,
      id,
    };
    void chrome.runtime.sendMessage(message).catch((error: unknown) => {
      if (isExtensionContextInvalidatedError(error)) {
        this.contextInvalidated = true;
      }
    });
  }

  private queryStatus(
    engine: DecisionEngine,
  ): Promise<DecisionStatusMessageResponse> {
    const message: DecisionStatusMessage = {
      type: DECISION_STATUS_MESSAGE_TYPE,
      id: this.nextId++,
      engine,
    };
    return chrome.runtime.sendMessage<DecisionStatusMessageResponse>(message);
  }

  reset(): void {
    if (this.active) this.cancelDecision(this.active.id);
    this.generation += 1;
    this.queued = undefined;
    this.completedKey = "";
    this.desiredKey = "";
  }

  destroy(): void {
    this.destroyed = true;
    this.reset();
  }
}
