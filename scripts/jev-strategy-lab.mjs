import fs from "node:fs";
import path from "node:path";
import {
  jevRequestCacheKey,
  readCachedJevResponse,
  writeCachedJevResponse,
} from "./jev-raw-cache.mjs";

const API_URL = "https://api.typesafe.ai/v1/systemone";
const MODEL = "jev-1.13.0";

function parseArgs(argv) {
  const config = {
    input: "benchmark-results/jev-lab/no-trades.jsonl",
    output: "benchmark-results/jev-lab/jev-pass1.jsonl",
    pass: "pass1",
    limit: 24,
    game: null,
    stage: "all",
    cacheDir: "benchmark-results/jev-lab/raw-cache",
    candidateOrder: "canonical",
    wordingVariant: "standard",
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--input") config.input = argv[++i];
    else if (arg === "--output") config.output = argv[++i];
    else if (arg === "--pass") config.pass = argv[++i];
    else if (arg === "--limit") config.limit = Number.parseInt(argv[++i], 10);
    else if (arg === "--game") config.game = Number.parseInt(argv[++i], 10);
    else if (arg === "--stage") config.stage = argv[++i];
    else if (arg === "--cache-dir") config.cacheDir = argv[++i];
    else if (arg === "--candidate-order") config.candidateOrder = argv[++i];
    else if (arg === "--wording-variant") config.wordingVariant = argv[++i];
    else if (arg === "--help" || arg === "-h") {
      console.log("node scripts/jev-strategy-lab.mjs --input decisions.jsonl --output jev.jsonl [--pass direct|pass1|pass2|pass3|pass4|pass5] [--limit N] [--game N] [--stage all|setup|main] [--cache-dir PATH] [--candidate-order canonical|reverse] [--wording-variant standard|alternate]");
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }
  if (!Number.isFinite(config.limit) || config.limit < 1) throw new Error("--limit must be positive");
  if (!["all", "setup", "main"].includes(config.stage)) {
    throw new Error("--stage must be all, setup, or main");
  }
  if (!["direct", "pass1", "pass2", "pass3", "pass4", "pass5"].includes(config.pass)) {
    throw new Error("--pass must be direct, pass1, pass2, pass3, pass4, or pass5");
  }
  if (!["canonical", "reverse"].includes(config.candidateOrder)) {
    throw new Error("--candidate-order must be canonical or reverse");
  }
  if (!["standard", "alternate"].includes(config.wordingVariant)) {
    throw new Error("--wording-variant must be standard or alternate");
  }
  return config;
}

function readJsonl(file) {
  const lines = fs.readFileSync(file, "utf8").split(/\r?\n/);
  const records = [];
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (!line) continue;
    try {
      records.push(JSON.parse(line));
    } catch (error) {
      const isLastPhysicalLine = index === lines.length - 1;
      if (isLastPhysicalLine) break;
      throw error;
    }
  }
  return records;
}

function extractId(action, kind) {
  const pattern = kind === "vertex"
    ? /(?:Settlement|City) \{ vertex: (\d+) \}/
    : /(?:Road|RoadBuilding) \{ (?:edge|first): (\d+)/;
  const match = action.match(pattern);
  return match ? Number.parseInt(match[1], 10) : null;
}

function hexSummary(hex) {
  if (!hex) return null;
  return {
    id: hex.id,
    resource: hex.resource ?? "desert",
    number: hex.number,
    robber: hex.robber,
  };
}

function vertexSummary(state, vertexId) {
  const vertex = state.vertices[vertexId];
  if (!vertex) return null;
  return {
    vertex: vertexId,
    building: vertex.building,
    port: vertex.port,
    adjacentHexes: vertex.adjacentHexes.map((id) => hexSummary(state.hexes[id])),
    adjacentVertices: vertex.adjacentVertices.map((id) => ({
      vertex: id,
      building: state.vertices[id]?.building ?? null,
      port: state.vertices[id]?.port ?? null,
    })),
    adjacentEdges: vertex.adjacentEdges.map((id) => ({
      edge: id,
      roadOwner: state.edges[id]?.roadOwner ?? null,
      vertices: state.edges[id]?.vertices ?? [],
    })),
  };
}

function edgeSummary(state, edgeId) {
  const edge = state.edges[edgeId];
  if (!edge) return null;
  return {
    edge: edgeId,
    roadOwner: edge.roadOwner,
    adjacentHexes: edge.adjacentHexes.map((id) => hexSummary(state.hexes[id])),
    endpoints: edge.vertices.map((vertex) => vertexSummary(state, vertex)),
  };
}

function actionLocalSummary(state, action) {
  if (/Settlement|City/.test(action)) {
    const vertex = extractId(action, "vertex");
    return vertex === null ? null : vertexSummary(state, vertex);
  }
  if (/Road/.test(action)) {
    const edge = extractId(action, "edge");
    return edge === null ? null : edgeSummary(state, edge);
  }
  return { actionClass: action.split(" ")[0] };
}

const PIPS_BY_NUMBER = Object.freeze({
  2: 1,
  3: 2,
  4: 3,
  5: 4,
  6: 5,
  7: 0,
  8: 5,
  9: 4,
  10: 3,
  11: 2,
  12: 1,
});

const RESOURCE_NAMES = Object.freeze(["lumber", "brick", "wool", "grain", "ore"]);

function emptyResourceVector() {
  return Object.fromEntries(RESOURCE_NAMES.map((resource) => [resource, 0]));
}

function standardPips(number) {
  return PIPS_BY_NUMBER[number] ?? 0;
}

function vertexProduction(state, vertexId) {
  const vertex = state.vertices[vertexId];
  const byResource = emptyResourceVector();
  const numbers = [];
  if (!vertex) {
    return { byResource, totalPips: 0, resourceDiversity: 0, numbers };
  }
  for (const hexId of vertex.adjacentHexes) {
    const hex = state.hexes[hexId];
    if (!hex?.resource) continue;
    const pips = standardPips(hex.number);
    byResource[hex.resource] += pips;
    numbers.push(hex.number);
  }
  return {
    byResource,
    totalPips: Object.values(byResource).reduce((sum, value) => sum + value, 0),
    resourceDiversity: Object.values(byResource).filter((value) => value > 0).length,
    numbers: numbers.sort((a, b) => a - b),
  };
}

function addResourceVectors(left, right) {
  return Object.fromEntries(
    RESOURCE_NAMES.map((resource) => [
      resource,
      Number(left?.[resource] ?? 0) + Number(right?.[resource] ?? 0),
    ]),
  );
}

const BUILD_COSTS = Object.freeze({
  road: Object.freeze({ lumber: 1, brick: 1, wool: 0, grain: 0, ore: 0 }),
  settlement: Object.freeze({ lumber: 1, brick: 1, wool: 1, grain: 1, ore: 0 }),
  city: Object.freeze({ lumber: 0, brick: 0, wool: 0, grain: 2, ore: 3 }),
  developmentCard: Object.freeze({ lumber: 0, brick: 0, wool: 1, grain: 1, ore: 1 }),
});

function resourceCostForAction(action) {
  if (/^BuildRoad\b/.test(action)) return BUILD_COSTS.road;
  if (/^BuildSettlement\b/.test(action)) return BUILD_COSTS.settlement;
  if (/^BuildCity\b/.test(action)) return BUILD_COSTS.city;
  if (/^BuyDevelopment\b/.test(action)) return BUILD_COSTS.developmentCard;
  return emptyResourceVector();
}

function resourceKey(value) {
  const key = String(value ?? "").toLowerCase();
  return RESOURCE_NAMES.includes(key) ? key : null;
}

function maritimeTradeForAction(action) {
  const match = action.match(
    /^MaritimeTrade \{ give: ([A-Za-z]+), receive: ([A-Za-z]+), ratio: (\d+) \}$/,
  );
  if (!match) return null;
  const giveResource = resourceKey(match[1]);
  const receiveResource = resourceKey(match[2]);
  const ratio = Number.parseInt(match[3], 10);
  if (!giveResource || !receiveResource || !Number.isInteger(ratio)) return null;
  return { giveResource, receiveResource, ratio };
}

function resourceDeltaForAction(action) {
  const delta = emptyResourceVector();
  const trade = maritimeTradeForAction(action);
  if (trade) {
    delta[trade.giveResource] -= trade.ratio;
    delta[trade.receiveResource] += 1;
    return delta;
  }
  const cost = resourceCostForAction(action);
  for (const resource of RESOURCE_NAMES) delta[resource] -= Number(cost[resource] ?? 0);
  return delta;
}

function applyResourceDelta(hand, delta) {
  return Object.fromEntries(
    RESOURCE_NAMES.map((resource) => [
      resource,
      Math.max(0, Number(hand?.[resource] ?? 0) + Number(delta?.[resource] ?? 0)),
    ]),
  );
}

function handAffordability(hand) {
  const canPay = (cost) =>
    RESOURCE_NAMES.every((resource) => Number(hand?.[resource] ?? 0) >= Number(cost[resource] ?? 0));
  return {
    road: canPay(BUILD_COSTS.road),
    settlement: canPay(BUILD_COSTS.settlement),
    city: canPay(BUILD_COSTS.city),
    developmentCard: canPay(BUILD_COSTS.developmentCard),
  };
}

function spendFeatures(actorState, action) {
  const before = actorState?.resourcesIfActor;
  if (!before || typeof before !== "object") return null;
  const cost = resourceCostForAction(action);
  const delta = resourceDeltaForAction(action);
  const trade = maritimeTradeForAction(action);
  const after = applyResourceDelta(before, delta);
  return {
    handBeforeAction: before,
    exactResourceCost: cost,
    exactResourceDelta: delta,
    maritimeTrade: trade,
    domesticTradeOfferPending: /^OfferTrade\b/.test(action),
    handAfterAction: after,
    immediateBuildAffordabilityAfterAction: handAffordability(after),
  };
}

function buildCoverage(production) {
  const access = (resources) => resources.filter((resource) => (production[resource] ?? 0) > 0).length;
  return {
    road: {
      accessibleResources: access(["lumber", "brick"]),
      requiredResources: 2,
      complete: access(["lumber", "brick"]) === 2,
    },
    settlement: {
      accessibleResources: access(["lumber", "brick", "wool", "grain"]),
      requiredResources: 4,
      complete: access(["lumber", "brick", "wool", "grain"]) === 4,
    },
    city: {
      accessibleResources: access(["grain", "ore"]),
      requiredResources: 2,
      complete: access(["grain", "ore"]) === 2,
    },
    developmentCard: {
      accessibleResources: access(["wool", "grain", "ore"]),
      requiredResources: 3,
      complete: access(["wool", "grain", "ore"]) === 3,
    },
  };
}

function actorExistingNumbers(state, actor) {
  const numbers = [];
  for (const vertex of state.vertices) {
    if (vertex.building?.player !== actor) continue;
    for (const hexId of vertex.adjacentHexes) {
      const hex = state.hexes[hexId];
      if (hex?.resource && hex.number) numbers.push(hex.number);
    }
  }
  return numbers;
}

function distanceLegalVertex(state, vertexId) {
  const vertex = state.vertices[vertexId];
  if (!vertex || vertex.building !== null) return false;
  return vertex.adjacentVertices.every((neighbor) => state.vertices[neighbor]?.building === null);
}

function settlementSiteSummary(state, vertexId) {
  const production = vertexProduction(state, vertexId);
  const vertex = state.vertices[vertexId];
  return {
    vertex: vertexId,
    ...production,
    port: vertex?.port ?? null,
    distanceLegalNow: distanceLegalVertex(state, vertexId),
  };
}

function roadFrontierFeatures(state, edgeId, actor) {
  const edge = state.edges[edgeId];
  if (!edge) return null;
  const connectedEndpoints = edge.vertices.filter((vertexId) => {
    const vertex = state.vertices[vertexId];
    if (vertex?.building?.player === actor) return true;
    return vertex?.adjacentEdges.some(
      (adjacentEdge) => adjacentEdge !== edgeId && state.edges[adjacentEdge]?.roadOwner === actor,
    ) ?? false;
  });
  const origin = connectedEndpoints.length === 1 ? connectedEndpoints[0] : null;
  const farEndpoints = connectedEndpoints.length === 1
    ? edge.vertices.filter((vertex) => vertex !== origin)
    : connectedEndpoints.length === 0
      ? [...edge.vertices]
      : [];
  const directSites = [...new Set(farEndpoints)]
    .filter((vertex) => distanceLegalVertex(state, vertex))
    .map((vertex) => settlementSiteSummary(state, vertex));
  const oneMoreRoadSites = [];
  const seen = new Set();
  for (const frontier of farEndpoints) {
    const vertex = state.vertices[frontier];
    if (!vertex) continue;
    for (const neighbor of vertex.adjacentVertices) {
      if (neighbor === origin || seen.has(neighbor)) continue;
      seen.add(neighbor);
      if (distanceLegalVertex(state, neighbor)) {
        oneMoreRoadSites.push(settlementSiteSummary(state, neighbor));
      }
    }
  }
  oneMoreRoadSites.sort((a, b) => b.totalPips - a.totalPips || a.vertex - b.vertex);
  return {
    edge: edgeId,
    origin,
    connectedEndpoints,
    connectionAmbiguous: connectedEndpoints.length === 0,
    closesExistingNetwork: connectedEndpoints.length === 2,
    farEndpoints,
    immediateDistanceLegalSettlementSites: directSites,
    settlementSitesAfterOneAdditionalRoad: oneMoreRoadSites,
    bestSiteAfterOneAdditionalRoad: oneMoreRoadSites[0] ?? null,
    frontierBranchCount: oneMoreRoadSites.length,
  };
}

function settlementRoadDirections(state, vertexId) {
  const vertex = state.vertices[vertexId];
  if (!vertex) return [];
  const directions = [];
  for (const edgeId of vertex.adjacentEdges) {
    const edge = state.edges[edgeId];
    if (!edge || edge.roadOwner !== null) continue;
    const farEndpoint = edge.vertices.find((endpoint) => endpoint !== vertexId);
    if (farEndpoint === undefined) continue;
    const targetSites = [];
    const farVertex = state.vertices[farEndpoint];
    if (farVertex) {
      for (const target of farVertex.adjacentVertices) {
        if (target === vertexId || !distanceLegalVertex(state, target)) continue;
        const connectingEdge = farVertex.adjacentEdges
          .map((candidateEdge) => state.edges[candidateEdge])
          .find((candidateEdge) => candidateEdge?.vertices.includes(target));
        if (!connectingEdge || connectingEdge.roadOwner !== null) continue;
        targetSites.push(settlementSiteSummary(state, target));
      }
    }
    targetSites.sort((a, b) => b.totalPips - a.totalPips || a.vertex - b.vertex);
    directions.push({
      setupRoadEdge: edgeId,
      farEndpoint,
      settlementSitesAfterOneAdditionalRoad: targetSites,
      bestSiteAfterOneAdditionalRoad: targetSites[0] ?? null,
      frontierBranchCount: targetSites.length,
    });
  }
  directions.sort((a, b) =>
    (b.bestSiteAfterOneAdditionalRoad?.totalPips ?? 0)
      - (a.bestSiteAfterOneAdditionalRoad?.totalPips ?? 0)
      || b.frontierBranchCount - a.frontierBranchCount
      || a.setupRoadEdge - b.setupRoadEdge
  );
  return directions;
}

function projectedMaritimeRatios(state, actor, candidateVertexId = null) {
  const ratios = Object.fromEntries(RESOURCE_NAMES.map((resource) => [resource, 4]));
  const portVertices = new Set();
  for (const vertex of state.vertices) {
    if (vertex.building?.player === actor) portVertices.add(vertex.id);
  }
  if (candidateVertexId !== null) portVertices.add(candidateVertexId);

  for (const vertexId of portVertices) {
    const port = state.vertices[vertexId]?.port;
    if (port === "3:1") {
      for (const resource of RESOURCE_NAMES) ratios[resource] = Math.min(ratios[resource], 3);
      continue;
    }
    if (typeof port === "string" && port.startsWith("2:1-")) {
      const resource = port.slice("2:1-".length);
      if (RESOURCE_NAMES.includes(resource)) ratios[resource] = 2;
    }
  }
  return ratios;
}

function maritimeRepairOptions(production, ratios) {
  const result = {};
  for (const target of RESOURCE_NAMES) {
    const sources = RESOURCE_NAMES
      .filter((source) => source !== target && (production[source] ?? 0) > 0)
      .map((source) => ({
        giveResource: source,
        giveRatio: ratios[source],
        sourcePips: production[source],
        pipRatePerImportedCard: production[source] / ratios[source],
      }))
      .sort((a, b) => b.pipRatePerImportedCard - a.pipRatePerImportedCard);
    result[target] = {
      directPips: production[target] ?? 0,
      missingDirectProduction: (production[target] ?? 0) <= 0,
      bestMaritimeRepairSource: sources[0] ?? null,
    };
  }
  return result;
}

function setupCardsFromVertex(state, vertexId) {
  const cards = emptyResourceVector();
  const vertex = state.vertices[vertexId];
  if (!vertex) return cards;
  for (const hexId of vertex.adjacentHexes) {
    const resource = state.hexes[hexId]?.resource;
    if (resource) cards[resource] += 1;
  }
  return cards;
}

function candidateDerivedFeatures(record, action) {
  const state = record.publicState;
  const actor = record.actor;
  const actorState = state.players.find((player) => player.player === actor);
  const currentProduction = actorState?.productionPips ?? emptyResourceVector();
  const spend = spendFeatures(actorState, action);

  if (/PlaceSettlement|BuildSettlement|BuildCity/.test(action)) {
    const vertexId = extractId(action, "vertex");
    if (vertexId === null) return null;
    const delta = vertexProduction(state, vertexId);
    const multiplier = /BuildCity/.test(action) ? 1 : 1;
    const deltaByResource = Object.fromEntries(
      RESOURCE_NAMES.map((resource) => [resource, delta.byResource[resource] * multiplier]),
    );
    const projectedProduction = addResourceVectors(currentProduction, deltaByResource);
    const maritimeRatios = projectedMaritimeRatios(state, actor, vertexId);
    const existingNumbers = actorExistingNumbers(state, actor);
    const duplicateNumbers = delta.numbers.filter((number) => existingNumbers.includes(number));
    const isSecondSetupSettlement =
      record.phase === "SetupSettlement" && (actorState?.settlementsBuilt ?? 0) >= 1;
    return {
      kind: /BuildCity/.test(action) ? "city" : "settlement",
      placement: settlementSiteSummary(state, vertexId),
      productionDeltaPips: deltaByResource,
      productionDeltaTotalPips: delta.totalPips,
      projectedProductionPips: projectedProduction,
      projectedTotalPips: Object.values(projectedProduction).reduce((sum, value) => sum + value, 0),
      projectedResourceAccessCount: Object.values(projectedProduction).filter((value) => value > 0).length,
      projectedBuildCoverage: buildCoverage(projectedProduction),
      projectedBuildCoverageInterpretation: "Direct production coverage only; use maritimeRepairOptions and projectedMaritimeRatios before treating a missing resource as a hard bottleneck.",
      projectedMaritimeRatios: maritimeRatios,
      maritimeRepairOptions: maritimeRepairOptions(projectedProduction, maritimeRatios),
      duplicateNumbersWithExistingPortfolio: duplicateNumbers,
      setupRoadDirections: record.phase === "SetupSettlement"
        ? settlementRoadDirections(state, vertexId)
        : null,
      setupStartingCardsIfThisIsSecondSettlement: isSecondSetupSettlement
        ? setupCardsFromVertex(state, vertexId)
        : null,
      spend,
    };
  }

  if (/PlaceRoad|BuildRoad/.test(action)) {
    const edgeId = extractId(action, "edge");
    return edgeId === null ? null : {
      kind: "road",
      frontier: roadFrontierFeatures(state, edgeId, actor),
      spend,
    };
  }

  if (/^BuyDevelopment\b/.test(action)) {
    return {
      kind: "developmentCardPurchase",
      spend,
      informationSetDrawBelief: state.developmentDrawBelief ?? null,
    };
  }

  return { kind: "other", spend };
}

function evidenceFor(record, action) {
  return record.rootEvidence.find((entry) => entry.action === action) ?? null;
}

function blindMechanicalStrategicEvidence(record, action) {
  const evidence = evidenceFor(record, action);
  if (!evidence) return null;
  return {
    roadIntent: evidence.roadIntent ?? null,
    introducedRoadFragility: evidence.introducedRoadFragility ?? null,
    roadCutContinuation: evidence.roadCutContinuation ?? null,
    closeoutGain: evidence.closeoutGain ?? 0,
    tradeRiskPosterior: evidence.tradeRiskPosterior ?? 0,
    dirtyMonopolyPosterior: evidence.dirtyMonopolyPosterior ?? 0,
    tradeHardVetoPosterior: evidence.tradeHardVetoPosterior ?? 0,
  };
}

function candidateOptions(record, max = 12, blind = false, candidateOrder = "canonical") {
  const actor = record.actor;
  const state = record.publicState;
  const shortlist = [...record.candidateActions]
    .sort((a, b) => b.actorValue - a.actorValue)
    .slice(0, max);
  if (blind) {
    shortlist.sort((a, b) => a.action.localeCompare(b.action));
    if (candidateOrder === "reverse") shortlist.reverse();
  }

  return shortlist.reduce((options, candidate, index) => {
    const key = `c${index}`;
    const mechanicalPlanEvidence = {
      completionMass: candidate.plannerCompletionMass ?? null,
      decisiveCompletionMass: candidate.plannerDecisiveCompletionMass ?? null,
      opponentResponseWindows: candidate.plannerResponseWindows ?? null,
    };
    options[key] = blind
      ? {
          action: candidate.action,
          local: actionLocalSummary(state, candidate.action),
          derived: candidateDerivedFeatures(record, candidate.action),
          mechanicalPlanEvidence,
          mechanicalStrategicEvidence: blindMechanicalStrategicEvidence(record, candidate.action),
          actor,
        }
      : {
          action: candidate.action,
          local: actionLocalSummary(state, candidate.action),
          derived: candidateDerivedFeatures(record, candidate.action),
          engineSearch: {
            actorValue: candidate.actorValue,
            actorLowerConfidenceValue: candidate.actorLowerConfidenceValue,
            legalWeight: candidate.legalWeight,
            selectedByEngine: candidate.chosen,
          },
          engineCausalEvidence: evidenceFor(record, candidate.action),
          mechanicalPlanEvidence,
          actor,
        };
    return options;
  }, {});
}

function selectedOption(record, options) {
  return Object.entries(options).find(([, value]) => value.action === record.chosenAction)?.[0] ?? null;
}

function compactBoard(record) {
  const state = record.publicState;
  return {
    rules: {
      objective: `Reach ${state.victoryTarget} victory points`,
      roadCost: "1 lumber + 1 brick",
      settlementCost: "1 lumber + 1 brick + 1 wool + 1 grain",
      cityCost: "2 grain + 3 ore",
      developmentCardCost: "1 wool + 1 grain + 1 ore",
      settlementDistanceRule: "new settlements cannot be adjacent to an existing settlement/city",
      maritimeTrading: "Without a port, give 4 cards of one resource for 1 chosen resource. A 3:1 port lowers every give ratio to 3. A 2:1 resource port lowers the give ratio for that produced resource to 2.",
      playerTrading: state.playerTradesEnabled ? "enabled" : "disabled",
      hiddenInformation: "Opponent resource identities are unknown. Use only the provided belief probabilities; do not infer exact hidden cards.",
    },
    context: {
      turn: state.turn,
      phase: state.phase,
      actor: state.actor,
      currentPlayer: state.currentPlayer,
      longestRoadHolder: state.longestRoadHolder,
      largestArmyHolder: state.largestArmyHolder,
    },
    players: state.players,
    board: {
      hexes: state.hexes,
      occupiedVertices: state.vertices
        .filter((vertex) => vertex.building !== null)
        .map((vertex) => ({
          id: vertex.id,
          building: vertex.building,
          port: vertex.port,
          adjacentHexes: vertex.adjacentHexes.map((id) => hexSummary(state.hexes[id])),
        })),
      occupiedRoads: state.edges
        .filter((edge) => edge.roadOwner !== null)
        .map((edge) => ({ id: edge.id, vertices: edge.vertices, roadOwner: edge.roadOwner })),
    },
  };
}

const SCORE_LEVELS = [
  "Very poor: creates a major strategic weakness or near-term dead end",
  "Poor: meaningful long-horizon weakness outweighs the benefit",
  "Mixed: viable but with material strategic trade-offs",
  "Strong: improves long-horizon position with manageable weaknesses",
  "Excellent: robust, flexible, and strongly advances multiple future plans",
];

function choiceQuestion(instructions, options) {
  return { type: "choice", instructions, criteria: options };
}

function pass1Questions(options, engineSelected) {
  const selected = engineSelected ? options[engineSelected] : null;
  return {
    strongest_resource_independence: choiceQuestion(
      "Which candidate best preserves a self-sufficient resource economy over the next several builds, especially when player trading is unavailable?",
      options,
    ),
    strongest_next_settlement_path: choiceQuestion(
      "Which candidate best improves the realistic path to the actor's next settlement, considering required roads, competition, and fallback routes?",
      options,
    ),
    strongest_city_transition: choiceQuestion(
      "Which candidate best supports a future city transition without creating a severe grain or ore bottleneck?",
      options,
    ),
    strongest_development_transition: choiceQuestion(
      "Which candidate best preserves the option to buy useful development cards while still funding core expansion?",
      options,
    ),
    strongest_road_resilience: choiceQuestion(
      "Which candidate produces the most resilient road and expansion network if an opponent makes one strong blocking move?",
      options,
    ),
    strongest_opponent_denial: choiceQuestion(
      "Which candidate creates the most useful denial pressure on opponents without sacrificing the actor's own economy?",
      options,
    ),
    strongest_hidden_hand_safety: choiceQuestion(
      "Given the provided opponent affordability belief probabilities, which candidate is least exposed to a plausible sudden opponent road or road-plus-settlement response?",
      options,
    ),
    strongest_optionality: choiceQuestion(
      "Which candidate leaves the actor with the widest set of strong follow-up plans rather than committing to one brittle line?",
      options,
    ),
    strongest_tempo: choiceQuestion(
      "Which candidate most improves useful tempo over the next few turns: converting resources into productive builds without waste or forced waiting?",
      options,
    ),
    strongest_long_horizon: choiceQuestion(
      "Considering only long-horizon strategic position rather than immediate engine score, which candidate is the strongest foundation for the actor?",
      options,
    ),
    engine_selected_strategic_quality: {
      type: "score",
      instructions: {
        question: "How strong is the engine-selected action as a long-horizon strategic decision?",
        selectedAction: selected?.action ?? null,
        focus: "Judge flexibility, future build paths, resource structure, blocking exposure, and strategic options. Do not simply repeat the engine numeric value.",
      },
      criteria: SCORE_LEVELS,
    },
    engine_selected_myopic: {
      type: "noul",
      instructions: {
        question: "Is the engine-selected action tactically attractive but strategically myopic compared with at least one supplied alternative?",
        selectedAction: selected?.action ?? null,
      },
      criteria: {
        true: "The selected action has an immediate/local appeal but gives up materially better long-horizon structure available in another candidate.",
        false: "The selected action remains competitive on long-horizon structure and is not merely a short-term optimization.",
      },
    },
    engine_selected_fragile: {
      type: "noul",
      instructions: {
        question: "Does the engine-selected action rely too heavily on one expansion route or one opponent not blocking a critical edge/vertex?",
        selectedAction: selected?.action ?? null,
      },
    },
    engine_selected_closed_economy_problem: {
      type: "noul",
      instructions: {
        question: "With player trading disabled, does the engine-selected action leave a strategically important resource bottleneck that its production and maritime options do not reasonably repair?",
        selectedAction: selected?.action ?? null,
      },
    },
  };
}

function pass2Questions(options, record) {
  const actorState = record.publicState.players.find((player) => player.player === record.actor);
  const isSettlementSetup = record.phase === "SetupSettlement";
  const isFirstSettlement = isSettlementSetup && (actorState?.settlementsBuilt ?? 0) === 0;
  const stageNote = isFirstSettlement
    ? "This is the actor's first setup settlement. Judge it as an anchor that will later be complemented by a second setup settlement; do not require this one vertex to cover every resource."
    : isSettlementSetup
      ? "This is the actor's second setup settlement. Judge the completed two-settlement portfolio, including how well it complements the first settlement."
      : record.phase.startsWith("SetupRoad")
        ? "This is a setup road. Judge where the road points, the follow-up vertices it contests or unlocks, and how recoverable the route is if blocked."
        : "This is a main-game decision. Judge the candidate against the actor's current complete position.";

  return {
    best_stage_fit: choiceQuestion(
      {
        question: "Which candidate best fits the strategic job of this exact game stage?",
        stage: stageNote,
      },
      options,
    ),
    best_resource_portfolio: choiceQuestion(
      {
        question: "Which candidate produces the strongest long-horizon resource portfolio for the actor?",
        stage: stageNote,
        focus: "Value complementary production and build-cost coverage. Player trading may be unavailable, so do not assume another player repairs missing resources.",
      },
      options,
    ),
    best_next_settlement_path: choiceQuestion(
      {
        question: "Which candidate gives the actor the strongest realistic path to the next useful settlement?",
        stage: stageNote,
        focus: "Consider road count, contested vertices, fallback routes, and whether an opponent can plausibly block before arrival.",
      },
      options,
    ),
    best_city_transition: choiceQuestion(
      "Which candidate best supports a future city transition without creating a severe grain or ore bottleneck?",
      options,
    ),
    best_development_transition: choiceQuestion(
      "Which candidate best preserves the option to buy development cards while still funding core expansion?",
      options,
    ),
    best_road_resilience: choiceQuestion(
      "Which candidate creates the most resilient road/expansion structure after one strong plausible opponent blocking move?",
      options,
    ),
    best_block_recovery: choiceQuestion(
      "Assume the strongest plausible opponent blocking response consistent with the supplied hidden-hand affordability beliefs. Which candidate leaves the best recovery path?",
      options,
    ),
    best_hidden_hand_safety: choiceQuestion(
      "Using only the supplied opponent affordability belief probabilities, which candidate is least exposed to a plausible sudden road or road-plus-settlement response?",
      options,
    ),
    best_opponent_denial: choiceQuestion(
      "Which candidate creates the most useful denial pressure on opponents without sacrificing the actor's own economy or route flexibility?",
      options,
    ),
    best_optionality: choiceQuestion(
      "Which candidate preserves the widest set of strong follow-up plans instead of committing the actor to one brittle line?",
      options,
    ),
    best_two_build_sequence: choiceQuestion(
      "Which candidate best supports a coherent two-build sequence where the first follow-up makes the second follow-up easier, safer, or more productive?",
      options,
    ),
    best_resource_conversion: choiceQuestion(
      "Which candidate best converts expected production into the actual road, settlement, city, and development-card costs needed, without assuming player trades?",
      options,
    ),
    best_strategic_information: choiceQuestion(
      "Which candidate keeps the actor least prematurely committed before observing opponents' next road and settlement choices?",
      options,
    ),
    best_long_horizon_foundation: choiceQuestion(
      {
        question: "Which candidate is the strongest long-horizon foundation for the actor?",
        stage: stageNote,
        focus: "Prefer robust compounding plans over locally attractive but strategically brittle gains.",
      },
      options,
    ),
  };
}

function directQuestion(options, record, wordingVariant) {
  const actorState = record.publicState.players.find((player) => player.player === record.actor);
  const isSetupSettlement = record.phase === "SetupSettlement";
  const isFirstSettlement = isSetupSettlement && (actorState?.settlementsBuilt ?? 0) === 0;
  const stage = isFirstSettlement
    ? "first setup settlement; a second setup settlement will later complement it"
    : isSetupSettlement
      ? "second setup settlement; this completes the actor's opening production portfolio"
      : record.phase.startsWith("SetupRoad")
        ? "setup road; evaluate its direction and reachable frontier"
        : "main-game decision";
  return {
    direct_best_candidate: choiceQuestion(
      {
        question: wordingVariant === "alternate"
          ? "Considering the full strategic position, which one candidate gives the actor the strongest choice?"
          : "Which single candidate is strategically best for the actor?",
        stage,
        evidenceRule: "Use supplied exact mechanics as authoritative, do not infer hidden cards, and judge the candidate as a whole. Return one independent direct judgment; do not decompose or average named dimensions.",
      },
      options,
    ),
  };
}

const CANDIDATE_SCORE_LEVELS = [
  "Actively harmful or strategically dead-ended on this dimension",
  "Weak: materially compromised with no convincing compensation on this dimension",
  "Viable but mixed: meaningful strengths and weaknesses",
  "Strong: clearly favorable with manageable weaknesses",
  "Exceptional: robust, flexible, and unusually strong on this dimension",
];

function pass3Questions(options, record, wordingVariant = "standard") {
  const actorState = record.publicState.players.find((player) => player.player === record.actor);
  const isSetupSettlement = record.phase === "SetupSettlement";
  const isFirstSettlement = isSetupSettlement && (actorState?.settlementsBuilt ?? 0) === 0;
  const isSetupRoad = record.phase.startsWith("SetupRoad");

  const dimensions = isFirstSettlement
    ? [
        ["anchor_production", "How strong is this first settlement as a production anchor?", "Use the precomputed pip totals and per-resource pips. High diversity does not compensate for very weak total production by itself."],
        ["build_resource_foundation", "How strong is this first settlement as a foundation for funding roads, settlements, cities, and development cards?", "Judge the resources it produces relative to build costs while remembering a second setup settlement will be added later."],
        ["complementability", "How easy will it be to complement this first settlement with a strategically strong second setup settlement?", "Reward anchors whose missing resources can plausibly be completed by the remaining candidate landscape; do not demand five-resource coverage from the first pick alone."],
        ["second_pick_flexibility", "How much flexibility does this first settlement preserve for the actor's second setup pick?", "Consider whether the anchor overcommits the portfolio or leaves several coherent resource/number combinations available."],
        ["road_direction_potential", "How promising are the road directions available from this settlement for future expansion?", "Judge adjacent topology and plausible settlement frontiers, not just immediate resource production."],
        ["closed_economy_potential", "How strong is this anchor for a no-player-trade economy after it is complemented by a second settlement?", "Penalize structural dependence on a hard-to-repair missing resource, but do not treat the current one-settlement state as the finished portfolio."],
        ["number_structure", "How healthy is this settlement's number structure as part of a future two-settlement portfolio?", "Use precomputed pips and number spread. Avoid treating raw number labels as arithmetic."],
        ["long_horizon_foundation", "How strong is this first settlement as a long-horizon strategic foundation?", "Prefer compounding production, complementability, route options, and resilience over cosmetic resource diversity."],
      ]
    : isSetupSettlement
      ? [
          ["completed_production", "How strong is the completed two-settlement production portfolio after choosing this candidate?", "Use projected per-resource pips and projected total pips directly."],
          ["build_cost_coverage", "How well does the completed portfolio cover the resource requirements of roads, settlements, cities, and development cards?", "Use direct projectedBuildCoverage together with projectedMaritimeRatios and maritimeRepairOptions. Do not treat a directly missing resource as a hard lock when strong 2:1 or 3:1 conversion can realistically repair it."],
          ["starting_hand", "How useful are the setup resources immediately collected from this second settlement?", "Value cards by how well they accelerate coherent first builds; a larger count is not automatically better if badly composed."],
          ["next_settlement_cadence", "How well does this completed opening support the next road-plus-settlement cycle?", "Judge lumber/brick/wool/grain access together with route options and bottlenecks."],
          ["city_dev_transition", "How well does this completed opening support cities and development cards without stalling expansion?", "Judge grain/ore/wool production in balance with the rest of the economy."],
          ["closed_economy_self_sufficiency", "How self-sufficient is this completed opening when player trading is unavailable?", "Judge direct production and maritime repair together. A productive 2:1 port can make a missing resource manageable; 4:1 repair from weak production is much less convincing."],
          ["number_resilience", "How resilient is the completed opening's number portfolio?", "Reward useful spread and production strength; penalize excessive duplicated exposure when it creates correlated drought risk."],
          ["long_horizon_portfolio", "How strong is the completed opening portfolio for long-horizon play?", "Integrate production, build coverage, immediate setup cards, expansion flexibility, and no-trade resilience."],
        ]
      : isSetupRoad
        ? [
            ["frontier_quality", "How strong is the expansion frontier created by this setup road?", "Use the precomputed settlement sites reachable after one additional road and their pip/resource/port quality."],
            ["best_target_quality", "How valuable is the best realistic settlement target this road points toward?", "Use the provided frontier site facts rather than inferring dice arithmetic."],
            ["route_branching", "How much useful route optionality does this road preserve?", "Reward multiple genuinely useful continuations; do not reward branches that lead only to weak or distance-illegal sites."],
            ["block_resilience", "How resilient is this road direction to one strong opponent blocking placement?", "Prefer routes with credible fallback settlement sites or alternate continuations."],
            ["opponent_denial", "How much useful opponent-denial pressure does this road create?", "Reward contested strategic territory only when denial does not sacrifice the actor's own development."],
            ["economic_fit", "How well does this road direction fit the actor's current and projected resource economy?", "A road toward an unreachable or low-value target is weak even if locally legal."],
            ["commitment_flexibility", "How well does this road avoid premature commitment to one brittle plan?", "Prefer roads that preserve strategic information and multiple viable follow-ups."],
            ["long_horizon_route", "How strong is this setup road as a long-horizon route choice?", "Integrate target quality, branching, resilience, denial value, and economic fit."],
          ]
        : [
            ["immediate_value", "How strong is this candidate's immediate strategic value?", "Judge only meaningful game progress, not superficial activity."],
            ["resource_efficiency", "How efficiently does this candidate use the actor's current economy?", "Prefer actions that convert scarce resources into compounding value."],
            ["future_optionality", "How much useful future optionality does this candidate preserve?", "Penalize unnecessary commitment or dead-end spending."],
            ["opponent_pressure", "How well does this candidate handle or create relevant opponent pressure?", "Use supplied belief probabilities rather than assuming hidden cards."],
            ["block_resilience", "How resilient is this candidate to plausible opponent responses?", "Reward credible recovery paths."],
            ["city_dev_balance", "How well does this candidate preserve city/development-card plans alongside expansion?", "Avoid evaluating one plan in isolation. For BuyDevelopment, use informationSetDrawBelief as the authoritative actor-safe distribution over possible next-card types instead of treating the draw as unspecified or assuming an exact hidden deck."],
            ["tempo_compounding", "How well does this candidate compound tempo over the next several builds?", "Prefer actions whose follow-ups become easier or more productive."],
            ["long_horizon_value", "How strong is this candidate for long-horizon strategy?", "Integrate the supplied exact facts without redoing arithmetic."],
          ];

  const stage = isFirstSettlement
    ? "first setup settlement; a second setup settlement will later complement it"
    : isSetupSettlement
      ? "second setup settlement; this completes the actor's opening production portfolio"
      : isSetupRoad
        ? "setup road; evaluate its direction and reachable frontier"
        : "main-game decision";

  const questions = {};
  for (const key of Object.keys(options)) {
    for (const [dimension, question, focus] of dimensions) {
      questions[`${key}__${dimension}`] = {
        type: "score",
        instructions: {
          question: wordingVariant === "alternate"
            ? `Considering only this dimension, rate the candidate: ${question}`
            : question,
          candidatePath: `candidates.${key}`,
          stage,
          focus,
          evidenceRule: "Use supplied derived numeric/topology facts as authoritative. mechanicalStrategicEvidence contains precomputed mechanics/belief facts, not engine utility or the engine-selected answer; use roadIntent target/ETA/survival and fragility/cut evidence when relevant. Do not recompute dice probabilities or invent hidden cards. A domestic OfferTrade is only a proposal: use mechanicalPlanEvidence.completionMass and opponentResponseWindows when supplied, and never treat requested cards as guaranteed. completionMass is bounded belief-weighted planner evidence, not an engine utility score. Judge only this one dimension.",
        },
        criteria: CANDIDATE_SCORE_LEVELS,
      };
    }
  }
  return questions;
}

function aggregateScores(answers, options) {
  const buckets = Object.fromEntries(
    Object.keys(options).map((key) => [
      key,
      { total: 0, confidence: 0, confidenceWeightedTotal: 0, confidenceWeight: 0, count: 0 },
    ]),
  );
  for (const [questionId, answer] of Object.entries(answers)) {
    if (answer?.type !== "score") continue;
    const split = questionId.indexOf("__");
    if (split < 0) continue;
    const key = questionId.slice(0, split);
    const bucket = buckets[key];
    if (!bucket) continue;
    const confidence = Math.max(0.05, Math.min(1, Number(answer.confidence ?? 0)));
    bucket.total += answer.score;
    bucket.confidence += confidence;
    bucket.confidenceWeightedTotal += answer.score * confidence;
    bucket.confidenceWeight += confidence;
    bucket.count += 1;
  }
  return Object.entries(buckets)
    .map(([option, bucket]) => {
      const meanScore = bucket.count ? bucket.total / bucket.count : 0;
      const confidenceWeightedScore = bucket.confidenceWeight
        ? bucket.confidenceWeightedTotal / bucket.confidenceWeight
        : meanScore;
      return {
        option,
        action: options[option].action,
        meanScore,
        meanNormalizedScore: meanScore / 4,
        confidenceWeightedScore,
        confidenceWeightedNormalizedScore: confidenceWeightedScore / 4,
        meanConfidence: bucket.count ? bucket.confidence / bucket.count : 0,
        dimensions: bucket.count,
      };
    })
    .sort((a, b) =>
      b.meanScore - a.meanScore
      || b.confidenceWeightedScore - a.confidenceWeightedScore
      || b.meanConfidence - a.meanConfidence
    );
}

function roadIntentMechanicallyDominated(candidate, competitor) {
  const candidateIntent = candidate?.mechanicalStrategicEvidence?.roadIntent;
  const competitorIntent = competitor?.mechanicalStrategicEvidence?.roadIntent;
  if (!candidateIntent || !competitorIntent) return false;
  if (candidateIntent.targetVertex === null || competitorIntent.targetVertex === null) return false;

  const noWorse = [
    competitorIntent.targetValue >= candidateIntent.targetValue,
    competitorIntent.survivalProbability >= candidateIntent.survivalProbability,
    competitorIntent.roadsRemaining <= candidateIntent.roadsRemaining,
    competitorIntent.expectedRolls <= candidateIntent.expectedRolls,
  ];
  const materiallyBetter =
    competitorIntent.targetValue > candidateIntent.targetValue + 0.25
    || competitorIntent.survivalProbability > candidateIntent.survivalProbability + 0.08
    || competitorIntent.roadsRemaining < candidateIntent.roadsRemaining
    || competitorIntent.expectedRolls + 1 < candidateIntent.expectedRolls;

  return noWorse.every(Boolean) && materiallyBetter;
}

function assessJevSignal(ranking, options, engineSelected) {
  const top = ranking[0] ?? null;
  const engine = ranking.find((entry) => entry.option === engineSelected) ?? null;
  if (!top || !engine) {
    return { status: "insufficient", counterfactualCandidate: false, reasons: ["missing ranking"] };
  }
  if (top.option === engineSelected) {
    return { status: "agreement", counterfactualCandidate: false, reasons: [] };
  }

  const margin = (top.meanScore - engine.meanScore) / 4;
  const reasons = [];
  if (roadIntentMechanicallyDominated(options[top.option], options[engineSelected])) {
    reasons.push("jev road is mechanically dominated by the engine road");
  }
  if (margin < 0.05) reasons.push("confidence-weighted normalized score margin is below 0.05");
  if (top.meanConfidence < 0.35 && margin < 0.10) {
    reasons.push("low Jev confidence without a large score margin");
  }

  return {
    status: reasons.length ? "screened_out" : "counterfactual_candidate",
    counterfactualCandidate: reasons.length === 0,
    normalizedScoreMargin: margin,
    confidenceWeightedNormalizedScoreMargin:
      (top.confidenceWeightedScore - engine.confidenceWeightedScore) / 4,
    topMeanConfidence: top.meanConfidence,
    reasons,
  };
}

function aggregateChoices(answers, options) {
  const scores = Object.fromEntries(Object.keys(options).map((key) => [key, 0]));
  const counts = Object.fromEntries(Object.keys(options).map((key) => [key, 0]));
  const choiceAnswers = Object.entries(answers).filter(([, answer]) => answer?.type === "choice");
  for (const [, answer] of choiceAnswers) {
    for (const [key, probability] of Object.entries(answer.probabilities ?? {})) {
      if (scores[key] === undefined) continue;
      scores[key] += probability;
      counts[key] += 1;
    }
  }
  return Object.keys(options)
    .map((key) => ({
      option: key,
      action: options[key].action,
      meanProbability: counts[key] ? scores[key] / counts[key] : 0,
      dimensions: counts[key],
    }))
    .sort((a, b) => b.meanProbability - a.meanProbability);
}

async function queryJev(apiKey, state, questions, cacheDirectory) {
  const request = { state, model: MODEL, questions };
  const cacheKey = jevRequestCacheKey(request);
  const cached = readCachedJevResponse(cacheDirectory, cacheKey);
  if (cached) {
    return { response: cached.record.response, cacheKey, cacheHit: true };
  }
  if (!apiKey) {
    throw new Error(
      `TYPESAFE_API_KEY is required for uncached Jev request ${cacheKey}; cached requests can be replayed offline`,
    );
  }
  const response = await fetch(API_URL, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${apiKey}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(request),
    signal: AbortSignal.timeout(15_000),
  });
  const body = await response.text();
  if (!response.ok) throw new Error(`TypeSafe HTTP ${response.status}: ${body.slice(0, 500)}`);
  const parsed = JSON.parse(body);
  writeCachedJevResponse(cacheDirectory, cacheKey, request, parsed);
  return { response: parsed, cacheKey, cacheHit: false };
}

function selectRecords(records, config) {
  const decisions = records.filter((record) =>
    record.recordType === "decision" &&
    (config.game === null || record.game === config.game)
  );
  const setup = decisions.filter((record) =>
    record.phase === "SetupSettlement" || record.phase.startsWith("SetupRoad")
  );
  const spatialMain = decisions.filter((record) =>
    record.phase === "Main" &&
    /BuildRoad|BuildSettlement|BuildCity|BuyDevelopment|PlayRoadBuilding|EndTurn/.test(record.chosenAction)
  );
  if (config.stage === "setup") return setup.slice(0, config.limit);
  if (config.stage === "main") return spatialMain.slice(0, config.limit);
  return [...setup, ...spatialMain.slice(0, Math.max(0, config.limit - setup.length))]
    .slice(0, config.limit);
}

async function main() {
  const config = parseArgs(process.argv.slice(2));
  const apiKey = process.env.TYPESAFE_API_KEY;

  const records = readJsonl(config.input);
  const selected = selectRecords(records, config);
  fs.mkdirSync(path.dirname(config.output), { recursive: true });
  const writer = fs.createWriteStream(config.output, { flags: "w" });

  let index = 0;
  for (const record of selected) {
    const blind = config.pass !== "pass1";
    const options = candidateOptions(record, 12, blind, config.candidateOrder);
    const engineSelected = selectedOption(record, options);
    if (!engineSelected || Object.keys(options).length < 2) continue;

    const questions = config.pass === "direct"
      ? directQuestion(options, record, config.wordingVariant)
      : ["pass3", "pass4", "pass5"].includes(config.pass)
        ? pass3Questions(options, record, config.wordingVariant)
        : blind
          ? pass2Questions(options, record)
          : pass1Questions(options, engineSelected);
    const state = {
      experiment: blind
        ? {
            pass: config.pass,
            purpose: "Blind local strategic evaluation of one Catan decision. Candidate order is neutral and engine scores/selection are withheld.",
          }
        : {
            pass: config.pass,
            purpose: "Independent local strategic critique of one Catan decision. Exact rules/math are supplied by code; judge bounded strategic consequences.",
            engineSelectedOption: engineSelected,
            engineSelectedAction: options[engineSelected].action,
          },
      ...compactBoard(record),
      candidates: options,
    };

    const query = await queryJev(apiKey, state, questions, config.cacheDir);
    const response = query.response;
    const atomicScorePass = ["pass3", "pass4", "pass5"].includes(config.pass);
    const ranking = atomicScorePass
      ? aggregateScores(response.answers, options)
      : aggregateChoices(response.answers, options);
    const result = {
      recordType: "jevEvaluation",
      schemaVersion: 1,
      pass: config.pass,
      candidateOrder: Object.values(options).map((option) => option.action),
      candidateOrderVariant: config.candidateOrder,
      wordingVariant: config.wordingVariant,
      source: {
        game: record.game,
        boardSeed: record.boardSeed,
        chanceSeed: record.chanceSeed,
        decisionIndex: record.decisionIndex,
        stateHash: record.stateHash,
        turn: record.turn,
        phase: record.phase,
        actor: record.actor,
        playerTradesEnabled: record.publicState.playerTradesEnabled,
      },
      engineSelected,
      engineSelectedAction: options[engineSelected].action,
      jevConsensus: ranking[0] ?? null,
      disagreement: ranking[0]?.option !== engineSelected,
      signalAssessment: atomicScorePass
        ? assessJevSignal(ranking, options, engineSelected)
        : null,
      aggregateRanking: ranking,
      answers: response.answers,
      model: response.model,
      usage: response.usage,
      rawCacheKey: query.cacheKey,
      rawCacheHit: query.cacheHit,
      candidates: options,
    };
    writer.write(JSON.stringify(result) + "\n");
    index += 1;
    console.log(
      `[${index}/${selected.length}] game=${record.game} ${record.phase} actor=${record.actor} engine=${engineSelected} jev=${ranking[0]?.option ?? "n/a"} disagree=${result.disagreement} signal=${result.signalAssessment?.status ?? "legacy"}`,
    );
  }

  await new Promise((resolve, reject) => {
    writer.end(resolve);
    writer.on("error", reject);
  });
}

main().catch((error) => {
  console.error(error?.stack ?? String(error));
  process.exitCode = 1;
});
