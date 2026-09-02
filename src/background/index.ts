import {
  analyzeDecisionRequest,
} from "../worker/analyze";
import { warmDeepSearchEngine } from "../worker/deep-search";
import { NativeGpuClient } from "./native-gpu";
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

const withNativeGpuStrengthProfile = (request: unknown): unknown => {
  if (!request || typeof request !== "object") return request;
  const typed = request as {
    effort?: {
      decisionTimeMs: number;
      tactical: { maxDepth: number; nodeBudget: number };
      cpu: { maxDepth: number; rootCap: number; nodesPerDepthWave: number };
      gpu: { rootCap: number; rolloutBudget: number; rolloutSteps: number };
    };
  };
  if (!typed.effort) return request;
  return {
    ...typed,
    effort: {
      ...typed.effort,
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

const shouldUseNativeGpu = (message: DecisionMessage): boolean =>
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
      if (shouldUseNativeGpu(message)) {
        const gpu = await nativeGpu.status();
        if (gpu) {
          const analysis = await analyzeDecisionRequest(
            message,
            (request) =>
              nativeGpu.analyze(withNativeGpuStrengthProfile(request), message.id),
          );
          return {
            ...analysis,
            runtime: "background-gpu" as const,
            runtimeReason: `CUDA resident search on ${gpu.device.name}`,
          };
        }
      }
      if (message.engine === "weighted") nativeGpu.release();
      const analysis = await analyzeDecisionRequest(message);
      return {
        ...analysis,
        runtime: analysis.deepSearch
          ? ("background-wasm" as const)
          : ("background-rollout" as const),
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
