import {
  analyzePublicEstimate,
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
  /** Engine-level seam for the future no-player-trades UI toggle. */
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
    request.playerTradesEnabled ?? true,
  );
};
