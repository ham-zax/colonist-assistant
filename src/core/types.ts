import type { BuildKind, Resource, ResourceVector } from "./resources";

export type DevCardKind =
  | "knight"
  | "monopoly"
  | "road-building"
  | "year-of-plenty"
  | "victory-point"
  | "unknown";

export type TrackerEvent =
  | { type: "discover"; player: string; color?: string }
  | {
      type: "gain";
      player: string;
      cards: ResourceVector;
      reason: "production" | "starting" | "bank" | "gold" | "other";
      color?: string;
    }
  | {
      type: "spend";
      player: string;
      cost: ResourceVector;
      reason: BuildKind;
      color?: string;
    }
  | {
      type: "transfer";
      from: string;
      to: string;
      cards: ResourceVector;
      reason: "robbery" | "trade";
      color?: string;
    }
  | {
      type: "trade";
      player: string;
      acceptingPlayer?: string;
      given: ResourceVector;
      received: ResourceVector;
      bank: boolean;
      color?: string;
    }
  | {
      type: "trade-offered";
      player: string;
      recipients: string[];
      give: ResourceVector;
      receive: ResourceVector;
      color?: string;
    }
  | {
      type: "trade-accepted";
      player: string;
      creator: string;
      give: ResourceVector;
      receive: ResourceVector;
      color?: string;
    }
  | {
      type: "trade-rejected";
      player: string;
      creator: string;
      give: ResourceVector;
      receive: ResourceVector;
      color?: string;
    }
  | {
      type: "trade-countered";
      player: string;
      creator: string;
      give: ResourceVector;
      receive: ResourceVector;
      counterGive: ResourceVector;
      counterReceive: ResourceVector;
      color?: string;
    }
  | {
      type: "trade-expired";
      player: string;
      recipients?: string[];
      give: ResourceVector;
      receive: ResourceVector;
      color?: string;
    }
  | {
      type: "unknown-transfer";
      from: string;
      to: string;
      count: number;
      color?: string;
    }
  | {
      type: "unknown-discard";
      player: string;
      count: number;
      color?: string;
    }
  | {
      type: "monopoly";
      player: string;
      resource: Resource;
      amount?: number;
      color?: string;
    }
  | { type: "buy-dev"; player: string; color?: string }
  | {
      type: "play-dev";
      player: string;
      card: DevCardKind;
      color?: string;
    }
  | {
      type: "roll";
      player: string;
      dice?: [number, number];
      color?: string;
    };

export type StoredEvent = TrackerEvent & {
  id: string;
  index?: number;
  timestamp: number;
  raw: string;
};

export interface PlayerMeta {
  name: string;
  color: string;
  devCards: Array<{ boughtOnTurn: number }>;
  playedDevCards: Record<DevCardKind, number>;
  builds: Record<BuildKind, number>;
  resourcesGained: ResourceVector;
  productionGained: ResourceVector;
  resourcesSpent: ResourceVector;
  opponentModel: {
    tradeAccepts: number;
    tradeRejects: number;
    offersMade: number;
    countersMade: number;
    policyPosterior: {
      balanced: number;
      expansion: number;
      cityDevelopment: number;
      tradeFlexible: number;
      tradeResistant: number;
    };
  };
}

export interface HandWorld {
  hands: Record<string, ResourceVector>;
  /** Normalized posterior mass carried through hidden-state updates. */
  weight: number;
}

export interface TrackerState {
  worlds: HandWorld[];
  players: Record<string, PlayerMeta>;
  playerOrder: string[];
  eventCount: number;
  currentTurn: { player?: string; sequence: number };
  diceRolls: Record<number, number>;
  uncertaintyEvents: number;
  possibilitiesTruncated: boolean;
  warnings: string[];
  recentEvents: StoredEvent[];
  /** Offer/accept behaviour keys already counted so panel + log do not double-count. */
  countedTradeBehaviour: Record<string, true>;
}

export interface ResourceEstimate {
  minimum: ResourceVector;
  maximum: ResourceVector;
  average: ResourceVector;
  totalMinimum: number;
  totalMaximum: number;
  possibilities: number;
  approximate: boolean;
}

export interface ParsedLogSnapshot {
  index?: number;
  visibleText: string;
  serialText: string;
  color?: string;
  language?: string;
}

export interface ParseResult {
  event: TrackerEvent;
  confidence: "exact" | "uncertain";
}
