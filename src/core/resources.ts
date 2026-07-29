export const RESOURCE_ORDER = ["lumber", "brick", "wool", "grain", "ore"] as const;

export type Resource = (typeof RESOURCE_ORDER)[number];
export type ResourceVector = Record<Resource, number>;

export const RESOURCE_LABELS: Record<Resource, string> = {
  lumber: "Lumber",
  brick: "Brick",
  wool: "Wool",
  grain: "Grain",
  ore: "Ore",
};

export const RESOURCE_SHORT_LABELS: Record<Resource, string> = {
  lumber: "L",
  brick: "B",
  wool: "W",
  grain: "G",
  ore: "O",
};

export const RESOURCE_COLORS: Record<Resource, string> = {
  lumber: "#277957",
  brick: "#b34f35",
  wool: "#7bbf66",
  grain: "#d7a72e",
  ore: "#687588",
};

/**
 * Baseline four-player conversion value from a representative winning build
 * mix. These are a prior, not a rule: live shortages, ports, hand composition,
 * reachable sites, and the current win race adjust them downstream.
 */
export const RESOURCE_STRATEGIC_WEIGHTS: Record<Resource, number> = {
  lumber: 0.98,
  brick: 0.98,
  wool: 0.73,
  grain: 1.22,
  ore: 1.1,
};

export const emptyResources = (): ResourceVector => ({
  lumber: 0,
  brick: 0,
  wool: 0,
  grain: 0,
  ore: 0,
});

export const cloneResources = (resources: ResourceVector): ResourceVector => ({
  lumber: resources.lumber,
  brick: resources.brick,
  wool: resources.wool,
  grain: resources.grain,
  ore: resources.ore,
});

export const addResources = (
  target: ResourceVector,
  delta: Partial<ResourceVector>,
  multiplier = 1,
): void => {
  for (const resource of RESOURCE_ORDER) {
    target[resource] += (delta[resource] ?? 0) * multiplier;
  }
};

export const resourceTotal = (resources: Partial<ResourceVector>): number =>
  RESOURCE_ORDER.reduce((total, resource) => total + (resources[resource] ?? 0), 0);

export const hasResources = (
  hand: ResourceVector,
  cost: Partial<ResourceVector>,
): boolean => RESOURCE_ORDER.every((resource) => hand[resource] >= (cost[resource] ?? 0));

export const vectorFromResources = (resources: Resource[]): ResourceVector => {
  const result = emptyResources();
  for (const resource of resources) result[resource] += 1;
  return result;
};

export const BUILD_COSTS = {
  road: { lumber: 1, brick: 1 },
  settlement: { lumber: 1, brick: 1, wool: 1, grain: 1 },
  city: { grain: 2, ore: 3 },
  development: { wool: 1, grain: 1, ore: 1 },
} satisfies Record<string, Partial<ResourceVector>>;

export type BuildKind = keyof typeof BUILD_COSTS;
