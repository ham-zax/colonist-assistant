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

// Product promotion is deliberately separate from host capability. Protocol 7
// can expose exact CUDA for parity diagnostics without authorizing the browser
// to switch computation backends. Flip this only after the stabilization
// release gate certifies the current reference and performance target.
const NATIVE_GPU_EXACT_PRODUCTION_PROMOTED = false;

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
  NATIVE_GPU_EXACT_PRODUCTION_PROMOTED &&
  nativeGpuSupportsStochasticModel(message.stochastic?.model) &&
  message.engine === "deep-search" &&
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
  /(?:native host has exited|native messaging host|gpu companion (?:is )?disconnected|gpu companion message could not be sent|disconnected port)/iu.test(
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
  (message: unknown, _sender, sendResponse) => {
    if (
      message &&
      typeof message === "object" &&
      (message as Partial<DecisionCancelMessage>).type ===
        DECISION_CANCEL_MESSAGE_TYPE &&
      typeof (message as Partial<DecisionCancelMessage>).id === "number"
    ) {
      nativeGpu.cancelDecision((message as DecisionCancelMessage).id);
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
        if (
          status.engine === "deep-search" &&
          NATIVE_GPU_EXACT_PRODUCTION_PROMOTED
        ) {
          const gpu = await nativeGpu.status();
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
        if (status.engine === "weighted") nativeGpu.release();
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
    void (async () => {
      const backgroundStartedAt = performance.now();
      const nativeGpuEligible = shouldUseNativeGpu(message);
      if (nativeGpuEligible) {
        let gpu;
        try {
          gpu = await nativeGpu.status();
        } catch (error) {
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
              (request) => nativeGpu.analyzeExact(request, message.id),
            );
            return {
              ...analysis,
              runtime: "background-gpu" as const,
              runtimeReason: `Exact CUDA MaxN on ${gpu.device.name}`,
              ...(gpu.build ? { nativeGpuBuild: gpu.build } : {}),
            };
          } catch (error) {
            if (!isNativeGpuTransportFailure(error)) throw error;
            return analyzeAfterNativeGpuTransportFailure(
              message,
              error,
              backgroundStartedAt,
            );
          }
        }
      }
      if (message.engine === "weighted") nativeGpu.release();
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
            ? `Strategy policy ${message.strategyPolicy} remains on CPU/WASM until exact CUDA MaxN is production-promoted`
            : message.engine === "deep-search" && message.board.initialPlacement
              ? "Dedicated opening solver runs on WASM/CPU"
              : nativeGpuEligible
                ? "Native GPU unavailable; using WASM Deep MaxN"
                : message.engine === "deep-search"
                  ? requestedStochasticModel === MREF_COLONIST_LINKED_2024_V1
                    ? "Mref preserved on CPU/WASM Deep MaxN; exact CUDA MaxN remains a fixed-work parity backend and gpu-root-rollout remains experimental"
                    : "Weighted-belief Deep MaxN is the production authority; exact CUDA MaxN remains parity-gated and gpu-root-rollout remains experimental"
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
        const response: DecisionMessageResponse = {
          id: message.id,
          analysis,
          execution: "background",
        };
        sendResponse(response);
      })
      .catch((error: unknown) => {
        const response: DecisionMessageResponse = {
          id: message.id,
          error: errorDetail(error, "Decision analysis failed"),
        };
        sendResponse(response);
      });
    return true;
  },
);
