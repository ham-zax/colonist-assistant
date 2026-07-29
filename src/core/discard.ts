import {
  BUILD_COSTS,
  RESOURCE_LABELS,
  RESOURCE_ORDER,
  cloneResources,
  emptyResources,
  hasResources,
  resourceTotal,
  type BuildKind,
  type Resource,
  type ResourceVector,
} from "./resources";
import type { PlayerBoardProfile } from "./strategy";

export interface DiscardContext {
  goal: BuildKind;
  profile: PlayerBoardProfile;
}

export interface DiscardRecommendation {
  discard: ResourceVector;
  keep: ResourceVector;
  count: number;
  score: number;
  reasons: string[];
}

const enumerateDiscards = (
  hand: ResourceVector,
  count: number,
  index = 0,
  current = emptyResources(),
  output: ResourceVector[] = [],
): ResourceVector[] => {
  if (index === RESOURCE_ORDER.length - 1) {
    const resource = RESOURCE_ORDER[index]!;
    if (count <= hand[resource]) {
      const complete = cloneResources(current);
      complete[resource] = count;
      output.push(complete);
    }
    return output;
  }
  const resource = RESOURCE_ORDER[index]!;
  for (let amount = 0; amount <= Math.min(hand[resource], count); amount += 1) {
    current[resource] = amount;
    enumerateDiscards(hand, count - amount, index + 1, current, output);
  }
  current[resource] = 0;
  return output;
};

const retainedBuildValue = (
  hand: ResourceVector,
  kind: BuildKind,
  goal: BuildKind,
): number => {
  const cost = BUILD_COSTS[kind] as Partial<ResourceVector>;
  const totalCost = resourceTotal(BUILD_COSTS[kind]);
  const covered = RESOURCE_ORDER.reduce(
    (sum, resource) => sum + Math.min(hand[resource], cost[resource] ?? 0),
    0,
  );
  const progress = covered / Math.max(1, totalCost);
  const complete = hasResources(hand, cost);
  const utility: Record<BuildKind, number> = {
    road: 7,
    settlement: 15,
    city: 18,
    development: 11,
  };
  return progress * utility[kind] + (complete ? utility[kind] * 0.8 : 0) +
    (kind === goal ? progress * 16 + (complete ? 22 : 0) : 0);
};

const replacementValue = (
  hand: ResourceVector,
  resource: Resource,
  profile: PlayerBoardProfile,
): number => {
  const pips = profile.activeProduction[resource];
  const scarcity = pips ? Math.min(12, 36 / pips) : 14;
  const ratio = Math.max(2, profile.tradeRatios[resource]);
  const portSet = Math.floor(hand[resource] / ratio);
  const withoutOne = Math.floor(Math.max(0, hand[resource] - 1) / ratio);
  const completesTradeSet = portSet > withoutOne ? 4.5 : 0;
  return scarcity + completesTradeSet;
};

const scoreKeep = (
  keep: ResourceVector,
  context: DiscardContext,
): number => {
  const buildValue = (Object.keys(BUILD_COSTS) as BuildKind[]).reduce(
    (sum, kind) => sum + retainedBuildValue(keep, kind, context.goal),
    0,
  );
  const optionValue = RESOURCE_ORDER.reduce(
    (sum, resource) =>
      sum + keep[resource] * replacementValue(keep, resource, context.profile),
    0,
  );
  const diversity = RESOURCE_ORDER.filter((resource) => keep[resource] > 0).length * 1.25;
  return buildValue + optionValue + diversity;
};

export const recommendDiscard = (
  hand: ResourceVector,
  count: number,
  context: DiscardContext,
): DiscardRecommendation | undefined => {
  if (!Number.isInteger(count) || count <= 0 || count > resourceTotal(hand)) {
    return undefined;
  }
  const ranked = enumerateDiscards(hand, count)
    .map((discard) => {
      const keep = cloneResources(hand);
      for (const resource of RESOURCE_ORDER) keep[resource] -= discard[resource];
      return { discard, keep, score: scoreKeep(keep, context) };
    })
    .sort((left, right) => right.score - left.score);
  const best = ranked[0];
  if (!best) return undefined;

  const discarded = RESOURCE_ORDER.filter((resource) => best.discard[resource] > 0);
  const goalCost = BUILD_COSTS[context.goal] as Partial<ResourceVector>;
  const preservedGoal = RESOURCE_ORDER.filter(
    (resource) => (goalCost[resource] ?? 0) > 0 && best.keep[resource] > 0,
  );
  const scarceKept = RESOURCE_ORDER
    .filter((resource) => best.keep[resource] > 0)
    .sort(
      (left, right) =>
        replacementValue(best.keep, right, context.profile) -
        replacementValue(best.keep, left, context.profile),
    )[0];
  const reasons = [
    `Keeps the strongest path toward your ${context.goal === "development" ? "development card" : context.goal}`,
  ];
  if (preservedGoal.length) {
    reasons.push(
      `Preserves ${preservedGoal
        .map((resource) => RESOURCE_LABELS[resource].toLowerCase())
        .join(" and ")} needed for that build`,
    );
  } else if (scarceKept) {
    reasons.push(
      `Protects slow-to-replace ${RESOURCE_LABELS[scarceKept].toLowerCase()}`,
    );
  }
  if (discarded.length === 1) {
    reasons.push(
      `Your safest surplus is ${RESOURCE_LABELS[discarded[0]!].toLowerCase()}`,
    );
  }
  return {
    ...best,
    count,
    score: Math.round(best.score * 10) / 10,
    reasons,
  };
};
