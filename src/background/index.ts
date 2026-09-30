import {
  analyzeDecisionRequest,
} from "../worker/analyze";
import { warmDeepSearchEngine } from "../worker/deep-search";
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

interface ActiveDecision {
  controller: AbortController;
}

const activeDecisions = new Map<string, ActiveDecision>();

const decisionKey = (sender: chrome.runtime.MessageSender, id: number): string =>
  JSON.stringify([sender.tab?.id, sender.documentId, sender.frameId, id]);

const cancelDecision = (decision: ActiveDecision): void => {
  decision.controller.abort(new Error("Decision cancelled as stale"));
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
      void (async () => {
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
      controller: new AbortController(),
    };
    activeDecisions.set(key, decision);
    const { signal } = decision.controller;
    const finish = () => {
      if (activeDecisions.get(key) === decision) activeDecisions.delete(key);
    };
    void (async () => {
      const backgroundStartedAt = performance.now();
      signal.throwIfAborted();
      const analysis = await analyzeDecisionRequest(
        withRemainingDecisionBudget(message, backgroundStartedAt),
      );
      const runtime = analysis.deepSearch
        ? ("background-wasm" as const)
        : ("background-rollout" as const);
      const runtimeReason =
        analysis.runtimeReason ??
        (runtime === "background-wasm"
          ? message.engine === "deep-search" && message.board.initialPlacement
            ? "Dedicated opening solver runs on WASM/CPU"
            : message.engine === "deep-search"
              ? "Deep MaxN runs on WASM"
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
