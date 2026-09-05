import type { DiceMode } from "./placement";

export interface PublicDiceRoll {
  actor: string;
  total: number;
  dice?: [number, number];
  eventId: string;
  logIndex?: number;
}

export interface PublicRollObservation {
  ordinal: number;
  actor: number;
  total: number;
}

export const M0_FAIR_IID_2D6_V1 = "m0-fair-iid-2d6-v1" as const;
export const MREF_COLONIST_LINKED_2024_V1 =
  "mref-colonist-linked-2024-v1" as const;
export const PUBLIC_HISTORY_BELIEF_V1 = "public-history-belief-v1" as const;
/** Ingestion/migration rules, not the stochastic model or session schema. */
export const DICE_HISTORY_INTEGRITY_VERSION = 1 as const;

export type StochasticModelId =
  | typeof M0_FAIR_IID_2D6_V1
  | typeof MREF_COLONIST_LINKED_2024_V1;
export type StochasticBeliefPolicyId = typeof PUBLIC_HISTORY_BELIEF_V1;

export type DiceHistoryProvenance =
  | "complete-from-first-gameplay-roll"
  | "gap-free-suffix"
  | "gapped"
  | "unknown";

export interface DiceHistoryGap {
  afterOrdinal: number;
  missingRolls?: number;
}

export interface DiceLogCoverage {
  ranges: Array<[number, number]>;
}

export interface DiceHistoryState {
  rolls: PublicDiceRoll[];
  provenance: DiceHistoryProvenance;
  coverage: DiceLogCoverage;
  /** Indexed log entries whose unresolved semantics could conceal contradictory gameplay-roll evidence. */
  ambiguousLogIndices: number[];
  /** Roll-capable uncertainty that cannot be assigned to exact log indexes. */
  hasUnlocatedRollAmbiguity: boolean;
  /** Independently established missing gameplay-roll count before rolls[0]. */
  missingPrefixRolls?: number;
  /** Missing gameplay-roll intervals. Omitted missingRolls means the gap length is unknown. */
  gaps: DiceHistoryGap[];
  hasUnknownRollGap: boolean;
}

export interface PublicStochasticInput {
  model: StochasticModelId;
  beliefPolicy?: StochasticBeliefPolicyId;
  playerMapping?: string[];
  rolls?: PublicRollObservation[];
  provenance?: DiceHistoryProvenance;
  missingPrefixRolls?: number;
  gaps?: DiceHistoryGap[];
  diceHistoryDigest?: string;
}

export interface StoredDiceHistoryState {
  /** Older writers cannot certify that all persisted uncertainty was reconciled. */
  integrityVersion?: typeof DICE_HISTORY_INTEGRITY_VERSION;
  rolls: PublicDiceRoll[];
  provenance: DiceHistoryProvenance;
  coverage: DiceLogCoverage;
  ambiguousLogIndices?: number[];
  hasUnlocatedRollAmbiguity?: boolean;
  missingPrefixRolls?: number;
  gaps: DiceHistoryGap[];
  hasUnknownRollGap: boolean;
}

const validIndex = (value: number): boolean =>
  Number.isInteger(value) && value >= 0;

const refreshProvenance = (state: DiceHistoryState): void => {
  const ranges = state.coverage.ranges;
  const coverageGap = ranges.length > 1;
  const explicitUnknownGap = state.gaps.some(
    (gap) => gap.missingRolls === undefined,
  );
  const parserAmbiguity =
    state.ambiguousLogIndices.length > 0 || state.hasUnlocatedRollAmbiguity;
  state.hasUnknownRollGap = coverageGap || explicitUnknownGap || parserAmbiguity;
  if (coverageGap || state.gaps.length > 0 || parserAmbiguity) {
    state.provenance = "gapped";
    return;
  }
  const first = ranges[0]?.[0];
  if (first === undefined) {
    state.provenance = "unknown";
  } else if ((state.missingPrefixRolls ?? 0) > 0 || first > 0) {
    state.provenance = "gap-free-suffix";
  } else {
    state.provenance = "complete-from-first-gameplay-roll";
  }
};

const normalizeCoverage = (ranges: Array<[number, number]>): Array<[number, number]> => {
  const sorted = ranges
    .filter(([start, end]) => validIndex(start) && validIndex(end) && start <= end)
    .map(([start, end]) => [start, end] as [number, number])
    .sort((left, right) => left[0] - right[0] || left[1] - right[1]);
  const merged: Array<[number, number]> = [];
  for (const [start, end] of sorted) {
    const previous = merged.at(-1);
    if (!previous || start > previous[1] + 1) {
      merged.push([start, end]);
      continue;
    }
    previous[1] = Math.max(previous[1], end);
  }
  return merged;
};

export const createDiceHistoryState = (): DiceHistoryState => ({
  rolls: [],
  provenance: "unknown",
  coverage: { ranges: [] },
  ambiguousLogIndices: [],
  hasUnlocatedRollAmbiguity: false,
  gaps: [],
  hasUnknownRollGap: false,
});

const markAmbiguousLogIndex = (
  state: DiceHistoryState,
  logIndex: number,
): void => {
  if (!validIndex(logIndex)) return;
  if (!state.ambiguousLogIndices.includes(logIndex)) {
    state.ambiguousLogIndices.push(logIndex);
    state.ambiguousLogIndices.sort((left, right) => left - right);
  }
  refreshProvenance(state);
};

export const observeLogCoverage = (
  state: DiceHistoryState,
  indices: readonly number[],
): void => {
  const observed = indices.filter(validIndex);
  if (observed.length) {
    const resolved = new Set(observed);
    const occupiedRollIndices = new Set(
      state.rolls.flatMap((roll) =>
        roll.logIndex === undefined ? [] : [roll.logIndex],
      ),
    );
    state.ambiguousLogIndices = state.ambiguousLogIndices.filter(
      (index) => !resolved.has(index) || occupiedRollIndices.has(index),
    );
  }
  const ranges = [
    ...state.coverage.ranges,
    ...observed.map((index) => [index, index] as [number, number]),
  ];
  state.coverage.ranges = normalizeCoverage(ranges);
  refreshProvenance(state);
};

export const noteRollCapableLogAmbiguity = (
  state: DiceHistoryState,
  logIndex: number | undefined,
): void => {
  if (logIndex === undefined || !validIndex(logIndex)) {
    state.hasUnlocatedRollAmbiguity = true;
    refreshProvenance(state);
    return;
  }
  if (state.rolls.some((roll) => roll.logIndex === logIndex)) return;
  markAmbiguousLogIndex(state, logIndex);
};

const validateRoll = (roll: PublicDiceRoll): void => {
  if (!Number.isInteger(roll.total) || roll.total < 2 || roll.total > 12) {
    throw new Error("Public dice roll total must be an integer in 2..12");
  }
  if (!roll.eventId.trim()) {
    throw new Error("Public dice roll requires a stable event id");
  }
  if (roll.logIndex !== undefined && !validIndex(roll.logIndex)) {
    throw new Error("Public dice roll log index must be a non-negative integer");
  }
  if (roll.dice) {
    const [left, right] = roll.dice;
    if (
      !Number.isInteger(left) ||
      !Number.isInteger(right) ||
      left < 1 ||
      left > 6 ||
      right < 1 ||
      right > 6 ||
      left + right !== roll.total
    ) {
      throw new Error("Public dice audit pair must be two dice in 1..6 matching the total");
    }
  }
};

const rollOrdinals = (state: DiceHistoryState): number[] => {
  let ordinal = state.missingPrefixRolls ?? 0;
  return state.rolls.map(() => {
    const current = ordinal;
    const missingAfter = state.gaps
      .filter((gap) => gap.afterOrdinal === current)
      .reduce((count, gap) => count + (gap.missingRolls ?? 0), 0);
    ordinal += 1 + missingAfter;
    return current;
  });
};

export const noteMissingPublicRoll = (state: DiceHistoryState): void => {
  if (!state.rolls.length) {
    state.missingPrefixRolls = (state.missingPrefixRolls ?? 0) + 1;
    refreshProvenance(state);
    return;
  }
  const afterOrdinal = rollOrdinals(state).at(-1)!;
  const existing = state.gaps.find((gap) => gap.afterOrdinal === afterOrdinal);
  if (existing) {
    existing.missingRolls = (existing.missingRolls ?? 0) + 1;
  } else {
    state.gaps.push({ afterOrdinal, missingRolls: 1 });
    state.gaps.sort((left, right) => left.afterOrdinal - right.afterOrdinal);
  }
  refreshProvenance(state);
};

export const appendPublicDiceRoll = (
  state: DiceHistoryState,
  roll: PublicDiceRoll,
): void => {
  validateRoll(roll);
  if (roll.logIndex !== undefined) {
    const indexed = state.rolls.find(
      (candidate) => candidate.logIndex === roll.logIndex,
    );
    if (indexed) {
      const diceConflict =
        indexed.dice !== undefined &&
        roll.dice !== undefined &&
        JSON.stringify(indexed.dice) !== JSON.stringify(roll.dice);
      if (
        indexed.actor !== roll.actor ||
        indexed.total !== roll.total ||
        diceConflict
      ) {
        markAmbiguousLogIndex(state, roll.logIndex);
        throw new Error(
          `Conflicting public dice evidence for log index ${roll.logIndex}`,
        );
      }
      if (!indexed.dice && roll.dice) {
        indexed.dice = [...roll.dice] as [number, number];
      }
      return;
    }
  }
  const existing = state.rolls.find((candidate) => candidate.eventId === roll.eventId);
  if (existing) {
    if (
      existing.actor !== roll.actor ||
      existing.total !== roll.total ||
      existing.logIndex !== roll.logIndex ||
      JSON.stringify(existing.dice) !== JSON.stringify(roll.dice)
    ) {
      throw new Error(`Conflicting public dice evidence for event ${roll.eventId}`);
    }
    return;
  }
  state.rolls.push({
    ...roll,
    ...(roll.dice ? { dice: [...roll.dice] as [number, number] } : {}),
  });
  state.rolls.sort((left, right) => {
    if (left.logIndex === undefined && right.logIndex === undefined) return 0;
    if (left.logIndex === undefined) return 1;
    if (right.logIndex === undefined) return -1;
    return left.logIndex - right.logIndex;
  });
};

export const cloneDiceHistoryState = (
  state: DiceHistoryState,
): DiceHistoryState => ({
  rolls: state.rolls.map((roll) => ({
    ...roll,
    ...(roll.dice ? { dice: [...roll.dice] as [number, number] } : {}),
  })),
  provenance: state.provenance,
  coverage: { ranges: state.coverage.ranges.map(([start, end]) => [start, end]) },
  ambiguousLogIndices: [...state.ambiguousLogIndices],
  hasUnlocatedRollAmbiguity: state.hasUnlocatedRollAmbiguity,
  ...(state.missingPrefixRolls !== undefined
    ? { missingPrefixRolls: state.missingPrefixRolls }
    : {}),
  gaps: state.gaps.map((gap) => ({ ...gap })),
  hasUnknownRollGap: state.hasUnknownRollGap,
});

export const serializeDiceHistoryState = (
  state: DiceHistoryState,
): StoredDiceHistoryState => ({
  ...cloneDiceHistoryState(state),
  integrityVersion: DICE_HISTORY_INTEGRITY_VERSION,
});

export const restoreDiceHistoryState = (
  stored: StoredDiceHistoryState,
): DiceHistoryState => {
  const restored = createDiceHistoryState();
  restored.coverage.ranges = normalizeCoverage(stored.coverage?.ranges ?? []);
  restored.ambiguousLogIndices = [
    ...new Set((stored.ambiguousLogIndices ?? []).filter(validIndex)),
  ].sort((left, right) => left - right);
  restored.hasUnlocatedRollAmbiguity = stored.hasUnlocatedRollAmbiguity === true;
  restored.missingPrefixRolls =
    stored.missingPrefixRolls !== undefined && validIndex(stored.missingPrefixRolls)
      ? stored.missingPrefixRolls
      : undefined;
  restored.gaps = (stored.gaps ?? [])
    .filter((gap) => validIndex(gap.afterOrdinal))
    .map((gap) => ({
      afterOrdinal: gap.afterOrdinal,
      ...(gap.missingRolls !== undefined && validIndex(gap.missingRolls)
        ? { missingRolls: gap.missingRolls }
        : {}),
    }));
  for (const roll of stored.rolls ?? []) appendPublicDiceRoll(restored, roll);
  refreshProvenance(restored);
  return restored;
};

const fnv1a64 = (value: string): string => {
  let hash = 0xcbf29ce484222325n;
  const prime = 0x100000001b3n;
  const mask = 0xffffffffffffffffn;
  const bytes = new TextEncoder().encode(value);
  for (const byte of bytes) {
    hash ^= BigInt(byte);
    hash = (hash * prime) & mask;
  }
  return hash.toString(16).padStart(16, "0");
};

export const publicRollObservations = (
  state: DiceHistoryState,
  playerMapping: readonly string[],
): PublicRollObservation[] => {
  const actors = new Map(playerMapping.map((player, index) => [player, index]));
  if (actors.size !== playerMapping.length) {
    throw new Error("Public stochastic player mapping contains duplicate players");
  }
  const ordinals = rollOrdinals(state);
  return state.rolls.map((roll, index) => {
    const actor = actors.get(roll.actor);
    if (actor === undefined) {
      throw new Error(`Public dice history references unmapped actor: ${roll.actor}`);
    }
    return { ordinal: ordinals[index]!, actor, total: roll.total };
  });
};

export const buildReferenceStochasticInput = (
  state: DiceHistoryState,
  playerMapping: readonly string[],
): PublicStochasticInput => {
  const rolls = publicRollObservations(state, playerMapping);
  const gaps = state.gaps.map((gap) => ({ ...gap }));
  if (
    state.provenance === "gapped" &&
    state.hasUnknownRollGap &&
    !gaps.some((gap) => gap.missingRolls === undefined)
  ) {
    gaps.push({
      afterOrdinal: Math.max(0, rolls.at(-1)?.ordinal ?? 0),
    });
  }
  return {
    model: MREF_COLONIST_LINKED_2024_V1,
    beliefPolicy: PUBLIC_HISTORY_BELIEF_V1,
    playerMapping: [...playerMapping],
    rolls,
    provenance: state.provenance,
    ...(state.missingPrefixRolls !== undefined
      ? { missingPrefixRolls: state.missingPrefixRolls }
      : {}),
    ...(gaps.length ? { gaps } : {}),
    diceHistoryDigest: diceHistoryDigest(state),
  };
};

const referenceHistoryAvailable = (state: DiceHistoryState): boolean => {
  if (state.provenance === "complete-from-first-gameplay-roll") return true;
  if (state.provenance === "gap-free-suffix") {
    return state.missingPrefixRolls !== undefined;
  }
  if (state.provenance === "gapped") {
    return (
      !state.hasUnknownRollGap &&
      state.gaps.every((gap) => gap.missingRolls !== undefined)
    );
  }
  return false;
};

export const buildLiveDecisionStochasticInput = (
  diceMode: DiceMode,
  state: DiceHistoryState | undefined,
  canonicalPlayerOrder: readonly string[] | undefined,
): PublicStochasticInput => {
  if (diceMode !== "balanced") return { model: M0_FAIR_IID_2D6_V1 };
  if (!state || !referenceHistoryAvailable(state)) {
    throw new Error("Balanced Dice requires usable public reference-dice history");
  }
  if (!canonicalPlayerOrder?.length) {
    throw new Error("Balanced Dice requires canonical engine player ordering");
  }
  return buildReferenceStochasticInput(state, canonicalPlayerOrder);
};

export const diceHistoryDigest = (state: DiceHistoryState): string =>
  fnv1a64(
    JSON.stringify({
      provenance: state.provenance,
      coverage: state.coverage.ranges,
      ...(state.ambiguousLogIndices.length
        ? { ambiguousLogIndices: state.ambiguousLogIndices }
        : {}),
      ...(state.hasUnlocatedRollAmbiguity
        ? { hasUnlocatedRollAmbiguity: true }
        : {}),
      hasUnknownRollGap: state.hasUnknownRollGap,
      missingPrefixRolls: state.missingPrefixRolls ?? null,
      gaps: state.gaps.map((gap) => [gap.afterOrdinal, gap.missingRolls ?? null]),
      rolls: state.rolls.map((roll, ordinal) => [ordinal, roll.actor, roll.total]),
    }),
  );
