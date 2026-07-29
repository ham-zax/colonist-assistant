import type { DecisionAnalysis } from "../core/engine";
import type { DecisionRequest } from "./analyze";

export const DECISION_MESSAGE_TYPE = "colonist-assistant:decision";
export const DECISION_STATUS_MESSAGE_TYPE =
  "colonist-assistant:decision-status";

export interface DecisionMessage extends DecisionRequest {
  type: typeof DECISION_MESSAGE_TYPE;
  id: number;
}

export interface DecisionMessageResponse {
  id: number;
  analysis?: DecisionAnalysis;
  execution?: "background";
  error?: string;
}

export interface DecisionStatusMessage {
  type: typeof DECISION_STATUS_MESSAGE_TYPE;
  id: number;
}

export interface DecisionStatusMessageResponse {
  id: number;
  runtime?: "background-wasm";
  engineRevision?: string;
  initializationMs?: number;
  error?: string;
}
