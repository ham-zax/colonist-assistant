import {
  analyzeDecisionRequest,
} from "../worker/analyze";
import { warmDeepSearchEngine } from "../worker/deep-search";
import {
  NativeGpuClient,
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

const NATIVE_GPU_ROOT_ACTIONS = 12;
const NATIVE_GPU_GLOBAL_ROLLOUTS = 12 * 32;
const NATIVE_GPU_ROLLOUT_STEPS = 96;
const NATIVE_GPU_DECISION_TIME_MS = 4_000;

const withNativeGpuStrengthProfile = (request: unknown): unknown => {
  if (!request || typeof request !== "object") return request;
  const typed = request as {
    timeBudgetMs?: number;
    effort?: {
      decisionTimeMs: number;
      tactical: { maxDepth: number; nodeBudget: number };
      cpu: {
        maxDepth: number;
        rootCap: number;
        nodesPerDepthWave: number;
        evidenceEscalationMs?: number;
      };
      gpu: { rootCap: number; rolloutBudget: number; rolloutSteps: number };
    };
  };
  if (!typed.effort) return request;
  return {
    ...typed,
    timeBudgetMs: Math.max(
      NATIVE_GPU_DECISION_TIME_MS,
      typed.timeBudgetMs ?? 0,
    ),
    effort: {
      ...typed.effort,
      decisionTimeMs: Math.max(
        NATIVE_GPU_DECISION_TIME_MS,
        typed.effort.decisionTimeMs,
      ),
      gpu: {
        rootCap: Math.max(NATIVE_GPU_ROOT_ACTIONS, typed.effort.gpu.rootCap),
        rolloutBudget: Math.max(
          NATIVE_GPU_GLOBAL_ROLLOUTS,
          typed.effort.gpu.rolloutBudget,
        ),
        rolloutSteps: Math.max(
          NATIVE_GPU_ROLLOUT_STEPS,
          typed.effort.gpu.rolloutSteps,
        ),
      },
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

const analyzeAfterNativeGpuTransportFailure = async (
  message: DecisionMessage,
  error: unknown,
) => {
  const detail = errorDetail(error, "Native GPU transport failed");
  nativeGpu.release();
  const analysis = await analyzeDecisionRequest(message);
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
        if (status.engine === "deep-search") {
          const gpu = await nativeGpu.status();
          if (gpu) {
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
      const nativeGpuEligible = shouldUseNativeGpu(message);
      if (nativeGpuEligible) {
        let gpu;
        try {
          gpu = await nativeGpu.status();
        } catch (error) {
          if (!isNativeGpuTransportFailure(error)) throw error;
          return analyzeAfterNativeGpuTransportFailure(message, error);
        }
        if (gpu && nativeGpuSupportsStochasticModel(message.stochastic?.model, gpu.stochasticModels)) {
          try {
            const analysis = await analyzeDecisionRequest(
              message,
              (request) =>
                nativeGpu.analyze(withNativeGpuStrengthProfile(request), message.id),
            );
            return {
              ...analysis,
              runtime: "background-gpu" as const,
              runtimeReason: `CUDA resident search on ${gpu.device.name}`,
              ...(gpu.build ? { nativeGpuBuild: gpu.build } : {}),
            };
          } catch (error) {
            if (!isNativeGpuTransportFailure(error)) throw error;
            return analyzeAfterNativeGpuTransportFailure(message, error);
          }
        }
      }
      if (message.engine === "weighted") nativeGpu.release();
      const analysis = await analyzeDecisionRequest(message);
      const runtime = analysis.deepSearch
        ? ("background-wasm" as const)
        : ("background-rollout" as const);
      const requestedStochasticModel =
        message.stochastic?.model ?? M0_FAIR_IID_2D6_V1;
      const runtimeReason =
        analysis.runtimeReason ??
        (runtime === "background-wasm"
          ? requestedStochasticModel === MREF_COLONIST_LINKED_2024_V1
            ? "Mref preserved on CPU/WASM; no eligible Mref-capable native route for this decision"
            : message.engine === "deep-search" && message.board.initialPlacement
              ? "Dedicated opening solver runs on WASM/CPU"
              : nativeGpuEligible
                ? "Native GPU unavailable; using WASM Deep MaxN"
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
