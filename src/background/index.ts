import {
  analyzeDecisionRequest,
} from "../worker/analyze";
import { warmDeepSearchEngine } from "../worker/deep-search";
import {
  NativeGpuClient,
  nativeGpuSupportsProductionExactMaxn,
  nativeGpuSupportsStochasticModel,
} from "./native-gpu";
import {
  M0_FAIR_IID_2D6_V1,
  MREF_COLONIST_LINKED_2024_V1,
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

const nativeGpu = new NativeGpuClient();

// Production deep-search prefers the native exact-CUDA backend when the
// companion advertises deadline/cancellation support and the same stochastic
// model. CPU/WASM remains the transport-failure and unsupported-state fallback.
const NATIVE_GPU_ENABLED = true;

interface ActiveDecision {
  nativeId: number;
  controller: AbortController;
}

const activeDecisions = new Map<string, ActiveDecision>();
let nextNativeDecisionId = 1;
let gpuOwner: ActiveDecision | undefined;

const decisionKey = (sender: chrome.runtime.MessageSender, id: number): string =>
  JSON.stringify([sender.tab?.id, sender.documentId, sender.frameId, id]);

const cancelDecision = (decision: ActiveDecision): void => {
  decision.controller.abort(new Error("Decision cancelled as stale"));
  nativeGpu.cancelDecision(decision.nativeId);
};

export const withRemainingDecisionBudget = (
  message: DecisionMessage,
  localStartedAt: number,
): DecisionMessage => {
  if (!message.decisionBudget) return message;
  const localElapsedMs = Math.max(0, performance.now() - localStartedAt);
  return {
    ...message,
    decisionBudget: {
      ...message.decisionBudget,
      remainingEngineMs: Math.max(
        0,
        Math.floor(message.decisionBudget.remainingEngineMs - localElapsedMs),
      ),
    },
  };
};

const hasPendingIncomingTrade = (message: DecisionMessage): boolean =>
  Boolean(
    message.board.activeTrades?.some(
      (trade) =>
        trade.incoming &&
        !trade.responsesComplete &&
        (trade.myResponse === undefined || trade.myResponse === "pending"),
    ),
  );

export const shouldUseNativeGpu = (message: DecisionMessage): boolean =>
  NATIVE_GPU_ENABLED &&
  nativeGpuSupportsStochasticModel(message.stochastic?.model) &&
  message.engine === "deep-search" &&
  // Explicit strategy policies remain on their declared CPU/WASM owner until
  // that policy has its own production acceptance lane.
  !message.strategyPolicy &&
  !message.board.initialPlacement &&
  (Boolean(message.board.isMyTurn) || hasPendingIncomingTrade(message));

const errorDetail = (error: unknown, fallback: string): string => {
  if (error instanceof Error) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  try {
    const serialized = JSON.stringify(error);
    if (serialized && serialized !== "{}") return serialized;
  } catch {
    // Fall through to a stable message when the thrown value is cyclic.
  }
  return fallback;
};

const isNativeGpuTransportFailure = (error: unknown): boolean =>
  /(?:native host has exited|native messaging host|gpu companion (?:is )?disconnected|gpu companion handshake timed out|gpu companion message could not be sent|disconnected port)/iu.test(
    errorDetail(error, ""),
  );

// This fallback is intentionally transport-only. If the native companion dies
// or its port disconnects, preserve the user's selected deep-search algorithm
// and stochastic model on CPU/WASM for this decision. Semantic/search errors are
// not permission to silently substitute another model or policy.
const analyzeAfterNativeGpuTransportFailure = async (
  message: DecisionMessage,
  error: unknown,
  localStartedAt: number,
) => {
  const detail = errorDetail(error, "Native GPU transport failed");
  nativeGpu.release();
  const analysis = await analyzeDecisionRequest(
    withRemainingDecisionBudget(message, localStartedAt),
  );
  const requestedStochasticModel =
    message.stochastic?.model ?? M0_FAIR_IID_2D6_V1;
  return {
    ...analysis,
    runtime: "background-wasm" as const,
    runtimeReason:
      requestedStochasticModel === MREF_COLONIST_LINKED_2024_V1
        ? `Native GPU transport failed (${detail}); Mref preserved on CPU/WASM Deep MaxN for this decision`
        : `Native GPU transport failed (${detail}); ${requestedStochasticModel} preserved on CPU/WASM Deep MaxN for this decision`,
  };
};

const isDecisionMessage = (value: unknown): value is DecisionMessage => {
  if (!value || typeof value !== "object") return false;
  const message = value as Partial<DecisionMessage>;
  return Boolean(
    message.type === DECISION_MESSAGE_TYPE &&
      typeof message.id === "number" &&
      message.state &&
      message.board &&
      typeof message.rootPlayer === "string" &&
      typeof message.engine === "string",
  );
};

chrome.runtime.onMessage.addListener(
  (message: unknown, sender, sendResponse) => {
    if (
      message &&
      typeof message === "object" &&
      (message as Partial<DecisionCancelMessage>).type ===
        DECISION_CANCEL_MESSAGE_TYPE &&
      typeof (message as Partial<DecisionCancelMessage>).id === "number"
    ) {
      const decision = activeDecisions.get(decisionKey(sender, (message as DecisionCancelMessage).id));
      if (decision) cancelDecision(decision);
      return undefined;
    }
    if (
      message &&
      typeof message === "object" &&
      (message as Partial<DecisionStatusMessage>).type ===
        DECISION_STATUS_MESSAGE_TYPE &&
      typeof (message as Partial<DecisionStatusMessage>).id === "number"
    ) {
      const status = message as DecisionStatusMessage;
      const startedAt = performance.now();
      void (async () => {
        if (status.engine === "deep-search" && NATIVE_GPU_ENABLED) {
          let gpu;
          try {
            gpu = await nativeGpu.status();
          } catch (error) {
            if (!isNativeGpuTransportFailure(error)) throw error;
          }
          if (gpu && nativeGpuSupportsProductionExactMaxn(gpu)) {
            const response: DecisionStatusMessageResponse = {
              id: status.id,
              runtime: "background-gpu",
              engineRevision: gpu.engineRevision,
              ...(gpu.build ? { nativeGpuBuild: gpu.build } : {}),
              deviceName: gpu.device.name,
              initializationMs: performance.now() - startedAt,
            };
            sendResponse(response);
            return;
          }
        }
        if (status.engine === "weighted" && !gpuOwner) nativeGpu.release();
        const wasm = await warmDeepSearchEngine();
        const response: DecisionStatusMessageResponse = {
          id: status.id,
          runtime: "background-wasm",
          engineRevision: wasm.engineRevision,
          initializationMs: wasm.initializationMs,
        };
        sendResponse(response);
      })().catch((error: unknown) => {
        const response: DecisionStatusMessageResponse = {
          id: status.id,
          error: errorDetail(error, "Decision engine initialization failed"),
        };
        sendResponse(response);
      });
      return true;
    }
    if (!isDecisionMessage(message)) return undefined;
    const key = decisionKey(sender, message.id);
    const previous = activeDecisions.get(key);
    if (previous) cancelDecision(previous);
    const decision: ActiveDecision = {
      nativeId: nextNativeDecisionId++,
      controller: new AbortController(),
    };
    activeDecisions.set(key, decision);
    const { signal } = decision.controller;
    const finish = () => {
      if (activeDecisions.get(key) === decision) activeDecisions.delete(key);
      if (gpuOwner === decision) gpuOwner = undefined;
    };
    void (async () => {
      const backgroundStartedAt = performance.now();
      const nativeGpuEligible = shouldUseNativeGpu(message);
      // One native search owns the companion at a time. Concurrent documents
      // use the same MaxN/stochastic policy on WASM rather than canceling the
      // owner's work or waiting away their live decision allowance.
      const gpuBusy = nativeGpuEligible && gpuOwner !== undefined;
      if (nativeGpuEligible && !gpuBusy) {
        gpuOwner = decision;
        let gpu;
        try {
          gpu = await nativeGpu.status();
          signal.throwIfAborted();
        } catch (error) {
          signal.throwIfAborted();
          if (!isNativeGpuTransportFailure(error)) throw error;
          return analyzeAfterNativeGpuTransportFailure(
            message,
            error,
            backgroundStartedAt,
          );
        }
        if (
          gpu &&
          nativeGpuSupportsProductionExactMaxn(gpu) &&
          nativeGpuSupportsStochasticModel(
            message.stochastic?.model,
            gpu.stochasticModels,
          )
        ) {
          try {
            const analysis = await analyzeDecisionRequest(
              withRemainingDecisionBudget(message, backgroundStartedAt),
              (request) => nativeGpu.analyzeExact(request, decision.nativeId, signal),
            );
            return {
              ...analysis,
              runtime: "background-gpu" as const,
              runtimeReason: `Exact CUDA MaxN on ${gpu.device.name}`,
              ...(gpu.build ? { nativeGpuBuild: gpu.build } : {}),
            };
          } catch (error) {
            signal.throwIfAborted();
            if (!isNativeGpuTransportFailure(error)) throw error;
            return analyzeAfterNativeGpuTransportFailure(
              message,
              error,
              backgroundStartedAt,
            );
          }
        }
      }
      signal.throwIfAborted();
      if (message.engine === "weighted" && !gpuOwner) nativeGpu.release();
      const analysis = await analyzeDecisionRequest(
        withRemainingDecisionBudget(message, backgroundStartedAt),
      );
      const runtime = analysis.deepSearch
        ? ("background-wasm" as const)
        : ("background-rollout" as const);
      const requestedStochasticModel =
        message.stochastic?.model ?? M0_FAIR_IID_2D6_V1;
      const runtimeReason =
        analysis.runtimeReason ??
        (runtime === "background-wasm"
          ? message.strategyPolicy
            ? `Strategy policy ${message.strategyPolicy} remains on its CPU/WASM owner`
            : message.engine === "deep-search" && message.board.initialPlacement
              ? "Dedicated opening solver runs on WASM/CPU"
              : !NATIVE_GPU_ENABLED && message.engine === "deep-search"
                ? "Native GPU disabled for CPU/WASM validation; using WASM Deep MaxN"
                : gpuBusy
                  ? "Native GPU busy with another decision; using WASM Deep MaxN"
                  : nativeGpuEligible
                    ? "Native GPU unavailable; using WASM Deep MaxN"
                    : message.engine === "deep-search"
                      ? requestedStochasticModel === MREF_COLONIST_LINKED_2024_V1
                        ? "Native exact CUDA MaxN is preferred when its protocol-7 capability is available; Mref remains authoritative and gpu-root-rollout remains experimental"
                        : "Native exact CUDA MaxN is preferred when its protocol-7 capability is available; weighted-belief MaxN remains the CPU/WASM fallback and gpu-root-rollout remains experimental"
                      : message.engine === "weighted"
                        ? "Weighted mode runs on WASM"
                        : undefined
          : undefined);
      return {
        ...analysis,
        runtime,
        ...(runtimeReason ? { runtimeReason } : {}),
      };
    })()
      .then((analysis) => {
        signal.throwIfAborted();
        finish();
        const response: DecisionMessageResponse = {
          id: message.id,
          analysis,
          execution: "background",
        };
        sendResponse(response);
      })
      .catch((error: unknown) => {
        finish();
        const response: DecisionMessageResponse = {
          id: message.id,
          error: errorDetail(error, "Decision analysis failed"),
        };
        sendResponse(response);
      });
    return true;
  },
);
