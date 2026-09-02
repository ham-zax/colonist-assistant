import {
  RESOURCE_ORDER,
  type Resource,
} from "../core/resources";
import type {
  ActiveTradeOffer,
  BoardAction,
  BoardEdge,
  BoardHex,
  BoardSnapshot,
  BoardVertex,
  DevelopmentCardVector,
} from "../core/placement";

const BRIDGE_SOURCE = "colonist-assistant-public-board";
const BOARD_ACTIONS: BoardAction[] = [
  "settlement",
  "city",
  "road",
  "robber",
  "discard",
  "none",
];
let bridgedSnapshot: BoardSnapshot | undefined;

const canonicalPlayer = (
  player: string | undefined,
  myPlayer: string | undefined,
): string | undefined =>
  player === "You" && myPlayer && myPlayer !== "You"
    ? myPlayer
    : player;

/**
 * Colonist uses both the literal label "You" and the account username in
 * different public surfaces. Normalize the complete snapshot at the bridge
 * boundary so the tracker, board graph, trades, and executor never create a
 * second local player.
 */
export const canonicalizeBoardPlayerAliases = (
  snapshot: BoardSnapshot,
): BoardSnapshot => {
  const myPlayer = canonicalPlayer(snapshot.myPlayer, snapshot.myPlayer);
  if (!myPlayer || myPlayer === "You") return snapshot;
  const player = (value: string): string =>
    canonicalPlayer(value, myPlayer) ?? value;
  const players = snapshot.players
    ? Object.fromEntries(
        Object.entries(snapshot.players).map(([name, publicState]) => [
          player(name),
          publicState,
        ]),
      )
    : undefined;
  return {
    ...snapshot,
    myPlayer,
    ...(players ? { players } : {}),
    ...(snapshot.playerOrder
      ? {
          playerOrder: snapshot.playerOrder
            .map(player)
            .filter((name, index, all) => all.indexOf(name) === index),
        }
      : {}),
    ...(snapshot.currentPlayer
      ? { currentPlayer: player(snapshot.currentPlayer) }
      : {}),
    vertices: snapshot.vertices.map((vertex) =>
      vertex.building
        ? {
            ...vertex,
            building: {
              ...vertex.building,
              player: player(vertex.building.player),
            },
          }
        : vertex,
    ),
    edges: snapshot.edges.map((edge) =>
      edge.player ? { ...edge, player: player(edge.player) } : edge,
    ),
    ...(snapshot.robberVictimPlayers
      ? { robberVictimPlayers: snapshot.robberVictimPlayers.map(player) }
      : {}),
    ...(snapshot.activeTrades
      ? {
          activeTrades: snapshot.activeTrades.map((trade) => ({
            ...trade,
            creator: player(trade.creator),
            tradeExecutor: player(trade.tradeExecutor),
            ...(trade.acceptedPlayers
              ? { acceptedPlayers: trade.acceptedPlayers.map(player) }
              : {}),
            ...(trade.pendingPlayers
              ? { pendingPlayers: trade.pendingPlayers.map(player) }
              : {}),
            ...(trade.rejectedPlayers
              ? { rejectedPlayers: trade.rejectedPlayers.map(player) }
              : {}),
            ...(trade.embargoedPlayers
              ? { embargoedPlayers: trade.embargoedPlayers.map(player) }
              : {}),
          })),
        }
      : {}),
  };
};

const isResource = (value: unknown): value is Resource =>
  typeof value === "string" && RESOURCE_ORDER.includes(value as Resource);

const validPoint = (value: unknown): boolean =>
  Boolean(
    value &&
      typeof value === "object" &&
      Number.isFinite((value as { x?: unknown }).x) &&
      Number.isFinite((value as { y?: unknown }).y) &&
      Math.abs((value as { x: number }).x) <= 100_000 &&
      Math.abs((value as { y: number }).y) <= 100_000,
  );

const stringList = (value?: string): string[] =>
  (value ?? "")
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);

const validResourceVector = (value: unknown): boolean =>
  Boolean(
    value &&
      typeof value === "object" &&
      RESOURCE_ORDER.every(
        (resource) =>
          Number.isInteger((value as Record<string, unknown>)[resource]) &&
          Number((value as Record<string, unknown>)[resource]) >= 0 &&
          Number((value as Record<string, unknown>)[resource]) <= 100,
      ),
  );

const DEVELOPMENT_CARD_NAMES = [
  "knight",
  "monopoly",
  "road-building",
  "year-of-plenty",
  "victory-point",
] as const;

const validDevelopmentCardVector = (
  value: unknown,
): value is DevelopmentCardVector =>
  Boolean(
    value &&
      typeof value === "object" &&
      DEVELOPMENT_CARD_NAMES.every(
        (card) =>
          Number.isInteger((value as Record<string, unknown>)[card]) &&
          Number((value as Record<string, unknown>)[card]) >= 0 &&
          Number((value as Record<string, unknown>)[card]) <= 25,
      ),
  );

const validActiveTrade = (value: unknown): value is ActiveTradeOffer => {
  if (!value || typeof value !== "object") return false;
  const trade = value as Partial<ActiveTradeOffer>;
  return Boolean(
    typeof trade.id === "string" &&
      trade.id.length > 0 &&
      trade.id.length <= 64 &&
      typeof trade.creator === "string" &&
      trade.creator.length <= 160 &&
      typeof trade.tradeExecutor === "string" &&
      trade.tradeExecutor.length <= 160 &&
      validResourceVector(trade.creatorGive) &&
      validResourceVector(trade.creatorReceive) &&
      typeof trade.incoming === "boolean" &&
      typeof trade.counterOffer === "boolean" &&
      (
        trade.counterOfferInResponseToTradeId === undefined ||
        (
          typeof trade.counterOfferInResponseToTradeId === "string" &&
          trade.counterOfferInResponseToTradeId.length > 0 &&
          trade.counterOfferInResponseToTradeId.length <= 64
        )
      ) &&
      typeof trade.canAccept === "boolean" &&
      (
        trade.creatorGiveOpenEnded === undefined ||
        typeof trade.creatorGiveOpenEnded === "boolean"
      ) &&
      (
        trade.creatorReceiveOpenEnded === undefined ||
        typeof trade.creatorReceiveOpenEnded === "boolean"
      ) &&
      (
        trade.acceptedPlayers === undefined ||
        (
          Array.isArray(trade.acceptedPlayers) &&
          trade.acceptedPlayers.every((player) => typeof player === "string")
        )
      ) &&
      (
        trade.pendingPlayers === undefined ||
        (
          Array.isArray(trade.pendingPlayers) &&
          trade.pendingPlayers.every((player) => typeof player === "string")
        )
      ) &&
      (
        trade.rejectedPlayers === undefined ||
        (
          Array.isArray(trade.rejectedPlayers) &&
          trade.rejectedPlayers.every((player) => typeof player === "string")
        )
      ) &&
      (
        trade.embargoedPlayers === undefined ||
        (
          Array.isArray(trade.embargoedPlayers) &&
          trade.embargoedPlayers.every((player) => typeof player === "string")
        )
      ) &&
      (
        trade.responsesComplete === undefined ||
        typeof trade.responsesComplete === "boolean"
      ) &&
      (
        trade.myResponse === undefined ||
        ["pending", "accepted", "rejected", "embargoed"].includes(
          trade.myResponse,
        )
      ),
  );
};

const validColonistAsset = (value: unknown): boolean =>
  typeof value === "string" &&
  /^https:\/\/(?:cdn\.)?colonist\.io\/dist\/assets\/[a-z0-9_.-]+\.(?:svg|png)$/iu.test(
    value,
  );

const optionalBoundedInteger = (
  value: unknown,
  minimum: number,
  maximum: number,
): boolean =>
  value === undefined ||
  (Number.isInteger(value) &&
    Number(value) >= minimum &&
    Number(value) <= maximum);

const validSnapshot = (value: unknown): value is BoardSnapshot => {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<BoardSnapshot>;
  const validShape =
    Array.isArray(candidate.hexes) &&
    candidate.hexes.length <= 200 &&
    Array.isArray(candidate.vertices) &&
    candidate.vertices.length <= 500 &&
    Array.isArray(candidate.edges) &&
    candidate.edges.length <= 1_000 &&
    candidate.hexes.every(
      (hex) =>
        hex &&
        typeof hex.id === "string" &&
        (hex.resource === undefined || isResource(hex.resource)) &&
        (hex.number === undefined || Number.isFinite(hex.number)) &&
        (hex.screen === undefined || validPoint(hex.screen)),
    ) &&
    candidate.vertices.every(
      (vertex) =>
        vertex &&
        typeof vertex.id === "string" &&
        Array.isArray(vertex.adjacentHexes) &&
        vertex.adjacentHexes.every((id) => typeof id === "string") &&
        Array.isArray(vertex.adjacentVertices) &&
        vertex.adjacentVertices.every((id) => typeof id === "string") &&
        (vertex.port === undefined ||
          vertex.port === "generic" ||
          isResource(vertex.port)) &&
        (vertex.building === undefined ||
          (typeof vertex.building.player === "string" &&
            (vertex.building.kind === "settlement" ||
              vertex.building.kind === "city"))) &&
        (vertex.screen === undefined || validPoint(vertex.screen)),
    ) &&
    candidate.edges.every(
      (edge) =>
        edge &&
        typeof edge.id === "string" &&
        Array.isArray(edge.vertices) &&
        edge.vertices.length === 2 &&
        edge.vertices.every((id) => typeof id === "string") &&
        (edge.player === undefined || typeof edge.player === "string") &&
        (edge.screen === undefined || validPoint(edge.screen)),
    ) &&
    (candidate.legalVertexIds === undefined ||
      (Array.isArray(candidate.legalVertexIds) &&
        candidate.legalVertexIds.every((id) => typeof id === "string"))) &&
    (candidate.legalEdgeIds === undefined ||
      (Array.isArray(candidate.legalEdgeIds) &&
        candidate.legalEdgeIds.every((id) => typeof id === "string"))) &&
    (candidate.buildableSettlementIds === undefined ||
      (Array.isArray(candidate.buildableSettlementIds) &&
        candidate.buildableSettlementIds.every((id) => typeof id === "string"))) &&
    (candidate.buildableCityIds === undefined ||
      (Array.isArray(candidate.buildableCityIds) &&
        candidate.buildableCityIds.every((id) => typeof id === "string"))) &&
    (candidate.buildableRoadIds === undefined ||
      (Array.isArray(candidate.buildableRoadIds) &&
        candidate.buildableRoadIds.every((id) => typeof id === "string"))) &&
    (candidate.myPlayer === undefined || typeof candidate.myPlayer === "string") &&
    (candidate.ownHand === undefined || validResourceVector(candidate.ownHand)) &&
    (candidate.ownDevelopmentCards === undefined ||
      (validDevelopmentCardVector(candidate.ownDevelopmentCards.cards) &&
        validDevelopmentCardVector(candidate.ownDevelopmentCards.playable) &&
        validDevelopmentCardVector(
          candidate.ownDevelopmentCards.boughtThisTurn,
        ) &&
        typeof candidate.ownDevelopmentCards.hasPlayedThisTurn ===
          "boolean")) &&
    (candidate.activeTrades === undefined ||
      (Array.isArray(candidate.activeTrades) &&
        candidate.activeTrades.length <= 40 &&
        candidate.activeTrades.every(validActiveTrade))) &&
    (candidate.bank === undefined || validResourceVector(candidate.bank)) &&
    (candidate.bankVisible === undefined || typeof candidate.bankVisible === "boolean") &&
    (candidate.players === undefined ||
      (candidate.players &&
        typeof candidate.players === "object" &&
        Object.entries(candidate.players).length <= 12 &&
        Object.entries(candidate.players).every(
          ([name, player]) =>
            name.length > 0 &&
            name.length <= 160 &&
            Number.isInteger(player.handSize) &&
            player.handSize >= 0 &&
            player.handSize <= 200 &&
            validResourceVector(player.tradeRatios) &&
            Number.isInteger(player.cardDiscardLimit) &&
            player.cardDiscardLimit >= 0 &&
            player.cardDiscardLimit <= 200 &&
            optionalBoundedInteger(player.developmentCards, 0, 100) &&
            (player.playedDevelopmentCards === undefined ||
              validDevelopmentCardVector(player.playedDevelopmentCards)) &&
            (player.hasPlayedDevelopmentThisTurn === undefined ||
              typeof player.hasPlayedDevelopmentThisTurn === "boolean") &&
            optionalBoundedInteger(player.visiblePoints, 0, 100) &&
            optionalBoundedInteger(player.longestRoad, 0, 100) &&
            (player.hasLongestRoad === undefined ||
              typeof player.hasLongestRoad === "boolean") &&
            (player.hasLargestArmy === undefined ||
              typeof player.hasLargestArmy === "boolean"),
        ))) &&
    (candidate.playerOrder === undefined ||
      (Array.isArray(candidate.playerOrder) &&
        candidate.playerOrder.length <= 12 &&
        candidate.playerOrder.every(
          (player) =>
            typeof player === "string" &&
            player.length > 0 &&
            player.length <= 160,
        ))) &&
    (candidate.assets === undefined ||
      (candidate.assets &&
        typeof candidate.assets === "object" &&
        Object.values(candidate.assets.resources ?? {}).every(validColonistAsset) &&
        Object.values(candidate.assets.pieces ?? {}).every(validColonistAsset))) &&
    (candidate.discardCount === undefined ||
      (Number.isInteger(candidate.discardCount) &&
        candidate.discardCount >= 1 &&
        candidate.discardCount <= 100)) &&
    (candidate.robberVictimSelection === undefined ||
      typeof candidate.robberVictimSelection === "boolean") &&
    (candidate.robberVictimPlayers === undefined ||
      (Array.isArray(candidate.robberVictimPlayers) &&
        candidate.robberVictimPlayers.length <= 12 &&
        candidate.robberVictimPlayers.every(
          (player) =>
            typeof player === "string" &&
            player.length > 0 &&
            player.length <= 160,
        ))) &&
    (candidate.gameKey === undefined ||
      (typeof candidate.gameKey === "string" && candidate.gameKey.length <= 500)) &&
    (candidate.isMyTurn === undefined || typeof candidate.isMyTurn === "boolean") &&
    (candidate.action === undefined ||
      BOARD_ACTIONS.includes(candidate.action as BoardAction)) &&
    (candidate.initialPlacement === undefined ||
      typeof candidate.initialPlacement === "boolean") &&
    (candidate.picksUntilNext === undefined ||
      (Number.isInteger(candidate.picksUntilNext) && candidate.picksUntilNext >= 0)) &&
    optionalBoundedInteger(candidate.victoryTarget, 3, 100) &&
    (candidate.friendlyRobber === undefined ||
      typeof candidate.friendlyRobber === "boolean") &&
    (candidate.privateGame === undefined ||
      typeof candidate.privateGame === "boolean") &&
    (candidate.botOnlyGame === undefined ||
      typeof candidate.botOnlyGame === "boolean") &&
    (candidate.currentPlayer === undefined ||
      typeof candidate.currentPlayer === "string") &&
    optionalBoundedInteger(candidate.turn, 0, 100_000) &&
    optionalBoundedInteger(candidate.lastRoll, 0, 12) &&
    (candidate.hasRolled === undefined ||
      typeof candidate.hasRolled === "boolean") &&
    (candidate.domesticTradeUsed === undefined ||
      typeof candidate.domesticTradeUsed === "boolean") &&
    (candidate.gameOver === undefined ||
      typeof candidate.gameOver === "boolean") &&
    (candidate.winner === undefined ||
      (typeof candidate.winner === "string" &&
        candidate.winner.length <= 160));
  if (!validShape) return false;

  const snapshot = candidate as BoardSnapshot;
  const hexIds = new Set(snapshot.hexes.map((hex) => hex.id));
  const vertexIds = new Set(snapshot.vertices.map((vertex) => vertex.id));
  const edgeIds = new Set(snapshot.edges.map((edge) => edge.id));
  if (
    hexIds.size !== snapshot.hexes.length ||
    vertexIds.size !== snapshot.vertices.length ||
    edgeIds.size !== snapshot.edges.length
  ) {
    return false;
  }
  return (
    snapshot.vertices.every(
      (vertex) =>
        vertex.adjacentHexes.every((id) => hexIds.has(id)) &&
        vertex.adjacentVertices.every((id) => vertexIds.has(id)),
    ) &&
    snapshot.edges.every((edge) => edge.vertices.every((id) => vertexIds.has(id))) &&
    (snapshot.legalVertexIds?.every((id) => vertexIds.has(id)) ?? true) &&
    (snapshot.legalEdgeIds?.every((id) => edgeIds.has(id)) ?? true) &&
    (snapshot.buildableSettlementIds?.every((id) => vertexIds.has(id)) ?? true) &&
    (snapshot.buildableCityIds?.every((id) => vertexIds.has(id)) ?? true) &&
    (snapshot.buildableRoadIds?.every((id) => edgeIds.has(id)) ?? true)
  );
};

export const parsePublicBoardMessage = (
  value: unknown,
): BoardSnapshot | "clear" | undefined => {
  if (!value || typeof value !== "object") return undefined;
  const message = value as {
    source?: unknown;
    type?: unknown;
    payload?: unknown;
  };
  if (message.source !== BRIDGE_SOURCE) return undefined;
  if (message.type === "clear") return "clear";
  if (message.type !== "snapshot" || !validSnapshot(message.payload)) return undefined;
  return canonicalizeBoardPlayerAliases({
    ...message.payload,
    observedAt: Date.now(),
  });
};

export const installPublicBoardBridge = (
  onChange: (snapshot?: BoardSnapshot) => void,
): (() => void) => {
  const handler = (event: MessageEvent<unknown>) => {
    if (event.source !== window) return;
    const parsed = parsePublicBoardMessage(event.data);
    if (!parsed) return;
    bridgedSnapshot = parsed === "clear" ? undefined : parsed;
    onChange(bridgedSnapshot);
  };
  window.addEventListener("message", handler);

  if (!document.querySelector("script[data-colonist-assistant-board-bridge]")) {
    const script = document.createElement("script");
    script.src = chrome.runtime.getURL("page-bridge.js");
    script.dataset.colonistAssistantBoardBridge = "true";
    script.addEventListener("load", () => script.remove(), { once: true });
    (document.head ?? document.documentElement).append(script);
  }

  return () => {
    window.removeEventListener("message", handler);
    bridgedSnapshot = undefined;
  };
};

const readJsonSnapshot = (root: ParentNode): BoardSnapshot | undefined => {
  const script = root.querySelector<HTMLScriptElement>(
    "script[type='application/json'][data-colonist-public-board]",
  );
  if (!script?.textContent) return undefined;
  try {
    const parsed: unknown = JSON.parse(script.textContent);
    return validSnapshot(parsed)
      ? canonicalizeBoardPlayerAliases(parsed)
      : undefined;
  } catch {
    return undefined;
  }
};

const readSemanticSnapshot = (root: ParentNode): BoardSnapshot | undefined => {
  const hexElements = [...root.querySelectorAll<HTMLElement>("[data-hex-id]")];
  const vertexElements = [...root.querySelectorAll<HTMLElement>("[data-vertex-id]")];
  const edgeElements = [...root.querySelectorAll<HTMLElement>("[data-edge-id]")];
  if (!hexElements.length || !vertexElements.length) return undefined;

  const hexes: BoardHex[] = hexElements.map((element) => {
    const resource = element.dataset.resource;
    const number = Number(element.dataset.number);
    return {
      id: element.dataset.hexId!,
      ...(isResource(resource) ? { resource } : {}),
      ...(Number.isFinite(number) && number >= 2 && number <= 12 ? { number } : {}),
      ...(element.dataset.blocked === "true" ? { blocked: true } : {}),
    };
  });
  const vertices: BoardVertex[] = vertexElements.map((element) => {
    const building = element.dataset.building;
    const player = element.dataset.player;
    const port = element.dataset.port;
    return {
      id: element.dataset.vertexId!,
      label: element.getAttribute("aria-label") ?? element.dataset.label ?? element.dataset.vertexId!,
      adjacentHexes: stringList(element.dataset.adjacentHexes),
      adjacentVertices: stringList(element.dataset.adjacentVertices),
      ...(port === "generic" || isResource(port) ? { port } : {}),
      ...(player && (building === "settlement" || building === "city")
        ? { building: { player, kind: building } }
        : {}),
    };
  });
  const edges: BoardEdge[] = edgeElements.flatMap((element): BoardEdge[] => {
      const vertices = stringList(element.dataset.vertices);
      if (vertices.length !== 2) return [];
      return [
        {
          id: element.dataset.edgeId!,
          label:
            element.getAttribute("aria-label") ??
            element.dataset.label ??
            element.dataset.edgeId!,
          vertices: [vertices[0]!, vertices[1]!],
          ...(element.dataset.player ? { player: element.dataset.player } : {}),
        },
      ];
    });
  const legalVertexIds = vertexElements
    .filter((element) => element.dataset.legal === "true")
    .map((element) => element.dataset.vertexId!);
  const legalEdgeIds = edgeElements
    .filter((element) => element.dataset.legal === "true")
    .map((element) => element.dataset.edgeId!);
  const snapshot: BoardSnapshot = {
    hexes,
    vertices,
    edges,
    ...(legalVertexIds.length ? { legalVertexIds } : {}),
    ...(legalEdgeIds.length ? { legalEdgeIds } : {}),
  };
  return validSnapshot(snapshot)
    ? canonicalizeBoardPlayerAliases(snapshot)
    : undefined;
};

export const readPublicBoardSnapshot = (
  root: ParentNode = document,
): BoardSnapshot | undefined =>
  bridgedSnapshot ?? readJsonSnapshot(root) ?? readSemanticSnapshot(root);
