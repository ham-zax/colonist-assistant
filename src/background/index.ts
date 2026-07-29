import {
  analyzeDecisionRequest,
} from "../worker/analyze";
import { warmDeepSearchEngine } from "../worker/deep-search";
import {
  DECISION_MESSAGE_TYPE,
  DECISION_STATUS_MESSAGE_TYPE,
  type DecisionMessage,
  type DecisionMessageResponse,
  type DecisionStatusMessage,
  type DecisionStatusMessageResponse,
} from "../worker/protocol";

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
      (message as Partial<DecisionStatusMessage>).type ===
        DECISION_STATUS_MESSAGE_TYPE &&
      typeof (message as Partial<DecisionStatusMessage>).id === "number"
    ) {
      const status = message as DecisionStatusMessage;
      void warmDeepSearchEngine()
        .then(({ engineRevision, initializationMs }) => {
          const response: DecisionStatusMessageResponse = {
            id: status.id,
            runtime: "background-wasm",
            engineRevision,
            initializationMs,
          };
          sendResponse(response);
        })
        .catch((error: unknown) => {
          const response: DecisionStatusMessageResponse = {
            id: status.id,
            error:
              error instanceof Error
                ? error.message
                : "WASM initialization failed",
          };
          sendResponse(response);
        });
      return true;
    }
    if (!isDecisionMessage(message)) return undefined;
    void analyzeDecisionRequest(message)
      .then((analysis) => {
        const response: DecisionMessageResponse = {
          id: message.id,
          analysis: {
            ...analysis,
            runtime: analysis.deepSearch
              ? "background-wasm"
              : "background-rollout",
          },
          execution: "background",
        };
        sendResponse(response);
      })
      .catch((error: unknown) => {
        const response: DecisionMessageResponse = {
          id: message.id,
          error:
            error instanceof Error
              ? error.message
              : "Decision analysis failed",
        };
        sendResponse(response);
      });
    return true;
  },
);
