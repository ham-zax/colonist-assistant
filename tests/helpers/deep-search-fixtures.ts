import type {
  ActiveTradeOffer,
  BoardPlayerPublicState,
  BoardSnapshot,
  DevelopmentCardVector,
} from "../../src/core/placement";
import { emptyResources, type ResourceVector } from "../../src/core/resources";
import { createTrackerState, reduceTracker } from "../../src/core/tracker";
import type { TrackerState } from "../../src/core/types";

export const resources = (
  values: Partial<ResourceVector> = {},
): ResourceVector => ({ ...emptyResources(), ...values });

export const development = (
  values: Partial<DevelopmentCardVector> = {},
): DevelopmentCardVector => ({
  knight: 0,
  monopoly: 0,
  "road-building": 0,
  "year-of-plenty": 0,
  "victory-point": 0,
  ...values,
});

export const publicPlayer = (
  overrides: Partial<BoardPlayerPublicState> = {},
): BoardPlayerPublicState => ({
  handSize: 0,
  tradeRatios: resources({ lumber: 4, brick: 4, wool: 4, grain: 4, ore: 4 }),
  cardDiscardLimit: 7,
  developmentCards: 0,
  playedKnights: 0,
  visiblePoints: 0,
  ...overrides,
});

export const makeState = (
  names: string[],
  overrides: Partial<TrackerState> = {},
): TrackerState => {
  let state = createTrackerState();
  for (const player of names) {
    state = reduceTracker(state, { type: "discover", player });
  }
  state.currentTurn = { player: names[0], sequence: 1 };
  state.eventCount = 1;
  state.recentEvents = [];
  state.worlds = [
    {
      weight: 1,
      hands: Object.fromEntries(names.map((name) => [name, resources()])),
    },
  ];
  return Object.assign(state, overrides);
};

export const makeBoard = (
  names: string[],
  overrides: Partial<BoardSnapshot> = {},
): BoardSnapshot => ({
  hexes: [{ id: "h0", number: 0, blocked: true }],
  vertices: [],
  edges: [],
  playerOrder: [...names],
  myPlayer: names[0],
  ownHand: resources(),
  players: Object.fromEntries(names.map((name) => [name, publicPlayer()])),
  currentPlayer: names[0],
  isMyTurn: true,
  action: "none",
  initialPlacement: false,
  hasRolled: true,
  turn: 1,
  victoryTarget: 10,
  ...overrides,
});

export const incomingTrade = (
  creator: string,
  overrides: Partial<ActiveTradeOffer> = {},
): ActiveTradeOffer => ({
  id: "trade-1",
  creator,
  tradeExecutor: creator,
  creatorGive: resources({ brick: 1 }),
  creatorReceive: resources({ lumber: 1 }),
  incoming: true,
  counterOffer: false,
  canAccept: true,
  myResponse: "pending",
  ...overrides,
});
