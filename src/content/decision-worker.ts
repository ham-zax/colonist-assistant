import type {
  DecisionAnalysis,
  DecisionEngine,
} from "../core/engine";
import type { BoardSnapshot } from "../core/placement";
import type { TrackerState } from "../core/types";
import {
  DECISION_MESSAGE_TYPE,
  DECISION_STATUS_MESSAGE_TYPE,
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

const SLOW_DECISION_MS = 5_000;
const TRANSPORT_RECOVERY_MS = 6_500;
const TRANSPORT_RECOVERY_ERROR =
  "Selected WASM request transport exceeded the recovery window";

export interface DecisionServiceStatus {
  runtime: "background-wasm" | "engine-error";
  detail: string;
  initializationMs?: number;
}

interface PendingDecision extends DecisionRequest {
  id: number;
  key: string;
  generation: number;
  enqueuedAt: number;
  callback: (analysis: DecisionAnalysis) => void;
  slowCallback?: (elapsedMs: number) => void;
  failureCallback?: (detail: string) => void;
  transportAttempts: number;
}

export class DecisionWorkerClient {
  private nextId = 1;
  private generation = 1;
  private active?: PendingDecision;
  private queued?: PendingDecision;
  private completedKey = "";
  private desiredKey = "";
  private readiness?: Promise<DecisionStatusMessageResponse>;
  private destroyed = false;
  private contextInvalidated = false;

  warm(callback: (status: DecisionServiceStatus) => void): void {
    this.readiness ??= this.queryStatus();
    void this.readiness
      .then((response) => {
        if (
          response.runtime !== "background-wasm" ||
          !response.engineRevision
        ) {
          throw new Error(response.error ?? "WASM service returned no status");
        }
        callback({
          runtime: "background-wasm",
          detail: `${response.engineRevision} ready`,
          ...(response.initializationMs !== undefined
            ? { initializationMs: response.initializationMs }
            : {}),
        });
      })
      .catch((error: unknown) => {
        if (isExtensionContextInvalidatedError(error)) {
          this.contextInvalidated = true;
        }
        this.readiness = undefined;
        callback({
          runtime: "engine-error",
          detail:
            this.contextInvalidated
              ? EXTENSION_CONTEXT_RELOAD_MESSAGE
              : error instanceof Error
              ? error.message
              : "The background WASM service did not respond",
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
  ): boolean {
    if (
      this.destroyed ||
      this.contextInvalidated ||
      engine === "race-eta" ||
      (
        this.active?.key === key &&
        this.active.generation === this.generation
      ) ||
      (
        this.queued?.key === key &&
        this.queued.generation === this.generation
      ) ||
      key === this.completedKey
    ) {
      return false;
    }
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
      transportAttempts: 0,
    };
    this.desiredKey = key;
    this.pump();
    return true;
  }

  private pump(): void {
    if (this.active || !this.queued) return;
    const request = this.queued;
    this.queued = undefined;
    this.active = request;
    const startedAt = performance.now();
    const message: DecisionMessage = {
      type: DECISION_MESSAGE_TYPE,
      id: request.id,
      state: request.state,
      board: request.board,
      rootPlayer: request.rootPlayer,
      engine: request.engine,
    };
    const slowTimer = globalThis.setTimeout(() => {
      const elapsedMs = performance.now() - startedAt;
      const stale =
        request.generation !== this.generation ||
        request.key !== this.desiredKey;
      console.warn("[Colonist Assistant] Decision still running", {
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
      });
      if (!stale) request.slowCallback?.(elapsedMs);
    }, SLOW_DECISION_MS);
    let recoveryTimer: ReturnType<typeof globalThis.setTimeout> | undefined;
    const response = Promise.race([
      this.send(message),
      new Promise<DecisionMessageResponse>((resolve) => {
        recoveryTimer = globalThis.setTimeout(
          () =>
            resolve({
              id: request.id,
              error: TRANSPORT_RECOVERY_ERROR,
            }),
          TRANSPORT_RECOVERY_MS,
        );
      }),
    ]);
    void response
      .then((response) => {
        const finishedAt = performance.now();
        const totalMs = finishedAt - request.enqueuedAt;
        if (totalMs >= 1_000) {
          const search = response.analysis?.deepSearch;
          console.warn("[Colonist Assistant] Slow decision", {
            key: request.key,
            engine: request.engine,
            totalMs: Math.round(totalMs),
            queueWaitMs: Math.round(startedAt - request.enqueuedAt),
            serviceMs: Math.round(finishedAt - startedAt),
            wasmSearchMs:
              search?.elapsedMs === undefined
                ? undefined
                : Math.round(search.elapsedMs),
            runtime: response.analysis?.runtime ?? "no-analysis",
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
            error:
              response.error ??
              response.analysis?.runtimeReason,
          });
        }
        const stale =
          request.generation !== this.generation ||
          request.key !== this.desiredKey ||
          response.id !== request.id;
        if (
          !stale &&
          response.error === TRANSPORT_RECOVERY_ERROR
        ) {
          // A valid live WASM search is bounded well below this window. A
          // message that never settles is a lost/suspended extension-service
          // transport, not permission to switch algorithms. Re-issue the same
          // selected-engine request with a fresh message id and generation.
          // The old result is ignored if it eventually arrives.
          console.warn("[Colonist Assistant] Retrying selected WASM request", {
            key: request.key,
            engine: request.engine,
            attempt: request.transportAttempts + 2,
            elapsedMs: Math.round(totalMs),
            policy: "selected-engine-only",
            fallbackStarted: false,
          });
          this.queued = {
            ...request,
            id: this.nextId++,
            generation: this.generation,
            enqueuedAt: performance.now(),
            transportAttempts: request.transportAttempts + 1,
          };
          return;
        }
        if (stale) {
          return;
        }
        if (!response.analysis) {
          request.failureCallback?.(
            response.error ?? "Decision service returned no analysis",
          );
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
        return {
          ...response,
          analysis: {
            ...response.analysis,
            runtime: response.analysis.deepSearch
              ? "background-wasm"
              : "background-rollout",
          },
        };
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

  private queryStatus(): Promise<DecisionStatusMessageResponse> {
    const message: DecisionStatusMessage = {
      type: DECISION_STATUS_MESSAGE_TYPE,
      id: this.nextId++,
    };
    return chrome.runtime.sendMessage<DecisionStatusMessageResponse>(message);
  }

  reset(): void {
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
