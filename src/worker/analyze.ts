import {
  analyzePublicEstimate,
  isDeepDecisionEngine,
  type DecisionAnalysis,
  type DecisionEngine,
  type EngineTuning,
  type FallbackEngine,
} from "../core/engine";
import type { BoardSnapshot } from "../core/placement";
import type { TrackerState } from "../core/types";
import { analyzeDeepSearch } from "./deep-search";

export interface DecisionRequest {
  state: TrackerState;
  board: BoardSnapshot;
  rootPlayer: string;
  engine: DecisionEngine;
  tuning?: EngineTuning;
  fallbackEngine?: FallbackEngine;
}

export const analyzeDecisionRequest = async (
  request: DecisionRequest,
): Promise<DecisionAnalysis> => {
  if (!isDeepDecisionEngine(request.engine)) {
    return {
      ...analyzePublicEstimate(request.state, request.board, request.rootPlayer),
      engine: request.engine,
      model: `${request.engine} public estimate (display-only policy)`,
    };
  }
  // This inexpensive public estimate stabilizes the presentation while the
  // Strategist's own root values arrive. It is never an action authority.
  const baseline = analyzePublicEstimate(
    request.state,
    request.board,
    request.rootPlayer,
  );
  try {
    return await analyzeDeepSearch(
      request.state,
      request.board,
      request.rootPlayer,
      baseline,
      request.engine === "deep-puct"
        ? "puct"
        : request.engine === "deep-alpha-beta"
          ? "alpha-beta"
          : "maxn",
      request.tuning,
    );
  } catch (error) {
    if (request.fallbackEngine === "weighted") {
      return {
        ...baseline,
        engine: request.engine,
        model: "Weighted public estimate fallback (advice only)",
        runtimeReason: `Selected engine failed; weighted estimate used: ${error instanceof Error ? error.message : "search error"}`,
      };
    }
    if (request.fallbackEngine === "alpha-beta-fast") {
      const quickTuning: EngineTuning = {
        maxDepth: 2,
        branchCap: 8,
        maxNodes: 4_000,
        beliefParticles: 8,
        strategicParticleLimit: 4,
        iterations: 128,
        rolloutActions: 48,
        maxThinkingTimeMs: 500,
      };
      const fallback = await analyzeDeepSearch(
        request.state,
        request.board,
        request.rootPlayer,
        baseline,
        "alpha-beta",
        quickTuning,
      );
      return {
        ...fallback,
        engine: request.engine,
        model: `Quick AlphaBeta fallback · ${fallback.model}`,
        runtimeReason: `Selected engine failed; quick AlphaBeta used: ${error instanceof Error ? error.message : "search error"}`,
      };
    }
    throw error;
  }
};
