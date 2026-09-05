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
import {
  M0_FAIR_IID_2D6_V1,
  MREF_COLONIST_LINKED_2024_V1,
  PUBLIC_HISTORY_BELIEF_V1,
  type PublicStochasticInput,
} from "../src/core/dice-history";
import {
  buildDeepSearchRequest,
  selectRepresentativeWorlds,
} from "../src/worker/deep-search";
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
  diceMode: "unknown",
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
  it("propagates all four canonical dice modes without changing the search seed", () => {
    const modes = ["unknown", "random", "balanced", "unsupported"] as const;
    const built = modes.map((diceMode) =>
      buildDeepSearchRequest(
        state,
        {
          ...board,
          diceMode,
          ...(diceMode === "unsupported" ? { diceModeRaw: 2 } : {}),
        },
        "You",
      ),
    );

    expect(built.map(({ request }) => request.state.diceMode)).toEqual(modes);
    expect(new Set(built.map(({ request }) => request.seed)).size).toBe(1);
    expect(built[3]?.request.state).not.toHaveProperty("diceModeRaw");
  });

  it("preserves the v12 M0 seed while serializing M_ref as public stochastic input", () => {
    const explicitM0: PublicStochasticInput = { model: M0_FAIR_IID_2D6_V1 };
    const reference: PublicStochasticInput = {
      model: MREF_COLONIST_LINKED_2024_V1,
      beliefPolicy: PUBLIC_HISTORY_BELIEF_V1,
      playerMapping: ["You", "Rival"],
      rolls: [
        { ordinal: 0, actor: 0, total: 8 },
        { ordinal: 1, actor: 1, total: 6 },
      ],
      provenance: "complete-from-first-gameplay-roll",
      diceHistoryDigest: "public-history-test",
    };
    const legacy = buildDeepSearchRequest(state, board, "You", {}, true, 24).request as any;
    const m0 = buildDeepSearchRequest(
      state,
      board,
      "You",
      {},
      true,
      24,
      explicitM0,
    ).request as any;
    const mref = buildDeepSearchRequest(
      state,
      board,
      "You",
      {},
      true,
      24,
      reference,
    ).request as any;

    expect(m0.seed).toBe(legacy.seed);
    expect(m0).not.toHaveProperty("stochastic");
    expect(mref.seed).not.toBe(legacy.seed);
    expect(mref.stochastic).toEqual(reference);
    expect(mref.state.worlds.length).toBeLessThanOrEqual(24);
    expect(mref.state.worlds.every((world: any) => !("stochastic" in world))).toBe(true);
  });

  it("binds M_ref actors to the exact canonical engine seat ordering and rejects reordering", () => {
    const canonicalBoard: BoardSnapshot = {
      ...board,
      playerOrder: ["Rival", "You"],
    };
    const canonicalReference: PublicStochasticInput = {
      model: MREF_COLONIST_LINKED_2024_V1,
      beliefPolicy: PUBLIC_HISTORY_BELIEF_V1,
      playerMapping: ["Rival", "You"],
      rolls: [
        { ordinal: 0, actor: 1, total: 8 },
        { ordinal: 1, actor: 0, total: 6 },
      ],
      provenance: "complete-from-first-gameplay-roll",
      diceHistoryDigest: "canonical-seat-order",
    };
    const built = buildDeepSearchRequest(
      state,
      canonicalBoard,
      "You",
      {},
      true,
      24,
      canonicalReference,
    );

    expect(built.players).toEqual(["Rival", "You"]);
    expect((built.request as any).stochastic.playerMapping).toEqual(built.players);

    expect(() =>
      buildDeepSearchRequest(
        state,
        canonicalBoard,
        "You",
        {},
        true,
        24,
        {
          ...canonicalReference,
          playerMapping: ["You", "Rival"],
        },
      ),
    ).toThrow(/canonical engine player ordering/);
  });

  it("executes M_ref deterministically in WASM and fails closed on unknown provenance", async () => {
    const bytes = await readFile(
      new URL(
        "../src/generated/wasm/colonist_search_bg.wasm",
        import.meta.url,
      ),
    );
    await initWasm({ module_or_path: bytes });
    const reference: PublicStochasticInput = {
      model: MREF_COLONIST_LINKED_2024_V1,
      beliefPolicy: PUBLIC_HISTORY_BELIEF_V1,
      playerMapping: ["You", "Rival"],
      rolls: [
        { ordinal: 0, actor: 0, total: 8 },
        { ordinal: 1, actor: 1, total: 6 },
      ],
      provenance: "complete-from-first-gameplay-roll",
      diceHistoryDigest: "mref-wasm-history",
    };
    const built = buildDeepSearchRequest(
      state,
      board,
      "You",
      {},
      true,
      24,
      reference,
    );
    const first = analyzeWasm(built.request);
    const second = analyzeWasm(built.request);

    expect(first.stochasticModel).toBe(MREF_COLONIST_LINKED_2024_V1);
    expect(first.beliefPolicy).toBe(PUBLIC_HISTORY_BELIEF_V1);
    expect(first.diceHistoryProvenance).toBe("complete-from-first-gameplay-roll");
    expect(first.publicHistoryDigest).toBe("mref-wasm-history");
    expect(first.stochasticBeliefParticleCount).toBe(1);
    expect(second.stochasticBeliefDigest).toBe(first.stochasticBeliefDigest);
    expect(second.chosen).toEqual(first.chosen);

    const unavailable = structuredClone(built.request) as any;
    unavailable.stochastic.provenance = "unknown";
    expect(() => analyzeWasm(unavailable)).toThrow(/reference stochastic history unavailable/);
  });

  it("preserves exact resource worlds and weights when no lossy sample is needed", () => {
    const source = [
      {
        hands: {
          You: resources(1, 1, 0, 0, 0),
          Rival: resources(2, 0, 0, 0, 0),
        },
        weight: 0.25,
      },
      {
        hands: {
          You: resources(1, 1, 0, 0, 0),
          Rival: resources(0, 2, 0, 0, 0),
        },
        weight: 0.75,
      },
    ];

    const selected = selectRepresentativeWorlds(
      source,
      ["You", "Rival"],
      24,
      91,
    );

    expect(selected).toHaveLength(2);
    expect(selected.map((world) => world.weight).sort()).toEqual([0.25, 0.75]);
    expect(
      selected.map((world) => JSON.stringify(world.hands.Rival)).sort(),
    ).toEqual(
      source.map((world) => JSON.stringify(world.hands.Rival)).sort(),
    );
  });

  it("samples resource posterior mass without nearest-state reassignment", () => {
    const source = Array.from({ length: 8 }, (_, index) => ({
      hands: {
        You: resources(1, 1, 0, 0, 0),
        Rival: resources(index, 8 - index, 0, 0, 0),
      },
      weight: 1 / 8,
    }));

    const first = selectRepresentativeWorlds(
      source,
      ["You", "Rival"],
      4,
      1234,
    );
    const second = selectRepresentativeWorlds(
      source,
      ["You", "Rival"],
      4,
      1234,
    );
    const sourceKeys = new Set(
      source.map((world) => JSON.stringify(world.hands.Rival)),
    );

    expect(second).toEqual(first);
    expect(first.reduce((sum, world) => sum + world.weight, 0)).toBeCloseTo(1, 8);
    expect(first.every((world) => sourceKeys.has(JSON.stringify(world.hands.Rival)))).toBe(true);
    expect(
      first.every((world) =>
        Math.abs(world.weight * 4 - Math.round(world.weight * 4)) < 1e-9,
      ),
    ).toBe(true);
  });

  it("samples hidden development identities directly into ready and bought slots", () => {
    const developmentState = structuredClone(state);
    developmentState.players.Rival!.devCards = [
      { boughtOnTurn: 1 },
      { boughtOnTurn: developmentState.currentTurn.sequence },
    ];
    const developmentBoard = structuredClone(board);
    developmentBoard.ownDevelopmentCards = {
      cards: development(),
      playable: development(),
      boughtThisTurn: development(),
      hasPlayedThisTurn: false,
    };
    developmentBoard.players!.You = {
      ...developmentBoard.players!.You!,
      developmentCards: 0,
      playedDevelopmentCards: development({
        knight: 12,
        monopoly: 1,
        "road-building": 2,
        "year-of-plenty": 2,
        "victory-point": 5,
      }),
    };
    developmentBoard.players!.Rival = {
      ...developmentBoard.players!.Rival!,
      developmentCards: 2,
      playedDevelopmentCards: development({ knight: 1 }),
    };

    const built = buildDeepSearchRequest(
      developmentState,
      developmentBoard,
      "You",
      {},
      true,
      24,
    );
    const worlds = (built.request as any).state.worlds as any[];
    const boughtKnightMass = worlds
      .filter((world) => world.boughtDevelopment[1][0] === 1)
      .reduce((sum, world) => sum + world.weight, 0);
    const boughtMonopolyMass = worlds
      .filter((world) => world.boughtDevelopment[1][4] === 1)
      .reduce((sum, world) => sum + world.weight, 0);

    expect(worlds.reduce((sum, world) => sum + world.weight, 0)).toBeCloseTo(1, 8);
    expect(boughtKnightMass).toBeCloseTo(0.5, 8);
    expect(boughtMonopolyMass).toBeCloseTo(0.5, 8);
    expect(
      worlds.every(
        (world) =>
          world.development[1][0] === 1 &&
          world.development[1][4] === 1 &&
          world.boughtDevelopment[1].reduce(
            (sum: number, count: number) => sum + count,
            0,
          ) === 1,
      ),
    ).toBe(true);
  });

  it("constructs deterministic 24/48/96 joint samples from one semantic seed", () => {
    const request24a = buildDeepSearchRequest(state, board, "You", {}, true, 24).request as any;
    const request24b = buildDeepSearchRequest(state, board, "You", {}, true, 24).request as any;
    const request48 = buildDeepSearchRequest(state, board, "You", {}, true, 48).request as any;
    const request96 = buildDeepSearchRequest(state, board, "You", {}, true, 96).request as any;

    expect(request24b).toEqual(request24a);
    expect(request24a.seed).toBe(request48.seed);
    expect(request24a.seed).toBe(request96.seed);
    expect(request24a.state.worlds.length).toBeLessThanOrEqual(24);
    expect(request48.state.worlds.length).toBeLessThanOrEqual(48);
    expect(request96.state.worlds.length).toBeLessThanOrEqual(96);
    for (const request of [request24a, request48, request96]) {
      expect(
        request.state.worlds.reduce(
          (sum: number, world: any) => sum + world.weight,
          0,
        ),
      ).toBeCloseTo(1, 8);
    }
  });

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
    const rejectedPanel = buildDeepSearchRequest(
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
    );
    const withRejectedPanel = rejectedPanel.request as any;
    expect(withRejectedPanel.state.domesticTradeUsed).toBe(true);
    expect(withRejectedPanel.state.phase).toBe("trade-responses");
    expect(withRejectedPanel.state.trade).toMatchObject({
      creator: 0,
      recipients: 2,
      accepted: 0,
      rejected: 2,
    });
    const rejectedPanelResponse = analyzeWasm(rejectedPanel.request);
    expect(rejectedPanelResponse.exactDecision).toBe(true);
    expect(rejectedPanelResponse.authority).toBe("exact-mandatory");
    expect(rejectedPanelResponse.chosen?.kind).toBe("cancel-trade");

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

  it("crosses the packaged WASM boundary inside the cold smoke budget", async () => {
    const bytes = await readFile(
      new URL(
        "../src/generated/wasm/colonist_search_bg.wasm",
        import.meta.url,
      ),
    );
    await initWasm({ module_or_path: bytes });
    const built = buildDeepSearchRequest(state, board, "You");

    // The live request deliberately keeps a larger quality budget. This cold
    // package smoke uses a short cooperative deadline so its <1s assertion
    // measures startup/bridge regressions rather than capping live search quality.
    expect(built.request.mode).toBe("maxn");
    expect(built.request.depth).toBe(5);
    expect(built.request.maxNodes).toBe(8_000);
    expect(built.request.branchCap).toBe(10);
    expect(built.request.tacticalNodes).toBe(900);
    expect(built.request.timeBudgetMs).toBe(2_000);
    expect(built.request.effort?.decisionTimeMs).toBe(2_000);
    expect(built.request.effort?.cpu.evidenceEscalationMs).toBe(2_500);

    const coldSmokeBudgetMs = 350;
    built.request.timeBudgetMs = coldSmokeBudgetMs;
    built.request.effort = {
      ...built.request.effort!,
      decisionTimeMs: coldSmokeBudgetMs,
      cpu: {
        ...built.request.effort!.cpu,
        evidenceEscalationMs: 0,
      },
    };
    const started = performance.now();
    const response = analyzeWasm(built.request);
    const elapsed = performance.now() - started;
    expect(response.algorithm).toBe("maxn");
    expect(response.engineRevision).toBe("deep-maxn-v12");
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
    expect(response.wasmParticles).toBe(built.request.state.worlds.length);
    expect(response.rustPosteriorParticles).toBe(response.wasmParticles);
    expect(response.rustSearchParticles).toBeGreaterThan(0);
    expect(response.rustSearchParticles).toBeLessThanOrEqual(
      response.rustPosteriorParticles,
    );
    expect(response.rootProvenance.rankedRootCount).toBeGreaterThanOrEqual(
      response.rootProvenance.retainedRoots.length,
    );
    expect(response.rootProvenance.retainedRoots.length).toBeLessThanOrEqual(
      built.request.branchCap,
    );
    expect(
      response.rootProvenance.retainedRoots.every(
        (candidate) =>
          candidate.preTruncationRank === undefined ||
          candidate.preTruncationRank > 0,
      ),
    ).toBe(true);
    expect(
      response.rootProvenance.retainedRoots.every(
        (candidate) => candidate.allocatedNodes > 0,
      ),
    ).toBe(true);
    expect([
      "exact-mandatory",
      "tactical-proven",
      "deep-maxn",
      "exact-family",
      "safety-override",
    ]).toContain(response.authorityTrace.initialAuthority);
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
    built.request.effort = {
      decisionTimeMs: 50,
      tactical: { maxDepth: 4, nodeBudget: 100 },
      cpu: {
        maxDepth: 6,
        rootCap: 32,
        nodesPerDepthWave: 250_000,
        evidenceEscalationMs: 0,
      },
      gpu: built.request.effort!.gpu,
    };
    const started = performance.now();
    const response = analyzeWasm(built.request);
    const elapsed = performance.now() - started;

    expect(response.deadlineReached).toBe(true);
    expect(response.nodes).toBeLessThan(
      built.request.effort!.cpu.nodesPerDepthWave,
    );
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

  it("answers pending incoming trades through the bounded strategic family", async () => {
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
    built.request.iterations = 512;
    built.request.maxNodes = 8_000;
    built.request.effort = {
      ...built.request.effort!,
      decisionTimeMs: 750,
    };
    const started = performance.now();
    const response = analyzeWasm(built.request);
    const elapsed = performance.now() - started;

    expect(response.exactDecision).toBe(false);
    expect(response.authority).not.toBe("exact-mandatory");
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
    retry.request.iterations = 512;
    retry.request.maxNodes = 8_000;
    retry.request.effort = {
      ...retry.request.effort!,
      decisionTimeMs: 750,
    };
    const retried = analyzeWasm(retry.request);
    expect(retried.exactDecision).toBe(false);
    expect(retried.authority).not.toBe("exact-mandatory");
    expect(
      retried.actions.some(
        (candidate) =>
          candidate.action.kind === "counter-trade" &&
          candidate.action.cards?.join(",") === counter!.cards!.join(",") &&
          candidate.action.receiveCards?.join(",") ===
            counter!.receiveCards!.join(","),
      ),
    ).toBe(false);
    expect(response.nodes + response.iterations + response.rollouts).toBeGreaterThan(0);
    expect(elapsed).toBeLessThan(5_000);
  });

  it("disables player negotiations while keeping bank and port trades legal across packaged WASM", async () => {
    const bytes = await readFile(
      new URL(
        "../src/generated/wasm/colonist_search_bg.wasm",
        import.meta.url,
      ),
    );
    await initWasm({ module_or_path: bytes });

    const maritimeHand = resources(0, 0, 0, 4, 0);
    const disabledState: TrackerState = {
      ...state,
      worlds: [
        {
          weight: 1,
          hands: {
            You: maritimeHand,
            Rival: resources(1, 0, 0, 0, 0),
          },
        },
      ],
    };
    const disabledBoard: BoardSnapshot = {
      ...board,
      ownHand: maritimeHand,
      players: {
        You: {
          ...board.players!.You!,
          handSize: 4,
          tradeRatios: resources(4, 4, 4, 2, 4),
        },
        Rival: {
          ...board.players!.Rival!,
          handSize: 1,
        },
      },
      activeTrades: [],
    };
    const main = buildDeepSearchRequest(
      disabledState,
      disabledBoard,
      "You",
      {},
      false,
    );
    // Domestic trading remains globally modeled for opponents while the
    // root-seat bitmask disables only the local player's negotiations.
    expect((main.request as any).state.playerTradesEnabled).toBe(true);
    expect((main.request as any).state.domesticTradeDisabled).toBe(1);
    main.request.branchCap = 32;
    const mainResponse = analyzeWasm(main.request);
    expect(
      mainResponse.actions.some(
        (candidate) => candidate.action.kind === "offer-trade",
      ),
    ).toBe(false);
    expect(
      mainResponse.actions.some(
        (candidate) => candidate.action.kind === "maritime-trade",
      ),
    ).toBe(true);

    const incomingBoard: BoardSnapshot = {
      ...disabledBoard,
      isMyTurn: false,
      currentPlayer: "Rival",
      activeTrades: [
        {
          id: "disabled-incoming",
          creator: "Rival",
          tradeExecutor: "Rival",
          creatorGive: resources(1, 0, 0, 0, 0),
          creatorReceive: resources(0, 0, 0, 1, 0),
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
    const incoming = buildDeepSearchRequest(
      disabledState,
      incomingBoard,
      "You",
      {},
      false,
    );
    const incomingResponse = analyzeWasm(incoming.request);
    expect(incomingResponse.chosen).toMatchObject({
      kind: "respond-trade",
      accept: false,
    });
    expect(
      incomingResponse.actions.some(
        (candidate) =>
          candidate.action.kind === "counter-trade" ||
          (candidate.action.kind === "respond-trade" &&
            candidate.action.accept === true),
      ),
    ).toBe(false);

    const acceptedBoard: BoardSnapshot = {
      ...disabledBoard,
      activeTrades: [
        {
          id: "disabled-accepted",
          creator: "You",
          tradeExecutor: "You",
          creatorGive: resources(0, 0, 0, 1, 0),
          creatorReceive: resources(1, 0, 0, 0, 0),
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
    const accepted = buildDeepSearchRequest(
      disabledState,
      acceptedBoard,
      "You",
      {},
      false,
    );
    const acceptedResponse = analyzeWasm(accepted.request);
    expect(acceptedResponse.chosen?.kind).toBe("cancel-trade");
    expect(
      acceptedResponse.actions.some(
        (candidate) => candidate.action.kind === "confirm-trade",
      ),
    ).toBe(false);
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
