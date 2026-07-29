import type { DecisionAnalysis } from "./engine";
import type { BoardSnapshot } from "./placement";

const normalizedProbabilities = (
  analysis: DecisionAnalysis,
): Map<string, number> => {
  const positive = analysis.players.map((estimate) => ({
    player: estimate.player,
    value: Math.max(0, estimate.probability),
  }));
  const total =
    positive.reduce((sum, estimate) => sum + estimate.value, 0) || 1;
  return new Map(
    positive.map((estimate) => [
      estimate.player,
      estimate.value / total,
    ]),
  );
};

const materialSignature = (
  board: BoardSnapshot | undefined,
  players: string[],
): string =>
  players
    .map((player) => {
      const publicState = board?.players?.[player];
      const settlements =
        board?.vertices.filter(
          (vertex) =>
            vertex.building?.player === player &&
            vertex.building.kind === "settlement",
        ).length ?? 0;
      const cities =
        board?.vertices.filter(
          (vertex) =>
            vertex.building?.player === player &&
            vertex.building.kind === "city",
        ).length ?? 0;
      return [
        player,
        publicState?.visiblePoints ?? 0,
        settlements,
        cities,
        publicState?.hasLongestRoad ? 1 : 0,
        publicState?.hasLargestArmy ? 1 : 0,
      ].join(":");
    })
    .join("|");

const evidenceWeight = (
  analysis: DecisionAnalysis,
  board: BoardSnapshot | undefined,
): number => {
  const target = Math.max(1, board?.victoryTarget ?? 10);
  const furthest = Math.max(
    0,
    ...analysis.players.map(
      (estimate) => board?.players?.[estimate.player]?.visiblePoints ?? 0,
    ),
  );
  const phase = Math.min(1, furthest / target);
  const confidence =
    analysis.players.reduce(
      (sum, estimate) =>
        sum +
        (estimate.confidence === "high"
          ? 1
          : estimate.confidence === "medium"
            ? 0.82
            : 0.64),
      0,
    ) / Math.max(1, analysis.players.length);
  return Math.min(0.86, (0.46 + phase * 0.34) * confidence);
};

/**
 * Converts volatile per-position search utilities into an honest display
 * estimate. The decision engine still receives the unsmoothed values; only
 * user-facing odds are regularized toward equal prior odds and rate-limited
 * between materially similar board states.
 */
export class WinPredictionStabilizer {
  private gameIdentity = "";
  private playersIdentity = "";
  private previousMaterial = "";
  private previousInput = "";
  private displayed = new Map<string, number>();

  reset(): void {
    this.gameIdentity = "";
    this.playersIdentity = "";
    this.previousMaterial = "";
    this.previousInput = "";
    this.displayed.clear();
  }

  update(
    analysis: DecisionAnalysis | undefined,
    board: BoardSnapshot | undefined,
  ): DecisionAnalysis | undefined {
    if (!analysis?.players.length) return analysis;
    const players = analysis.players.map((estimate) => estimate.player);
    const identity = [...players].sort().join("|");
    const game = board?.gameKey ?? identity;
    if (
      game !== this.gameIdentity ||
      identity !== this.playersIdentity
    ) {
      this.reset();
      this.gameIdentity = game;
      this.playersIdentity = identity;
    }

    const material = materialSignature(board, players);
    const raw = normalizedProbabilities(analysis);
    const input = [
      analysis.engine,
      analysis.model,
      material,
      ...players.map((player) => raw.get(player)?.toFixed(6) ?? "0"),
    ].join("|");
    if (input === this.previousInput && this.displayed.size) {
      return this.withDisplayedProbabilities(analysis);
    }

    const victoryTarget = Math.max(1, board?.victoryTarget ?? 10);
    const winner = players.find(
      (player) =>
        (board?.players?.[player]?.visiblePoints ?? 0) >= victoryTarget,
    );
    const equal = 1 / players.length;
    const weight = evidenceWeight(analysis, board);
    const target = new Map(
      players.map((player) => [
        player,
        winner
          ? player === winner
            ? 1
            : 0
          : equal * (1 - weight) + (raw.get(player) ?? equal) * weight,
      ]),
    );

    if (!this.displayed.size || winner) {
      this.displayed = target;
    } else {
      const materiallyChanged = material !== this.previousMaterial;
      const maximumStep = materiallyChanged ? 0.11 : 0.025;
      const preferredBlend = materiallyChanged ? 0.46 : 0.18;
      const largestDifference = Math.max(
        ...players.map((player) =>
          Math.abs(
            (target.get(player) ?? equal) -
              (this.displayed.get(player) ?? equal),
          ),
        ),
      );
      const blend =
        largestDifference > 0
          ? Math.min(
              preferredBlend,
              maximumStep / largestDifference,
            )
          : 0;
      this.displayed = new Map(
        players.map((player) => {
          const previous = this.displayed.get(player) ?? equal;
          const next = target.get(player) ?? equal;
          return [player, previous + (next - previous) * blend];
        }),
      );
    }

    this.previousMaterial = material;
    this.previousInput = input;
    return this.withDisplayedProbabilities(analysis);
  }

  private withDisplayedProbabilities(
    analysis: DecisionAnalysis,
  ): DecisionAnalysis {
    return {
      ...analysis,
      players: analysis.players.map((estimate) => ({
        ...estimate,
        probability:
          this.displayed.get(estimate.player) ?? estimate.probability,
        reasons: [
          "Displayed odds are regularized and smoothed across materially similar board states",
          ...estimate.reasons,
        ],
      })),
    };
  }
}
