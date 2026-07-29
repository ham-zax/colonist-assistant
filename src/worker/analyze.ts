import {
  analyzeDecision,
  isDeepDecisionEngine,
  type DecisionAnalysis,
  type DecisionEngine,
} from "../core/engine";
import type { BoardSnapshot } from "../core/placement";
import type { TrackerState } from "../core/types";
import { analyzeDeepSearch } from "./deep-search";

export interface DecisionRequest {
  state: TrackerState;
  board: BoardSnapshot;
  rootPlayer: string;
  engine: DecisionEngine;
}

export const analyzeDecisionRequest = async (
  request: DecisionRequest,
): Promise<DecisionAnalysis> => {
  const fallback = analyzeDecision(
    request.state,
    request.board,
    request.rootPlayer,
    isDeepDecisionEngine(request.engine) ? "hybrid" : request.engine,
  );
  if (!isDeepDecisionEngine(request.engine)) {
    return fallback;
  }
  const algorithm =
    request.engine === "deep-puct"
      ? "puct"
      : request.engine === "deep-alpha-beta"
        ? "alpha-beta"
        : "maxn";
  return analyzeDeepSearch(
    request.state,
    request.board,
    request.rootPlayer,
    fallback,
    algorithm,
  );
};
