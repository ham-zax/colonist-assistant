import {
  BUILD_COSTS,
  RESOURCE_LABELS,
  RESOURCE_ORDER,
  RESOURCE_STRATEGIC_WEIGHTS,
  addResources,
  cloneResources,
  emptyResources,
  hasResources,
  resourceTotal,
  type BuildKind,
  type Resource,
  type ResourceVector,
} from "./resources";
import type {
  ActiveTradeOffer,
  BoardSnapshot,
} from "./placement";
import {
  estimateBuildTempo,
  likelyUpgradePath,
  playerBoardProfile,
  strategicThreatScore,
} from "./strategy";
import {
  getPlayerEstimate,
} from "./tracker";
import type { TrackerState } from "./types";

export type TradeVerdictKind = "accept" | "counter" | "decline";

export interface TradeVerdict {
  tradeId: string;
  kind: TradeVerdictKind;
  score: number;
  label: string;
  reason: string;
  detail: string;
  counterGive?: ResourceVector;
  counterReceive?: ResourceVector;
}

export interface TradeProposal {
  give: ResourceVector;
  receive: ResourceVector;
  partner: string;
  acceptanceProbability: number;
  score: number;
  ownTempoGain: number;
  opponentTempoGain: number;
  completesBuild?: BuildKind;
  reason: string;
}

export interface TradeEvaluationContext {
  primaryKind: BuildKind;
  primaryDeficit: ResourceVector;
  phase: "opening" | "middle" | "closing";
}

const buildUtility = (
  kind: BuildKind,
  board: BoardSnapshot,
  player: string,
): number => {
  const profile = playerBoardProfile(board, player);
  if (kind === "city") {
    return profile.settlements && profile.remaining.cities ? 38 : -100;
  }
  if (kind === "settlement") {
    return profile.openSettlementSites && profile.remaining.settlements
      ? 34
      : -100;
  }
  if (kind === "development") {
    return 19 + (profile.knightsToLargest <= 2 ? 9 : 0);
  }
  return profile.roadsToLongest <= 1 ? 31 : 6;
};

const bestNewBuild = (
  before: ResourceVector,
  after: ResourceVector,
  board: BoardSnapshot,
  player: string,
): { kind: BuildKind; utility: number } | undefined =>
  (Object.keys(BUILD_COSTS) as BuildKind[])
    .filter(
      (kind) =>
        !hasResources(before, BUILD_COSTS[kind]) &&
        hasResources(after, BUILD_COSTS[kind]),
    )
    .map((kind) => ({
      kind,
      utility: buildUtility(kind, board, player),
    }))
    .filter((candidate) => candidate.utility > 0)
    .sort((left, right) => right.utility - left.utility)[0];

const resourceShadowValue = (
  resource: Resource,
  board: BoardSnapshot,
  player: string,
  context: TradeEvaluationContext,
): number => {
  const profile = playerBoardProfile(board, player);
  const directPips = profile.activeProduction[resource];
  const deficit = context.primaryDeficit[resource];
  const bankScarcity =
    board.bankVisible && board.bank
      ? Math.max(0, 5 - board.bank[resource]) * 0.12
      : 0;
  const productionScarcity = directPips
    ? Math.max(0, 5 - directPips) * 0.1
    : 0.9;
  const bottleneck = deficit ? 1.05 + Math.min(2, deficit) * 0.32 : 0;
  return (
    RESOURCE_STRATEGIC_WEIGHTS[resource] +
    bankScarcity +
    productionScarcity +
    bottleneck
  );
};

const vectorValue = (
  vector: ResourceVector,
  board: BoardSnapshot,
  player: string,
  context: TradeEvaluationContext,
): number =>
  RESOURCE_ORDER.reduce(
    (sum, resource) =>
      sum +
      vector[resource] *
        resourceShadowValue(resource, board, player, context),
    0,
  );

const singleResource = (
  vector: ResourceVector,
): { resource: Resource; count: number } | undefined => {
  const entries = RESOURCE_ORDER.filter((resource) => vector[resource] > 0);
  if (entries.length !== 1) return undefined;
  const resource = entries[0]!;
  return { resource, count: vector[resource] };
};

const vectorLabel = (vector: ResourceVector): string =>
  RESOURCE_ORDER.filter((resource) => vector[resource] > 0)
    .map(
      (resource) =>
        `${vector[resource]} ${RESOURCE_LABELS[resource].toLowerCase()}`,
    )
    .join(" + ");

const vectorKey = (vector: ResourceVector): string =>
  RESOURCE_ORDER.map((resource) => vector[resource]).join(",");

const paymentProbability = (
  state: TrackerState,
  player: string,
  payment: ResourceVector,
): number => {
  if (!state.worlds.length) return 0;
  const total = state.worlds.reduce(
    (sum, world) => sum + Math.max(0, world.weight ?? 0),
    0,
  );
  if (total <= Number.EPSILON) return 0;
  return (
    state.worlds.reduce(
      (sum, world) =>
        sum +
        (
          hasResources(
            world.hands[player] ?? emptyResources(),
            payment,
          )
            ? Math.max(0, world.weight ?? 0)
            : 0
        ),
      0,
    ) / total
  );
};

const tradeRace = (
  board: BoardSnapshot,
  left: string,
  right: string,
): boolean => {
  const byVertex = new Map(board.vertices.map((vertex) => [vertex.id, vertex]));
  return board.vertices.some((vertex) => {
    if (
      vertex.building ||
      vertex.adjacentVertices.some(
        (neighbor) => byVertex.get(neighbor)?.building,
      )
    ) {
      return false;
    }
    let leftTouches = false;
    let rightTouches = false;
    for (const edge of board.edges) {
      if (!edge.vertices.includes(vertex.id)) continue;
      if (edge.player === left) leftTouches = true;
      if (edge.player === right) rightTouches = true;
    }
    return leftTouches && rightTouches;
  });
};

const applyTrade = (
  hand: ResourceVector,
  give: ResourceVector,
  receive: ResourceVector,
): ResourceVector => {
  const after = cloneResources(hand);
  addResources(after, give, -1);
  addResources(after, receive);
  return after;
};

const weightedCards = (
  vector: ResourceVector,
  board: BoardSnapshot,
  player: string,
  missing: ResourceVector,
): number => {
  const profile = playerBoardProfile(board, player);
  return RESOURCE_ORDER.reduce((sum, resource) => {
    const scarcity =
      profile.activeProduction[resource] > 0
        ? Math.max(0, 5 - profile.activeProduction[resource]) * 0.08
        : 0.72;
    const bottleneck = missing[resource] > 0 ? 0.75 : 0;
    return (
      sum +
      vector[resource] *
        (RESOURCE_STRATEGIC_WEIGHTS[resource] + scarcity + bottleneck)
    );
  }, 0);
};

export const predictTradeAcceptance = (
  state: TrackerState,
  board: BoardSnapshot,
  sender: string,
  receiver: string,
  giveToReceiver: ResourceVector,
  receiveFromReceiver: ResourceVector,
): {
  probability: number;
  receiverTempoGain: number;
  receiverPath: BuildKind;
  reasons: string[];
} => {
  const canPay = paymentProbability(state, receiver, receiveFromReceiver);
  if (canPay <= 0) {
    return {
      probability: 0,
      receiverTempoGain: 0,
      receiverPath: "development",
      reasons: ["Tracked hands cannot legally pay the request"],
    };
  }
  const receiverHand = getPlayerEstimate(state, receiver).average;
  const before = likelyUpgradePath(board, receiver, receiverHand);
  const afterHand = applyTrade(
    receiverHand,
    receiveFromReceiver,
    giveToReceiver,
  );
  const after = likelyUpgradePath(board, receiver, afterHand);
  const tempoGain = Math.max(-20, before.rolls - after.rolls);
  const directConversion = !before.affordable && after.affordable;
  const valueGain =
    weightedCards(giveToReceiver, board, receiver, before.missing) -
    weightedCards(receiveFromReceiver, board, receiver, before.missing);
  const senderProfile = playerBoardProfile(board, sender);
  const senderPointsRemaining =
    senderProfile.victoryTarget - senderProfile.visiblePoints;
  const leaderPenalty =
    senderPointsRemaining <= 1
      ? 5.4
      : senderPointsRemaining === 2
        ? 2.2
        : senderPointsRemaining === 3
          ? 0.75
          : 0;
  const racePenalty = tradeRace(board, sender, receiver) ? 2.4 : 0;
  const utility =
    tempoGain * 0.34 +
    valueGain * 0.72 +
    (directConversion ? 2.25 : 0) -
    leaderPenalty -
    racePenalty -
    0.35;
  const rationalResponse = 1 / (1 + Math.exp(-utility));
  const probability =
    canPay *
    Math.max(
      0.015,
      Math.min(0.985, rationalResponse * 0.9 + 0.05),
    );
  return {
    probability,
    receiverTempoGain: tempoGain,
    receiverPath: after.kind,
    reasons: [
      `${Math.round(canPay * 100)}% tracked chance they can pay`,
      tempoGain > 0.25
        ? `Improves their ${after.kind} ETA by about ${tempoGain.toFixed(1)} rolls`
        : "Does not materially improve their fastest build ETA",
      ...(leaderPenalty
        ? ["They should resist advancing a visible leader"]
        : []),
      ...(racePenalty ? ["Both players contest the same expansion site"] : []),
    ],
  };
};

const giveCombinations = (
  hand: ResourceVector,
): ResourceVector[] => {
  const combinations: ResourceVector[] = [];
  for (let left = 0; left < RESOURCE_ORDER.length; left += 1) {
    const first = RESOURCE_ORDER[left]!;
    if (hand[first] <= 0) continue;
    const single = emptyResources();
    single[first] = 1;
    combinations.push(single);
    for (let right = left; right < RESOURCE_ORDER.length; right += 1) {
      const second = RESOURCE_ORDER[right]!;
      const pair = cloneResources(single);
      pair[second] += 1;
      if (hasResources(hand, pair)) combinations.push(pair);
    }
  }
  return combinations;
};

export const generateTradeProposals = (
  state: TrackerState,
  board: BoardSnapshot,
  player: string,
  context: TradeEvaluationContext,
  onlyPartner?: string,
): TradeProposal[] => {
  const hand =
    player === board.myPlayer && board.ownHand
      ? board.ownHand
      : getPlayerEstimate(state, player).average;
  const profile = playerBoardProfile(board, player);
  const beforeTempo = estimateBuildTempo(
    hand,
    context.primaryKind,
    profile,
  );
  if (beforeTempo.affordable) return [];
  const receiveCandidates = RESOURCE_ORDER.filter(
    (resource) => context.primaryDeficit[resource] > 0,
  ).sort(
    (left, right) =>
      context.primaryDeficit[right] * RESOURCE_STRATEGIC_WEIGHTS[right] -
      context.primaryDeficit[left] * RESOURCE_STRATEGIC_WEIGHTS[left],
  );
  const giveCandidates = giveCombinations(hand)
    .map((give) => {
      const afterGive = applyTrade(hand, give, emptyResources());
      const reservationLoss = RESOURCE_ORDER.reduce(
        (sum, resource) =>
          sum +
          Math.max(
            0,
            ((BUILD_COSTS[context.primaryKind] as Partial<ResourceVector>)[
              resource
            ] ?? 0) -
              afterGive[resource],
          ) *
            RESOURCE_STRATEGIC_WEIGHTS[resource],
        0,
      );
      return { give, reservationLoss };
    })
    .sort(
      (left, right) =>
        left.reservationLoss - right.reservationLoss ||
        resourceTotal(left.give) - resourceTotal(right.give),
    )
    .slice(0, 12);
  const partners = state.playerOrder.filter(
    (candidate) =>
      candidate !== player &&
      (!onlyPartner || candidate === onlyPartner),
  );
  const proposals: TradeProposal[] = [];
  const seen = new Set<string>();
  for (const receiveResource of receiveCandidates) {
    const receive = emptyResources();
    receive[receiveResource] = 1;
    for (const { give, reservationLoss } of giveCandidates) {
      for (const partner of partners) {
        const key = `${partner}|${vectorKey(give)}|${vectorKey(receive)}`;
        if (seen.has(key)) continue;
        seen.add(key);
        const response = predictTradeAcceptance(
          state,
          board,
          player,
          partner,
          give,
          receive,
        );
        if (response.probability <= 0) continue;
        const after = applyTrade(hand, give, receive);
        const afterTempo = estimateBuildTempo(
          after,
          context.primaryKind,
          profile,
        );
        const ownTempoGain = Math.max(
          -20,
          beforeTempo.rolls - afterTempo.rolls,
        );
        const completedBuild =
          !beforeTempo.affordable && afterTempo.affordable
            ? context.primaryKind
            : undefined;
        const partnerProfile = playerBoardProfile(board, partner);
        const partnerPointsRemaining =
          partnerProfile.victoryTarget - partnerProfile.visiblePoints;
        if (
          partnerPointsRemaining <= 1 &&
          !(
            completedBuild &&
            profile.visiblePoints + 1 >= profile.victoryTarget
          )
        ) {
          continue;
        }
        const giveSingle = singleResource(give);
        const requestedFromPort =
          Boolean(
            giveSingle &&
              giveSingle.count >= profile.tradeRatios[giveSingle.resource],
          ) && !completedBuild;
        const opponentThreatMultiplier =
          partnerPointsRemaining <= 2
            ? 2.4
            : partnerPointsRemaining === 3
              ? 1.45
              : 0.75;
        const acceptedMargin =
          ownTempoGain * 2.8 +
          (completedBuild ? 28 : 0) -
          response.receiverTempoGain * opponentThreatMultiplier -
          reservationLoss * 3.2 -
          (requestedFromPort ? 8 : 0);
        const score =
          response.probability * acceptedMargin -
          (1 - response.probability) * 0.8;
        proposals.push({
          give,
          receive,
          partner,
          acceptanceProbability: response.probability,
          score,
          ownTempoGain,
          opponentTempoGain: response.receiverTempoGain,
          ...(completedBuild ? { completesBuild: completedBuild } : {}),
          reason: completedBuild
            ? `${Math.round(response.probability * 100)}% modeled acceptance; it completes your ${completedBuild}`
            : `${Math.round(response.probability * 100)}% modeled acceptance and saves about ${Math.max(0, ownTempoGain).toFixed(1)} rolls toward ${context.primaryKind}`,
        });
      }
    }
  }
  return proposals
    .filter((proposal) => proposal.score > 0.35)
    .sort(
      (left, right) =>
        right.score - left.score ||
        right.acceptanceProbability - left.acceptanceProbability,
    );
};

export const localTradeBundles = (
  trade: Pick<ActiveTradeOffer, "creatorGive" | "creatorReceive" | "incoming">,
): { give: ResourceVector; receive: ResourceVector } =>
  trade.incoming
    ? { give: trade.creatorReceive, receive: trade.creatorGive }
    : { give: trade.creatorGive, receive: trade.creatorReceive };

const opponentBenefit = (
  state: TrackerState,
  board: BoardSnapshot,
  player: string,
  trade: ActiveTradeOffer,
): {
  score: number;
  pointsRemaining: number;
  path?: BuildKind;
} => {
  const opponent = trade.creator;
  if (opponent === player || !state.players[opponent]) {
    return { score: 0, pointsRemaining: 99 };
  }
  const estimate = getPlayerEstimate(state, opponent);
  const before = cloneResources(estimate.average);
  const after = cloneResources(before);
  // Active trades are creator-relative. Estimate the creator after surrendering
  // creatorGive and receiving creatorReceive. Estimates can be fractional, so
  // clamp after subtraction.
  addResources(after, trade.creatorGive, -1);
  addResources(after, trade.creatorReceive);
  for (const resource of RESOURCE_ORDER) {
    after[resource] = Math.max(0, after[resource]);
  }
  const beforePath = likelyUpgradePath(board, opponent, before);
  const afterPath = likelyUpgradePath(board, opponent, after);
  const tempoGain = Math.max(0, beforePath.rolls - afterPath.rolls);
  const visiblePoints = board.players?.[opponent]?.visiblePoints ?? 0;
  const pointsRemaining = (board.victoryTarget ?? 10) - visiblePoints;
  const threat = strategicThreatScore(
    board,
    opponent,
    before,
    board.players?.[opponent]?.developmentCards ?? 0,
    board.players?.[opponent]?.playedDevelopmentCards?.knight ?? 0,
  );
  const directBuild =
    !beforePath.affordable && afterPath.affordable
      ? afterPath.kind === "city"
        ? 22
        : afterPath.kind === "settlement"
          ? 18
          : 10
      : 0;
  const leaderMultiplier =
    pointsRemaining <= 1
      ? 3.2
      : pointsRemaining === 2
        ? 2
        : pointsRemaining === 3
          ? 1.35
          : 1;
  return {
    score:
      (tempoGain * 1.8 + directBuild + threat * 0.24) *
      leaderMultiplier,
    pointsRemaining,
    path: afterPath.kind,
  };
};

export const evaluateTradeOffer = (
  state: TrackerState,
  board: BoardSnapshot,
  player: string,
  trade: ActiveTradeOffer,
  context: TradeEvaluationContext,
): TradeVerdict => {
  const hand = board.ownHand;
  if (!hand || !trade.incoming) {
    return {
      tradeId: trade.id,
      kind: "decline",
      score: -100,
      label: "DECLINE",
      reason: "This is not an actionable incoming offer",
      detail: "Colonist Assistant only grades fully specified offers to you.",
    };
  }
  const { give, receive } = localTradeBundles(trade);
  const fullySpecified =
    resourceTotal(trade.creatorGive) > 0 &&
    resourceTotal(trade.creatorReceive) > 0;
  if (!fullySpecified) {
    const counter = generateTradeProposals(
      state,
      board,
      player,
      context,
      trade.creator,
    )[0];
    if (counter) {
      return {
        tradeId: trade.id,
        kind: "counter",
        score: 0,
        label: "COUNTER",
        reason: "The open-ended offer needs a concrete exchange",
        detail: `Counter with ${vectorLabel(counter.give)} for ${vectorLabel(counter.receive)}; modeled acceptance ${Math.round(counter.acceptanceProbability * 100)}%.`,
        counterGive: counter.give,
        counterReceive: counter.receive,
      };
    }
    return {
      tradeId: trade.id,
      kind: "decline",
      score: -100,
      label: "DECLINE",
      reason: "The open-ended offer has no useful concrete counter",
      detail: "Do not map an unspecified side of the offer to a zero-card trade.",
    };
  }
  if (!trade.canAccept || !hasResources(hand, give)) {
    return {
      tradeId: trade.id,
      kind: "decline",
      score: -100,
      label: "DECLINE",
      reason: "You cannot legally pay this offer",
      detail: "Keep the trade card closed or counter with cards you actually hold.",
    };
  }
  const after = cloneResources(hand);
  addResources(after, give, -1);
  addResources(after, receive);
  const completedBuild = bestNewBuild(hand, after, board, player);
  const beforePrimary = hasResources(hand, BUILD_COSTS[context.primaryKind]);
  const afterPrimary = hasResources(after, BUILD_COSTS[context.primaryKind]);
  const immediatePrimary = !beforePrimary && afterPrimary;
  const receiveValue = vectorValue(
    receive,
    board,
    player,
    context,
  );
  const giveValue = vectorValue(give, board, player, context);
  const profile = playerBoardProfile(board, player);
  const handRiskRelief =
    resourceTotal(hand) > profile.cardDiscardLimit
      ? Math.max(0, resourceTotal(give) - resourceTotal(receive)) *
        2.5
      : 0;
  const ownGain =
    (receiveValue - giveValue) * 8 +
    (completedBuild?.utility ?? 0) +
    (immediatePrimary ? 16 : 0) +
    handRiskRelief;
  const opponent = opponentBenefit(state, board, player, trade);

  const giveSingle = singleResource(give);
  const receiveSingle = singleResource(receive);
  const portAlternativePenalty =
    giveSingle &&
    receiveSingle &&
    giveSingle.count >= profile.tradeRatios[giveSingle.resource] &&
    !completedBuild
      ? 13
      : 0;
  const leaderVeto =
    opponent.pointsRemaining <= 1 && opponent.score >= ownGain * 0.55;
  const netScore =
    ownGain -
    opponent.score -
    portAlternativePenalty -
    (context.phase === "closing" && opponent.pointsRemaining <= 2 ? 8 : 0);

  if (
    !leaderVeto &&
    (netScore >= 7 ||
      (completedBuild && netScore >= 1) ||
      (immediatePrimary && opponent.pointsRemaining > 1))
  ) {
    const conversionKind =
      completedBuild?.kind ??
      (immediatePrimary ? context.primaryKind : undefined);
    const buildLabel = conversionKind
      ? conversionKind === "development"
        ? "development card"
        : conversionKind
      : undefined;
    return {
      tradeId: trade.id,
      kind: "accept",
      score: Math.round(netScore),
      label: "ACCEPT",
      reason: buildLabel
        ? `It completes your ${buildLabel} immediately`
        : "You gain more bottleneck value than you give away",
      detail: `Net strategic margin ${Math.round(netScore)} after your ports, current plan, and ${trade.creator}'s visible win path.`,
    };
  }

  if (
    ownGain > 2 &&
    !leaderVeto &&
    resourceTotal(give) > resourceTotal(receive)
  ) {
    const counter = generateTradeProposals(
      state,
      board,
      player,
      context,
      trade.creator,
    ).find(
      (proposal) =>
        vectorKey(proposal.give) !== vectorKey(give) ||
        vectorKey(proposal.receive) !== vectorKey(receive),
    );
    if (counter) {
      return {
        tradeId: trade.id,
        kind: "counter",
        score: Math.round(netScore),
        label: "COUNTER",
        reason: "The resource direction helps, but the price is too high",
        detail: `Counter with ${vectorLabel(counter.give)} for ${vectorLabel(counter.receive)}; modeled acceptance ${Math.round(counter.acceptanceProbability * 100)}%.`,
        counterGive: counter.give,
        counterReceive: counter.receive,
      };
    }
  }

  return {
    tradeId: trade.id,
    kind: "decline",
    score: Math.round(netScore),
    label: "DECLINE",
    reason: leaderVeto
      ? `It materially advances ${trade.creator}'s win line`
      : portAlternativePenalty
        ? "Your own port or bank preserves more strategic value"
        : completedBuild
          ? "The build is real, but the opponent gains too much"
          : "It does not create enough immediate tempo for you",
    detail: `Net strategic margin ${Math.round(netScore)} after production scarcity, ports, hand risk, and opponent threat.`,
  };
};
