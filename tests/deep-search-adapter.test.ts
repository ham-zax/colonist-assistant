import { describe, expect, it } from "vitest";
import { readFile } from "node:fs/promises";

import { emptyResources } from "../src/core/resources";
import type { TrackerState } from "../src/core/types";
import type {
  BoardSnapshot,
  DevelopmentCardVector,
} from "../src/core/placement";
import {
  reweightTradeEvidence,
  seedPublicResourceWorlds,
} from "../src/core/tracker";
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

const development = (
  values: Partial<DevelopmentCardVector> = {},
): DevelopmentCardVector => ({
  knight: 0,
  monopoly: 0,
  "road-building": 0,
  "year-of-plenty": 0,
  "victory-point": 0,
  ...values,
});

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
      playedDevelopmentCards: development({ knight: 1 }),
      hasPlayedDevelopmentThisTurn: false,
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
  it("accepts a seeded public-board fallback posterior after a midgame attach", () => {
    const fallbackState: TrackerState = {
      ...state,
      worlds: seedPublicResourceWorlds({
        playerOrder: state.playerOrder,
        ownPlayer: "You",
        exactOwnHand: board.ownHand!,
        handSizes: { You: 4, Rival: 5 },
        resourceSupply: 19,
        seed: 54,
        sampleCount: 16,
      }),
    };

    const built = buildDeepSearchRequest(fallbackState, board, "You");
    const request = built.request as any;

    expect(fallbackState.worlds.length).toBeGreaterThan(0);
    expect(request.state.worlds.length).toBeGreaterThan(0);
    expect(
      request.state.worlds.every(
        (world: any) =>
          world.hands[1].reduce(
            (sum: number, count: number) => sum + count,
            0,
          ) === 5,
      ),
    ).toBe(true);
  });

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

  it("conserves full public development history and rejects a phantom deck remainder", async () => {
    const affordableHand = resources(0, 0, 1, 1, 1);
    const exhaustionState: TrackerState = {
      ...state,
      worlds: state.worlds.map((world) => ({
        ...world,
        hands: { ...world.hands, You: affordableHand },
      })),
    };
    const exhaustedBoard: BoardSnapshot = {
      ...board,
      ownHand: affordableHand,
      ownDevelopmentCards: {
        cards: development(),
        playable: development(),
        boughtThisTurn: development(),
        hasPlayedThisTurn: false,
      },
      players: {
        You: {
          ...board.players!.You!,
          handSize: 3,
          developmentCards: 0,
          playedDevelopmentCards: development(),
          hasPlayedDevelopmentThisTurn: false,
        },
        Rival: {
          ...board.players!.Rival!,
          developmentCards: 9,
          playedDevelopmentCards: development({
            knight: 10,
            monopoly: 2,
            "road-building": 2,
            "year-of-plenty": 2,
          }),
          hasPlayedDevelopmentThisTurn: true,
        },
      },
    };
    const built = buildDeepSearchRequest(exhaustionState, exhaustedBoard, "You");
    const request = built.request as any;

    expect(request.state.playedDevelopment).toEqual([10, 0, 2, 2, 2]);
    expect(request.state.players[1].playedDevelopmentThisTurn).toBe(true);
    expect(
      request.state.worlds.every((world: any) =>
        world.developmentDeck.every((count: number) => count === 0),
      ),
    ).toBe(true);
    expect(
      request.state.worlds.every((world: any) => {
        const held = world.development.flat().reduce(
          (sum: number, count: number) => sum + count,
          0,
        );
        const deck = world.developmentDeck.reduce(
          (sum: number, count: number) => sum + count,
          0,
        );
        const played = request.state.playedDevelopment.reduce(
          (sum: number, count: number) => sum + count,
          0,
        );
        return held + deck + played === 25;
      }),
    ).toBe(true);

    const bytes = await readFile(
      new URL(
        "../src/generated/wasm/colonist_search_bg.wasm",
        import.meta.url,
      ),
    );
    await initWasm({ module_or_path: bytes });
    const response = analyzeWasm(built.request);
    expect(
      response.actions.every((entry) => entry.action.kind !== "buy-development"),
    ).toBe(true);

    const impossibleBoard: BoardSnapshot = {
      ...exhaustedBoard,
      players: {
        ...exhaustedBoard.players,
        Rival: {
          ...exhaustedBoard.players!.Rival!,
          developmentCards: 10,
        },
      },
    };
    expect(() =>
      buildDeepSearchRequest(exhaustionState, impossibleBoard, "You"),
    ).toThrow(/development-card state integrity error/i);

    const impossiblePublicBoard: BoardSnapshot = {
      ...exhaustedBoard,
      players: {
        ...exhaustedBoard.players,
        Rival: {
          ...exhaustedBoard.players!.Rival!,
          developmentCards: 0,
          playedDevelopmentCards: development({ knight: 15 }),
        },
      },
    };
    expect(() =>
      buildDeepSearchRequest(exhaustionState, impossiblePublicBoard, "You"),
    ).toThrow(/knight public plays/i);
  }, 20_000);

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

  it("preserves the Bayesian accepted-trade posterior through the WASM boundary", async () => {
    const bytes = await readFile(
      new URL(
        "../src/generated/wasm/colonist_search_bg.wasm",
        import.meta.url,
      ),
    );
    await initWasm({ module_or_path: bytes });
    const unsupportedHands = [
      resources(0, 0, 5, 0, 0),
      resources(0, 0, 4, 1, 0),
      resources(0, 0, 4, 0, 1),
      resources(0, 0, 3, 2, 0),
      resources(0, 0, 3, 1, 1),
      resources(0, 0, 3, 0, 2),
      resources(0, 0, 2, 3, 0),
      resources(0, 0, 2, 2, 1),
      resources(0, 0, 2, 1, 2),
      resources(0, 0, 2, 0, 3),
      resources(0, 0, 1, 4, 0),
    ];
    const posterior = reweightTradeEvidence(
      {
        ...state,
        worlds: [
          {
            hands: {
              You: resources(1, 0, 0, 0, 0),
              Rival: resources(0, 1, 4, 0, 0),
            },
            weight: 1 / 12,
          },
          ...unsupportedHands.map((hand) => ({
            hands: {
              You: resources(1, 0, 0, 0, 0),
              Rival: hand,
            },
            weight: 1 / 12,
          })),
        ],
      },
      [
        {
          id: "accepted-hard-evidence",
          creator: "You",
          give: resources(1, 0, 0, 0, 0),
          receive: resources(0, 1, 0, 0, 0),
          acceptedPlayers: ["Rival"],
        },
      ],
    );
    const acceptedBoard: BoardSnapshot = {
      ...board,
      ownHand: resources(1, 0, 0, 0, 0),
      ownDevelopmentCards: {
        cards: {
          knight: 0,
          monopoly: 0,
          "road-building": 0,
          "year-of-plenty": 0,
          "victory-point": 0,
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
        hasPlayedThisTurn: false,
      },
      players: {
        You: {
          ...board.players!.You!,
          handSize: 1,
          developmentCards: 0,
        },
        Rival: {
          ...board.players!.Rival!,
          handSize: 5,
          developmentCards: 0,
        },
      },
      activeTrades: [
        {
          id: "accepted-hard-evidence",
          creator: "You",
          tradeExecutor: "You",
          creatorGive: resources(1, 0, 0, 0, 0),
          creatorReceive: resources(0, 1, 0, 0, 0),
          incoming: false,
          counterOffer: false,
          canAccept: false,
          acceptedPlayers: ["Rival"],
          pendingPlayers: [],
          rejectedPlayers: [],
          responsesComplete: true,
        },
      ],
    };
    const built = buildDeepSearchRequest(posterior, acceptedBoard, "You");
    const request = built.request as any;
    const supportedMass = request.state.worlds
      .filter((world: any) => (world.hands[1]?.[1] ?? 0) >= 1)
      .reduce((sum: number, world: any) => sum + world.weight, 0);
    const response = analyzeWasm(built.request);

    expect(supportedMass).toBeCloseTo(0.9889001, 6);
    expect(response.exactDecision).toBe(true);
    expect(response.authority).toBe("exact-mandatory");
    expect(["confirm-trade", "cancel-trade"]).toContain(response.chosen?.kind);
  });

  it("carries a rejected outgoing bundle into Rust without exhausting every trade root", () => {
    const give = resources(0, 1, 0, 0, 0);
    const receive = resources(0, 0, 0, 0, 1);
    const withRejectedPanel = buildDeepSearchRequest(
      state,
      {
        ...board,
        activeTrades: [
          {
            id: "trade-1",
            creator: "You",
            tradeExecutor: "You",
            creatorGive: give,
            creatorReceive: receive,
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
    expect(withRejectedPanel.state.domesticTradeUsed).toBe(true);

    const unconstrained = buildDeepSearchRequest(
      state,
      { ...board, activeTrades: [] },
      "You",
    ).request as any;
    expect(unconstrained.state.domesticTradeUsed).toBe(false);
    unconstrained.branchCap = 32;
    const raw = analyzeWasm(unconstrained);
    const generated = raw.actions.find(
      (candidate: any) => candidate.action.kind === "offer-trade",
    )?.action;
    if (!generated?.cards || !generated.receiveCards) {
      throw new Error("fixture must expose at least one generated domestic trade root");
    }
    const generatedCards = generated.cards;
    const generatedReceiveCards = generated.receiveCards;
    const [giveLumber, giveBrick, giveWool, giveGrain, giveOre] = generatedCards;
    const [receiveLumber, receiveBrick, receiveWool, receiveGrain, receiveOre] =
      generatedReceiveCards;
    const generatedGive = resources(
      giveLumber,
      giveBrick,
      giveWool,
      giveGrain,
      giveOre,
    );
    const generatedReceive = resources(
      receiveLumber,
      receiveBrick,
      receiveWool,
      receiveGrain,
      receiveOre,
    );

    const constrained = buildDeepSearchRequest(
      state,
      { ...board, activeTrades: [] },
      "You",
      {
        lastRejectedTrade: {
          give: generatedGive,
          receive: generatedReceive,
        },
      },
    ).request as any;
    const uiRetry = buildDeepSearchRequest(
      state,
      { ...board, activeTrades: [] },
      "You",
      {
        rootExclusions: [
          {
            kind: "offer-trade",
            give: generatedGive,
            receive: generatedReceive,
          },
        ],
      },
    ).request as any;

    expect(constrained.state.domesticTradeUsed).toBe(false);
    expect(constrained.lastRejectedTrade).toEqual({
      give: generatedCards,
      receive: generatedReceiveCards,
    });
    expect(uiRetry.rootExclusions).toEqual([
      {
        kind: "offer-trade",
        give: generatedCards,
        receive: generatedReceiveCards,
      },
    ]);
    constrained.branchCap = 32;
    uiRetry.branchCap = 32;
    const rejected = analyzeWasm(constrained);
    const retried = analyzeWasm(uiRetry);
    const matchesRejectedOffer = (candidate: any) =>
      candidate.action.kind === "offer-trade" &&
      candidate.action.cards?.join(",") === generatedCards.join(",") &&
      candidate.action.receiveCards?.join(",") ===
        generatedReceiveCards.join(",");
    expect(raw.actions.some(matchesRejectedOffer)).toBe(true);
    expect(rejected.actions.some(matchesRejectedOffer)).toBe(false);
    expect(retried.actions.some(matchesRejectedOffer)).toBe(false);
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
    expect([
      "exact-mandatory",
      "tactical-proven",
      "deep-maxn",
      "exact-family",
      "safety-override",
    ]).toContain(response.authority);
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
          // Active trades are stored relative to the creator (Rival).
          creatorGive: resources(1, 0, 0, 0, 0),
          creatorReceive: resources(0, 1, 0, 0, 0),
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
    expect(response.authority).toBe("exact-mandatory");
    expect(["respond-trade", "counter-trade"]).toContain(
      response.chosen?.kind,
    );
    const counter = response.actions.find(
      (candidate) => candidate.action.kind === "counter-trade",
    )?.action;
    expect(counter?.cards).toBeDefined();
    expect(counter?.receiveCards).toBeDefined();
    const retry = buildDeepSearchRequest(state, incomingBoard, "You", {
      rootExclusions: [
        {
          kind: "counter-trade",
          give: resources(...counter!.cards!),
          receive: resources(...counter!.receiveCards!),
        },
      ],
    });
    retry.request.mode = "puct";
    retry.request.iterations = 50_000;
    retry.request.maxNodes = 250_000;
    const retried = analyzeWasm(retry.request);
    expect(retried.exactDecision).toBe(true);
    expect(retried.authority).toBe("exact-mandatory");
    expect(
      retried.actions.some(
        (candidate) =>
          candidate.action.kind === "counter-trade" &&
          candidate.action.cards?.join(",") === counter!.cards!.join(",") &&
          candidate.action.receiveCards?.join(",") ===
            counter!.receiveCards!.join(","),
      ),
    ).toBe(false);
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
