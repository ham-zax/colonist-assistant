import {
  analyzePublicEstimate,
  type DecisionAnalysis,
  type DecisionEngine,
  type DecisionSearchConstraints,
} from "../core/engine";
import type { BoardSnapshot } from "../core/placement";
import type { TrackerState } from "../core/types";
import { analyzeDeepSearch } from "./deep-search";

export interface DecisionRequest {
  state: TrackerState;
  board: BoardSnapshot;
  rootPlayer: string;
  engine: DecisionEngine;
  searchConstraints?: DecisionSearchConstraints;
  /** Live rule seam: false forbids player negotiation while preserving maritime trades. */
  playerTradesEnabled?: boolean;
}

export const analyzeDecisionRequest = async (
  request: DecisionRequest,
): Promise<DecisionAnalysis> => {
  // This inexpensive public estimate stabilizes the presentation while the
  // Strategist's own root values arrive. It is never an action authority.
  const baseline = analyzePublicEstimate(
    request.state,
    request.board,
    request.rootPlayer,
  );
  return analyzeDeepSearch(
    request.state,
    request.board,
    request.rootPlayer,
    baseline,
    request.searchConstraints,
    request.playerTradesEnabled ?? true,
  );
};
