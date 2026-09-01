import {
  addResources,
  BUILD_COSTS,
  cloneResources,
  emptyResources,
  hasResources,
  RESOURCE_ORDER,
  resourceTotal,
  type BuildKind,
  type Resource,
  type ResourceVector,
} from "./resources";
import type {
  DevCardKind,
  HandWorld,
  PlayerMeta,
  ResourceEstimate,
  StoredEvent,
  TrackerEvent,
  TrackerState,
} from "./types";

export const MAX_WORLDS = 4096;
const PLAYER_COLORS = ["#53a6e8", "#ed665f", "#efb83f", "#7ac36a", "#9b7bd5", "#55c7bd"];

const emptyPlayedCards = (): Record<DevCardKind, number> => ({
  knight: 0,
  monopoly: 0,
  "road-building": 0,
  "year-of-plenty": 0,
  "victory-point": 0,
  unknown: 0,
});

const emptyBuilds = (): Record<BuildKind, number> => ({
  road: 0,
  settlement: 0,
  city: 0,
  development: 0,
});

const cloneWorld = (world: HandWorld): HandWorld => ({
  weight: world.weight,
  hands: Object.fromEntries(
    Object.entries(world.hands).map(([player, hand]) => [player, cloneResources(hand)]),
  ),
});

const ensureHand = (world: HandWorld, player: string): ResourceVector => {
  world.hands[player] ??= emptyResources();
  return world.hands[player];
};

export const createTrackerState = (): TrackerState => ({
  worlds: [{ hands: {}, weight: 1 }],
  players: {},
  playerOrder: [],
  eventCount: 0,
  currentTurn: { sequence: 0 },
  diceRolls: Object.fromEntries(Array.from({ length: 11 }, (_, index) => [index + 2, 0])),
  uncertaintyEvents: 0,
  possibilitiesTruncated: false,
  warnings: [],
  recentEvents: [],
  pendingTradeBehaviour: {},
});

const cloneState = (state: TrackerState): TrackerState => ({
  ...state,
  worlds: state.worlds.map(cloneWorld),
  players: Object.fromEntries(
    Object.entries(state.players).map(([name, player]) => [
      name,
      {
        ...player,
        devCards: player.devCards.map((card) => ({ ...card })),
        playedDevCards: { ...player.playedDevCards },
        builds: { ...player.builds },
        resourcesGained: cloneResources(player.resourcesGained),
        productionGained: cloneResources(player.productionGained),
        resourcesSpent: cloneResources(player.resourcesSpent),
        opponentModel: {
          ...player.opponentModel,
          policyPosterior: {
            ...player.opponentModel.policyPosterior,
          },
        },
      },
    ]),
  ),
  playerOrder: [...state.playerOrder],
  diceRolls: { ...state.diceRolls },
  warnings: [...state.warnings],
  recentEvents: [...state.recentEvents],
  currentTurn: { ...state.currentTurn },
  pendingTradeBehaviour: { ...(state.pendingTradeBehaviour ?? {}) },
});

const resourceBundleKey = (cards: ResourceVector): string =>
  RESOURCE_ORDER.map((resource) => `${resource}:${cards[resource] ?? 0}`).join(",");

const tradeBehaviourKey = (
  kind: "offer" | "accept",
  state: TrackerState,
  creator: string,
  give: ResourceVector,
  receive: ResourceVector,
  acceptingPlayer?: string,
): string => [
  kind,
  state.currentTurn.sequence,
  creator,
  acceptingPlayer ?? "-",
  resourceBundleKey(give),
  resourceBundleKey(receive),
].join(":");

const addPendingTradeBehaviour = (state: TrackerState, key: string): void => {
  state.pendingTradeBehaviour[key] = (state.pendingTradeBehaviour[key] ?? 0) + 1;
};

const consumePendingTradeBehaviour = (state: TrackerState, key: string): boolean => {
  const count = state.pendingTradeBehaviour[key] ?? 0;
  if (count <= 0) return false;
  if (count === 1) delete state.pendingTradeBehaviour[key];
  else state.pendingTradeBehaviour[key] = count - 1;
  return true;
};

const ensurePlayer = (
  state: TrackerState,
  name: string,
  preferredColor?: string,
): PlayerMeta => {
  if (!state.players[name]) {
    const meta: PlayerMeta = {
      name,
      color: preferredColor || PLAYER_COLORS[state.playerOrder.length % PLAYER_COLORS.length]!,
      devCards: [],
      playedDevCards: emptyPlayedCards(),
      builds: emptyBuilds(),
      resourcesGained: emptyResources(),
      productionGained: emptyResources(),
      resourcesSpent: emptyResources(),
      opponentModel: {
        tradeAccepts: 1,
        tradeRejects: 1,
        offersMade: 0,
        countersMade: 0,
        policyPosterior: {
          balanced: 0.2,
          expansion: 0.2,
          cityDevelopment: 0.2,
          tradeFlexible: 0.2,
          tradeResistant: 0.2,
        },
      },
    };
    state.players[name] = meta;
    state.playerOrder.push(name);
    for (const world of state.worlds) ensureHand(world, name);
  } else if (preferredColor) {
    state.players[name].color = preferredColor;
  }
  return state.players[name];
};

const worldKey = (world: HandWorld): string =>
  Object.keys(world.hands)
    .sort()
    .map(
      (player) =>
        `${player}:${RESOURCE_ORDER.map((resource) => world.hands[player]?.[resource] ?? 0).join(",")}`,
    )
    .join("|");

const normalizeWorldWeights = (worlds: HandWorld[]): HandWorld[] => {
  const total = worlds.reduce(
    (sum, world) => sum + Math.max(0, world.weight),
    0,
  );
  if (!worlds.length) return worlds;
  if (!(total > 0)) {
    const weight = 1 / worlds.length;
    return worlds.map((world) => ({ ...world, weight }));
  }
  return worlds.map((world) => ({
    ...world,
    weight: Math.max(0, world.weight) / total,
  }));
};

export const effectiveParticleCount = (state: TrackerState): number => {
  const normalized = normalizeWorldWeights(state.worlds);
  const squared = normalized.reduce(
    (sum, world) => sum + world.weight * world.weight,
    0,
  );
  return squared > 0 ? 1 / squared : 0;
};

const compactWorlds = (state: TrackerState, candidates: HandWorld[]): void => {
  const merged = new Map<string, HandWorld>();
  for (const candidate of candidates) {
    const key = worldKey(candidate);
    const existing = merged.get(key);
    if (existing) {
      existing.weight += candidate.weight;
    } else {
      merged.set(key, cloneWorld(candidate));
    }
  }
  const unique = normalizeWorldWeights([...merged.values()]);
  if (unique.length <= MAX_WORLDS) {
    state.worlds = unique;
    return;
  }

  state.possibilitiesTruncated = true;
  const selected = new Map<number, number>();
  const sorted = unique
    .map((world, index) => ({ world, index }))
    .sort(
      (left, right) =>
        worldKey(left.world).localeCompare(worldKey(right.world)),
    );
  let cursor = 0;
  let cumulative = sorted[0]?.world.weight ?? 1;
  for (let stratum = 0; stratum < MAX_WORLDS; stratum += 1) {
    const target = (stratum + 0.5) / MAX_WORLDS;
    while (
      cursor < sorted.length - 1 &&
      cumulative < target
    ) {
      cursor += 1;
      cumulative += sorted[cursor]?.world.weight ?? 0;
    }
    const index = sorted[cursor]?.index;
    if (index !== undefined) {
      selected.set(index, (selected.get(index) ?? 0) + 1 / MAX_WORLDS);
    }
  }
  state.worlds = normalizeWorldWeights(
    [...selected.entries()]
      .map(([index, weight]) => {
        const world = unique[index];
        return world ? { ...cloneWorld(world), weight } : undefined;
      })
      .filter((world): world is HandWorld => Boolean(world)),
  );
};

const addWarning = (state: TrackerState, warning: string): void => {
  if (!state.warnings.includes(warning)) state.warnings.push(warning);
};

const applySpend = (
  state: TrackerState,
  player: string,
  cost: Partial<ResourceVector>,
): void => {
  const valid = state.worlds.filter((world) => hasResources(ensureHand(world, player), cost));
  if (valid.length) {
    for (const world of valid) addResources(ensureHand(world, player), cost, -1);
    state.worlds = normalizeWorldWeights(valid);
    return;
  }

  addWarning(state, "Some earlier game history was unavailable; lower bounds were repaired.");
  for (const world of state.worlds) {
    const hand = ensureHand(world, player);
    for (const resource of RESOURCE_ORDER) {
      hand[resource] = Math.max(0, hand[resource] - (cost[resource] ?? 0));
    }
  }
};

const transfer = (
  state: TrackerState,
  from: string,
  to: string,
  cards: Partial<ResourceVector>,
): void => {
  const valid = state.worlds.filter((world) => hasResources(ensureHand(world, from), cards));
  if (!valid.length) {
    addWarning(state, "A transfer referenced cards from before tracking began.");
    for (const world of state.worlds) addResources(ensureHand(world, to), cards);
    return;
  }
  for (const world of valid) {
    addResources(ensureHand(world, from), cards, -1);
    addResources(ensureHand(world, to), cards);
  }
  state.worlds = normalizeWorldWeights(valid);
};

const applyUnknownTransferOnce = (
  state: TrackerState,
  from: string,
  to: string,
): void => {
  const candidates: HandWorld[] = [];
  for (const world of state.worlds) {
    const fromHand = ensureHand(world, from);
    const totalCards = resourceTotal(fromHand);
    if (!totalCards) continue;
    for (const resource of RESOURCE_ORDER) {
      if (fromHand[resource] <= 0) continue;
      const branch = cloneWorld(world);
      branch.weight *= fromHand[resource] / totalCards;
      ensureHand(branch, from)[resource] -= 1;
      ensureHand(branch, to)[resource] += 1;
      candidates.push(branch);
    }
  }
  if (!candidates.length) {
    addWarning(state, "An unknown robbery could not be resolved from the available history.");
    return;
  }
  compactWorlds(state, candidates);
};

const boundedDiscards = (
  hand: ResourceVector,
  count: number,
  resourceIndex = 0,
  current = emptyResources(),
  output: ResourceVector[] = [],
): ResourceVector[] => {
  if (resourceIndex === RESOURCE_ORDER.length - 1) {
    const resource = RESOURCE_ORDER[resourceIndex]!;
    if (count <= hand[resource]) {
      const complete = cloneResources(current);
      complete[resource] = count;
      output.push(complete);
    }
    return output;
  }
  const resource = RESOURCE_ORDER[resourceIndex]!;
  for (let amount = 0; amount <= Math.min(hand[resource], count); amount += 1) {
    current[resource] = amount;
    boundedDiscards(hand, count - amount, resourceIndex + 1, current, output);
  }
  current[resource] = 0;
  return output;
};

const applyUnknownDiscard = (state: TrackerState, player: string, count: number): void => {
  const candidates: HandWorld[] = [];
  for (const world of state.worlds) {
    const hand = ensureHand(world, player);
    const options = boundedDiscards(hand, count);
    const likelihoods = options.map((discarded) => {
      const kept = cloneResources(hand);
      addResources(kept, discarded, -1);
      const retained = RESOURCE_ORDER.reduce(
        (sum, resource, index) =>
          sum +
          kept[resource] * [0.98, 0.98, 0.73, 1.22, 1.1][index]!,
        0,
      );
      const settlementReady = hasResources(kept, BUILD_COSTS.settlement)
        ? 1.7
        : 0;
      const cityReady = hasResources(kept, BUILD_COSTS.city) ? 1.5 : 0;
      const roadReady = hasResources(kept, BUILD_COSTS.road) ? 0.35 : 0;
      return Math.exp((retained * 0.18 + settlementReady + cityReady + roadReady) * 0.7);
    });
    const likelihoodTotal = likelihoods.reduce((sum, value) => sum + value, 0);
    options.forEach((discarded, index) => {
      const branch = cloneWorld(world);
      branch.weight *=
        (likelihoods[index] ?? 0) / Math.max(Number.EPSILON, likelihoodTotal);
      addResources(ensureHand(branch, player), discarded, -1);
      candidates.push(branch);
    });
  }
  if (!candidates.length) {
    addWarning(state, "A hidden discard referenced cards from before tracking began.");
    return;
  }
  compactWorlds(state, candidates);
};

const markResources = (
  meta: PlayerMeta,
  vector: Partial<ResourceVector>,
  direction: "gained" | "spent",
): void => {
  addResources(direction === "gained" ? meta.resourcesGained : meta.resourcesSpent, vector);
};

type PolicyEvidence = Partial<
  PlayerMeta["opponentModel"]["policyPosterior"]
>;

const updatePolicyPosterior = (
  meta: PlayerMeta,
  evidence: PolicyEvidence,
): void => {
  const posterior = meta.opponentModel.policyPosterior;
  for (const key of Object.keys(posterior) as Array<keyof typeof posterior>) {
    posterior[key] *= Math.max(0.01, evidence[key] ?? 1);
  }
  const total = Object.values(posterior).reduce(
    (sum, probability) => sum + probability,
    0,
  );
  for (const key of Object.keys(posterior) as Array<keyof typeof posterior>) {
    posterior[key] /= Math.max(Number.EPSILON, total);
  }
};

export const reduceTracker = (
  previous: TrackerState,
  event: TrackerEvent,
  storedEvent?: StoredEvent,
): TrackerState => {
  if (event.type === "roll" && event.player.trim().toLowerCase() === "dice") {
    const matchingPlayers = event.color
      ? previous.playerOrder.filter(
          (player) => previous.players[player]?.color === event.color,
        )
      : [];
    if (matchingPlayers.length !== 1) return previous;
    event = { ...event, player: matchingPlayers[0]! };
  }

  const state = cloneState(previous);
  state.eventCount += 1;

  const names =
    event.type === "transfer" || event.type === "unknown-transfer"
      ? [event.from, event.to]
      : event.type === "trade" && event.acceptingPlayer
        ? [event.player, event.acceptingPlayer]
        : event.type === "trade-accepted" ||
            event.type === "trade-rejected" ||
            event.type === "trade-countered"
          ? [event.player, event.creator]
          : event.type === "trade-offered"
            ? [event.player, ...event.recipients]
            : "player" in event
              ? [event.player]
              : [];
  for (const name of names) ensurePlayer(state, name);
  const actor =
    event.type === "transfer" || event.type === "unknown-transfer"
      ? event.to
      : "player" in event
        ? event.player
        : undefined;
  if (actor) ensurePlayer(state, actor, event.color);

  switch (event.type) {
    case "discover":
      break;
    case "gain": {
      for (const world of state.worlds) addResources(ensureHand(world, event.player), event.cards);
      markResources(state.players[event.player]!, event.cards, "gained");
      if (event.reason === "production" || event.reason === "starting") {
        addResources(state.players[event.player]!.productionGained, event.cards);
      }
      break;
    }
    case "spend": {
      applySpend(state, event.player, event.cost);
      markResources(state.players[event.player]!, event.cost, "spent");
      state.players[event.player]!.builds[event.reason] += 1;
      updatePolicyPosterior(
        state.players[event.player]!,
        event.reason === "road" || event.reason === "settlement"
          ? { expansion: 1.32, cityDevelopment: 0.88 }
          : event.reason === "city" || event.reason === "development"
            ? { cityDevelopment: 1.32, expansion: 0.9 }
            : {},
      );
      break;
    }
    case "transfer": {
      transfer(state, event.from, event.to, event.cards);
      markResources(state.players[event.from]!, event.cards, "spent");
      markResources(state.players[event.to]!, event.cards, "gained");
      break;
    }
    case "trade-offered": {
      const key = tradeBehaviourKey("offer", state, event.player, event.give, event.receive);
      state.players[event.player]!.opponentModel.offersMade += 1;
      updatePolicyPosterior(state.players[event.player]!, {
        tradeFlexible: 1.18,
        tradeResistant: 0.94,
      });
      addPendingTradeBehaviour(state, key);
      const offered = reweightTradeEvidence(state, [{
        id: `offer:${event.player}:${state.eventCount}`,
        creator: event.player,
        give: event.give,
        receive: event.receive,
      }]);
      state.worlds = offered.worlds;
      break;
    }
    case "trade-accepted": {
      const key = tradeBehaviourKey(
        "accept", state, event.creator, event.give, event.receive, event.player,
      );
      state.players[event.player]!.opponentModel.tradeAccepts += 1;
      updatePolicyPosterior(state.players[event.player]!, {
        tradeFlexible: 1.35,
        tradeResistant: 0.82,
      });
      addPendingTradeBehaviour(state, key);
      const accepted = reweightTradeEvidence(state, [{
        id: `accept:${event.player}:${state.eventCount}`,
        creator: event.creator,
        give: event.give,
        receive: event.receive,
        acceptedPlayers: [event.player],
      }]);
      state.worlds = accepted.worlds;
      break;
    }
    case "trade-rejected": {
      const rejected = reweightTradeEvidence(state, [
        {
          id: `reject:${event.player}:${state.eventCount}`,
          creator: event.creator,
          give: event.give,
          receive: event.receive,
          rejectedPlayers: [event.player],
        },
      ]);
      state.worlds = rejected.worlds;
      state.players = rejected.players;
      break;
    }
    case "trade-countered": {
      const countered = reweightTradeEvidence(state, [
        {
          id: `counter:${event.player}:${state.eventCount}`,
          creator: event.creator,
          give: event.give,
          receive: event.receive,
          counteringPlayers: [event.player],
        },
      ]);
      state.worlds = countered.worlds;
      state.players = countered.players;
      break;
    }
    case "trade-expired": {
      const key = tradeBehaviourKey("offer", state, event.player, event.give, event.receive);
      consumePendingTradeBehaviour(state, key);
      updatePolicyPosterior(state.players[event.player]!, { tradeFlexible: 0.97 });
      break;
    }
    case "trade": {
      if (event.bank || !event.acceptingPlayer) {
        applySpend(state, event.player, event.given);
        for (const world of state.worlds) {
          addResources(ensureHand(world, event.player), event.received);
        }
      } else {
        const counterparty = event.acceptingPlayer;
        const valid = state.worlds.filter(
          (world) =>
            hasResources(ensureHand(world, event.player), event.given) &&
            hasResources(ensureHand(world, counterparty), event.received),
        );
        if (valid.length) {
          for (const world of valid) {
            addResources(ensureHand(world, event.player), event.given, -1);
            addResources(ensureHand(world, counterparty), event.given);
            addResources(ensureHand(world, counterparty), event.received, -1);
            addResources(ensureHand(world, event.player), event.received);
          }
          state.worlds = normalizeWorldWeights(valid);
        } else {
          addWarning(state, "A trade referenced cards from before tracking began.");
          for (const world of state.worlds) {
            addResources(ensureHand(world, event.player), event.received);
            addResources(ensureHand(world, counterparty), event.given);
          }
        }
      }
      markResources(state.players[event.player]!, event.given, "spent");
      markResources(state.players[event.player]!, event.received, "gained");
      const offerKey = tradeBehaviourKey(
        "offer", state, event.player, event.given, event.received,
      );
      if (!consumePendingTradeBehaviour(state, offerKey)) {
        state.players[event.player]!.opponentModel.offersMade += 1;
        updatePolicyPosterior(state.players[event.player]!, {
          tradeFlexible: 1.28,
          tradeResistant: 0.9,
        });
      }
      if (event.acceptingPlayer) {
        const acceptKey = tradeBehaviourKey(
          "accept", state, event.player, event.given, event.received, event.acceptingPlayer,
        );
        if (!consumePendingTradeBehaviour(state, acceptKey)) {
          state.players[event.acceptingPlayer]!.opponentModel.tradeAccepts += 1;
          updatePolicyPosterior(state.players[event.acceptingPlayer]!, {
            tradeFlexible: 1.35,
            tradeResistant: 0.82,
          });
        }
      }
      break;
    }
    case "unknown-transfer":
      state.uncertaintyEvents += 1;
      for (let index = 0; index < event.count; index += 1) {
        applyUnknownTransferOnce(state, event.from, event.to);
      }
      break;
    case "unknown-discard":
      state.uncertaintyEvents += 1;
      applyUnknownDiscard(state, event.player, event.count);
      break;
    case "monopoly":
      for (const world of state.worlds) {
        const receiver = ensureHand(world, event.player);
        for (const player of state.playerOrder) {
          if (player === event.player) continue;
          const hand = ensureHand(world, player);
          receiver[event.resource] += hand[event.resource];
          hand[event.resource] = 0;
        }
      }
      break;
    case "buy-dev": {
      applySpend(state, event.player, BUILD_COSTS.development);
      markResources(state.players[event.player]!, BUILD_COSTS.development, "spent");
      state.players[event.player]!.builds.development += 1;
      updatePolicyPosterior(state.players[event.player]!, {
        cityDevelopment: 1.32,
        expansion: 0.9,
      });
      state.players[event.player]!.devCards.push({
        boughtOnTurn: state.currentTurn.sequence,
      });
      break;
    }
    case "play-dev": {
      const meta = state.players[event.player]!;
      const readyIndex = meta.devCards.findIndex(
        (card) => card.boughtOnTurn < state.currentTurn.sequence,
      );
      if (readyIndex >= 0) meta.devCards.splice(readyIndex, 1);
      else if (meta.devCards.length) meta.devCards.shift();
      meta.playedDevCards[event.card] += 1;
      break;
    }
    case "roll": {
      state.currentTurn = {
        player: event.player,
        sequence: state.currentTurn.sequence + 1,
      };
      if (event.dice) {
        const total = event.dice[0] + event.dice[1];
        state.diceRolls[total] = (state.diceRolls[total] ?? 0) + 1;
      }
      break;
    }
  }

  if (storedEvent) state.recentEvents = [...state.recentEvents, storedEvent].slice(-50);
  return state;
};

export const replayEvents = (events: StoredEvent[]): TrackerState =>
  events.reduce(
    (state, stored) => reduceTracker(state, stored, stored),
    createTrackerState(),
  );

export const getPlayerEstimate = (
  state: TrackerState,
  player: string,
): ResourceEstimate => {
  const worlds = state.worlds.length
    ? normalizeWorldWeights(state.worlds)
    : [{ hands: { [player]: emptyResources() }, weight: 1 }];
  const minimum = emptyResources();
  const maximum = emptyResources();
  const average = emptyResources();
  for (const resource of RESOURCE_ORDER) {
    const values = worlds.map((world) => world.hands[player]?.[resource] ?? 0);
    minimum[resource] = Math.min(...values);
    maximum[resource] = Math.max(...values);
    average[resource] = worlds.reduce(
      (sum, world) =>
        sum + (world.hands[player]?.[resource] ?? 0) * world.weight,
      0,
    );
  }
  return {
    minimum,
    maximum,
    average,
    totalMinimum: Math.min(
      ...worlds.map((world) => resourceTotal(world.hands[player] ?? emptyResources())),
    ),
    totalMaximum: Math.max(
      ...worlds.map((world) => resourceTotal(world.hands[player] ?? emptyResources())),
    ),
    possibilities: worlds.length,
    approximate: state.possibilitiesTruncated,
  };
};

export const affordability = (
  state: TrackerState,
  player: string,
  build: BuildKind,
): number => {
  if (!state.worlds.length) return 0;
  return normalizeWorldWeights(state.worlds)
    .filter((world) =>
      hasResources(world.hands[player] ?? emptyResources(), BUILD_COSTS[build]),
    )
    .reduce((sum, world) => sum + world.weight, 0);
};

export interface PublicResourceSeedInput {
  playerOrder: string[];
  ownPlayer: string;
  exactOwnHand: ResourceVector;
  handSizes: Record<string, number>;
  bank?: ResourceVector;
  resourceSupply: number;
  seed: number;
  sampleCount?: number;
}

const deterministicUnitOffset = (seed: number, drawIndex: number): number => {
  let value = (seed ^ Math.imul(drawIndex + 1, 0x9e3779b1)) >>> 0;
  value ^= value >>> 16;
  value = Math.imul(value, 0x7feb352d) >>> 0;
  value ^= value >>> 15;
  value = Math.imul(value, 0x846ca68b) >>> 0;
  value ^= value >>> 16;
  return (value >>> 0) / 0x1_0000_0000;
};

const drawResourceWithoutReplacement = (
  remaining: ResourceVector,
  unit: number,
): Resource => {
  const total = resourceTotal(remaining);
  if (total <= 0) {
    throw new Error("Public resource snapshot exceeds physical resource supply");
  }
  const target = Math.max(0, Math.min(1 - Number.EPSILON, unit)) * total;
  let cumulative = 0;
  for (const resource of RESOURCE_ORDER) {
    cumulative += remaining[resource];
    if (target < cumulative) return resource;
  }
  return RESOURCE_ORDER[RESOURCE_ORDER.length - 1]!;
};

/**
 * Seeds fallback hidden-resource worlds from the physical-card conditional used
 * when the assistant attaches after public log history has already been lost.
 * Opponent hand slots are sampled uniformly without replacement. If the bank is
 * public it is conditioned on exactly; otherwise the unsampled residual cards
 * are the hidden bank.
 */
export const seedPublicResourceWorlds = (
  input: PublicResourceSeedInput,
): HandWorld[] => {
  const sampleCount = Math.min(
    MAX_WORLDS,
    Math.max(1, Math.floor(input.sampleCount ?? 24)),
  );
  const resourceSupply = Math.floor(input.resourceSupply);
  if (!(resourceSupply > 0)) {
    throw new Error("Public resource snapshot has an invalid physical resource supply");
  }
  if (!input.playerOrder.includes(input.ownPlayer)) {
    throw new Error("Public resource snapshot is missing the local player");
  }
  const orderedPlayers = input.playerOrder.filter(
    (player, index, all) => all.indexOf(player) === index,
  );
  if (orderedPlayers.length !== input.playerOrder.length) {
    throw new Error("Public resource snapshot has duplicate player slots");
  }
  for (const player of orderedPlayers) {
    const size = input.handSizes[player];
    if (!Number.isInteger(size) || size < 0) {
      throw new Error(`Public resource snapshot has an invalid hand size for ${player}`);
    }
  }
  const ownSize = input.handSizes[input.ownPlayer];
  if (resourceTotal(input.exactOwnHand) !== ownSize) {
    throw new Error("Public resource snapshot conflicts with the exact local hand size");
  }

  const initialRemaining = emptyResources();
  for (const resource of RESOURCE_ORDER) {
    const own = input.exactOwnHand[resource];
    const bank = input.bank?.[resource] ?? 0;
    if (
      !Number.isInteger(own) ||
      own < 0 ||
      !Number.isInteger(bank) ||
      bank < 0 ||
      own + bank > resourceSupply
    ) {
      throw new Error("Public resource snapshot exceeds physical resource supply");
    }
    initialRemaining[resource] = resourceSupply - own - bank;
  }

  const opponentSlots = orderedPlayers
    .filter((player) => player !== input.ownPlayer)
    .reduce((sum, player) => sum + input.handSizes[player]!, 0);
  const remainingCards = resourceTotal(initialRemaining);
  if (
    opponentSlots > remainingCards ||
    (input.bank && opponentSlots !== remainingCards)
  ) {
    throw new Error("Public resource snapshot exceeds physical resource supply");
  }

  const merged = new Map<string, HandWorld>();
  for (let sampleIndex = 0; sampleIndex < sampleCount; sampleIndex += 1) {
    const remaining = cloneResources(initialRemaining);
    const world: HandWorld = {
      weight: 1 / sampleCount,
      hands: Object.fromEntries(
        orderedPlayers.map((player) => [
          player,
          player === input.ownPlayer
            ? cloneResources(input.exactOwnHand)
            : emptyResources(),
        ]),
      ),
    };
    let drawIndex = 0;
    for (const player of orderedPlayers) {
      if (player === input.ownPlayer) continue;
      const hand = world.hands[player]!;
      for (let slot = 0; slot < input.handSizes[player]!; slot += 1) {
        const offset = deterministicUnitOffset(input.seed, drawIndex);
        const unit = (sampleIndex + offset) / sampleCount;
        const resource = drawResourceWithoutReplacement(remaining, unit);
        hand[resource] += 1;
        remaining[resource] -= 1;
        drawIndex += 1;
      }
    }
    const key = worldKey(world);
    const existing = merged.get(key);
    if (existing) existing.weight += world.weight;
    else merged.set(key, world);
  }
  return normalizeWorldWeights([...merged.values()]);
};

export interface PublicResourceEvidence {
  exactHands?: Record<string, ResourceVector>;
  handSizes?: Record<string, number>;
  bank?: ResourceVector;
  resourceSupply?: number;
}

export interface TradeBeliefEvidence {
  id: string;
  creator?: string;
  give: ResourceVector;
  receive: ResourceVector;
  acceptedPlayers?: string[];
  rejectedPlayers?: string[];
  counteringPlayers?: string[];
}

/**
 * Treats structured offer responses as Bayesian evidence. An acceptance is
 * hard evidence that the accepting player held the requested bundle; a
 * rejection remains soft evidence because strategic rejection is possible.
 */
export const reweightTradeEvidence = (
  state: TrackerState,
  evidence: TradeBeliefEvidence[],
): TrackerState => {
  if (!state.worlds.length || !evidence.length) return state;
  const updated = cloneState(state);
  const worlds = updated.worlds;
  for (const trade of evidence) {
    for (const world of worlds) {
      let likelihood = 1;
      for (const player of trade.acceptedPlayers ?? []) {
        likelihood *= hasResources(
          world.hands[player] ?? emptyResources(),
          trade.receive,
        )
          ? 0.98
          : 0.001;
      }
      for (const player of trade.rejectedPlayers ?? []) {
        likelihood *= hasResources(
          world.hands[player] ?? emptyResources(),
          trade.receive,
        )
          ? 0.38
          : 0.82;
      }
      for (const player of trade.counteringPlayers ?? []) {
        const hand = world.hands[player] ?? emptyResources();
        const requestedPressure = RESOURCE_ORDER.reduce(
          (sum, resource) =>
            sum + Math.min(hand[resource], trade.receive[resource]),
          0,
        );
        likelihood *= 0.45 + Math.min(0.5, requestedPressure * 0.16);
      }
      if (trade.creator && world.hands[trade.creator]) {
        const creatorHand = world.hands[trade.creator]!;
        if (!hasResources(creatorHand, trade.give)) {
          likelihood *= 0.01;
        } else {
          const offeredSurplus = RESOURCE_ORDER.reduce(
            (sum, resource) =>
              sum +
              Math.max(
                0,
                creatorHand[resource] - trade.give[resource],
              ) *
                trade.give[resource],
            0,
          );
          const requestedScarcity = RESOURCE_ORDER.reduce(
            (sum, resource) =>
              sum +
              trade.receive[resource] /
                Math.max(1, creatorHand[resource] + 1),
            0,
          );
          // An offer is soft evidence of surplus in what is given and a
          // bottleneck in what is requested. Keep the likelihood deliberately
          // broad: players bluff, clean up seven-risk, and make speculative
          // offers that do not map perfectly to their current hand plan.
          likelihood *= Math.min(
            0.995,
            0.62 +
              Math.min(0.22, offeredSurplus * 0.035) +
              Math.min(0.15, requestedScarcity * 0.08),
          );
        }
      }
      world.weight *= likelihood;
    }
    for (const player of trade.rejectedPlayers ?? []) {
      const meta = updated.players[player];
      if (!meta) continue;
      meta.opponentModel.tradeRejects += 1;
      updatePolicyPosterior(meta, {
        tradeResistant: 1.26,
        tradeFlexible: 0.9,
      });
    }
    for (const player of trade.counteringPlayers ?? []) {
      const meta = updated.players[player];
      if (!meta) continue;
      meta.opponentModel.countersMade += 1;
      updatePolicyPosterior(meta, {
        tradeFlexible: 1.18,
        balanced: 1.08,
      });
    }
  }
  return {
    ...updated,
    worlds: normalizeWorldWeights(worlds),
  };
};

const vectorsEqual = (left: ResourceVector, right: ResourceVector): boolean =>
  RESOURCE_ORDER.every((resource) => left[resource] === right[resource]);

/**
 * Reconciles the possibility set with information Colonist already shows publicly.
 * Exact matches are filtered so robbery branches keep their correlations. If log
 * history is incomplete, only the user's exact hand is repaired; opponent card
 * compositions are never invented.
 */
export const reconcilePublicResourceEvidence = (
  state: TrackerState,
  evidence: PublicResourceEvidence,
): TrackerState => {
  let worlds = state.worlds.map(cloneWorld);
  const warnings = [...state.warnings];
  const exactHands = Object.entries(evidence.exactHands ?? {}).filter(([player]) =>
    Boolean(state.players[player]),
  );

  for (const [player, exactHand] of exactHands) {
    const matching = worlds.filter((world) =>
      vectorsEqual(world.hands[player] ?? emptyResources(), exactHand),
    );
    if (matching.length) {
      worlds = matching;
      continue;
    }
    worlds = worlds.map((world) => ({
      weight: world.weight,
      hands: {
        ...world.hands,
        [player]: cloneResources(exactHand),
      },
    }));
    const warning =
      "Your exact hand repaired an incomplete section of the public game log.";
    if (!warnings.includes(warning)) warnings.push(warning);
  }

  const handSizes = Object.entries(evidence.handSizes ?? {}).filter(([player]) =>
    Boolean(state.players[player]),
  );
  if (handSizes.length) {
    const matching = worlds.filter((world) =>
      handSizes.every(
        ([player, size]) =>
          resourceTotal(world.hands[player] ?? emptyResources()) === size,
      ),
    );
    worlds = matching;
  }

  if (evidence.bank && evidence.resourceSupply) {
    const matching = worlds.filter((world) =>
      RESOURCE_ORDER.every((resource) => {
        const held = state.playerOrder.reduce(
          (sum, player) => sum + (world.hands[player]?.[resource] ?? 0),
          0,
        );
        return held + evidence.bank![resource] === evidence.resourceSupply;
      }),
    );
    worlds = matching;
  }

  return {
    ...state,
    worlds: normalizeWorldWeights(worlds),
    warnings,
  };
};

export const readyDevCardCount = (state: TrackerState, player: string): number => {
  const meta = state.players[player];
  if (!meta) return 0;
  return meta.devCards.filter((card) => card.boughtOnTurn < state.currentTurn.sequence).length;
};

export const exactVector = (partial: Partial<ResourceVector>): ResourceVector => {
  const vector = emptyResources();
  addResources(vector, partial);
  return vector;
};

export const vectorFor = (resource: Resource, count = 1): ResourceVector =>
  exactVector({ [resource]: count });
