import { describe, expect, it } from "vitest";
import { readFile } from "node:fs/promises";

import { emptyResources } from "../src/core/resources";
import type { TrackerState } from "../src/core/types";
import type { BoardSnapshot } from "../src/core/placement";
import { buildDeepSearchRequest } from "../src/worker/deep-search";
import initWasm, {
  analyze as analyzeWasm,
} from "../src/generated/wasm/colonist_search.js";

const resources = (
  lumber: number,
  brick: number,
  wool: number,
  grain: number,
  ore: number,
) => ({ lumber, brick, wool, grain, ore });

const state: TrackerState = {
  worlds: [
    {
      hands: {
        You: resources(1, 1, 1, 1, 0),
        Rival: resources(0, 0, 0, 2, 3),
      },
      weight: 0.7,
    },
    {
      hands: {
        You: resources(1, 1, 1, 1, 0),
        Rival: resources(0, 1, 0, 1, 3),
      },
      weight: 0.3,
    },
  ],
  players: {
    You: {
      name: "You",
      color: "#00f",
      devCards: [],
      playedDevCards: {
        knight: 0,
        monopoly: 0,
        "road-building": 0,
        "year-of-plenty": 0,
        "victory-point": 0,
        unknown: 0,
      },
      builds: { road: 0, settlement: 1, city: 0, development: 0 },
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
    },
    Rival: {
      name: "Rival",
      color: "#f00",
      devCards: [{ boughtOnTurn: 1 }],
      playedDevCards: {
        knight: 1,
        monopoly: 0,
        "road-building": 0,
        "year-of-plenty": 0,
        "victory-point": 0,
        unknown: 0,
      },
      builds: { road: 0, settlement: 1, city: 0, development: 1 },
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
    },
  },
  playerOrder: ["You", "Rival"],
  eventCount: 7,
  currentTurn: { player: "You", sequence: 3 },
  diceRolls: {},
  uncertaintyEvents: 1,
  possibilitiesTruncated: false,
  warnings: [],
  recentEvents: [],
  pendingTradeBehaviour: {},
};

const board: BoardSnapshot = {
  hexes: [
    { id: "h0", resource: "grain", number: 6, blocked: true },
    { id: "h1" },
  ],
  vertices: [
    {
      id: "v0",
      adjacentHexes: ["h0"],
      adjacentVertices: ["v1"],
      port: "grain",
      building: { player: "You", kind: "settlement" },
    },
    {
      id: "v1",
      adjacentHexes: ["h0", "h1"],
      adjacentVertices: ["v0", "v2"],
    },
    {
      id: "v2",
      adjacentHexes: ["h1"],
      adjacentVertices: ["v1"],
      building: { player: "Rival", kind: "settlement" },
    },
  ],
  edges: [
    { id: "e0", vertices: ["v0", "v1"] },
    { id: "e1", vertices: ["v1", "v2"] },
  ],
  myPlayer: "You",
  ownHand: resources(1, 1, 1, 1, 0),
  ownDevelopmentCards: {
    cards: {
      knight: 1,
      monopoly: 0,
      "road-building": 0,
      "year-of-plenty": 0,
      "victory-point": 0,
    },
    playable: {
      knight: 1,
      monopoly: 0,
      "road-building": 0,
      "year-of-plenty": 0,
      "victory-point": 0,
    },
    boughtThisTurn: {
      knight: 0,
      monopoly: 0,
      "road-building": 0,
      "year-of-plenty": 0,
      "victory-point": 0,
    },
    hasPlayedThisTurn: false,
  },
  players: {
    You: {
      handSize: 4,
      tradeRatios: resources(4, 4, 4, 2, 4),
      cardDiscardLimit: 7,
      developmentCards: 1,
      visiblePoints: 1,
    },
    Rival: {
      handSize: 5,
      tradeRatios: resources(4, 4, 4, 4, 4),
      cardDiscardLimit: 7,
      developmentCards: 1,
      playedKnights: 1,
      visiblePoints: 1,
    },
  },
  bankVisible: false,
  gameKey: "fixture",
  isMyTurn: true,
  action: "none",
  initialPlacement: false,
  victoryTarget: 10,
  hasRolled: true,
};

describe("deep-search state adapter", () => {
  it("preserves exact private evidence and public topology", () => {
    const built = buildDeepSearchRequest(state, board, "You");
    const request = built.request as any;

    expect(built.players).toEqual(["You", "Rival"]);
    expect(request.state.board.vertices[0].port).toBe(3);
    expect(request.state.board.edges[0].vertices).toEqual([0, 1]);
    expect(request.state.robberHex).toBe(0);
    expect(request.state.players[0].resources).toEqual([1, 1, 1, 1, 0]);
    expect(request.state.players[0].development).toEqual([1, 0, 0, 0, 0]);
    expect(request.state.worlds.length).toBeGreaterThan(2);
    expect(request.state.worlds.length).toBeLessThanOrEqual(32);
    expect(
      request.state.worlds.reduce(
        (sum: number, world: any) => sum + world.weight,
        0,
      ),
    ).toBeCloseTo(1, 6);
    expect(
      new Set(
        request.state.worlds.map((world: any) =>
          JSON.stringify(world.development[1]),
        ),
      ).size,
    ).toBeGreaterThan(1);
    expect(
      request.state.worlds.every(
        (world: any) =>
          world.boughtDevelopment[1].reduce(
            (total: number, count: number) => total + count,
            0,
          ) === 0,
      ),
    ).toBe(true);
    expect(request.state.worlds[0].hands[0]).toEqual([1, 1, 1, 1, 0]);
    expect(request.state.bankVisible).toBe(false);
    expect(
      request.state.worlds.every((world: any) =>
        world.bank?.every(
          (count: number, resource: number) =>
            count +
              world.hands.reduce(
                (sum: number, hand: number[]) =>
                  sum + (hand[resource] ?? 0),
                0,
              ) ===
            19,
        ) === true,
      ),
    ).toBe(true);
    expect(request.state.phase).toBe("main");
  });

  it("keeps newly bought opponent development cards unplayable in every world", () => {
    const justBoughtState: TrackerState = {
      ...state,
      currentTurn: { player: "Rival", sequence: 1 },
    };
    const request = buildDeepSearchRequest(
      justBoughtState,
      board,
      "You",
    ).request as any;

    expect(
      request.state.worlds.every(
        (world: any) =>
          world.boughtDevelopment[1].reduce(
            (total: number, count: number) => total + count,
            0,
          ) === 1,
      ),
    ).toBe(true);
    expect(
      request.state.worlds.every(
        (world: any) =>
          world.boughtDevelopment[1].every(
            (count: number, card: number) =>
              count <= world.development[1][card],
          ),
      ),
    ).toBe(true);
  });

  it("bootstraps hidden hands from public card counts before the log is ready", () => {
    const publicOnlyState: TrackerState = {
      ...state,
      worlds: Array.from({ length: 24 }, () => ({
        hands: {
          You: resources(1, 1, 1, 1, 0),
          Rival: resources(0, 0, 0, 0, 0),
        },
        weight: 1,
      })),
    };

    const request = buildDeepSearchRequest(
      publicOnlyState,
      board,
      "You",
    ).request as any;

    expect(request.state.worlds.length).toBeGreaterThanOrEqual(24);
    expect(request.state.worlds.length).toBeLessThanOrEqual(32);
    expect(
      request.state.worlds.every(
        (world: any) =>
          world.hands[1].reduce(
            (total: number, count: number) => total + count,
            0,
          ) === board.players?.Rival?.handSize,
      ),
    ).toBe(true);
  });

  it("models pre-roll and forced discard phases explicitly", () => {
    const preRoll = buildDeepSearchRequest(
      state,
      { ...board, hasRolled: false },
      "You",
    ).request as any;
    expect(preRoll.state.phase).toBe("pre-roll");

    const discard = buildDeepSearchRequest(
      state,
      { ...board, action: "discard", discardCount: 2 },
      "You",
    ).request as any;
    expect(discard.state.phase).toBe("discard");
    expect(discard.state.discardRemaining).toEqual([2, 0, 0, 0]);
  });

  it("anchors an opponent setup road to the acting player without making the root omniscient", () => {
    const opponentRoad = buildDeepSearchRequest(
      {
        ...state,
        currentTurn: { player: "Rival", sequence: 1 },
      },
      {
        ...board,
        initialPlacement: true,
        isMyTurn: false,
        currentPlayer: "Rival",
        action: "road",
      },
      "You",
    ).request as any;

    expect(opponentRoad.state.currentPlayer).toBe(1);
    expect(opponentRoad.state.phase).toBe("setup-road");
    expect(opponentRoad.state.phaseParameter).toBe(2);
  });

  it("treats a setup-road prompt without the acting player's public anchor as a stale settlement transition", () => {
    const withoutRivalAnchor: BoardSnapshot = {
      ...board,
      vertices: board.vertices.map((vertex) =>
        vertex.building?.player === "Rival"
          ? { ...vertex, building: undefined }
          : vertex
      ),
      initialPlacement: true,
      isMyTurn: false,
      currentPlayer: "Rival",
      action: "road",
    };
    const request = buildDeepSearchRequest(
      {
        ...state,
        currentTurn: { player: "Rival", sequence: 1 },
      },
      withoutRivalAnchor,
      "You",
    ).request as any;

    expect(request.state.currentPlayer).toBe(1);
    expect(request.state.phase).toBe("setup-settlement");
    expect(request.state.phaseParameter).toBeUndefined();
  });

  it("moves on after an outgoing trade has received its responses", () => {
    const withRejectedTrade = buildDeepSearchRequest(
      state,
      {
        ...board,
        activeTrades: [
          {
            id: "trade-1",
            creator: "You",
            tradeExecutor: "You",
            give: resources(0, 1, 0, 0, 0),
            receive: resources(1, 0, 0, 0, 0),
            incoming: false,
            counterOffer: false,
            canAccept: false,
            acceptedPlayers: [],
            pendingPlayers: [],
            rejectedPlayers: ["Rival"],
            responsesComplete: true,
          },
        ],
      },
      "You",
    ).request as any;

    expect(withRejectedTrade.state.domesticTradeUsed).toBe(true);

    const afterOfferCloses = buildDeepSearchRequest(
      state,
      {
        ...board,
        domesticTradeUsed: true,
        activeTrades: [],
      },
      "You",
    ).request as any;
    expect(afterOfferCloses.state.domesticTradeUsed).toBe(true);
  });

  it("crosses the packaged WASM boundary and returns a legal report", async () => {
    const bytes = await readFile(
      new URL(
        "../src/generated/wasm/colonist_search_bg.wasm",
        import.meta.url,
      ),
    );
    await initWasm({ module_or_path: bytes });
    const built = buildDeepSearchRequest(state, board, "You");
    const started = performance.now();
    const response = analyzeWasm(built.request);
    const elapsed = performance.now() - started;

    expect(built.request.mode).toBe("maxn");
    expect(built.request.depth).toBe(4);
    expect(built.request.maxNodes).toBe(4_000);
    expect(built.request.branchCap).toBe(8);
    expect(built.request.tacticalNodes).toBe(900);
    expect(built.request.timeBudgetMs).toBe(350);
    expect(response.algorithm).toBe("maxn");
    expect(response.engineRevision).toBe("deep-maxn-v9");
    expect(response.chosen).toBeDefined();
    expect(response.actions.length).toBeGreaterThan(0);
    expect(response.particles).toBe(built.request.state.worlds.length);
    expect(response.nodes).toBeLessThanOrEqual(built.request.maxNodes);
    expect(typeof response.deadlineReached).toBe("boolean");
    // This is deliberately the cold packaged boundary, not fastest-of-three.
    expect(elapsed).toBeLessThan(1_000);
  }, 20_000);

  it("rejects unknown WASM search modes instead of silently changing algorithms", async () => {
    const bytes = await readFile(
      new URL(
        "../src/generated/wasm/colonist_search_bg.wasm",
        import.meta.url,
      ),
    );
    await initWasm({ module_or_path: bytes });
    const built = buildDeepSearchRequest(state, board, "You");
    built.request.mode = "not-an-engine";

    expect(() => analyzeWasm(built.request)).toThrow(
      /unknown search mode/u,
    );
  });

  it("honors the cooperative MaxN deadline at the packaged WASM boundary", async () => {
    const bytes = await readFile(
      new URL(
        "../src/generated/wasm/colonist_search_bg.wasm",
        import.meta.url,
      ),
    );
    await initWasm({ module_or_path: bytes });
    const built = buildDeepSearchRequest(state, board, "You");
    built.request.depth = 6;
    built.request.branchCap = 32;
    built.request.maxNodes = 250_000;
    built.request.tacticalNodes = 100;
    built.request.timeBudgetMs = 250;
    const started = performance.now();
    const response = analyzeWasm(built.request);
    const elapsed = performance.now() - started;

    expect(response.deadlineReached).toBe(true);
    expect(response.nodes).toBeLessThan(built.request.maxNodes);
    expect(response.chosen).toBeDefined();
    expect(elapsed).toBeLessThan(2_000);
  }, 10_000);

  it("returns mandatory discard decisions without paying the strategic search budget", async () => {
    const bytes = await readFile(
      new URL(
        "../src/generated/wasm/colonist_search_bg.wasm",
        import.meta.url,
      ),
    );
    await initWasm({ module_or_path: bytes });
    const discardBoard: BoardSnapshot = {
      ...board,
      ownHand: resources(3, 2, 1, 1, 1),
      action: "discard",
      discardCount: 4,
    };
    const discardState: TrackerState = {
      ...state,
      worlds: state.worlds.map((world) => ({
        ...world,
        hands: {
          ...world.hands,
          You: resources(3, 2, 1, 1, 1),
        },
      })),
    };
    const built = buildDeepSearchRequest(
      discardState,
      discardBoard,
      "You",
    );
    built.request.mode = "puct";
    built.request.iterations = 50_000;
    const response = analyzeWasm(built.request);

    expect(response.exactDecision).toBe(true);
    expect(response.chosen?.kind).toBe("discard");
    expect(response.chosen?.cards?.reduce(
      (sum: number, count: number) => sum + count,
      0,
    )).toBe(4);
    expect(response.iterations).toBe(0);
    expect(response.rollouts).toBe(0);
  });

  it("answers incoming trades through the exact family without running strategic search", async () => {
    const bytes = await readFile(
      new URL(
        "../src/generated/wasm/colonist_search_bg.wasm",
        import.meta.url,
      ),
    );
    await initWasm({ module_or_path: bytes });
    const incomingBoard: BoardSnapshot = {
      ...board,
      isMyTurn: false,
      currentPlayer: "Rival",
      activeTrades: [
        {
          id: "incoming-fast-path",
          creator: "Rival",
          tradeExecutor: "Rival",
          // Board offers are normalized to the user's perspective.
          give: resources(0, 1, 0, 0, 0),
          receive: resources(1, 0, 0, 0, 0),
          incoming: true,
          counterOffer: false,
          canAccept: true,
          acceptedPlayers: [],
          pendingPlayers: ["You"],
          rejectedPlayers: [],
          responsesComplete: false,
          myResponse: "pending",
        },
      ],
    };
    const built = buildDeepSearchRequest(state, incomingBoard, "You");
    built.request.mode = "puct";
    built.request.iterations = 50_000;
    built.request.maxNodes = 250_000;
    const started = performance.now();
    const response = analyzeWasm(built.request);
    const elapsed = performance.now() - started;

    expect(response.exactDecision).toBe(true);
    expect(["respond-trade", "counter-trade"]).toContain(
      response.chosen?.kind,
    );
    expect(response.nodes).toBe(0);
    expect(response.iterations).toBe(0);
    expect(response.rollouts).toBe(0);
    expect(elapsed).toBeLessThan(1_000);
  });

  it("returns a universally forced end turn without strategic search", async () => {
    const bytes = await readFile(
      new URL(
        "../src/generated/wasm/colonist_search_bg.wasm",
        import.meta.url,
      ),
    );
    await initWasm({ module_or_path: bytes });
    const idleHand = resources(0, 0, 1, 0, 0);
    const idleState: TrackerState = {
      ...state,
      worlds: state.worlds.map((world) => ({
        ...world,
        hands: {
          ...world.hands,
          You: idleHand,
        },
      })),
    };
    const idleBoard: BoardSnapshot = {
      ...board,
      ownHand: idleHand,
      domesticTradeUsed: true,
      ownDevelopmentCards: {
        cards: {
          knight: 0,
          monopoly: 0,
          "road-building": 0,
          "year-of-plenty": 0,
          "victory-point": 1,
        },
        playable: {
          knight: 0,
          monopoly: 0,
          "road-building": 0,
          "year-of-plenty": 0,
          "victory-point": 0,
        },
        boughtThisTurn: {
          knight: 0,
          monopoly: 0,
          "road-building": 0,
          "year-of-plenty": 0,
          "victory-point": 0,
        },
        hasPlayedThisTurn: true,
      },
      players: {
        ...board.players,
        You: {
          ...board.players!.You!,
          handSize: 1,
          developmentCards: 1,
        },
      },
    };
    const built = buildDeepSearchRequest(
      idleState,
      idleBoard,
      "You",
    );
    built.request.mode = "maxn";
    built.request.maxNodes = 250_000;
    built.request.tacticalNodes = 100_000;
    const started = performance.now();
    const response = analyzeWasm(built.request);
    const elapsed = performance.now() - started;

    expect(response.exactDecision).toBe(true);
    expect(response.chosen?.kind).toBe("end-turn");
    expect(response.nodes).toBe(0);
    expect(response.iterations).toBe(0);
    expect(elapsed).toBeLessThan(100);
  });

  it("keeps experimental PUCT bounded outside the live default", async () => {
    const bytes = await readFile(
      new URL(
        "../src/generated/wasm/colonist_search_bg.wasm",
        import.meta.url,
      ),
    );
    await initWasm({ module_or_path: bytes });
    const built = buildDeepSearchRequest(state, board, "You");
    built.request.mode = "puct";
    built.request.iterations = 112;
    built.request.maxNodes = 8_000;
    built.request.rolloutActions = 72;
    built.request.tacticalNodes = 2_000;
    const started = performance.now();
    const response = analyzeWasm(built.request);
    const elapsed = performance.now() - started;

    expect(response.algorithm).toBe("puct");
    expect(response.iterations).toBeGreaterThan(0);
    expect(elapsed).toBeLessThan(1_500);
  }, 15_000);
});
