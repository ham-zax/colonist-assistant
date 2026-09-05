import { resolveLocalBoardAction } from "../core/forced-action";
import { isTerminalGameHeading } from "../core/game-over";
import { resolveLocalIdentity } from "../core/local-identity";
import {
  openingRoadEdgeIds,
  type BoardAction,
} from "../core/placement";
import { observeColonistDiceMode } from "./dice-mode";
import {
  bumpManagerGeneration,
  readManagerGeneration,
} from "./game-generation";

(() => {
  const SOURCE = "colonist-assistant-public-board";
  const RESOURCE_BY_TILE_TYPE: Record<number, string | undefined> = {
    1: "lumber",
    2: "brick",
    3: "wool",
    4: "grain",
    5: "ore",
  };
  const PORT_BY_TYPE: Record<number, string> = {
    1: "generic",
    2: "lumber",
    3: "brick",
    4: "wool",
    5: "grain",
    6: "ore",
  };
  const ACTION_BY_STATE: Record<number, BoardAction> = {
    1: "settlement",
    2: "settlement",
    3: "road",
    4: "road",
    5: "road",
    6: "settlement",
    7: "city",
    8: "city",
    24: "robber",
    25: "robber",
    28: "discard",
    30: "road",
    31: "road",
  };
  const COLOR_ASSET_NAME: Record<number, string> = {
    1: "red",
    2: "blue",
    3: "orange",
    4: "green",
    5: "black",
    6: "bronze",
    7: "silver",
    8: "gold",
    9: "white",
    10: "purple",
    11: "mysticblue",
    12: "pink",
  };
  const DEVELOPMENT_CARD_BY_TYPE = {
    11: "knight",
    12: "victory-point",
    13: "monopoly",
    14: "road-building",
    15: "year-of-plenty",
  } as const;
  type DevelopmentCardName =
    (typeof DEVELOPMENT_CARD_BY_TYPE)[keyof typeof DEVELOPMENT_CARD_BY_TYPE];

  type WebpackRequire = {
    (id: string | number): any;
    m: Record<string, (...args: unknown[]) => unknown>;
  };

  let webpackRequire: WebpackRequire | undefined;
  let manager: Record<string, any> | undefined;
  let managerIdentity: Record<string, any> | undefined;
  let managerModuleId: string | undefined;
  let managerGeneration = readManagerGeneration(sessionStorage);
  let managerResolutionSource:
    | "cached-module"
    | "module-scan"
    | "cached-fallback"
    | undefined;
  let validatorExports: Record<string, any> | undefined;
  let previousPayload = "";
  let previousLiveProgress:
    | { completedTurns: number; placedPieces: number }
    | undefined;
  let assetModuleIds: Map<string, string> | undefined;
  const assetUrls = new Map<string, string>();

  const key = (prefix: "h" | "v" | "e", value: { x: number; y: number; z?: number }) =>
    `${prefix}:${value.x},${value.y}${value.z === undefined ? "" : `,${value.z}`}`;

  const captureRequire = (): WebpackRequire | undefined => {
    if (webpackRequire) return webpackRequire;
    const chunks = (window as any).webpackChunkkatan;
    if (!Array.isArray(chunks)) return undefined;
    chunks.push([
      [`colonist-assistant-${Date.now()}`],
      {},
      (runtime: WebpackRequire) => {
        webpackRequire = runtime;
      },
    ]);
    return webpackRequire;
  };

  const findAsset = (name: string): string | undefined => {
    if (assetUrls.has(name)) return assetUrls.get(name);
    const runtime = captureRequire();
    if (!runtime) return undefined;
    if (!assetModuleIds) {
      assetModuleIds = new Map();
      for (const [id, factory] of Object.entries(runtime.m)) {
        const source = Function.prototype.toString.call(factory);
        const match = source.match(
          /assets\/([a-z0-9_-]+)\.[a-f0-9]+\.(?:svg|png)/iu,
        );
        if (match?.[1]) assetModuleIds.set(match[1], id);
      }
    }
    const id = assetModuleIds.get(name);
    if (!id) return undefined;
    try {
      const value = runtime(id);
      const url =
        typeof value === "string"
          ? value
          : typeof value?.default === "string"
            ? value.default
            : undefined;
      if (
        url &&
        /^https:\/\/(?:cdn\.)?colonist\.io\/dist\/assets\/[a-z0-9_.-]+\.(?:svg|png)$/iu.test(
          url,
        )
      ) {
        assetUrls.set(name, url);
        return url;
      }
    } catch {
      // A lazy chunk may not be ready yet; retry on the next snapshot.
    }
    return undefined;
  };

  const managerFromModule = (
    runtime: WebpackRequire,
    id: string,
  ): Record<string, any> | undefined => {
    try {
      const exports = runtime(id);
      return Object.values(exports).find(
        (value) =>
          value &&
          typeof value === "object" &&
          (value as Record<string, unknown>).gameController &&
          (value as Record<string, unknown>).mapController,
      ) as Record<string, any> | undefined;
    } catch {
      return undefined;
    }
  };

  const acceptManager = (
    candidate: Record<string, any>,
  ): Record<string, any> => {
    if (managerIdentity && managerIdentity !== candidate) {
      managerGeneration = bumpManagerGeneration(sessionStorage, managerGeneration);
      validatorExports = undefined;
    }
    managerIdentity = candidate;
    manager = candidate;
    return candidate;
  };

  const findManager = (): Record<string, any> | undefined => {
    const runtime = captureRequire();
    if (!runtime) return undefined;
    if (managerModuleId) {
      const current = managerFromModule(runtime, managerModuleId);
      if (current) {
        managerResolutionSource = "cached-module";
        return acceptManager(current);
      }
    }
    for (const [id, factory] of Object.entries(runtime.m)) {
      const source = Function.prototype.toString.call(factory);
      if (!source.includes("initializeGameManager") || !source.includes("completeInitialization")) {
        continue;
      }
      const candidate = managerFromModule(runtime, id);
      if (!candidate) continue;
      managerModuleId = id;
      managerResolutionSource = "module-scan";
      return acceptManager(candidate);
    }
    if (manager) managerResolutionSource = "cached-fallback";
    return manager;
  };

  const findValidators = (): Record<string, any> | undefined => {
    if (validatorExports) return validatorExports;
    const runtime = captureRequire();
    if (!runtime) return undefined;
    for (const [id, factory] of Object.entries(runtime.m)) {
      const source = Function.prototype.toString.call(factory);
      if (
        !source.includes("canPlayerPlaceSettlement") ||
        !source.includes("canPlayerPlaceRoad") ||
        !source.includes("validHoverLocations")
      ) {
        continue;
      }
      try {
        const candidate = runtime(id);
        const methods = [
          "canPlayerPlaceSettlement",
          "canPlayerPlaceCity",
          "canPlayerPlaceRoad",
        ];
        if (
          methods.every((method) =>
            Object.values(candidate).some(
              (value) =>
                typeof value === "function" &&
                typeof value.prototype?.[method] === "function",
            ),
          )
        ) {
          validatorExports = candidate;
          return validatorExports;
        }
      } catch {
        // Retry after the chunk finishes loading.
      }
    }
    return undefined;
  };

  const classWithMethod = (
    exports: Record<string, any> | undefined,
    method: string,
  ): (new (...args: any[]) => any) | undefined =>
    Object.values(exports ?? {}).find(
      (value) =>
        typeof value === "function" &&
        typeof value.prototype?.[method] === "function",
    ) as (new (...args: any[]) => any) | undefined;

  const mappedPlayerName = (
    gameController: Record<string, any>,
    color: unknown,
  ): string | undefined => {
    if (typeof color !== "number") return undefined;
    try {
      const value = gameController.getPlayerNameWithColor(color);
      if (typeof value === "string" && value) return value;
      if (typeof value?.options?.value === "string" && value.options.value) {
        return value.options.value;
      }
    } catch {
      // The diagnostic resolver records the unresolved mapping below.
    }
    return undefined;
  };

  const playerName = (gameController: Record<string, any>, color: unknown): string => {
    if (typeof color !== "number") return String(color ?? "");
    return mappedPlayerName(gameController, color) ?? `Player ${color}`;
  };

  const userId = (value: unknown): string | number | undefined => {
    if (typeof value === "string" && value) return value;
    if (typeof value === "number" && Number.isFinite(value)) return value;
    return undefined;
  };

  const screenPoint = (
    value: { toPixel?: (center: { x: number; y: number }, radius: number) => { x: number; y: number }; centerPointToPixel?: (center: { x: number; y: number }, radius: number) => { x: number; y: number } },
    mapView: Record<string, any>,
    canvas: HTMLCanvasElement,
  ): { x: number; y: number } | undefined => {
    try {
      const radius = mapView.hexagonHeight / 2;
      const local = value.toPixel
        ? value.toPixel(mapView.mapCenter, radius)
        : value.centerPointToPixel?.(mapView.mapCenter, radius);
      if (!local) return undefined;
      const global = mapView.toGlobal(local);
      const bounds = canvas.getBoundingClientRect();
      return {
        x: Math.round((bounds.x + global.x) * 10) / 10,
        y: Math.round((bounds.y + global.y) * 10) / 10,
      };
    } catch {
      return undefined;
    }
  };

  const exactLegalLocations = (
    gameManager: Record<string, any>,
    action: string,
  ): string[] | undefined => {
    const exports = findValidators();
    const gameController = gameManager.gameController;
    const state = gameManager.gameStore?.getState?.().gameState;
    if (!exports || !gameController || !state) return undefined;
    try {
      const playerActionState =
        gameController.currentStateValidator?.getPlayerActionState?.(
          gameController.myColor,
        ) ?? gameController.currentState?.actionState;
      let locations: Array<number | { x: number; y: number; z: number }>;
      if (action === "settlement") {
        const Validator = classWithMethod(exports, "canPlayerPlaceSettlement");
        if (!Validator) return undefined;
        const validator = new Validator(
          gameController.getPlayerValidators(),
          gameController.mapValidator,
          gameController.currentStateValidator,
          state.mechanicSettlementState,
        );
        locations =
          playerActionState === 1 || playerActionState === 2
            ? validator.whereCanPlayerPlaceBeginningSettlements()
            : validator.canPlayerPlaceSettlement(gameController.myColor, true)
                .validHoverLocations ?? [];
      } else if (action === "city") {
        const Validator = classWithMethod(exports, "canPlayerPlaceCity");
        if (!Validator) return undefined;
        locations =
          new Validator(
          gameController.getPlayerValidators(),
          gameController.mapValidator,
          gameController.currentStateValidator,
          state.mechanicCityState,
          ).canPlayerPlaceCity(gameController.myColor).validHoverLocations ?? [];
      } else if (action === "road") {
        const Validator = classWithMethod(exports, "canPlayerPlaceRoad");
        if (!Validator) return undefined;
        const validator = new Validator(
          gameController.getPlayerValidators(),
          gameController.mapValidator,
          gameController.currentStateValidator,
          state.mechanicRoadState,
        );
        locations =
          playerActionState === 3
            ? validator.whereCanPlayerPlaceBeginningRoads(gameController.myColor)
            : validator.canPlayerPlaceRoad(gameController.myColor, true)
                .validHoverLocations ?? [];
      } else {
        return undefined;
      }
      const prefix = action === "road" ? "e" : "v";
      const collection =
        action === "road"
          ? gameManager.gameState?.mapState?.tileState?._tileEdges
          : gameManager.gameState?.mapState?.tileState?._tileCorners;
      return locations.flatMap((location) => {
        const coordinate =
          typeof location === "number"
            ? collection?.[location]?.[action === "road" ? "hexEdge" : "hexCorner"]
            : location;
        return coordinate ? [key(prefix, coordinate)] : [];
      });
    } catch {
      return undefined;
    }
  };

  const buildSnapshot = (): Record<string, unknown> | undefined => {
    const gameManager = findManager();
    const gameController = gameManager?.gameController;
    const rootStoreState = gameManager?.gameStore?.getState?.();
    const storeGameState = rootStoreState?.gameState;
    const managerGameState = gameManager?.gameState;
    const placedPieceCount = (gameState: Record<string, any> | undefined) => {
      const candidate = gameState?.mapState?.tileState;
      if (!candidate) return -1;
      const buildings = (candidate._tileCorners ?? []).filter(
        (corner: Record<string, any>) => {
          const buildingType =
            corner.buildingType ?? corner.state?.buildingType;
          return buildingType === 1 || buildingType === 2;
        },
      ).length;
      const roads = (candidate._tileEdges ?? []).filter(
        (edge: Record<string, any>) =>
          (edge.owner ?? edge.state?.owner) !== undefined &&
          (edge.owner ?? edge.state?.owner) !== null,
      ).length;
      return buildings + roads;
    };
    const managerPlacedPieceCount = placedPieceCount(managerGameState);
    const storePlacedPieceCount = placedPieceCount(storeGameState);
    const selectedGameStateSource =
      managerPlacedPieceCount > storePlacedPieceCount ? "manager" : "store";
    const liveGameState =
      selectedGameStateSource === "manager"
        ? managerGameState
        : storeGameState ?? managerGameState;
    const tileState =
      liveGameState?.mapState?.tileState ??
      gameManager?.gameState?.mapState?.tileState;
    const portState =
      liveGameState?.mapState?.portState ??
      gameManager?.gameState?.mapState?.portState;
    const mapView = gameManager?.mapController?.mapView;
    const canvas = document.querySelector<HTMLCanvasElement>("#game-canvas");
    if (
      !gameManager ||
      !gameController ||
      !tileState?._tiles?.length ||
      !tileState?._tileCorners?.length ||
      !mapView ||
      !canvas
    ) {
      manager = undefined;
      return undefined;
    }

    const myColor = gameController.myColor;
    const playOrder: number[] = Array.isArray(gameController.playOrder)
      ? gameController.playOrder.filter(
          (color: unknown): color is number =>
            typeof color === "number" && Number.isInteger(color),
        )
      : [];
    const isReplay = Boolean(rootStoreState?.gameReplay?.isReplay);
    const mappedMyPlayer = mappedPlayerName(gameController, myColor);
    const currentUserId = userId(
      rootStoreState?.gameClientConfig?.currentUserId,
    );
    const playerMappings = playOrder.map((color) => {
      let controllerPlayer: string | undefined;
      try {
        const playerState = gameController.getPlayerState?.(color);
        controllerPlayer =
          typeof playerState?.username === "string" && playerState.username
            ? playerState.username
            : typeof playerState?.userState?.username === "string" &&
                playerState.userState.username
              ? playerState.userState.username
              : undefined;
      } catch {
        controllerPlayer = undefined;
      }

      const gameUserStates = rootStoreState?.gameUserStates;
      const storeUserState =
        gameUserStates?.getUserState?.(color) ??
        (Array.isArray(gameUserStates?.userStates)
          ? gameUserStates.userStates.find(
              (candidate: Record<string, any>) =>
                Number(candidate?.selectedColor) === color,
            )
          : undefined);
      const storePlayer =
        typeof storeUserState?.username === "string" && storeUserState.username
          ? storeUserState.username
          : undefined;
      const storeUserId = userId(storeUserState?.userId);
      const currentUserMatch =
        currentUserId !== undefined &&
        storeUserId !== undefined &&
        String(currentUserId) === String(storeUserId);
      return {
        color,
        mappedPlayer: mappedPlayerName(gameController, color),
        controllerPlayer,
        storePlayer,
        storeUserId,
        currentUserMatch,
      };
    });
    const playerSignals = playerMappings.map((player) => ({
      color: player.color,
      ...(player.storePlayer ? { name: player.storePlayer } : {}),
      ...(player.storeUserId !== undefined ? { userId: player.storeUserId } : {}),
    }));
    const identity = resolveLocalIdentity({
      myColor,
      ...(mappedMyPlayer ? { mappedMyPlayer } : {}),
      playOrder,
      players: playerSignals,
      ...(currentUserId !== undefined ? { currentUserId } : {}),
      ...(managerResolutionSource ? { managerResolutionSource } : {}),
      isReplay,
    });
    const identityResolved = identity.status === "resolved";
    const myPlayer = identityResolved ? (identity.myPlayer ?? "") : "";
    const myPlayerMapping =
      typeof myColor === "number"
        ? playerMappings.find((player) => player.color === myColor)
        : undefined;
    const playerStateMyPlayer = myPlayerMapping?.controllerPlayer;
    const storeMyPlayer = myPlayerMapping?.storePlayer;
    const gameStoreState = liveGameState;
    const diceState =
      gameController.diceState?.state ??
      gameController.diceState ??
      storeGameState?.diceState ??
      managerGameState?.diceState;
    const bankState =
      gameController.bankState?.state ??
      gameController.bankState ??
      storeGameState?.bankState ??
      managerGameState?.bankState;
    const tradeState =
      gameController.tradeState?.state ??
      gameController.tradeState ??
      storeGameState?.tradeState ??
      managerGameState?.tradeState;
    const robberIndex = [
      gameStoreState?.mechanicRobberState?.locationTileIndex,
      gameManager?.gameState?.mechanicRobberState?.locationTileIndex,
      managerGameState?.mechanicRobberState?.locationTileIndex,
      storeGameState?.mechanicRobberState?.locationTileIndex,
      rootStoreState?.gameState?.mechanicRobberState?.locationTileIndex,
    ]
      .map((candidate) => Number(candidate))
      .find((candidate) => Number.isInteger(candidate) && candidate >= 0);
    const currentState = gameController.currentState ?? {};
    const playerActionState =
      gameController.currentStateValidator?.getPlayerActionState?.(myColor) ??
      currentState.actionState;
    let action: BoardAction = ACTION_BY_STATE[playerActionState] ?? "none";
    const actionBoxData = rootStoreState?.actionBox?.actionBoxData;
    const actionBoxEvidence = JSON.stringify({
      type: actionBoxData?.type,
      title: actionBoxData?.props?.title,
      body: actionBoxData?.props?.body,
    }).toLowerCase();
    const visibleDiscardPrompt = [
      ...document.querySelectorAll<HTMLElement>(
        "[class*='actionBoxContainer-'], [class*='actionBox-']",
      ),
    ].find((element) => {
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      return (
        rect.width >= 40 &&
        rect.height >= 40 &&
        rect.bottom > 0 &&
        rect.right > 0 &&
        rect.top < innerHeight &&
        rect.left < innerWidth &&
        style.display !== "none" &&
        style.visibility !== "hidden" &&
        /discard cards|cards to discard/iu.test(element.textContent ?? "")
      );
    });
    const visibleDiscardMatch = (visibleDiscardPrompt?.textContent ?? "").match(
      /discard cards\s*\(\s*\d+\s*\/\s*(\d{1,3})\s*\)|need to discard\s+(\d{1,3})/iu,
    );
    const visibleRobberVictimPrompt = [
      ...document.querySelectorAll<HTMLElement>(
        "[class*='actionBoxContainer-'], [class*='actionBox-']",
      ),
    ].find((element) => {
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      return (
        rect.width >= 40 &&
        rect.height >= 40 &&
        rect.bottom > 0 &&
        rect.right > 0 &&
        rect.top < innerHeight &&
        rect.left < innerWidth &&
        style.display !== "none" &&
        style.visibility !== "hidden" &&
        /choose a player to steal/iu.test(element.textContent ?? "")
      );
    });
    const robberVictimPlayers = visibleRobberVictimPrompt
      ? [
          ...visibleRobberVictimPrompt.querySelectorAll<HTMLElement>(
            "[class*='playerName-']",
          ),
        ]
          .map((element) => (element.textContent ?? "").trim())
          .filter(
            (player, index, players) =>
              Boolean(player) && players.indexOf(player) === index,
          )
      : [];
    const visibleDiscardCount = Number(
      visibleDiscardMatch?.[1] ?? visibleDiscardMatch?.[2] ?? 0,
    );
    const discardPromptVisible =
      (
        actionBoxData?.type === "pickCard" &&
        actionBoxEvidence.includes("discard")
      ) ||
      visibleDiscardCount > 0;
    action = resolveLocalBoardAction(action, discardPromptVisible);
    if (!identityResolved) action = "none";
    const initialPlacement = currentState.turnState === 0;

    const hexes = tileState._tiles.flatMap((tile: Record<string, any>, index: number) => {
      const resource = RESOURCE_BY_TILE_TYPE[tile.state?.type];
      const number = tile.state?.diceNumber;
      const face = tile.hexFace;
      if (!face || (!resource && number !== 0)) return [];
      return [
        {
          id: key("h", face),
          ...(resource ? { resource } : {}),
          ...(number >= 2 && number <= 12 ? { number } : {}),
          ...(robberIndex === index ? { blocked: true } : {}),
          ...(screenPoint(face, mapView, canvas)
            ? { screen: screenPoint(face, mapView, canvas) }
            : {}),
        },
      ];
    });

    const portsByVertex = new Map<string, string>();
    for (const port of portState?._portEdges ?? []) {
      const portName = PORT_BY_TYPE[port.state?.type];
      if (!portName) continue;
      for (const endpoint of port.hexEdge?.endPoints?.() ?? []) {
        portsByVertex.set(key("v", endpoint), portName);
      }
    }

    const vertices = tileState._tileCorners.map((corner: Record<string, any>) => {
      const location = corner.hexCorner;
      const owner = corner.owner ?? corner.state?.owner;
      const buildingType = corner.buildingType ?? corner.state?.buildingType;
      const building =
        owner !== undefined && owner !== null && (buildingType === 1 || buildingType === 2)
          ? {
              player: playerName(gameController, owner),
              kind: buildingType === 2 ? "city" : "settlement",
            }
          : undefined;
      const adjacentHexes = (location.touchingFaces?.() ?? [])
        .map((face: { x: number; y: number }) => key("h", face))
        .filter((id: string) => hexes.some((hex: { id: string }) => hex.id === id));
      const adjacentVertices = (location.adjacentCorners?.() ?? [])
        .map((vertex: { x: number; y: number; z: number }) => key("v", vertex))
        .filter((id: string) =>
          tileState._tileCorners.some(
            (candidate: Record<string, any>) => key("v", candidate.hexCorner) === id,
          ),
        );
      const adjacentTiles = adjacentHexes
        .map((id: string) => hexes.find((hex: { id: string }) => hex.id === id))
        .filter((hex: Record<string, any> | undefined) => hex?.resource && hex?.number)
        .sort((left: Record<string, any>, right: Record<string, any>) => right.number - left.number);
      const port = portsByVertex.get(key("v", location));
      const label = adjacentTiles.length
        ? adjacentTiles
            .map((hex: Record<string, any>) => `${hex.number} ${hex.resource}`)
            .join(" · ")
        : key("v", location);
      return {
        id: key("v", location),
        label: port ? `${label} · ${port === "generic" ? "3:1" : `${port} port`}` : label,
        adjacentHexes,
        adjacentVertices,
        ...(port ? { port } : {}),
        ...(building ? { building } : {}),
        ...(screenPoint(location, mapView, canvas)
          ? { screen: screenPoint(location, mapView, canvas) }
          : {}),
      };
    });

    const edges = tileState._tileEdges.map((edge: Record<string, any>) => {
      const location = edge.hexEdge;
      const owner = edge.owner ?? edge.state?.owner;
      const endpoints = location.endPoints();
      const targetLabels = endpoints
        .map((endpoint: { x: number; y: number; z: number }) =>
          vertices.find(
            (vertex: { id: string }) => vertex.id === key("v", endpoint),
          ),
        )
        .filter(Boolean)
        .map((vertex: { label: string }) => vertex.label);
      return {
        id: key("e", location),
        label: targetLabels.length ? `Toward ${targetLabels[0]}` : key("e", location),
        vertices: [key("v", endpoints[0]), key("v", endpoints[1])],
        ...(owner !== undefined && owner !== null
          ? { player: playerName(gameController, owner) }
          : {}),
        ...(screenPoint(location, mapView, canvas)
          ? { screen: screenPoint(location, mapView, canvas) }
          : {}),
      };
    });

    // Colonist can leave the per-player action enum on "settlement" for a
    // render tick after the opening settlement is already on the map. The
    // unroaded owned settlement is an unambiguous public signal that the next
    // legal prompt is the paired opening road.
    if (
      initialPlacement &&
      action === "settlement" &&
      openingRoadEdgeIds({ vertices, edges } as any, myPlayer)?.length
    ) {
      action = "road";
    }

    const openVertex = (vertex: Record<string, any>) =>
      !vertex.building &&
      vertex.adjacentVertices.every(
        (id: string) => !vertices.find((candidate: { id: string }) => candidate.id === id)?.building,
      );
    const myRoadTouches = (vertexId: string) =>
      edges.some(
        (edge: Record<string, any>) =>
          edge.player === myPlayer && edge.vertices.includes(vertexId),
      );
    const fallbackLegalVertices =
      action === "city"
        ? vertices
            .filter(
              (vertex: Record<string, any>) =>
                vertex.building?.player === myPlayer &&
                vertex.building?.kind === "settlement",
            )
            .map((vertex: Record<string, any>) => vertex.id)
        : vertices
            .filter(
              (vertex: Record<string, any>) =>
                openVertex(vertex) && (initialPlacement || myRoadTouches(vertex.id)),
            )
            .map((vertex: Record<string, any>) => vertex.id);
    const openingEdges =
      initialPlacement && action === "road"
        ? openingRoadEdgeIds({ vertices, edges } as any, myPlayer)
        : undefined;
    const fallbackLegalEdges = edges
      .filter((edge: Record<string, any>) => {
        if (edge.player) return false;
        if (openingEdges) return openingEdges.includes(edge.id);
        return edge.vertices.some((vertexId: string) => {
          const vertex = vertices.find(
            (candidate: { id: string }) => candidate.id === vertexId,
          );
          if (vertex?.building?.player === myPlayer) return true;
          if (vertex?.building && vertex.building.player !== myPlayer) return false;
          return myRoadTouches(vertexId);
        });
      })
      .map((edge: Record<string, any>) => edge.id);

    const exactLegal = exactLegalLocations(gameManager, action);
    const buildableSettlementIds = vertices
      .filter(
        (vertex: Record<string, any>) =>
          openVertex(vertex) && myRoadTouches(vertex.id),
      )
      .map((vertex: Record<string, any>) => vertex.id);
    const buildableCityIds = vertices
      .filter(
        (vertex: Record<string, any>) =>
          vertex.building?.player === myPlayer &&
          vertex.building?.kind === "settlement",
      )
      .map((vertex: Record<string, any>) => vertex.id);
    const buildableRoadIds =
      initialPlacement && action === "road"
        ? fallbackLegalEdges
        : edges
            .filter((edge: Record<string, any>) => {
              if (edge.player) return false;
              return edge.vertices.some((vertexId: string) => {
                const vertex = vertices.find(
                  (candidate: { id: string }) => candidate.id === vertexId,
                );
                if (vertex?.building?.player === myPlayer) return true;
                if (vertex?.building && vertex.building.player !== myPlayer) return false;
                return myRoadTouches(vertexId);
              });
            })
            .map((edge: Record<string, any>) => edge.id);
    const botOnlyGame =
      identityResolved &&
      playOrder.length >= 2 &&
      playOrder
        .filter((color) => color !== myColor)
        .every(
          (color) =>
            gameController.getPlayerState?.(color)?.userState?.isBot === true,
        );
    const myOrderIndex = identityResolved ? playOrder.indexOf(myColor) : -1;
    const ownedSettlements = vertices.filter(
      (vertex: Record<string, any>) => vertex.building?.player === myPlayer,
    ).length;
    const picksUntilNext =
      initialPlacement && ownedSettlements === 0 && myOrderIndex >= 0
        ? Math.max(0, 2 * (playOrder.length - 1 - myOrderIndex))
        : 0;

    const resourceVector = (cards: Array<number> | Record<string, number>) => {
      const result: Record<string, number> = {
        lumber: 0,
        brick: 0,
        wool: 0,
        grain: 0,
        ore: 0,
      };
      if (Array.isArray(cards)) {
        for (const card of cards) {
          const resource = RESOURCE_BY_TILE_TYPE[card];
          if (resource) result[resource] = (result[resource] ?? 0) + 1;
        }
      } else {
        for (const [type, count] of Object.entries(cards ?? {})) {
          const resource = RESOURCE_BY_TILE_TYPE[Number(type)];
          if (resource && Number.isInteger(count) && count >= 0) {
            result[resource] = count;
          }
        }
      }
      return result;
    };
    const developmentCardVector = (cards: unknown[]) => {
      const result: Record<DevelopmentCardName, number> = {
        knight: 0,
        monopoly: 0,
        "road-building": 0,
        "year-of-plenty": 0,
        "victory-point": 0,
      };
      for (const card of cards) {
        const name =
          DEVELOPMENT_CARD_BY_TYPE[
            Number(card) as keyof typeof DEVELOPMENT_CARD_BY_TYPE
          ];
        if (name) result[name] += 1;
      }
      return result;
    };
    const subtractDevelopmentCards = (
      cards: Record<DevelopmentCardName, number>,
      unavailable: Record<DevelopmentCardName, number>,
    ): Record<DevelopmentCardName, number> =>
      Object.fromEntries(
        Object.keys(cards).map((name) => [
          name,
          Math.max(
            0,
            cards[name as DevelopmentCardName] -
              unavailable[name as DevelopmentCardName],
          ),
        ]),
      ) as Record<DevelopmentCardName, number>;
    const stateCandidates = [
      gameStoreState,
      managerGameState,
      storeGameState,
    ].filter(Boolean);
    const playerStates = Object.fromEntries(
      playOrder.flatMap((color) => {
        const state =
          stateCandidates
            .map((candidate) => candidate?.playerStates?.[color])
            .find(Boolean) ??
          gameController.getPlayerState?.(color)?.state ??
          gameController.getPlayerState?.(color);
        return state ? [[color, state]] : [];
      }),
    ) as Record<string, any>;
    // Colonist renders the local card inventory from the Redux game store.
    // Do not let the placement-freshness arbitration above substitute the
    // manager snapshot for private development-card ownership: buying or
    // playing a development card does not change the placed-piece count.
    const developmentStates =
      [storeGameState, managerGameState]
        .map(
          (candidate) =>
            candidate?.mechanicDevelopmentCardsState?.players,
        )
        .find(
          (candidate) =>
            candidate && Object.keys(candidate).length > 0,
        ) ?? {};
    const longestRoadStates =
      gameStoreState?.mechanicLongestRoadState ?? {};
    const publicPlayers: Record<string, unknown> = {};
    for (const [colorKey, playerState] of Object.entries<Record<string, any>>(
      playerStates,
    )) {
      const name = playerName(gameController, Number(colorKey));
      const cards = Array.isArray(playerState.resourceCards?.cards)
        ? playerState.resourceCards.cards
        : [];
      const developmentCards = Array.isArray(
        developmentStates[colorKey]?.developmentCards?.cards,
      )
        ? developmentStates[colorKey].developmentCards.cards.length
        : 0;
      const developmentCardsUsed = Array.isArray(
        developmentStates[colorKey]?.developmentCardsUsed,
      )
        ? developmentStates[colorKey].developmentCardsUsed
        : [];
      const victoryPointsState = playerState.victoryPointsState ?? {};
      const longestRoad = longestRoadStates[colorKey]?.longestRoad;
      publicPlayers[name] = {
        handSize: cards.length,
        tradeRatios: resourceVector(playerState.bankTradeRatiosState ?? {}),
        cardDiscardLimit:
          Number.isInteger(playerState.cardDiscardLimit)
            ? playerState.cardDiscardLimit
            : rootStoreState?.gameSettings?.cardDiscardLimit ?? 7,
        developmentCards,
        playedDevelopmentCards: developmentCardVector(developmentCardsUsed),
        hasPlayedDevelopmentThisTurn: Boolean(
          developmentStates[colorKey]?.hasUsedDevelopmentCardThisTurn,
        ),
        visiblePoints:
          Number(victoryPointsState[0] ?? 0) +
          Number(victoryPointsState[1] ?? 0) * 2 +
          Number(victoryPointsState[3] ?? 0) * 2 +
          Number(victoryPointsState[4] ?? 0) * 2,
        ...(Number.isInteger(longestRoad)
          ? { longestRoad: Number(longestRoad) }
          : {}),
        hasLargestArmy: Number(victoryPointsState[3] ?? 0) > 0,
        hasLongestRoad: Number(victoryPointsState[4] ?? 0) > 0,
      };
    }
    const ownCards = identityResolved
      ? playerStates[myColor]?.resourceCards?.cards ??
        gameController.getAllCardsInHand?.(myColor)
      : undefined;
    const ownHand = Array.isArray(ownCards)
      ? resourceVector(ownCards.filter((card: unknown) => Number(card) >= 1))
      : undefined;
    const ownDevelopmentState = identityResolved
      ? developmentStates[myColor]
      : undefined;
    const ownDevelopmentCardList = Array.isArray(
      ownDevelopmentState?.developmentCards?.cards,
    )
      ? ownDevelopmentState.developmentCards.cards
      : [];
    const ownBoughtDevelopmentCardList = Array.isArray(
      ownDevelopmentState?.developmentCardsBoughtThisTurn,
    )
      ? ownDevelopmentState.developmentCardsBoughtThisTurn
      : [];
    const ownDevelopmentCards = developmentCardVector(
      ownDevelopmentCardList,
    );
    const boughtThisTurn = developmentCardVector(
      ownBoughtDevelopmentCardList,
    );
    const ownPlayableDevelopmentCards =
      identityResolved &&
      gameController.isMyTurn &&
      !ownDevelopmentState?.hasUsedDevelopmentCardThisTurn
        ? subtractDevelopmentCards(ownDevelopmentCards, boughtThisTurn)
        : developmentCardVector([]);
    const currentTurnPlayers: number[] = Array.isArray(
      rootStoreState?.gameState?.experimentalMechanicState?.currentState
        ?.currentTurnPlayers,
    )
      ? rootStoreState.gameState.experimentalMechanicState.currentState
          .currentTurnPlayers
      : [Number(currentState.currentTurnPlayerColor)].filter(Number.isFinite);
    const activeTrades = identityResolved
      ? Object.values<Record<string, any>>(
          tradeState?.activeOffers ?? {},
        ).flatMap((offer) => {
      if (!offer || typeof offer.id !== "string") return [];
      const counterOfferInResponseToTradeId =
        offer.counterOfferInResponseToTradeId != null
          ? String(offer.counterOfferInResponseToTradeId)
          : undefined;
      const counterOffer = counterOfferInResponseToTradeId !== undefined;
      const creatorGiveCards = offer.offeredResources;
      const creatorReceiveCards = offer.wantedResources;
      const creatorGiveOpenEnded =
        Array.isArray(creatorGiveCards) && creatorGiveCards.length === 0;
      const creatorReceiveOpenEnded =
        Array.isArray(creatorReceiveCards) && creatorReceiveCards.length === 0;
      const fullySpecified = [creatorGiveCards, creatorReceiveCards].every(
        (cards) =>
          Array.isArray(cards) &&
          cards.length > 0 &&
          cards.every(
            (card: unknown) =>
              Number(card) >= 1 && Number(card) <= 5,
          ),
      );
      const creatorGive = resourceVector(
        Array.isArray(creatorGiveCards) ? creatorGiveCards : [],
      );
      const creatorReceive = resourceVector(
        Array.isArray(creatorReceiveCards) ? creatorReceiveCards : [],
      );
      const incoming = offer.creator !== myColor;
      const localGive = incoming ? creatorReceive : creatorGive;
      const canAfford =
        ownHand &&
        Object.entries(localGive).every(
          ([resource, count]) =>
            (ownHand?.[resource as keyof typeof ownHand] ?? 0) >= count,
        );
      const tradeExecutorColor = counterOffer
        ? currentTurnPlayers[0]
        : offer.creator;
      const responses = Object.entries<number>(
        offer.playerResponses ?? {},
      ).map(([color, status]) => ({
        player: playerName(gameController, Number(color)),
        status: Number(status),
      }));
      const acceptedPlayers = responses
        .filter((response) => response.status === 1)
        .map((response) => response.player);
      const pendingPlayers = responses
        .filter((response) => response.status === 0)
        .map((response) => response.player);
      const rejectedPlayers = responses
        .filter((response) => response.status === 2)
        .map((response) => response.player);
      const embargoedPlayers = responses
        .filter((response) => response.status === 3)
        .map((response) => response.player);
      const myResponseStatus = Number(
        offer.playerResponses?.[myColor],
      );
      const myResponse =
        myResponseStatus === 0
          ? "pending"
          : myResponseStatus === 1
            ? "accepted"
            : myResponseStatus === 2
              ? "rejected"
              : myResponseStatus === 3
                ? "embargoed"
                : undefined;
      return [
        {
          id: offer.id,
          creator: playerName(gameController, offer.creator),
          tradeExecutor: playerName(
            gameController,
            tradeExecutorColor,
          ),
          creatorGive,
          creatorReceive,
          ...(creatorGiveOpenEnded ? { creatorGiveOpenEnded: true } : {}),
          ...(creatorReceiveOpenEnded ? { creatorReceiveOpenEnded: true } : {}),
          incoming,
          counterOffer,
          ...(counterOfferInResponseToTradeId
            ? { counterOfferInResponseToTradeId }
            : {}),
          canAccept: Boolean(fullySpecified && canAfford),
          acceptedPlayers,
          pendingPlayers,
          rejectedPlayers,
          embargoedPlayers,
          responsesComplete:
            responses.length > 0 && pendingPlayers.length === 0,
          ...(myResponse ? { myResponse } : {}),
        },
      ];
    })
      : [];
    const bankVisible =
      rootStoreState?.gameSettings?.hideBankCards === false &&
      bankState?.hideBankCards === false;
    const bank = bankVisible
      ? resourceVector(bankState?.resourceCards ?? {})
      : undefined;
    const ownCardCount = Array.isArray(ownCards) ? ownCards.length : 0;
    const actionBoxDiscardCount = Number(
      actionBoxData?.props?.amountOfCardsToSelect ??
        actionBoxData?.props?.selectCardFormat?.amountOfCardsToSelect ??
        actionBoxData?.props?.cardSelector?.amountOfCardsToSelect ??
        actionBoxData?.props?.amountToDiscard ??
        actionBoxData?.props?.amountOfCardsToDiscard ??
        currentState.selectCardFormat?.amountOfCardsToSelect ??
        currentState.amountOfCardsToDiscard ??
        visibleDiscardCount ??
        0,
    );
    const discardCount =
      action === "discard"
        ? actionBoxDiscardCount > 0
          ? actionBoxDiscardCount
          : ownCardCount > 0
            ? Math.floor(ownCardCount / 2)
            : undefined
        : undefined;
    if (action === "discard" && discardCount === undefined) action = "none";

    const placedPieces =
      vertices.filter((vertex: Record<string, any>) => vertex.building).length +
      edges.filter((edge: Record<string, any>) => edge.player).length;
    const completedTurns = Number(currentState.completedTurns ?? 0);
    const setupTurnCount = 2 * playOrder.length;
    const gameplayRollCount = initialPlacement
      ? 0
      : playOrder.length >= 2 && completedTurns >= setupTurnCount
        ? completedTurns - setupTurnCount + (diceState?.diceThrown ? 1 : 0)
        : undefined;
    if (
      !isReplay &&
      previousLiveProgress &&
      previousLiveProgress.completedTurns >= 3 &&
      completedTurns <= 1 &&
      placedPieces < previousLiveProgress.placedPieces
    ) {
      managerGeneration = bumpManagerGeneration(sessionStorage, managerGeneration);
      previousLiveProgress = undefined;
    }
    if (!isReplay) previousLiveProgress = { completedTurns, placedPieces };
    const roomId = String(rootStoreState?.gameSettings?.roomId ?? "");
    const gameKey = `${location.pathname}${location.search}|${roomId}|${managerGeneration}`;
    const victoryTarget = Number(
      rootStoreState?.gameSettings?.victoryPointsToWin ?? 10,
    );
    const diceModeObservation = observeColonistDiceMode(
      rootStoreState?.gameSettings?.diceSetting,
    );
    const visibleWinner = Object.entries(publicPlayers).find(
      ([, player]) =>
        Number(
          (player as { visiblePoints?: number }).visiblePoints ?? 0,
        ) >= victoryTarget,
    )?.[0];
    const endgameHeading = [
      ...document.querySelectorAll<HTMLElement>(
        "[class*='heading'], [role='dialog'] h1, [role='dialog'] h2",
      ),
    ].find((element) => {
      const style = getComputedStyle(element);
      const rect = element.getBoundingClientRect();
      return (
        style.display !== "none" &&
        style.visibility !== "hidden" &&
        rect.width > 0 &&
        rect.height > 0 &&
        isTerminalGameHeading(element.textContent)
      );
    });
    const winnerText = (document.body.textContent ?? "").match(
      /([^\n]{1,80}?)\s+won the game/iu,
    )?.[1]?.trim();
    const gameOver = Boolean(visibleWinner || endgameHeading || winnerText);
    const winner = visibleWinner ?? winnerText;
    const colorAsset = identityResolved
      ? COLOR_ASSET_NAME[myColor] ?? "blue"
      : undefined;
    const assets = {
      resources: {
        lumber: findAsset("card_lumber"),
        brick: findAsset("card_brick"),
        wool: findAsset("card_wool"),
        grain: findAsset("card_grain"),
        ore: findAsset("card_ore"),
      },
      pieces: {
        ...(colorAsset
          ? {
              road: findAsset(`road_${colorAsset}`),
              settlement: findAsset(`settlement_${colorAsset}`),
              city: findAsset(`city_${colorAsset}`),
            }
          : {}),
        development: findAsset("card_devcardback"),
        robber: findAsset("icon_robber"),
        longestRoad: findAsset("icon_longest_road"),
        largestArmy: findAsset("icon_largest_army"),
        knight: findAsset("card_knight"),
        monopoly: findAsset("card_monopoly"),
        roadBuilding: findAsset("card_roadbuilding"),
        yearOfPlenty: findAsset("card_yearofplenty"),
        victoryPoint: findAsset("card_vp"),
      },
    };

    return {
      hexes,
      vertices,
      edges,
      ...(action === "road"
        ? {
            legalEdgeIds:
              openingEdges ?? exactLegal ?? fallbackLegalEdges,
          }
        : action === "settlement" || action === "city"
          ? { legalVertexIds: exactLegal ?? fallbackLegalVertices }
          : {}),
      buildableSettlementIds,
      buildableCityIds,
      buildableRoadIds,
      ...(identityResolved && myPlayer ? { myPlayer } : {}),
      localSeatDiagnostics: {
        identity,
        ...(typeof myColor === "number" ? { rawMyColor: myColor } : {}),
        ...(identityResolved && myPlayer ? { resolvedMyPlayer: myPlayer } : {}),
        ...(mappedMyPlayer ? { mappedMyPlayer } : {}),
        ...(playerStateMyPlayer ? { playerStateMyPlayer } : {}),
        ...(storeMyPlayer ? { storeMyPlayer } : {}),
        playerMappings: playerMappings.map((player) => ({
          color: player.color,
          ...(player.mappedPlayer ? { mappedPlayer: player.mappedPlayer } : {}),
          ...(player.controllerPlayer
            ? { controllerPlayer: player.controllerPlayer }
            : {}),
          ...(player.storePlayer ? { storePlayer: player.storePlayer } : {}),
          currentUserMatch: player.currentUserMatch,
        })),
        rawPlayOrderColors: [...playOrder],
        ...(currentTurnPlayers[0] !== undefined
          ? {
              currentActorColor: currentTurnPlayers[0],
              currentActorPlayer: playerName(gameController, currentTurnPlayers[0]),
            }
          : {}),
        isMyTurn: Boolean(gameController.isMyTurn),
        ...(Number.isInteger(playerActionState)
          ? { localActionState: Number(playerActionState) }
          : {}),
        ...(managerModuleId ? { managerModuleId } : {}),
        managerGeneration,
        ...(managerResolutionSource ? { managerResolutionSource } : {}),
        managerMatchesStoreState: managerGameState === storeGameState,
        selectedGameStateSource,
        managerPlacedPieceCount,
        storePlacedPieceCount,
        isReplay,
        occupiedBuildings: tileState._tileCorners.flatMap(
          (corner: Record<string, any>) => {
            const owner = corner.owner ?? corner.state?.owner;
            const buildingType = corner.buildingType ?? corner.state?.buildingType;
            return typeof owner === "number" && (buildingType === 1 || buildingType === 2)
              ? [{
                  vertexId: key("v", corner.hexCorner),
                  ownerColor: owner,
                  player: playerName(gameController, owner),
                }]
              : [];
          },
        ),
        occupiedRoads: tileState._tileEdges.flatMap((edge: Record<string, any>) => {
          const owner = edge.owner ?? edge.state?.owner;
          return typeof owner === "number"
            ? [{
                edgeId: key("e", edge.hexEdge),
                ownerColor: owner,
                player: playerName(gameController, owner),
              }]
            : [];
        }),
        seatSource:
          identity.source === "controller+account-user-id+store-roster"
            ? "gameController.myColor+currentUserId+gameUserStates"
            : identity.source === "replay-perspective"
              ? "replay-perspective"
              : "unresolved",
      },
      ...(ownHand ? { ownHand } : {}),
      ...(identityResolved
        ? {
            ownDevelopmentCards: {
              cards: ownDevelopmentCards,
              playable: ownPlayableDevelopmentCards,
              boughtThisTurn,
              hasPlayedThisTurn: Boolean(
                ownDevelopmentState?.hasUsedDevelopmentCardThisTurn,
              ),
            },
          }
        : {}),
      activeTrades: gameOver ? [] : activeTrades,
      ...(bank ? { bank } : {}),
      bankVisible,
      players: publicPlayers,
      playerOrder: playOrder.map((color) =>
        playerName(gameController, color),
      ),
      assets,
      ...(discardCount ? { discardCount } : {}),
      ...(visibleRobberVictimPrompt
        ? {
            robberVictimSelection: true,
            robberVictimPlayers,
          }
        : {}),
      gameKey,
      isMyTurn:
        identityResolved &&
        !gameOver &&
        (action === "discard" || Boolean(gameController.isMyTurn)),
      action: identityResolved && !gameOver ? action : "none",
      initialPlacement,
      picksUntilNext,
      victoryTarget,
      diceMode: diceModeObservation.mode,
      ...(diceModeObservation.rawUnsupportedSetting !== undefined
        ? { diceModeRaw: diceModeObservation.rawUnsupportedSetting }
        : {}),
      friendlyRobber: Boolean(rootStoreState?.gameSettings?.friendlyRobber),
      privateGame: Boolean(
        rootStoreState?.gameSettings?.isPrivateGame ??
          rootStoreState?.gameSettings?.privateGame ??
          rootStoreState?.lobbyState?.isPrivateGame ??
          false,
      ),
      ...(identityResolved ? { botOnlyGame } : {}),
      ...(currentTurnPlayers[0] !== undefined
        ? {
            currentPlayer: playerName(
              gameController,
              currentTurnPlayers[0],
            ),
          }
        : {}),
      turn: Math.max(0, completedTurns),
      ...(gameplayRollCount !== undefined ? { gameplayRollCount } : {}),
      hasRolled: Boolean(diceState?.diceThrown),
      gameOver,
      ...(winner ? { winner } : {}),
      ...(diceState?.diceThrown
        ? {
            lastRoll:
              Number(diceState.dice1 ?? 0) +
              Number(diceState.dice2 ?? 0),
          }
        : {}),
    };
  };

  const publish = () => {
    const snapshot = buildSnapshot();
    const payload = snapshot ? JSON.stringify(snapshot) : "";
    if (payload === previousPayload) return;
    previousPayload = payload;
    window.postMessage(
      {
        source: SOURCE,
        type: snapshot ? "snapshot" : "clear",
        payload: snapshot,
      },
      window.location.origin,
    );
  };

  window.addEventListener(
    "message",
    (rawEvent) => {
      if (
        rawEvent.source !== window ||
        rawEvent.origin !== window.location.origin
      ) {
        return;
      }
      const detail = rawEvent.data as
        | {
            source?: string;
            type?: string;
            action?: "road" | "settlement" | "city" | "robber";
            targetId?: string;
            signature?: string;
            attempt?: number;
          }
        | undefined;
      if (
        detail?.source !== "colonist-assistant-content" ||
        detail.type !== "execute-board-action" ||
        !detail?.action ||
        !detail.targetId ||
        !detail.signature
      ) {
        return;
      }
      const gameManager = findManager();
      const gameController = gameManager?.gameController;
      const socket = gameManager?.socketGameSend;
      const snapshot = buildSnapshot() as
        | {
            isMyTurn?: boolean;
            action?: string;
            initialPlacement?: boolean;
            legalVertexIds?: string[];
            legalEdgeIds?: string[];
            hexes?: Array<{ id: string; blocked?: boolean }>;
            localSeatDiagnostics?: {
              identity?: { status?: "resolved" | "unresolved" };
            };
          }
        | undefined;
      if (
        !gameManager ||
        !gameController?.isMyTurn ||
        !socket ||
        snapshot?.localSeatDiagnostics?.identity?.status !== "resolved" ||
        !snapshot.isMyTurn ||
        snapshot.action !== detail.action
      ) {
        return;
      }
      const tileState = gameManager.gameState?.mapState?.tileState;
      let index = -1;
      if (detail.action === "road") {
        if (!snapshot.legalEdgeIds?.includes(detail.targetId)) return;
        index =
          tileState?._tileEdges?.findIndex(
            (edge: Record<string, any>) =>
              key("e", edge.hexEdge) === detail.targetId,
          ) ?? -1;
      } else if (
        detail.action === "settlement" ||
        detail.action === "city"
      ) {
        if (!snapshot.legalVertexIds?.includes(detail.targetId)) return;
        index =
          tileState?._tileCorners?.findIndex(
            (corner: Record<string, any>) =>
              key("v", corner.hexCorner) === detail.targetId,
          ) ?? -1;
      } else {
        const target = snapshot.hexes?.find(
          (hex) => hex.id === detail.targetId,
        );
        if (!target || target.blocked) return;
        index =
          tileState?._tiles?.findIndex(
            (tile: Record<string, any>) =>
              key("h", tile.hexFace) === detail.targetId,
          ) ?? -1;
      }
      if (!Number.isInteger(index) || index < 0) return;
      try {
        const retryWithSelectionBypass =
          Number.isInteger(detail.attempt) &&
          Number(detail.attempt) >= 3 &&
          !snapshot.initialPlacement;
        if (detail.action === "settlement") {
          if (snapshot.initialPlacement) {
            socket.selectedInitialPlacementIndex?.(index);
          }
          if (
            retryWithSelectionBypass &&
            typeof socket.confirmBuildSettlementSkippingSelection ===
              "function"
          ) {
            socket.confirmBuildSettlementSkippingSelection(index);
          } else {
            socket.confirmBuildSettlement(index);
          }
        } else if (detail.action === "city") {
          if (
            retryWithSelectionBypass &&
            typeof socket.confirmBuildCitySkippingSelection === "function"
          ) {
            socket.confirmBuildCitySkippingSelection(index);
          } else {
            socket.confirmBuildCity(index);
          }
        } else if (detail.action === "road") {
          if (
            retryWithSelectionBypass &&
            typeof socket.confirmBuildRoadSkippingSelection === "function"
          ) {
            socket.confirmBuildRoadSkippingSelection(index);
          } else {
            socket.confirmBuildRoad(index);
          }
        } else {
          socket.selectedTile(index);
        }
        previousPayload = "";
        window.setTimeout(publish, 80);
        window.setTimeout(publish, 260);
      } catch {
        // Colonist validates the action and the next snapshot keeps advice live.
      }
    },
  );

  publish();
  window.setInterval(publish, 700);
  window.addEventListener("resize", publish, { passive: true });
  window.addEventListener("colonist-assistant-board-refresh", () => {
    previousPayload = "";
    publish();
    window.setTimeout(publish, 90);
    window.setTimeout(publish, 260);
    window.setTimeout(publish, 700);
  });
})();
