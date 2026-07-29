import {
  analyzeDecision,
  isDeepDecisionEngine,
  type DecisionAnalysis,
  type DecisionEngine,
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

const STATUS_TIMEOUT_MS = 5_000;
const ANALYSIS_TIMEOUT_MS = 1_250;

export interface DecisionServiceStatus {
  runtime: "background-wasm" | "local-fallback";
  detail: string;
  initializationMs?: number;
}

interface PendingDecision extends DecisionRequest {
  id: number;
  key: string;
  generation: number;
  enqueuedAt: number;
  callback: (analysis: DecisionAnalysis) => void;
}

export class DecisionWorkerClient {
  private nextId = 1;
  private generation = 1;
  private active?: PendingDecision;
  private queued?: PendingDecision;
  private completedKey = "";
  private desiredKey = "";
  private readiness?: Promise<DecisionStatusMessageResponse>;

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
        this.readiness = undefined;
        callback({
          runtime: "local-fallback",
          detail:
            error instanceof Error
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
  ): void {
    if (
      engine === "race-eta" ||
      this.active?.key === key ||
      this.queued?.key === key ||
      key === this.completedKey
    ) {
      return;
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
    };
    this.desiredKey = key;
    this.pump();
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
    void this.send(message)
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
            error: response.error,
          });
        }
        if (
          request.generation !== this.generation ||
          request.key !== this.desiredKey ||
          response.id !== request.id ||
          !response.analysis
        ) {
          return;
        }
        this.completedKey = request.key;
        request.callback(response.analysis);
      })
      .finally(() => {
        if (this.active?.id === request.id) this.active = undefined;
        this.pump();
      });
  }

  private async send(
    message: DecisionMessage,
  ): Promise<DecisionMessageResponse> {
    try {
      const response = await this.withTimeout(
        chrome.runtime.sendMessage<DecisionMessageResponse>(message),
        ANALYSIS_TIMEOUT_MS,
        "Deep search exceeded the 1.25-second interactive limit",
      );
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
      throw new Error("Decision service returned no response");
    } catch (error) {
      // Edge can restrict extension worker URLs from a content-script origin.
      // The extension service worker is preferred. A lightweight local model
      // keeps rendering and synchronization alive if that context is absent.
      try {
        const deep = isDeepDecisionEngine(message.engine);
        const fallback = analyzeDecision(
          message.state,
          message.board,
          message.rootPlayer,
          deep ? "hybrid" : message.engine,
        );
        const analysis = deep
          ? {
              ...fallback,
              engine: message.engine,
              model:
                "Deep Search service unavailable; using the local multiplayer rollout fallback",
              runtime: "local-fallback" as const,
              runtimeReason:
                error instanceof Error
                  ? error.message
                  : "The extension background service did not respond",
            }
          : {
              ...fallback,
              runtime: "local-fallback" as const,
              runtimeReason:
                error instanceof Error
                  ? error.message
                  : "The extension background service did not respond",
            };
        return { id: message.id, analysis };
      } catch (error) {
        return {
          id: message.id,
          error:
            error instanceof Error
              ? error.message
              : "Decision analysis failed",
        };
      }
    }
  }

  private queryStatus(): Promise<DecisionStatusMessageResponse> {
    const message: DecisionStatusMessage = {
      type: DECISION_STATUS_MESSAGE_TYPE,
      id: this.nextId++,
    };
    return this.withTimeout(
      chrome.runtime.sendMessage<DecisionStatusMessageResponse>(message),
      STATUS_TIMEOUT_MS,
      "Background WASM did not initialize within 5 seconds",
    );
  }

  private withTimeout<T>(
    promise: Promise<T>,
    timeoutMs: number,
    message: string,
  ): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const timeout = globalThis.setTimeout(
        () => reject(new Error(message)),
        timeoutMs,
      );
      void promise.then(
        (value) => {
          globalThis.clearTimeout(timeout);
          resolve(value);
        },
        (error: unknown) => {
          globalThis.clearTimeout(timeout);
          reject(error);
        },
      );
    });
  }

  reset(): void {
    this.generation += 1;
    this.active = undefined;
    this.queued = undefined;
    this.completedKey = "";
    this.desiredKey = "";
  }

  destroy(): void {
    this.reset();
  }
}
