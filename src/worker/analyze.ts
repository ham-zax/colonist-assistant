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
  const deep = isDeepDecisionEngine(request.engine);
  // Deep engines use this only for auxiliary ETA/win-race presentation.
  // Running the full JavaScript rollout policy here delayed WASM by roughly
  // two seconds and did not influence the selected deep-search action.
  const baseline = analyzeDecision(
    request.state,
    request.board,
    request.rootPlayer,
    deep ? "race-eta" : request.engine,
  );
  if (!deep) {
    return baseline;
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
    baseline,
    algorithm,
  );
};
