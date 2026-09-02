import type {
  DecisionAnalysis,
  DecisionEngine,
  DecisionRuntime,
  NativeGpuBuildIdentity,
} from "../core/engine";
import type { DecisionRequest } from "./analyze";

export const DECISION_MESSAGE_TYPE = "colonist-assistant:decision";
export const DECISION_CANCEL_MESSAGE_TYPE = "colonist-assistant:decision-cancel";
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

export interface DecisionCancelMessage {
  type: typeof DECISION_CANCEL_MESSAGE_TYPE;
  id: number;
}

export interface DecisionStatusMessage {
  type: typeof DECISION_STATUS_MESSAGE_TYPE;
  id: number;
  engine: DecisionEngine;
}

export interface DecisionStatusMessageResponse {
  id: number;
  runtime?: Extract<DecisionRuntime, "background-gpu" | "background-wasm">;
  engineRevision?: string;
  nativeGpuBuild?: NativeGpuBuildIdentity;
  deviceName?: string;
  initializationMs?: number;
  error?: string;
}
