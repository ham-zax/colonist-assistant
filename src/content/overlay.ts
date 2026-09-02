import {
  createCoachReport,
  type CoachReport,
} from "../core/coach";
import {
  applyConfirmedPlacement,
  PLACEMENT_SYNC_TIMEOUT_MS,
  placementConfirmedByPublicLog,
  placementHasAdvanced,
  placementIsAwaitingSync,
  type PendingBoardPlacement,
} from "../core/board-progress";
import {
  recommendDiscard,
  type DiscardRecommendation,
} from "../core/discard";
import {
  DecisionTraceRecorder,
  type DecisionActionSource,
} from "../core/decision-trace";
import {
  downloadRecordedGame,
  GameRecordRecorder,
  readRecordedGame,
} from "../core/game-record";
import {
  shouldFastTrackEndTurn,
  shouldFastTrackRoll,
} from "../core/forced-action";
import {
  NUMBER_PIPS,
  scoreCityPlacements,
  scoreRoadPlacements,
  scoreRobberPlacements,
  scoreSettlementPlacements,
  type BoardAction,
  type BoardPoint,
  type BoardSnapshot,
  type PlacementContext,
  type PlacementRecommendation,
} from "../core/placement";
import {
  BUILD_COSTS,
  RESOURCE_COLORS,
  RESOURCE_LABELS,
  RESOURCE_ORDER,
  cloneResources,
  emptyResources,
  hasResources,
  resourceTotal,
  type BuildKind,
  type Resource,
  type ResourceVector,
} from "../core/resources";
import {
  createTrackerState,
  getPlayerEstimate,
  reconcilePublicResourceEvidence,
  reduceTracker,
  seedPublicResourceWorlds,
} from "../core/tracker";
import {
  likelyUpgradePath,
  playerBoardProfile,
} from "../core/strategy";
import {
  evaluateTradeOffer,
  localTradeBundles,
} from "../core/trades";
import {
  isFullySpecifiedTrade,
  outgoingTradeDisposition,
  tradeMemoryScopeChanged,
  tradeOfferKey,
  unansweredIncomingTrades,
} from "../core/trade-guard";
import {
  snapshotActiveTrades,
  tradeBeliefEventsFromDiff,
  type TradeOfferSnapshot,
} from "../core/trade-beliefs";
import type { TradeVerdict } from "../core/trades";
import type {
  DecisionAnalysis,
  DecisionAuthority,
  DecisionRuntime,
  DecisionSearchConstraints,
  DomesticTradeState,
  RootTradeActionExclusion,
} from "../core/engine";
import {
  isSearchDecisionRuntime,
  isWasmDecisionEngine,
} from "../core/engine";
import type { TrackerState } from "../core/types";
import { WinPredictionStabilizer } from "../core/win-prediction";
import type { GameSession } from "./session";
import {
  AUTOPILOT_DELAY_OPTIONS,
  normalizeAutopilotDelaySeconds,
  normalizeDecisionEngine,
  readPosition,
  saveSettings,
  type AssistantSettings,
  type OverlayPosition,
  savePosition,
} from "./settings";
import {
  EXTENSION_CONTEXT_RELOAD_MESSAGE,
  isExtensionContextInvalidatedError,
} from "./extension-context";
import { OVERLAY_STYLES } from "./styles";
import {
  destroyTradeVerdicts,
  renderTradeVerdicts,
} from "./trade-verdicts";
import {
  activeWorkflowAction,
  destroyActionGuide,
  renderActionGuide,
  tradePanelIsOpen,
  visibleTurnControl,
  type NextClick,
} from "./action-guide";
import { destroyWinOdds, renderWinOdds } from "./win-odds";
import { DecisionWorkerClient } from "./decision-worker";
import { InteractionRenderGate } from "./render-gate";

type ViewName = "advice" | "cards" | "settings";

const STRATEGIST_LABEL = "Strategist ★";
const WEIGHTED_LABEL = "Weighted";

const resourceSupplyForPlayerCount = (playerCount: number): number =>
  playerCount > 6 ? 29 : playerCount > 4 ? 24 : 19;

const publicResourceSeed = (value: string): number => {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash >>> 0;
};

export const autonomousExecutionAllowed = (enabled: boolean): boolean =>
  enabled;

const escapeHtml = (value: string): string =>
  value.replace(/[&<>"']/g, (character) => {
    const entities: Record<string, string> = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#039;",
    };
    return entities[character]!;
  });

const safeColor = (value: string): string =>
  /^(?:#[0-9a-f]{3,8}|rgba?\([\d\s,.%]+\)|hsla?\([\d\s,.%]+\))$/iu.test(value)
    ? value
    : "#7b93a3";

const assistantMark = () => `
  <svg viewBox="0 0 32 32" aria-hidden="true">
    <path d="M16 2.8 27.4 9.4v13.2L16 29.2 4.6 22.6V9.4L16 2.8Z" fill="none" stroke="currentColor" stroke-width="1.7"/>
    <path d="M10 18.8 16 8l6 10.8M12.2 15h7.6M9.1 22.1h13.8" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="square"/>
    <circle cx="16" cy="8" r="1.8" fill="currentColor"/>
  </svg>`;

const resourceIcon = (resource: Resource): string => {
  const paths: Record<Resource, string> = {
    lumber:
      '<path d="M12 2.5 5.2 10h3.2L4 15h5v5h6v-5h5l-4.4-5h3.2L12 2.5Z" fill="currentColor"/>',
    brick:
      '<path d="M3 5h8v6H3V5Zm10 0h8v6h-8V5ZM7 13h8v6H7v-6Zm-4 0h2v6H3v-6Zm14 0h4v6h-4v-6Z" fill="currentColor"/>',
    wool:
      '<path d="M7.5 18.5a4 4 0 0 1-1.3-7.8A4.5 4.5 0 0 1 14.6 8a4 4 0 0 1 2 7.5l-1.1 3H7.5Z" fill="currentColor"/><path d="M9 18v3m6-3v3" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>',
    grain:
      '<path d="M12 21V5m0 4C8 9 7 6 7 4c4 0 5 3 5 5Zm0 5c-4 0-5-3-5-5 4 0 5 3 5 5Zm0 5c-4 0-5-3-5-5 4 0 5 3 5 5Zm0-10c4 0 5-3 5-5-4 0-5 3-5 5Zm0 5c4 0 5-3 5-5-4 0-5 3-5 5Zm0 5c4 0 5-3 5-5-4 0-5 3-5 5Z" fill="currentColor"/>',
    ore:
      '<path d="m4 14 3-8 7-3 6 5-1 9-7 4-8-7Z" fill="currentColor"/><path d="m9 8 5-2 3 3-1 5-5 2-3-3 1-5Z" fill="white" opacity=".25"/>',
  };
  return `<svg viewBox="0 0 24 24" aria-hidden="true">${paths[resource]}</svg>`;
};

type PieceAsset =
  | "road"
  | "settlement"
  | "city"
  | "development"
  | "robber"
  | "longestRoad"
  | "largestArmy"
  | "knight"
  | "monopoly"
  | "roadBuilding"
  | "yearOfPlenty"
  | "victoryPoint";

const pieceFallback = (piece: PieceAsset): string => {
  const shapes: Record<PieceAsset, string> = {
    road:
      '<path d="M4 16.5 18.5 6l2 3L6 20Z" fill="currentColor"/><path d="m5.5 15.8 2.1 2.8" stroke="white" opacity=".3"/>',
    settlement:
      '<path d="m4 11 8-7 8 7v9H4Z" fill="currentColor"/><path d="M9 20v-6h6v6" fill="white" opacity=".28"/>',
    city:
      '<path d="M3 9h8v11H3Zm8-3h5v14h-5Zm5 5h5v9h-5Z" fill="currentColor"/><path d="M6 12h2m6-3v3m4 2v3" stroke="white" opacity=".32"/>',
    development:
      '<rect x="5" y="3" width="12" height="17" fill="currentColor"/><path d="M9 3V1h11v16h-3" fill="none" stroke="currentColor" stroke-width="1.8"/>',
    robber:
      '<path d="M12 2a4 4 0 0 1 4 4c0 1.4-.7 2.7-1.7 3.4 2.6 1 4.2 3.4 4.2 6.1V21h-13v-5.5c0-2.7 1.7-5.1 4.2-6.1A4.1 4.1 0 0 1 8 6a4 4 0 0 1 4-4Z" fill="currentColor"/>',
    longestRoad:
      '<path d="M3 17.5 8.5 12l3 2.8 8-8" fill="none" stroke="currentColor" stroke-width="3"/><path d="M17 5h4v4" fill="none" stroke="currentColor" stroke-width="2"/>',
    largestArmy:
      '<path d="m12 2 3 6 6 .8-4.5 4.3 1.2 6.4L12 16.4l-5.7 3.1 1.2-6.4L3 8.8 9 8Z" fill="currentColor"/>',
    knight:
      '<path d="M7 21V9l3-6 6 2-2 4 4 4-2 8Z" fill="currentColor"/><path d="m10 8 2 1" stroke="white" opacity=".45"/>',
    monopoly:
      '<path d="M4 7h12l-3-3m3 3-3 3M20 17H8l3-3m-3 3 3 3" fill="none" stroke="currentColor" stroke-width="2.2"/>',
    roadBuilding:
      '<path d="M3 17 10 6l3 2-7 11Zm9 2 7-11 3 2-7 11Z" fill="currentColor"/>',
    yearOfPlenty:
      '<path d="m12 2 1.7 5.1L19 9l-5.3 1.9L12 16l-1.7-5.1L5 9l5.3-1.9Z" fill="currentColor"/><path d="m5 14 1 3 3 1-3 1-1 3-1-3-3-1 3-1Z" fill="currentColor"/>',
    victoryPoint:
      '<path d="m12 2 3 6 6 .8-4.5 4.3 1.2 6.4L12 16.4l-5.7 3.1 1.2-6.4L3 8.8 9 8Z" fill="currentColor"/>',
  };
  return `<svg viewBox="0 0 24 24" aria-hidden="true">${shapes[piece]}</svg>`;
};

const collapseIcon = (collapsed: boolean) =>
  `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="${
    collapsed ? "m8 10 4 4 4-4" : "m8 14 4-4 4 4"
  }"/></svg>`;

const cardsIcon = (back: boolean) =>
  back
    ? '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="m15 6-6 6 6 6"/></svg>'
    : '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true"><rect x="4" y="5" width="11" height="14"/><path d="M8 5V3h12v14h-5"/></svg>';

const settingsIcon = () =>
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="square" aria-hidden="true"><path d="M4 7h16M4 17h16"/><circle cx="9" cy="7" r="2.5" fill="currentColor" stroke="none"/><circle cx="15" cy="17" r="2.5" fill="currentColor" stroke="none"/></svg>';

const warningIcon = () =>
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M12 3 2.7 20h18.6L12 3Z"/><path d="M12 9v5m0 3h.01"/></svg>';

const formatRange = (minimum: number, maximum: number, approximate: boolean): string => {
  const prefix = approximate ? "~" : "";
  return minimum === maximum ? `${prefix}${minimum}` : `${prefix}${minimum}–${maximum}`;
};

const tupleResources = (
  cards?: [number, number, number, number, number],
): ResourceVector =>
  Object.fromEntries(
    RESOURCE_ORDER.map((resource, index) => [
      resource,
      cards?.[index] ?? 0,
    ]),
  ) as ResourceVector;

const playerThreat = (state: TrackerState, player: string): number => {
  const meta = state.players[player];
  if (!meta) return 0;
  return (
    meta.builds.settlement +
    meta.builds.city * 2 +
    meta.playedDevCards.knight * 0.55 +
    meta.devCards.length * 0.25
  );
};

const decisionStateDigest = (serialized: string): string => {
  let first = 2166136261;
  let second = 5381;
  for (let index = 0; index < serialized.length; index += 1) {
    const code = serialized.charCodeAt(index);
    first = Math.imul(first ^ code, 16777619);
    second = Math.imul(second ^ code, 1597334677);
  }
  return `d2:${(first >>> 0).toString(36)}:${(second >>> 0).toString(36)}:${serialized.length.toString(36)}`;
};

const extensionBuildInfo = (): {
  identity: string;
  label: string;
  builtAt?: string;
} => {
  const manifest = chrome.runtime.getManifest();
  const identity = manifest.version_name ?? `v${manifest.version}`;
  const parts = manifest.version_name?.split(" · ");
  return {
    identity,
    label: parts?.slice(0, 2).join(" · ") ?? identity,
    builtAt: parts?.[2],
  };
};

export class AssistantOverlay {
  private readonly buildInfo = extensionBuildInfo();
  private readonly host: HTMLDivElement;
  private readonly shadow: ShadowRoot;
  private activeView: ViewName = "advice";
  private lastRenderedView?: ViewName;
  private collapsed: boolean;
  private session?: GameSession;
  private board?: BoardSnapshot;
  private tradeOfferSnapshots = new Map<string, TradeOfferSnapshot>();
  private position: OverlayPosition = {};
  private drag?: { offsetX: number; offsetY: number };
  private roadPlan?: { gameKey?: string; targetId: string };
  private freeRoadPlan?: { gameKey?: string; edgeIds: string[] };
  private queuedPlacement?: {
    gameKey?: string;
    action: "road" | "settlement" | "city";
    targetId: string;
    point: BoardPoint;
  };
  private pendingPlacement?: PendingBoardPlacement;
  private pendingPlacementHandBefore?: ResourceVector;
  private confirmedPlacement?: {
    pending: PendingBoardPlacement;
    player: string;
    expiresAt: number;
  };
  private confirmedPlacementSpend?: {
    gameKey?: string;
    player: string;
    before: ResourceVector;
    after: ResourceVector;
    expiresAt: number;
  };
  private pendingPlacementTimer?: number;
  private activeSpatial?: ReturnType<AssistantOverlay["spatialRecommendation"]>;
  private tradeRenderFrame?: number;
  private readonly decisionWorker = new DecisionWorkerClient();
  private readonly renderGate = new InteractionRenderGate();
  private readonly gameRecorder = new GameRecordRecorder();
  private readonly decisionTraces = new DecisionTraceRecorder(() =>
    this.captureGameRecord(),
  );
  private readonly winPredictions = new WinPredictionStabilizer();
  private decisionAnalysis?: DecisionAnalysis;
  private decisionKey = "";
  private decisionPendingKey = "";
  private decisionSlowKey = "";
  private decisionRuntime?: DecisionRuntime;
  private decisionRuntimeDetail = "Connecting to the packaged search engine.";
  private decisionRuntimeError = "";
  private decisionContextInvalidated = false;
  private decisionWaitingForPreviousSearch = false;
  private lastRejectedDomesticTrade?: DomesticTradeState;
  private readonly rootTradeActionExclusions: RootTradeActionExclusion[] = [];
  private readonly failedTradeActions = new Set<string>();
  private readonly completedIncomingTradeIds = new Set<string>();
  private readonly outgoingTradeSeenAt = new Map<string, number>();
  private readonly outgoingTradeWatchdogs = new Map<string, number>();
  private decisionTraceGameScope: string | null | undefined;
  private resetGameScope: string | null | undefined;
  private robberVictimPlan?: {
    gameKey?: string;
    turn?: number;
    player: string;
  };
  private actionGuideSignature = "";

  constructor(
    private settings: AssistantSettings,
    private readonly callbacks: { reset: () => void | Promise<void> },
  ) {
    this.collapsed = settings.startCollapsed;
    this.decisionTraces.setLegacyStorageEnabled(!settings.recordGame);
    this.host = document.createElement("div");
    this.host.id = "colonist-assistant-root";
    this.host.style.cssText =
      `position:fixed;z-index:2147483000;right:${window.innerWidth <= 700 ? 8 : 18}px;top:72px;pointer-events:auto;color-scheme:dark;`;
    // Keep the boundary open so accessibility tooling and the packaged
    // end-to-end harness can inspect the same runtime/action state the player
    // sees. A closed shadow root is not a security boundary and made live
    // failures impossible to diagnose.
    this.shadow = this.host.attachShadow({ mode: "open" });
    const styles = OVERLAY_STYLES.replace(
      "__CA_FONT_URL__",
      chrome.runtime.getURL("assets/fonts/ArchivoNarrow-Variable.ttf"),
    );
    this.shadow.innerHTML = `<style>${styles}</style><div id="mount"></div>`;
    document.documentElement.append(this.host);
    this.installHandlers();
    void this.restorePosition();
    this.warmDecisionEngine();
    this.render();
  }

  private warmDecisionEngine(): void {
    this.decisionWorker.warm(this.settings.engine, (status) => {
      if (
        status.runtime === "background-gpu" ||
        status.runtime === "background-wasm"
      ) {
        this.decisionRuntime = status.runtime;
        this.decisionRuntimeError = "";
        this.decisionRuntimeDetail =
          `${status.detail}${status.initializationMs !== undefined ? ` in ${Math.max(1, Math.round(status.initializationMs))} ms` : ""}.`;
      } else {
        this.decisionRuntime = undefined;
        this.decisionRuntimeError = status.detail;
        if (status.detail === EXTENSION_CONTEXT_RELOAD_MESSAGE) {
          this.decisionContextInvalidated = true;
        }
      }
      this.render();
    });
  }

  update(session?: GameSession): void {
    this.session = session;
    if (session?.events?.length) this.resetGameScope = undefined;
    this.confirmPendingPlacementFromLog();
    this.captureGameRecord();
    this.render();
  }

  updateBoard(board?: BoardSnapshot): void {
    let nextBoard = board;
    if (nextBoard) {
      const traceScope = nextBoard.gameKey ?? null;
      if (
        this.decisionTraceGameScope === undefined ||
        this.decisionTraceGameScope !== traceScope
      ) {
        this.decisionTraceGameScope = traceScope;
        // Queue deletion ahead of the first trace for this game. The recorder
        // serializes its storage writes, so a fast first decision cannot be
        // erased by a slower stale-game cleanup.
        void this.decisionTraces.reset().catch(() => undefined);
      }
    }
    if (
      this.resetGameScope !== undefined &&
      this.resetGameScope !== (nextBoard?.gameKey ?? null)
    ) {
      this.resetGameScope = undefined;
    }
    if (tradeMemoryScopeChanged(this.board, nextBoard)) {
      this.lastRejectedDomesticTrade = undefined;
      this.rootTradeActionExclusions.length = 0;
      this.failedTradeActions.clear();
      this.completedIncomingTradeIds.clear();
      this.outgoingTradeSeenAt.clear();
      this.tradeOfferSnapshots.clear();
      this.clearOutgoingTradeWatchdogs();
    }
    if (this.session && nextBoard) {
      const nextSnapshots = snapshotActiveTrades(nextBoard.activeTrades);
      const tradeEvents = tradeBeliefEventsFromDiff(
        this.tradeOfferSnapshots,
        nextSnapshots,
        this.session.state.tradeEmbargoes,
      );
      this.tradeOfferSnapshots = nextSnapshots;
      if (tradeEvents.length) {
        this.session.ingestEvents(tradeEvents, "active-trade");
      }
    } else if (!nextBoard) {
      this.tradeOfferSnapshots.clear();
    }
    const outgoingTrades =
      nextBoard?.activeTrades?.filter((trade) => !trade.incoming) ?? [];
    if (outgoingTrades.length) {
      for (const trade of outgoingTrades) {
        if (!this.outgoingTradeSeenAt.has(trade.id)) {
          this.outgoingTradeSeenAt.set(trade.id, Date.now());
          const timer = window.setTimeout(() => {
            this.outgoingTradeWatchdogs.delete(trade.id);
            if (this.outgoingTradeSeenAt.has(trade.id)) {
              window.dispatchEvent(
                new CustomEvent("colonist-assistant-board-refresh"),
              );
              this.render();
            }
          }, 18_100);
          this.outgoingTradeWatchdogs.set(trade.id, timer);
        }
        const { give, receive } = localTradeBundles(trade);
        if (
          trade.responsesComplete &&
          !trade.acceptedPlayers?.length &&
          Boolean(trade.rejectedPlayers?.length)
        ) {
          this.lastRejectedDomesticTrade = {
            give: { ...give },
            receive: { ...receive },
          };
        }
        const observedKey = tradeOfferKey(give, receive);
        for (let index = this.rootTradeActionExclusions.length - 1; index >= 0; index -= 1) {
          const exclusion = this.rootTradeActionExclusions[index]!;
          if (
            exclusion.kind === "offer-trade" &&
            tradeOfferKey(exclusion.give, exclusion.receive) === observedKey
          ) {
            this.rootTradeActionExclusions.splice(index, 1);
          }
        }
      }
    }
    const outgoingIds = new Set(outgoingTrades.map((trade) => trade.id));
    for (const id of this.outgoingTradeSeenAt.keys()) {
      if (!outgoingIds.has(id)) {
        this.outgoingTradeSeenAt.delete(id);
        const timer = this.outgoingTradeWatchdogs.get(id);
        if (timer !== undefined) window.clearTimeout(timer);
        this.outgoingTradeWatchdogs.delete(id);
      }
    }
    if (this.confirmedPlacement && nextBoard) {
      if (
        placementHasAdvanced(this.confirmedPlacement.pending, nextBoard) ||
        Date.now() >= this.confirmedPlacement.expiresAt
      ) {
        this.confirmedPlacement = undefined;
      } else {
        nextBoard = applyConfirmedPlacement(
          this.confirmedPlacement.pending,
          nextBoard,
          this.confirmedPlacement.player,
        );
      }
    }
    if (this.confirmedPlacementSpend && nextBoard) {
      const guard = this.confirmedPlacementSpend;
      const observedHand = nextBoard.ownHand;
      const observedHandAdvanced = Boolean(
        observedHand &&
        RESOURCE_ORDER.some(
          (resource) => observedHand[resource] !== guard.before[resource],
        ),
      );
      if (
        Date.now() >= guard.expiresAt ||
        (
          guard.gameKey &&
          nextBoard.gameKey &&
          guard.gameKey !== nextBoard.gameKey
        ) ||
        nextBoard.gameOver ||
        observedHandAdvanced
      ) {
        this.confirmedPlacementSpend = undefined;
      }
    }
    if (
      this.roadPlan &&
      nextBoard?.gameKey &&
      this.roadPlan.gameKey &&
      nextBoard.gameKey !== this.roadPlan.gameKey
    ) {
      this.roadPlan = undefined;
      this.freeRoadPlan = undefined;
      this.queuedPlacement = undefined;
    }
    if (
      this.board?.gameKey &&
      nextBoard?.gameKey &&
      this.board.gameKey !== nextBoard.gameKey
    ) {
      // A canvas command may be waiting for a bounded commit retry. Base-map
      // target IDs repeat between games, so cancel it before the replacement
      // board can make the old target appear legal again.
      destroyActionGuide();
      this.actionGuideSignature = "";
      this.decisionAnalysis = undefined;
      this.freeRoadPlan = undefined;
      this.queuedPlacement = undefined;
      this.decisionKey = "";
      this.decisionPendingKey = "";
      this.decisionSlowKey = "";
      this.decisionWaitingForPreviousSearch = false;
      this.decisionRuntimeError = "";
      this.decisionWorker.reset();
      this.winPredictions.reset();
      this.confirmedPlacement = undefined;
      this.confirmedPlacementSpend = undefined;
      this.robberVictimPlan = undefined;
      this.lastRejectedDomesticTrade = undefined;
      this.rootTradeActionExclusions.length = 0;
      this.failedTradeActions.clear();
      this.completedIncomingTradeIds.clear();
      this.outgoingTradeSeenAt.clear();
      this.clearOutgoingTradeWatchdogs();
    }
    if (
      this.robberVictimPlan &&
      nextBoard &&
      (
        this.robberVictimPlan.gameKey !== nextBoard.gameKey ||
        this.robberVictimPlan.turn !== nextBoard.turn
      )
    ) {
      this.robberVictimPlan = undefined;
    }
    if (
      this.queuedPlacement &&
      nextBoard &&
      (
        (
          this.queuedPlacement.gameKey &&
          nextBoard.gameKey &&
          this.queuedPlacement.gameKey !== nextBoard.gameKey
        ) ||
        !nextBoard.isMyTurn ||
        (
          nextBoard.action !== "none" &&
          nextBoard.action !== this.queuedPlacement.action
        )
      )
    ) {
      this.queuedPlacement = undefined;
    }
    if (
      this.pendingPlacement &&
      nextBoard &&
      placementHasAdvanced(this.pendingPlacement, nextBoard)
    ) {
      this.clearPendingPlacement();
      this.confirmedPlacementSpend = undefined;
    }
    this.board = nextBoard;
    if (nextBoard?.gameOver) {
      this.decisionAnalysis = undefined;
      this.decisionKey = "";
      this.decisionPendingKey = "";
      this.decisionSlowKey = "";
      this.decisionWaitingForPreviousSearch = false;
      this.decisionRuntimeError = "";
      this.decisionWorker.reset();
      this.activeSpatial = undefined;
      this.roadPlan = undefined;
      this.freeRoadPlan = undefined;
      this.queuedPlacement = undefined;
      this.robberVictimPlan = undefined;
      this.clearPendingPlacement();
      this.clearOutgoingTradeWatchdogs();
      destroyTradeVerdicts();
      destroyActionGuide();
      destroyWinOdds();
    }
    this.captureGameRecord(Boolean(nextBoard?.gameOver));
    this.render();
  }

  setSettings(settings: AssistantSettings): void {
    const wasRecording = this.settings.recordGame;
    const engineChanged = settings.engine !== this.settings.engine;
    if (
      engineChanged ||
      settings.disablePlayerTrades !== this.settings.disablePlayerTrades
    ) {
      this.decisionAnalysis = undefined;
      this.decisionKey = "";
      this.decisionPendingKey = "";
      this.decisionSlowKey = "";
      this.decisionWaitingForPreviousSearch = false;
      this.decisionRuntimeError = "";
      this.decisionWorker.reset();
      this.winPredictions.reset();
    }
    this.settings = settings;
    if (engineChanged) this.warmDecisionEngine();
    this.decisionTraces.setLegacyStorageEnabled(!settings.recordGame);
    if (wasRecording && !settings.recordGame) {
      void this.gameRecorder.flush();
    } else if (!wasRecording && settings.recordGame) {
      this.captureGameRecord();
    }
    this.host.style.display = settings.enabled ? "block" : "none";
    this.render();
  }

  async clearStoredSessionData(): Promise<void> {
    this.resetGameScope = this.board?.gameKey ?? null;
    this.session = undefined;
    this.board = undefined;
    this.decisionAnalysis = undefined;
    this.decisionKey = "";
    this.decisionPendingKey = "";
    this.decisionSlowKey = "";
    this.decisionWaitingForPreviousSearch = false;
    this.decisionRuntimeError = "";
    this.decisionWorker.reset();
    this.winPredictions.reset();
    this.activeSpatial = undefined;
    this.roadPlan = undefined;
    this.freeRoadPlan = undefined;
    this.queuedPlacement = undefined;
    this.robberVictimPlan = undefined;
    this.confirmedPlacement = undefined;
    this.confirmedPlacementSpend = undefined;
    this.lastRejectedDomesticTrade = undefined;
    this.rootTradeActionExclusions.length = 0;
    this.failedTradeActions.clear();
    this.completedIncomingTradeIds.clear();
    this.outgoingTradeSeenAt.clear();
    this.clearOutgoingTradeWatchdogs();
    this.clearPendingPlacement();
    destroyTradeVerdicts();
    destroyActionGuide();
    destroyWinOdds();
    await this.decisionTraces.reset();
    await this.gameRecorder.reset();
    this.render();
  }

  destroy(): void {
    if (this.tradeRenderFrame !== undefined) {
      window.cancelAnimationFrame(this.tradeRenderFrame);
    }
    destroyTradeVerdicts();
    destroyActionGuide();
    destroyWinOdds();
    this.decisionWorker.destroy();
    if (this.settings.recordGame) void this.gameRecorder.flush();
    this.clearOutgoingTradeWatchdogs();
    this.clearPendingPlacement();
    this.confirmedPlacementSpend = undefined;
    document.removeEventListener("pointerup", this.handleBoardPointer, true);
    this.host.remove();
  }

  private clearOutgoingTradeWatchdogs(): void {
    for (const timer of this.outgoingTradeWatchdogs.values()) {
      window.clearTimeout(timer);
    }
    this.outgoingTradeWatchdogs.clear();
  }

  private async restorePosition(): Promise<void> {
    this.position = await readPosition();
    if (this.position.left !== undefined && this.position.top !== undefined) {
      this.place(this.position.left, this.position.top);
    }
  }

  private place(left: number, top: number): void {
    const width = this.host.offsetWidth || 392;
    const height = Math.min(this.host.offsetHeight || 560, window.innerHeight);
    const safeLeft = Math.max(8, Math.min(left, window.innerWidth - width - 8));
    const safeTop = Math.max(8, Math.min(top, window.innerHeight - Math.min(height, 72) - 8));
    this.host.style.left = `${safeLeft}px`;
    this.host.style.top = `${safeTop}px`;
    this.host.style.right = "auto";
    this.position = { left: safeLeft, top: safeTop };
  }

  private installHandlers(): void {
    this.shadow.addEventListener("click", (rawEvent) => {
      const target = (rawEvent.target as Element).closest<HTMLElement>("[data-action]");
      if (!target) return;
      const action = target.dataset.action;
      if (action === "collapse") this.collapsed = !this.collapsed;
      if (action === "view") {
        const requested = target.dataset.view as ViewName;
        this.activeView =
          requested === this.activeView ? "advice" : requested;
      }
      if (action === "reset" && confirm("Reset the current Colonist Assistant session?")) {
        void this.callbacks.reset();
      }
      if (action === "export-record") {
        void this.exportGameRecord();
        return;
      }
      if (action === "retry-engine") {
        this.retryDecisionEngine();
        return;
      }
      this.render();
    });

    this.shadow.addEventListener("change", (rawEvent) => {
      const target = rawEvent.target;
      if (
        target instanceof HTMLInputElement &&
        target.dataset.setting &&
        target.type === "checkbox"
      ) {
        const key = target.dataset.setting as
          | "highlightNextAction"
          | "disablePlayerTrades"
          | "recordGame"
          | "autonomousPrivateGames";
        this.applySettings({
          ...this.settings,
          [key]: target.checked,
        });
        return;
      }
      if (target instanceof HTMLSelectElement) {
        if (target.dataset.setting === "engine") {
          this.releaseSettingsInteraction(target, false);
          this.applySettings({
            ...this.settings,
            engine: normalizeDecisionEngine(target.value),
          });
          return;
        }
        if (target.dataset.setting === "autopilotDelaySeconds") {
          this.releaseSettingsInteraction(target, false);
          this.applySettings({
            ...this.settings,
            autopilotDelaySeconds: normalizeAutopilotDelaySeconds(
              Number(target.value),
            ),
          });
        }
      }
    });

    this.shadow.addEventListener("pointerdown", (event) => {
      const target = event.composedPath?.()?.[0];
      const key = this.settingsInteractionKey(target);
      if (key) this.renderGate.hold(key);
    });
    this.shadow.addEventListener("focusin", (event) => {
      const target = event.composedPath?.()?.[0];
      const key = this.settingsInteractionKey(target);
      if (key) this.renderGate.hold(key);
    });
    this.shadow.addEventListener("focusout", (event) => {
      const target = event.composedPath?.()?.[0];
      this.releaseSettingsInteraction(target, true);
    });

    this.shadow.addEventListener("pointerdown", (event) => {
      const path = event.composedPath() as Element[];
      const header = path.find(
        (element) => element instanceof Element && element.classList?.contains("topbar"),
      );
      const interactive = path.find(
        (element) =>
          element instanceof Element && ["BUTTON", "INPUT", "A"].includes(element.tagName),
      );
      if (!header || interactive || !(event instanceof PointerEvent)) return;
      const bounds = this.host.getBoundingClientRect();
      this.drag = { offsetX: event.clientX - bounds.left, offsetY: event.clientY - bounds.top };
      this.host.setPointerCapture(event.pointerId);
    });
    this.host.addEventListener("pointermove", (event) => {
      if (!this.drag) return;
      this.place(event.clientX - this.drag.offsetX, event.clientY - this.drag.offsetY);
    });
    this.host.addEventListener("pointerup", (event) => {
      if (!this.drag) return;
      this.drag = undefined;
      this.host.releasePointerCapture(event.pointerId);
      void savePosition(this.position);
    });
    window.addEventListener("resize", () => {
      if (this.position.left !== undefined && this.position.top !== undefined) {
        this.place(this.position.left, this.position.top);
      } else {
        this.host.style.right = `${window.innerWidth <= 700 ? 8 : 18}px`;
      }
    });
    document.addEventListener("pointerup", this.handleBoardPointer, true);
  }

  private readonly handleBoardPointer = (event: PointerEvent): void => {
    if (event.button !== 0 || event.composedPath().includes(this.host)) return;
    const spatial = this.activeSpatial;
    const board = this.board;
    if (
      !spatial ||
      spatial.proactive ||
      !board ||
      board.action !== spatial.action
    ) {
      return;
    }
    const source =
      spatial.action === "road"
        ? board.edges.find((edge) => edge.id === spatial.recommendation.id)
        : spatial.action === "robber"
          ? board.hexes.find((hex) => hex.id === spatial.recommendation.id)
          : board.vertices.find(
              (vertex) => vertex.id === spatial.recommendation.id,
            );
    if (
      !source?.screen ||
      Math.hypot(
        event.clientX - source.screen.x,
        event.clientY - source.screen.y,
      ) > 48
    ) {
      return;
    }
    this.clearPendingPlacement();
    this.pendingPlacement = {
      action: spatial.action,
      targetId: spatial.recommendation.id,
      point: source.screen,
      gameKey: board.gameKey,
      startedAt: Date.now(),
    };
    this.pendingPlacementHandBefore = board.ownHand
      ? cloneResources(board.ownHand)
      : undefined;
    this.activeSpatial = undefined;
    this.pendingPlacementTimer = window.setTimeout(() => {
      this.pendingPlacement = undefined;
      this.pendingPlacementTimer = undefined;
      this.render();
    }, PLACEMENT_SYNC_TIMEOUT_MS);
    window.dispatchEvent(
      new CustomEvent("colonist-assistant-board-refresh"),
    );
    this.render();
  };

  private clearPendingPlacement(): void {
    this.pendingPlacement = undefined;
    this.pendingPlacementHandBefore = undefined;
    if (this.pendingPlacementTimer !== undefined) {
      window.clearTimeout(this.pendingPlacementTimer);
      this.pendingPlacementTimer = undefined;
    }
  }

  private registerPendingPlacement(
    action: Extract<NextClick, { kind: "board" }>,
  ): void {
    const board = this.board;
    if (!board) return;
    this.clearPendingPlacement();
    this.queuedPlacement = undefined;
    this.pendingPlacement = {
      action: action.boardAction,
      targetId: action.targetId,
      point: action.point,
      gameKey: board.gameKey,
      startedAt: Date.now(),
    };
    this.pendingPlacementHandBefore = board.ownHand
      ? cloneResources(board.ownHand)
      : undefined;
    this.activeSpatial = undefined;
    this.pendingPlacementTimer = window.setTimeout(() => {
      this.pendingPlacement = undefined;
      this.pendingPlacementTimer = undefined;
      this.render();
    }, PLACEMENT_SYNC_TIMEOUT_MS);
  }

  private rememberBuildPlacement(
    next: Extract<NextClick, { kind: "build" }>,
    spatial: ReturnType<AssistantOverlay["spatialRecommendation"]> | undefined,
  ): void {
    const placementAction =
      next.build === "road" ||
      next.build === "settlement" ||
      next.build === "city"
        ? next.build
        : undefined;
    if (!placementAction) return;
    const deep = this.decisionAnalysis?.deepSearch?.chosen;
    const deepKind = {
      road: "build-road",
      settlement: "build-settlement",
      city: "build-city",
    }[placementAction];
    const targetId =
      (
        isSearchDecisionRuntime(this.decisionAnalysis?.runtime) &&
        deep?.kind === deepKind
      )
        ? deep.targetId
        : spatial?.proactive && spatial.action === placementAction
          ? spatial.recommendation.id
          : undefined;
    if (!targetId) return;
    const source =
      placementAction === "road"
        ? this.board?.edges.find((edge) => edge.id === targetId)
        : this.board?.vertices.find((vertex) => vertex.id === targetId);
    if (!source?.screen) return;
    // A build action and its parameter click are one selected-engine
    // transaction. Preserve the target returned by the completed WASM search
    // before opening Colonist's placement modal; otherwise the modal can
    // trigger a redundant search and leave autopilot waiting if background
    // timer/message delivery is throttled.
    this.queuedPlacement = {
      gameKey: this.board?.gameKey,
      action: placementAction,
      targetId,
      point: source.screen,
    };
  }

  private confirmPendingPlacementFromLog(): void {
    const pending = this.pendingPlacement;
    const state = this.session?.state;
    const board = this.board;
    const player = this.userPlayer(state) ?? board?.myPlayer;
    if (
      !pending ||
      !state ||
      !board ||
      !player ||
      !placementConfirmedByPublicLog(pending, state, player)
    ) {
      return;
    }
    this.confirmedPlacement = {
      pending,
      player,
      expiresAt: Date.now() + PLACEMENT_SYNC_TIMEOUT_MS,
    };
    const before = this.pendingPlacementHandBefore;
    const cost =
      pending.action === "road" ||
      pending.action === "settlement" ||
      pending.action === "city"
        ? BUILD_COSTS[pending.action]
        : undefined;
    if (before && cost && hasResources(before, cost)) {
      const after = cloneResources(before);
      const resourceCost = cost as Partial<ResourceVector>;
      for (const resource of RESOURCE_ORDER) {
        after[resource] -= resourceCost[resource] ?? 0;
      }
      this.confirmedPlacementSpend = {
        gameKey: board.gameKey,
        player,
        before,
        after,
        expiresAt: Date.now() + PLACEMENT_SYNC_TIMEOUT_MS,
      };
    }
    this.board = applyConfirmedPlacement(pending, board, player);
    this.clearPendingPlacement();
    this.decisionAnalysis = undefined;
    this.decisionKey = "";
    this.decisionPendingKey = "";
    this.decisionWorker.reset();
  }

  private settingsInteractionKey(
    target: EventTarget | null | undefined,
  ): string | undefined {
    if (!(target instanceof HTMLSelectElement)) return undefined;
    const setting = target.dataset.setting;
    return setting === "engine" || setting === "autopilotDelaySeconds"
      ? `settings-select:${setting}`
      : undefined;
  }

  private releaseSettingsInteraction(
    target: EventTarget | null | undefined,
    renderDeferred: boolean,
  ): void {
    const key = this.settingsInteractionKey(target);
    if (!key) return;
    const shouldRender = this.renderGate.release(key);
    if (renderDeferred && shouldRender) this.render();
  }

  private retryDecisionEngine(): void {
    if (this.decisionContextInvalidated) return;
    this.decisionAnalysis = undefined;
    this.decisionPendingKey = "";
    this.decisionSlowKey = "";
    this.decisionWaitingForPreviousSearch = false;
    this.decisionRuntimeError = "";
    this.decisionRuntimeDetail = "Retrying the selected WASM engine.";
    this.decisionWorker.reset();
    this.render();
  }

  private applySettings(settings: AssistantSettings): void {
    this.setSettings(settings);
    void saveSettings(settings);
  }

  private captureGameRecord(finalize = false): void {
    if (!this.settings.recordGame || !this.session) return;
    const boardGameKey = this.board?.gameKey;
    const sessionGameKey = this.session.gameKey;
    if (
      boardGameKey &&
      sessionGameKey &&
      boardGameKey !== sessionGameKey
    ) {
      // GameSession publishes its reset synchronously when Colonist rolls to a
      // new game key, while the board bridge can still hold the previous game
      // for the rest of this callback. Never combine evidence across scopes.
      return;
    }
    const gameKey = boardGameKey ?? sessionGameKey;
    const capture = {
      scope: gameKey ?? this.session.id,
      sessionId: this.session.id,
      ...(gameKey ? { gameKey } : {}),
      startedAt: this.session.startedAt,
      partialHistory:
        this.session.partialHistory || Boolean(this.session.state.warnings.length),
      unmatchedCount: this.session.unmatchedCount,
      playerOrder: [...this.session.state.playerOrder],
      assistant: {
        engine: this.settings.engine,
        disablePlayerTrades: this.settings.disablePlayerTrades,
        autopilot: this.settings.autonomousPrivateGames,
      },
      events: this.session.events,
      // Record Mode consumes bounded replay evidence only for decisions that
      // changed since the previous capture, then aliases/deduplicates it.
      decisions: this.decisionTraces.snapshotForRecord(),
      ...(this.board ? { board: this.board } : {}),
    };
    if (finalize || this.board?.gameOver) this.gameRecorder.finalize(capture);
    else this.gameRecorder.capture(capture);
  }

  private async exportGameRecord(): Promise<void> {
    if (this.settings.recordGame) this.captureGameRecord(Boolean(this.board?.gameOver));
    await this.gameRecorder.flush();
    const record = await readRecordedGame();
    if (!record) {
      alert("No recorded Colonist game is available yet.");
      return;
    }
    downloadRecordedGame(record);
  }

  private userPlayer(state?: TrackerState): string | undefined {
    if (this.board?.myPlayer && state?.players[this.board.myPlayer]) {
      return this.board.myPlayer;
    }
    if (state?.players.You) return "You";
    return undefined;
  }

  private resourceArt(resource: Resource): string {
    const source = this.board?.assets?.resources?.[resource];
    return source
      ? `<img class="native-card-art" src="${escapeHtml(source)}" alt="" aria-hidden="true" draggable="false">`
      : resourceIcon(resource);
  }

  private pieceArt(piece: PieceAsset): string {
    const source = this.board?.assets?.pieces?.[piece];
    return source
      ? `<img class="native-piece-art" src="${escapeHtml(source)}" alt="" aria-hidden="true" draggable="false">`
      : pieceFallback(piece);
  }

  private render(): void {
    if (!this.renderGate.tryRender()) return;
    const mount = this.shadow.querySelector("#mount");
    if (!mount) return;
    const previousPanel = this.shadow.querySelector<HTMLElement>(".panel");
    const preservedScrollTop =
      this.lastRenderedView === this.activeView
        ? (previousPanel?.scrollTop ?? 0)
        : 0;
    const state = this.reconciledState();
    const ready = Boolean(this.session || this.board);
    if (
      this.pendingPlacement &&
      this.board &&
      placementHasAdvanced(this.pendingPlacement, this.board)
    ) {
      this.clearPendingPlacement();
    }
    const awaitingPlacement = placementIsAwaitingSync(
      this.pendingPlacement,
      this.board,
    );
    const spatial = awaitingPlacement
      ? undefined
      : this.spatialRecommendation(state);
    this.activeSpatial = spatial;
    const user = this.userPlayer(state);
    this.scheduleDecisionAnalysis(state, user);
    const report =
      spatial?.report ??
      (state && user
        ? this.coachReport(state, user)
        : undefined);
    const displayedWinAnalysis = this.winPredictions.update(
      report?.decisionAnalysis,
      this.board,
    );
    const workflow = this.board?.gameOver
      ? undefined
      : activeWorkflowAction(
          this.board?.action,
          this.board?.robberVictimSelection,
        );
    const next = workflow ?? this.nextClick(state, spatial, report);
    const marker =
      spatial &&
      (
        next?.kind === "board" ||
        (
          next?.kind === "build" &&
          next.build !== "development" &&
          next.build === spatial.action
        )
      )
        ? this.renderBoardMarker(spatial.action, spatial.recommendation)
        : "";
    const advice =
      this.renderEngineStrip() +
      this.renderAdvice(state, spatial, report, next);
    const panel =
      this.activeView === "advice"
        ? advice
        : this.activeView === "cards"
          ? this.renderCards(state, displayedWinAnalysis)
          : this.renderSettings();
    mount.innerHTML = `
      ${marker}
      <section class="assistant ${this.collapsed ? "collapsed" : ""}" aria-label="Colonist Assistant">
        <header class="topbar">
          <span class="brand-mark">${assistantMark()}</span>
          <span class="product-name">Colonist Assistant</span>
          <span class="status ${ready ? "live" : ""}"><i></i>${ready ? (this.settings.recordGame ? "LIVE · REC" : "LIVE") : "WAITING"}</span>
          <button class="view-button ${this.activeView === "cards" ? "active" : ""}" data-action="view" data-view="cards" aria-pressed="${this.activeView === "cards"}" aria-label="${this.activeView === "cards" ? "Back to your advice" : "Show tracked cards"}" title="${this.activeView === "cards" ? "Your advice" : "Tracked cards"}">
            ${cardsIcon(this.activeView === "cards")}
            <span>${this.activeView === "cards" ? "ADVICE" : "CARDS"}</span>
          </button>
          <button class="icon-button ${this.activeView === "settings" ? "active" : ""}" data-action="view" data-view="settings" aria-pressed="${this.activeView === "settings"}" aria-label="${this.activeView === "settings" ? "Back to your advice" : "Open assistant settings"}" title="${this.activeView === "settings" ? "Your advice" : "Settings"}">${settingsIcon()}</button>
          <button class="icon-button" data-action="collapse" aria-label="${this.collapsed ? "Expand assistant" : "Collapse assistant"}" title="${this.collapsed ? "Expand" : "Collapse"}">${collapseIcon(this.collapsed)}</button>
        </header>
        <div class="body">
          <main class="panel">
            ${panel}
          </main>
        </div>
      </section>`;
    const renderedPanel = this.shadow.querySelector<HTMLElement>(".panel");
    if (renderedPanel && preservedScrollTop > 0) {
      renderedPanel.scrollTop = preservedScrollTop;
    }
    this.lastRenderedView = this.activeView;
    this.scheduleTradeVerdicts(state);
    if (this.settings.enabled && !this.board?.gameOver) {
      renderWinOdds(displayedWinAnalysis, state);
    } else {
      destroyWinOdds();
    }
    const nextSignature = next?.signature ?? "";
    const traceKey = this.decisionKey
      ? decisionStateDigest(this.decisionKey)
      : "";
    const guideGameScope = this.board?.gameKey ?? location.pathname;
    const stillInGuideGame = (): boolean =>
      (this.board?.gameKey ?? location.pathname) === guideGameScope;
    const transactionCommit = (() => {
      if (
        !next ||
        (
          next.kind !== "trade-builder" &&
          !(
            next.kind === "trade" &&
            next.verdict === "counter" &&
            next.counterGive &&
            next.counterReceive
          )
        )
      ) {
        return undefined;
      }
      const baselineOfferIds = new Set(
        (this.board?.activeTrades ?? [])
          .filter((trade) => !trade.incoming)
          .map((trade) => trade.id),
      );
      const give =
        next.kind === "trade-builder" ? next.give : next.counterGive!;
      const receive =
        next.kind === "trade-builder" ? next.receive : next.counterReceive!;
      const offerKey = tradeOfferKey(give, receive);
      const baselineHand = this.board?.ownHand
        ? { ...this.board.ownHand }
        : undefined;
      const bankTrade =
        next.kind === "trade-builder" && next.mode === "bank";
      return (): boolean => {
        const current = this.board;
        if (!current || !stillInGuideGame()) return false;
        if (bankTrade) {
          if (!baselineHand || !current.ownHand) return false;
          return RESOURCE_ORDER.every(
            (resource) =>
              current.ownHand![resource] ===
              baselineHand[resource] -
                give[resource] +
                receive[resource],
          );
        }
        return Boolean(
          current.activeTrades?.some(
            (trade) =>
              !trade.incoming &&
              !baselineOfferIds.has(trade.id) &&
              tradeOfferKey(
                localTradeBundles(trade).give,
                localTradeBundles(trade).receive,
              ) === offerKey,
          ),
        );
      };
    })();
    if (next && traceKey) {
      this.decisionTraces.final(
        traceKey,
        next,
        this.decisionSource(next, Boolean(workflow)),
      );
    }
    this.actionGuideSignature = nextSignature;
    renderActionGuide(next, {
      highlight: this.settings.highlightNextAction,
      autonomous: autonomousExecutionAllowed(
        this.settings.autonomousPrivateGames,
      ),
      autopilotDelayMs: this.settings.autopilotDelaySeconds * 1_000,
      validate: () =>
        Boolean(next && nextSignature) &&
        stillInGuideGame() &&
        this.actionGuideSignature === nextSignature &&
        Boolean(next && this.nextClickStillLegal(next)),
      validateBoardContinuation: () =>
        next?.kind === "board" && stillInGuideGame()
          ? this.nextClickStillLegal(next)
          : false,
      validateBoardCommit: () =>
        next?.kind === "board" && stillInGuideGame()
          ? this.boardActionCommitted(next)
          : false,
      validateContinuation: () =>
        Boolean(next && nextSignature) &&
        stillInGuideGame() &&
        this.actionGuideSignature === nextSignature &&
        Boolean(next && this.workflowContinuationStillLegal(next)),
      ...(transactionCommit
        ? { validateTransactionCommit: transactionCommit }
        : {}),
      onExecutionStart: () => {
        if (traceKey) this.decisionTraces.executionStarted(traceKey);
      },
      onExecution: ({ succeeded, reason }) => {
        if (succeeded && next?.kind === "build") {
          this.rememberBuildPlacement(next, spatial);
        }
        if (succeeded && next?.kind === "board") {
          this.registerPendingPlacement(next);
        }
        if (traceKey) {
          this.decisionTraces.execution(
            traceKey,
            succeeded,
            reason,
          );
        }
        if (!succeeded && next?.kind === "trade-builder") {
          this.rememberRootTradeFailure();
          this.decisionAnalysis = undefined;
          this.decisionKey = "";
          this.decisionPendingKey = "";
          this.decisionWorker.reset();
          this.render();
        }
        if (
          !succeeded &&
          next?.kind === "build" &&
          next.build !== "development"
        ) {
          this.decisionAnalysis = undefined;
          this.decisionKey = "";
          this.decisionPendingKey = "";
          this.decisionWorker.reset();
          this.render();
        }
        if (!succeeded && next?.kind === "board") {
          this.decisionAnalysis = undefined;
          this.decisionKey = "";
          this.decisionPendingKey = "";
          this.decisionWorker.reset();
          this.render();
        }
        if (!succeeded && next?.kind === "trade") {
          this.rememberRootTradeFailure();
          this.failedTradeActions.add(next.signature);
          this.decisionAnalysis = undefined;
          this.decisionKey = "";
          this.decisionPendingKey = "";
          this.decisionWorker.reset();
          this.render();
        }
        if (succeeded && next?.kind === "trade") {
          const trade = this.board?.activeTrades?.find(
            (candidate) => candidate.id === next.tradeId,
          );
          const completedId =
            trade?.incoming ? next.tradeId : undefined;
          if (completedId) {
            this.completedIncomingTradeIds.add(completedId);
          }
        }
      },
    });
  }

  private rememberRootTradeFailure(): void {
    const action = this.decisionAnalysis?.deepSearch?.chosen;
    if (
      !action ||
      (action.kind !== "offer-trade" && action.kind !== "counter-trade") ||
      !action.cards ||
      !action.receiveCards
    ) {
      return;
    }
    const exclusion: RootTradeActionExclusion = {
      kind: action.kind,
      give: tupleResources(action.cards),
      receive: tupleResources(action.receiveCards),
    };
    const key = `${exclusion.kind}|${tradeOfferKey(exclusion.give, exclusion.receive)}`;
    if (
      this.rootTradeActionExclusions.some(
        (candidate) =>
          `${candidate.kind}|${tradeOfferKey(candidate.give, candidate.receive)}` === key,
      )
    ) {
      return;
    }
    this.rootTradeActionExclusions.push(exclusion);
  }

  private preferredDeepAction(
    _state: TrackerState | undefined,
    _player: string | undefined,
    tradeId?: string,
  ): NonNullable<
    NonNullable<DecisionAnalysis["deepSearch"]>["chosen"]
  > | undefined {
    const action = this.decisionAnalysis?.deepSearch?.chosen;
    if (!tradeId) return action;
    if (
      !action ||
      (
        action.kind !== "respond-trade" &&
        action.kind !== "counter-trade" &&
        action.kind !== "confirm-trade" &&
        action.kind !== "cancel-trade"
      ) ||
      action.tradeId !== tradeId
    ) {
      return undefined;
    }
    return action;
  }

  private nextClickStillLegal(next: NextClick): boolean {
    const board = this.board;
    if (!board || board.gameOver) return false;
    if (next.kind === "board") {
      if (next.boardAction === "road") {
        const legal = board.action === "road"
          ? board.legalEdgeIds
          : board.buildableRoadIds;
        return (
          Boolean(board.isMyTurn) &&
          !board.edges.some(
            (edge) => edge.id === next.targetId && Boolean(edge.player),
          ) &&
          (!legal || legal.includes(next.targetId))
        );
      }
      if (next.boardAction === "robber") {
        return (
          board.action === "robber" &&
          !board.hexes.some(
            (hex) => hex.id === next.targetId && Boolean(hex.blocked),
          )
        );
      }
      const legal = board.action === next.boardAction
        ? board.legalVertexIds
        : next.boardAction === "city"
          ? board.buildableCityIds
          : board.buildableSettlementIds;
      return Boolean(board.isMyTurn) && (!legal || legal.includes(next.targetId));
    }
    if (next.kind === "discard") {
      return Boolean(board.isMyTurn) && board.action === "discard";
    }
    if (next.kind === "player") {
      return Boolean(
        board.robberVictimSelection &&
        board.robberVictimPlayers?.includes(next.player),
      );
    }
    if (next.kind === "trade") {
      if (this.settings.disablePlayerTrades && next.verdict !== "decline") {
        return false;
      }
      const trade = board.activeTrades?.[next.offerIndex];
      return Boolean(
        trade?.incoming &&
        trade.id === next.tradeId &&
        (!trade.myResponse || trade.myResponse === "pending") &&
        (next.verdict !== "accept" || trade.canAccept),
      );
    }
    if (next.kind === "trade-partner") {
      if (this.settings.disablePlayerTrades) return false;
      const trade = board.activeTrades?.[next.offerIndex];
      return Boolean(
        trade &&
        trade.id === next.tradeId &&
        !trade.incoming &&
        trade.acceptedPlayers?.includes(next.player),
      );
    }
    if (next.kind === "trade-cancel") {
      const trade = board.activeTrades?.[next.offerIndex];
      return Boolean(
        trade &&
        trade.id === next.tradeId &&
        !trade.incoming,
      );
    }
    if (next.kind === "turn-control") {
      if (!board.isMyTurn) return false;
      const current = visibleTurnControl();
      return next.control === "confirm" || current === next.control;
    }
    if (next.kind === "development") {
      return Boolean(
        board.isMyTurn &&
        (board.ownDevelopmentCards?.playable[next.card] ?? 0) > 0,
      );
    }
    if (next.kind === "trade-builder") {
      if (this.settings.disablePlayerTrades && next.mode === "player") {
        return false;
      }
      return Boolean(board.isMyTurn) && board.action === "none";
    }
    if (next.kind === "build") {
      return Boolean(board.isMyTurn) && board.action === "none";
    }
    if (next.kind === "resource") return Boolean(board.isMyTurn);
    return true;
  }

  private boardActionCommitted(
    next: Extract<NextClick, { kind: "board" }>,
  ): boolean {
    const board = this.board;
    if (!board) return false;
    if (next.boardAction === "road") {
      return board.edges.some(
        (edge) =>
          edge.id === next.targetId &&
          Boolean(edge.player) &&
          (!board.myPlayer || edge.player === board.myPlayer),
      );
    }
    if (next.boardAction === "robber") {
      return board.hexes.some(
        (hex) => hex.id === next.targetId && Boolean(hex.blocked),
      );
    }
    return board.vertices.some(
      (vertex) =>
        vertex.id === next.targetId &&
        vertex.building?.kind === next.boardAction &&
        (
          !board.myPlayer ||
          vertex.building.player === board.myPlayer
        ),
    );
  }

  private workflowContinuationStillLegal(next: NextClick): boolean {
    const board = this.board;
    if (!board || board.gameOver) return false;
    if (next.kind === "discard") {
      return Boolean(board.isMyTurn) && board.action === "discard";
    }
    if (next.kind === "player") {
      return Boolean(
        board.isMyTurn &&
        (
          board.robberVictimSelection ||
          board.action === "none"
        ),
      );
    }
    if (next.kind === "trade-builder") {
      if (this.settings.disablePlayerTrades && next.mode === "player") {
        return false;
      }
      return Boolean(
        board.isMyTurn &&
        (
          board.action === "none" ||
          tradePanelIsOpen()
        ),
      );
    }
    if (next.kind === "trade" && next.verdict === "counter") {
      if (this.settings.disablePlayerTrades) return false;
      const trade = board.activeTrades?.[next.offerIndex];
      return Boolean(
        trade?.incoming &&
        trade.id === next.tradeId &&
        (
          !trade.myResponse ||
          trade.myResponse === "pending"
        ),
      );
    }
    if (next.kind === "development") {
      // Once the card is confirmed it may disappear from `playable` before
      // its resource/victim/road parameter modal is complete. The retained
      // workflow signature plus turn ownership is the correct transaction
      // boundary for those follow-up clicks.
      return Boolean(
        board.isMyTurn &&
        (
          board.action === "none" ||
          board.action === "road" ||
          board.action === "robber"
        ),
      );
    }
    return this.nextClickStillLegal(next);
  }

  private decisionSourceForAuthority(
    authority: DecisionAuthority,
  ): DecisionActionSource {
    return authority;
  }

  private selectedDecisionAuthority(): DecisionAuthority {
    return this.settings.engine === "weighted" ? "weighted-policy" : "deep-maxn";
  }

  private decisionSource(
    next: NextClick,
    _workflow: boolean,
  ): DecisionActionSource {
    if (this.nextClickMatchesDeepAction(next)) {
      const authority = this.decisionAnalysis?.deepSearch?.authority;
      return authority
        ? this.decisionSourceForAuthority(authority)
        : this.selectedDecisionAuthority();
    }
    if (next.kind === "discard" || next.kind === "player") {
      return "mandatory";
    }
    if (next.kind === "trade") {
      return "incoming-trade-evaluator";
    }
    if (next.kind === "trade-cancel") {
      return "incoming-trade-evaluator";
    }
    if (
      next.kind === "board" &&
      (
        (
          this.queuedPlacement?.gameKey === this.board?.gameKey &&
          this.queuedPlacement?.action === next.boardAction &&
          this.queuedPlacement?.targetId === next.targetId
        ) ||
        (
          next.boardAction === "road" &&
          this.freeRoadPlan?.gameKey === this.board?.gameKey &&
          this.freeRoadPlan?.edgeIds[0] === next.targetId
        )
      )
    ) {
      return this.decisionAnalysis?.deepSearch?.authority ?? this.selectedDecisionAuthority();
    }
    if (next.kind === "turn-control" && next.control === "end") {
      return "end-turn-fallback";
    }
    if (next.kind === "board") return "placement-heuristic";
    if (next.kind === "build" && next.build === "road" && this.roadPlan) {
      return "road-plan";
    }
    return "coach-goal";
  }

  private nextClickMatchesDeepAction(next: NextClick): boolean {
    const action = this.decisionAnalysis?.deepSearch?.chosen;
    if (!action) return false;
    if (next.kind === "board") {
      const expected = {
        road: ["build-road", "place-road"],
        settlement: ["build-settlement", "place-settlement"],
        city: ["build-city"],
        robber: ["move-robber", "play-knight"],
      }[next.boardAction];
      return (
        expected.includes(action.kind) &&
        action.targetId === next.targetId
      );
    }
    if (next.kind === "build") {
      return (
        {
          road: "build-road",
          settlement: "build-settlement",
          city: "build-city",
          development: "buy-development",
        }[next.build] === action.kind
      );
    }
    if (next.kind === "turn-control") {
      return (
        (next.control === "roll" && action.kind === "roll") ||
        (next.control === "end" && action.kind === "end-turn")
      );
    }
    if (next.kind === "development") {
      return (
        {
          knight: "play-knight",
          monopoly: "play-monopoly",
          "road-building": "play-road-building",
          "year-of-plenty": "play-year-of-plenty",
          "victory-point": "",
        }[next.card] === action.kind
      );
    }
    if (next.kind === "trade-builder") {
      const sameVector = (
        tuple: [number, number, number, number, number] | undefined,
        vector: ResourceVector,
      ): boolean =>
        Boolean(tuple) &&
        RESOURCE_ORDER.every(
          (resource, index) => tuple?.[index] === vector[resource],
        );
      if (next.mode === "player") {
        return (
          action.kind === "offer-trade" &&
          sameVector(action.cards, next.give) &&
          sameVector(action.receiveCards, next.receive)
        );
      }
      return (
        action.kind === "maritime-trade" &&
        Boolean(action.resource) &&
        Boolean(action.otherResource) &&
        next.give[action.resource!] === Math.max(2, action.ratio ?? 4) &&
        next.receive[action.otherResource!] === 1
      );
    }
    if (next.kind === "trade-partner") {
      return action.kind === "confirm-trade" && action.player === next.player;
    }
    if (next.kind === "trade-cancel") {
      return action.kind === "cancel-trade";
    }
    if (next.kind === "trade") {
      if (next.verdict === "counter") return action.kind === "counter-trade";
      return (
        action.kind === "respond-trade" &&
        action.accept === (next.verdict === "accept")
      );
    }
    if (next.kind === "discard") {
      return (
        action.kind === "discard" &&
        Boolean(action.cards) &&
        RESOURCE_ORDER.every(
          (resource, index) => action.cards?.[index] === next.cards[resource],
        )
      );
    }
    if (next.kind === "player") {
      return (
        (action.kind === "move-robber" || action.kind === "play-knight") &&
        action.player === next.player
      );
    }
    if (next.kind === "resource") {
      return (
        (action.kind === "play-monopoly" && action.resource === next.resource) ||
        (
          action.kind === "play-year-of-plenty" &&
          (
            action.resource === next.resource ||
            action.otherResource === next.resource
          )
        )
      );
    }
    return false;
  }

  private decisionEngineLabel(): string {
    return this.settings.engine === "weighted" ? WEIGHTED_LABEL : STRATEGIST_LABEL;
  }

  private renderEngineStrip(): string {
    const runtime = this.runtimePresentation();
    return `<button class="model-strip engine-strip ${runtime.state}" data-action="view" data-view="settings" aria-label="Open decision engine settings. Runtime: ${escapeHtml(runtime.label.toLowerCase())}">
      <span>Decision engine <small>${escapeHtml(runtime.label.toUpperCase())}</small></span>
      <b>${escapeHtml(this.decisionEngineLabel())}</b>
    </button>`;
  }

  private runtimePresentation(): {
    label: string;
    detail: string;
    state: "healthy" | "searching" | "slow" | "error" | "connecting";
  } {
    const observedRuntime =
      this.decisionAnalysis?.runtime ?? this.decisionRuntime;
    if (this.decisionContextInvalidated) {
      return {
        label: "Reload tab",
        detail: EXTENSION_CONTEXT_RELOAD_MESSAGE,
        state: "error",
      };
    }
    if (this.decisionRuntimeError) {
      return {
        label: "WASM error",
        detail: `${this.decisionRuntimeError} Retry the selected engine when ready. No other algorithm was substituted.`,
        state: "error",
      };
    }
    if (
      this.decisionWaitingForPreviousSearch &&
      this.decisionPendingKey
    ) {
      return {
        label: "Waiting for previous search",
        detail: `An obsolete search is still finishing. ${this.decisionEngineLabel()} is queued and will start as soon as the background worker is free.`,
        state: "connecting",
      };
    }
    if (this.decisionPendingKey) {
      if (this.decisionSlowKey === this.decisionPendingKey) {
        return {
          label:
            observedRuntime === "background-gpu" ? "GPU · 1s+" : "WASM · 1s+",
          detail:
            `${this.decisionEngineLabel()} is still evaluating this position. No fallback policy is allowed to replace it.`,
          state: "slow",
        };
      }
      return {
        label:
          observedRuntime === "background-gpu"
            ? "GPU searching"
            : observedRuntime === "background-wasm"
              ? "WASM searching"
              : "Connecting",
        detail:
          observedRuntime === "background-gpu"
            ? "The native CUDA companion is evaluating this position with resident rollouts."
            : observedRuntime === "background-wasm"
              ? "The background engine is evaluating this position with a bounded node budget."
              : "Waking the selected decision engine before evaluating this position.",
        state: isSearchDecisionRuntime(observedRuntime) ? "searching" : "connecting",
      };
    }
    if (this.decisionAnalysis?.runtime === "background-rollout") {
      return {
        label: "Background rollout",
        detail:
          "The service worker is ready; lightweight win odds run while deep search waits for your turn.",
        state: "healthy",
      };
    }
    if (observedRuntime === "background-gpu") {
      const search = this.decisionAnalysis?.deepSearch;
      return {
        label: "Native CUDA GPU",
        detail: search
          ? `Last GPU search completed in ${Math.max(1, Math.round(search.elapsedMs)).toLocaleString()} ms across ${search.rollouts.toLocaleString()} resident rollouts.`
          : this.decisionRuntimeDetail,
        state: "healthy",
      };
    }
    if (observedRuntime === "background-wasm") {
      const search = this.decisionAnalysis?.deepSearch;
      return {
        label: "Background WASM",
        detail: search
          ? `Last search completed in ${Math.max(1, Math.round(search.elapsedMs)).toLocaleString()} ms across ${search.nodes.toLocaleString()} bounded nodes.`
          : this.decisionRuntimeDetail,
        state: "healthy",
      };
    }
    return {
      label: "Connecting",
      detail: "Waking the selected decision engine.",
      state: "connecting",
    };
  }

  private scheduleTradeVerdicts(state?: TrackerState): void {
    if (this.tradeRenderFrame !== undefined) {
      window.cancelAnimationFrame(this.tradeRenderFrame);
    }
    this.tradeRenderFrame = window.requestAnimationFrame(() => {
      this.tradeRenderFrame = undefined;
      const board = this.board;
      const player = this.userPlayer(state);
      const activeTrades = board?.activeTrades ?? [];
      if (
        !board ||
        board.gameOver ||
        !state ||
        !player ||
        !activeTrades.length
      ) {
        renderTradeVerdicts([], new Map());
        return;
      }
      const pendingIncoming = unansweredIncomingTrades(
        activeTrades,
        this.completedIncomingTradeIds,
      );
      if (this.settings.disablePlayerTrades) {
        renderTradeVerdicts(
          activeTrades,
          new Map(
            pendingIncoming.map((trade) => [
              trade.id,
              {
                tradeId: trade.id,
                kind: "decline" as const,
                score: 0,
                label: "DECLINE",
                reason: "Player trades are disabled",
                detail: "Only bank and port trades are allowed.",
              },
            ]),
          ),
        );
        return;
      }
      const report = this.coachReport(state, player);
      if (!report) {
        renderTradeVerdicts([], new Map());
        return;
      }
      if (
        this.decisionPendingKey &&
        isWasmDecisionEngine(this.settings.engine)
      ) {
        // Never paint a contradictory heuristic verdict while the
        // authoritative trade-response search is still running.
        renderTradeVerdicts(activeTrades, new Map());
        return;
      }
      const verdicts = new Map(
        pendingIncoming
          .flatMap((trade) => {
            const deepAction = this.preferredDeepAction(state, player, trade.id);
            const searchedKind =
              deepAction?.kind === "counter-trade"
                ? "counter"
                : deepAction?.kind === "respond-trade"
                  ? deepAction.accept
                    ? "accept"
                    : "decline"
                  : undefined;
            if (
              isFullySpecifiedTrade(trade) &&
              this.decisionAnalysis?.deepSearch &&
              !searchedKind
            ) {
              return [];
            }
            const verdict: TradeVerdict = searchedKind
              ? {
                  tradeId: trade.id,
                  kind: searchedKind,
                  score: 0,
                  label:
                    searchedKind === "accept"
                      ? "ACCEPT"
                      : searchedKind === "counter"
                        ? "COUNTER"
                        : "DECLINE",
                  reason:
                    searchedKind === "accept"
                      ? "The belief-weighted continuation improves your win race"
                      : searchedKind === "counter"
                        ? "A nearby bundle produces a stronger continuation"
                        : "Accepting helps the sender more than it helps your race",
                  detail:
                    "Strategist evaluated accept, reject, and legal counteroffer continuations.",
                }
              : evaluateTradeOffer(
                  state,
                  board,
                  player,
                  trade,
                  {
                    primaryKind: report.primary.kind,
                    primaryDeficit: report.primary.deficit,
                    phase: report.phase,
                  },
                );
            return [[trade.id, verdict] as const];
          }),
      );
      renderTradeVerdicts(activeTrades, verdicts);
    });
  }

  private decisionSearchConstraints(): DecisionSearchConstraints {
    return {
      ...(this.lastRejectedDomesticTrade
        ? {
            lastRejectedTrade: {
              give: { ...this.lastRejectedDomesticTrade.give },
              receive: { ...this.lastRejectedDomesticTrade.receive },
            },
          }
        : {}),
      ...(this.rootTradeActionExclusions.length
        ? {
            rootExclusions: this.rootTradeActionExclusions.map((exclusion) => ({
              kind: exclusion.kind,
              give: { ...exclusion.give },
              receive: { ...exclusion.receive },
            })),
          }
        : {}),
    };
  }

  private decisionSignature(
    state: TrackerState,
    board: BoardSnapshot,
    player: string,
    searchConstraints: DecisionSearchConstraints,
  ): string {
    return JSON.stringify({
      engine: this.settings.engine,
      playerTradesEnabled: !this.settings.disablePlayerTrades,
      game: board.gameKey,
      trackerTurn: state.currentTurn.sequence,
      eventCount: state.eventCount,
      boardTurn: board.turn,
      lastRoll: board.lastRoll,
      victoryTarget: board.victoryTarget,
      friendlyRobber: board.friendlyRobber,
      player,
      action: board.action,
      hasRolled: board.hasRolled,
      discardCount: board.discardCount,
      robberHex: board.hexes.find((hex) => hex.blocked)?.id,
      robberVictimSelection: board.robberVictimSelection,
      robberVictimPlayers: board.robberVictimPlayers,
      domesticTradeUsed: board.domesticTradeUsed,
      searchConstraints,
      currentPlayer: board.currentPlayer,
      legalVertices: [...(board.legalVertexIds ?? [])].sort(),
      legalEdges: [...(board.legalEdgeIds ?? [])].sort(),
      buildableSettlements: [
        ...(board.buildableSettlementIds ?? []),
      ].sort(),
      buildableCities: [...(board.buildableCityIds ?? [])].sort(),
      buildableRoads: [...(board.buildableRoadIds ?? [])].sort(),
      hand: board.ownHand,
      dev: board.ownDevelopmentCards,
      bank: board.bankVisible ? board.bank : undefined,
      trades: board.activeTrades?.map((trade) => ({
        id: trade.id,
        creator: trade.creator,
        executor: trade.tradeExecutor,
        incoming: trade.incoming,
        counter: trade.counterOffer,
        canAccept: trade.canAccept,
        give: localTradeBundles(trade).give,
        receive: localTradeBundles(trade).receive,
        accepted: trade.acceptedPlayers,
        pending: trade.pendingPlayers,
        rejected: trade.rejectedPlayers,
        complete: trade.responsesComplete,
        response: trade.myResponse,
      })),
      public: board.players,
      beliefs: state.playerOrder.map((candidate) => {
        const estimate = getPlayerEstimate(state, candidate);
        return [
          candidate,
          estimate.minimum,
          estimate.maximum,
          estimate.possibilities,
          state.players[candidate]?.opponentModel.policyPosterior,
        ];
      }),
      worlds: state.worlds.map((world) => ({
        weight: world.weight,
        hands: world.hands,
      })),
      pieces: [
        ...board.vertices.flatMap((vertex) =>
          vertex.building
            ? [`${vertex.id}:${vertex.building.player}:${vertex.building.kind}`]
            : [],
        ),
        ...board.edges.flatMap((edge) =>
          edge.player ? [`${edge.id}:${edge.player}`] : [],
        ),
      ],
    });
  }

  private scheduleDecisionAnalysis(
    state: TrackerState | undefined,
    player: string | undefined,
  ): void {
    const board = this.board;
    const hasPendingIncomingTrade = unansweredIncomingTrades(
      board?.activeTrades,
      this.completedIncomingTradeIds,
    ).some(isFullySpecifiedTrade);
    if (
      !state ||
      !player ||
      !board ||
      board.gameOver
    ) {
      this.decisionPendingKey = "";
      this.decisionSlowKey = "";
      this.decisionWaitingForPreviousSearch = false;
      return;
    }
    const decisionBoard = board.activeTrades
      ? {
          ...board,
          activeTrades: board.activeTrades.filter(
            (trade) =>
              !trade.incoming ||
              !this.completedIncomingTradeIds.has(trade.id),
          ),
        }
      : board;
    if (this.decisionContextInvalidated) {
      this.decisionPendingKey = "";
      this.decisionSlowKey = "";
      this.decisionWaitingForPreviousSearch = false;
      return;
    }
    if (this.decisionRuntimeError) {
      this.decisionPendingKey = "";
      this.decisionSlowKey = "";
      this.decisionWaitingForPreviousSearch = false;
      return;
    }
    if (
      board.initialPlacement &&
      !board.isMyTurn &&
      !hasPendingIncomingTrade
    ) {
      // Setup search returns the acting player's placement. Running it during
      // another seat's road transition both wastes the live budget and can
      // observe Colonist's previous prompt before the new settlement anchor
      // is public. Start the bounded draft search when our setup turn begins.
      if (
        this.decisionKey ||
        this.decisionPendingKey ||
        this.decisionAnalysis
      ) {
        this.decisionAnalysis = undefined;
        this.decisionKey = "";
        this.decisionPendingKey = "";
        this.decisionSlowKey = "";
        this.decisionRuntimeError = "";
        this.decisionWorker.reset();
      }
      return;
    }
    if (
      !board.isMyTurn &&
      !hasPendingIncomingTrade
    ) {
      // Opponent-turn Deep Search runs synchronously inside the background
      // worker and cannot be cancelled once WASM starts. Skip speculative
      // pondering so stale work cannot block the next authoritative decision.
      if (
        this.decisionKey ||
        this.decisionPendingKey ||
        this.decisionAnalysis
      ) {
        this.decisionAnalysis = undefined;
        this.decisionKey = "";
        this.decisionPendingKey = "";
        this.decisionSlowKey = "";
        this.decisionRuntimeError = "";
        this.decisionWorker.reset();
      }
      return;
    }
    const retainedPlacementTarget =
      (
        (
          board.action === "road" ||
          board.action === "settlement" ||
          board.action === "city"
        ) &&
        this.queuedPlacement?.gameKey === board.gameKey &&
        this.queuedPlacement?.action === board.action
      )
        ? this.queuedPlacement.targetId
        : board.action === "road" &&
            this.freeRoadPlan?.gameKey === board.gameKey
          ? this.freeRoadPlan?.edgeIds[0]
          : undefined;
    const retainedAnalysis = this.decisionAnalysis;
    if (
      retainedPlacementTarget &&
      retainedAnalysis &&
      isSearchDecisionRuntime(retainedAnalysis.runtime)
    ) {
      const searchConstraints = this.decisionSearchConstraints();
      const key = this.decisionSignature(
        state,
        decisionBoard,
        player,
        searchConstraints,
      );
      const traceKey = decisionStateDigest(key);
      if (key !== this.decisionKey) {
        this.decisionKey = key;
        this.decisionPendingKey = "";
        this.decisionSlowKey = "";
        this.decisionRuntimeError = "";
        this.decisionTraces.begin(traceKey, state, board);
        // This is the parameterized continuation of the already-completed
        // deep build/development action, not a fresh strategic position.
        this.decisionTraces.complete(traceKey, retainedAnalysis);
      }
      return;
    }
    const visibleControl = visibleTurnControl();
    if (
      shouldFastTrackRoll(board, visibleControl) ||
      shouldFastTrackEndTurn(board, visibleControl)
    ) {
      this.decisionAnalysis = undefined;
      this.decisionKey = "";
      this.decisionPendingKey = "";
      this.decisionSlowKey = "";
      this.decisionWorker.reset();
      return;
    }
    const searchConstraints = this.decisionSearchConstraints();
    const key = this.decisionSignature(
      state,
      decisionBoard,
      player,
      searchConstraints,
    );
    const traceKey = decisionStateDigest(key);
    if (key !== this.decisionKey) {
      this.decisionKey = key;
      this.decisionAnalysis = undefined;
      this.decisionPendingKey = "";
      this.decisionSlowKey = "";
      this.decisionWaitingForPreviousSearch = false;
      this.decisionRuntimeError = "";
      if (board.isMyTurn || hasPendingIncomingTrade) {
        this.decisionTraces.begin(traceKey, state, board);
      }
    }
    const requestDisposition = this.decisionWorker.request(
      key,
      state,
      {
        ...decisionBoard,
        hasRolled:
          decisionBoard.hasRolled ??
          visibleTurnControl() !== "roll",
      },
      player,
      this.settings.engine,
      (analysis) => {
        if (this.decisionKey !== key) return;
        this.decisionPendingKey = "";
        this.decisionSlowKey = "";
        this.decisionWaitingForPreviousSearch = false;
        this.decisionRuntimeError = "";
        this.decisionAnalysis = analysis;
        this.decisionTraces.complete(traceKey, analysis);
        if (analysis.runtime) {
          this.decisionRuntime = analysis.runtime;
          this.decisionRuntimeDetail =
            analysis.runtimeReason ?? this.decisionRuntimeDetail;
        }
        const action = analysis.deepSearch?.chosen;
        if (
          action?.kind === "play-road-building" &&
          action.targetId
        ) {
          this.freeRoadPlan = {
            gameKey: board.gameKey,
            edgeIds: [
              action.targetId,
              ...(action.secondTargetId ? [action.secondTargetId] : []),
            ],
          };
        }
        this.render();
      },
      () => {
        if (this.decisionKey !== key || this.decisionPendingKey !== key) {
          return;
        }
        this.decisionWaitingForPreviousSearch = false;
        this.decisionSlowKey = key;
        this.decisionTraces.slow(traceKey);
        this.render();
      },
      (detail) => {
        if (this.decisionKey !== key) return;
        // Keep authoritative-decision gates closed. A failed WASM request
        // must never expose an executable coaching fallback.
        this.decisionPendingKey = "";
        this.decisionSlowKey = "";
        this.decisionWaitingForPreviousSearch = false;
        this.decisionAnalysis = undefined;
        const contextInvalidated =
          detail === EXTENSION_CONTEXT_RELOAD_MESSAGE ||
          isExtensionContextInvalidatedError(detail);
        if (contextInvalidated) this.decisionContextInvalidated = true;
        const displayedDetail = this.decisionContextInvalidated
          ? EXTENSION_CONTEXT_RELOAD_MESSAGE
          : detail;
        this.decisionRuntimeError = displayedDetail;
        this.decisionRuntimeDetail = displayedDetail;
        this.decisionTraces.failure(traceKey, displayedDetail);
        console.error(
          `[Colonist Assistant] Selected decision engine failed: ${displayedDetail}`,
          {
            key,
            engine: this.settings.engine,
            detail: displayedDetail,
            policy: "selected-engine-only",
            fallbackStarted: false,
          },
        );
        this.render();
      },
      searchConstraints,
      !this.settings.disablePlayerTrades,
      () => {
        if (
          this.decisionKey !== key ||
          this.decisionContextInvalidated ||
          this.decisionPendingKey !== key
        ) {
          return;
        }
        this.decisionWaitingForPreviousSearch = false;
        this.render();
      },
    );
    if (
      requestDisposition === "started" ||
      requestDisposition === "queued"
    ) {
      // A failed request may be retried for the same decision key. Associate
      // each accepted worker request with a bounded attempt record instead of
      // letting a later success overwrite the earlier failure evidence.
      this.decisionTraces.begin(traceKey, state, board);
      this.decisionPendingKey = key;
      this.decisionSlowKey = "";
      this.decisionWaitingForPreviousSearch = requestDisposition === "queued";
      this.decisionRuntimeError = "";
      return;
    }
    if (requestDisposition === "context-invalidated") {
      this.decisionPendingKey = "";
      this.decisionSlowKey = "";
      this.decisionWaitingForPreviousSearch = false;
      this.decisionAnalysis = undefined;
      this.decisionContextInvalidated = true;
      this.decisionRuntimeError = EXTENSION_CONTEXT_RELOAD_MESSAGE;
      this.decisionRuntimeDetail = EXTENSION_CONTEXT_RELOAD_MESSAGE;
      this.decisionTraces.failure(traceKey, EXTENSION_CONTEXT_RELOAD_MESSAGE);
    }
  }

  private coachReport(
    state: TrackerState,
    player: string,
  ): CoachReport | undefined {
    const hasPendingIncomingTrade = Boolean(
      unansweredIncomingTrades(
        this.board?.activeTrades,
        this.completedIncomingTradeIds,
      ).length,
    );
    const prepared =
      (this.board?.isMyTurn || hasPendingIncomingTrade) &&
      this.decisionAnalysis?.engine === this.settings.engine
        ? this.decisionAnalysis
        : undefined;
    return createCoachReport(
      state,
      player,
      this.board,
      prepared,
    );
  }

  private reconciledState(): TrackerState | undefined {
    const board = this.board;
    const sessionState = this.session?.state;
    const hasTrackedPlayers = Boolean(sessionState?.playerOrder.length);
    const state = hasTrackedPlayers
      ? sessionState
      : this.stateFromPublicBoard(board);
    if (!state || !board) return state;
    const playerCount = Object.keys(board.players ?? {}).length;
    const resourceSupply = resourceSupplyForPlayerCount(playerCount);
    const resources = reconcilePublicResourceEvidence(state, {
      exactHands:
        board.myPlayer && board.ownHand
          ? {
              [board.myPlayer]:
                this.confirmedPlacementSpend?.player === board.myPlayer &&
                this.confirmedPlacementSpend.gameKey === board.gameKey
                  ? this.confirmedPlacementSpend.after
                  : board.ownHand,
            }
          : undefined,
      handSizes: board.players
        ? Object.fromEntries(
            Object.entries(board.players).map(([player, publicState]) => [
              player,
              publicState.handSize,
            ]),
          )
        : undefined,
      ...(board.bankVisible && board.bank
        ? { bank: board.bank, resourceSupply }
        : {}),
    });
    if (resources.worlds.length || !hasTrackedPlayers) return resources;

    // A midgame attach can recover enough public log history to discover the
    // players while still leaving its hidden-resource worlds incompatible with
    // the exact hand sizes/bank Colonist currently exposes. The public board is
    // authoritative at that boundary, so reuse the existing physical-card
    // fallback posterior instead of leaving the selected engine with no worlds.
    const publicFallback = this.stateFromPublicBoard(board);
    return publicFallback?.worlds.length
      ? { ...resources, worlds: publicFallback.worlds }
      : resources;
  }

  private stateFromPublicBoard(
    board?: BoardSnapshot,
  ): TrackerState | undefined {
    if (!board?.players || !Object.keys(board.players).length) return undefined;
    const players = [
      ...(board.playerOrder ?? []),
      ...Object.keys(board.players),
    ].filter((player, index, all) => all.indexOf(player) === index);
    if (players.length < 2) return undefined;
    let state = createTrackerState();
    for (const player of players) {
      state = reduceTracker(state, { type: "discover", player });
      const meta = state.players[player]!;
      meta.builds.road = board.edges.filter(
        (edge) => edge.player === player,
      ).length;
      meta.builds.settlement = board.vertices.filter(
        (vertex) =>
          vertex.building?.player === player &&
          vertex.building.kind === "settlement",
      ).length;
      meta.builds.city = board.vertices.filter(
        (vertex) =>
          vertex.building?.player === player &&
          vertex.building.kind === "city",
      ).length;
      meta.builds.development =
        board.players[player]?.developmentCards ?? 0;
      meta.devCards = Array.from(
        { length: meta.builds.development },
        () => ({ boughtOnTurn: 0 }),
      );
      Object.assign(
        meta.playedDevCards,
        board.players[player]?.playedDevelopmentCards ?? {},
      );
    }
    state.currentTurn = {
      player: board.currentPlayer,
      sequence: board.turn ?? 0,
    };
    // If the public log mounts late, reconstruct a finite posterior from the
    // physical resource-card conditional instead of creating empty opponent
    // hands that reconciliation would immediately filter to zero worlds.
    if (board.myPlayer && board.ownHand) {
      const handSizes = Object.fromEntries(
        players.map((player) => [
          player,
          board.players?.[player]?.handSize ?? 0,
        ]),
      );
      const resourceSupply = resourceSupplyForPlayerCount(players.length);
      const particleCount = players.length >= 3 ? 24 : 16;
      const seed = publicResourceSeed(
        JSON.stringify({
          game: board.gameKey ?? null,
          turn: board.turn ?? 0,
          currentPlayer: board.currentPlayer ?? null,
          players: players.map((player) => [player, handSizes[player]]),
          ownHand: board.ownHand,
          bank: board.bankVisible ? board.bank ?? null : null,
        }),
      );
      state.worlds = seedPublicResourceWorlds({
        playerOrder: players,
        ownPlayer: board.myPlayer,
        exactOwnHand: board.ownHand,
        handSizes,
        ...(board.bankVisible && board.bank ? { bank: board.bank } : {}),
        resourceSupply,
        seed,
        sampleCount: particleCount,
      });
    }
    return state;
  }

  private placementContext(
    board: BoardSnapshot,
    state?: TrackerState,
    report?: CoachReport,
  ): PlacementContext {
    const player = report?.player ?? this.userPlayer(state) ?? board.myPlayer ?? "You";
    const production = emptyResources();
    const currentNumbers: number[] = [];
    for (const vertex of board.vertices) {
      if (vertex.building?.player !== player) continue;
      const multiplier = vertex.building.kind === "city" ? 2 : 1;
      for (const id of vertex.adjacentHexes) {
        const hex = board.hexes.find((candidate) => candidate.id === id);
        if (!hex?.resource || !hex.number) continue;
        production[hex.resource] += (NUMBER_PIPS[hex.number] ?? 0) * multiplier;
        currentNumbers.push(hex.number);
      }
    }
    const currentResources = RESOURCE_ORDER.filter((resource) => production[resource] > 0);
    const desiredResources = board.initialPlacement
      ? currentResources.length
        ? RESOURCE_ORDER.filter((resource) => !currentResources.includes(resource))
        : (["grain", "ore", "wool"] as Resource[])
      : report
        ? RESOURCE_ORDER.filter((resource) => report.primary.deficit[resource] > 0)
        : (["grain", "ore", "wool"] as Resource[]);
    const opponentThreat = state
      ? Object.fromEntries(
          state.playerOrder
            .filter((candidate) => candidate !== player)
            .map((candidate) => [candidate, playerThreat(state, candidate)]),
        )
      : {};
    return {
      player,
      production,
      currentResources,
      currentNumbers,
      desiredResources,
      opponentThreat,
      stealPriority: report?.steal
        ? { [report.steal.player]: Math.min(10, report.steal.score / 8) }
        : {},
      legalVertexIds: board.legalVertexIds,
      legalEdgeIds: board.legalEdgeIds,
      preferredRoadTargetId: this.roadPlan?.targetId,
      initialPlacement: board.initialPlacement,
      picksUntilNext: board.picksUntilNext,
      requireConnection: !board.initialPlacement,
    };
  }

  private spatialRecommendation(
    state?: TrackerState,
  ):
    | {
        action: Exclude<BoardAction, "none" | "discard">;
        recommendation: PlacementRecommendation;
        alternatives: PlacementRecommendation[];
        report?: CoachReport;
        proactive: boolean;
      }
    | undefined {
    const board = this.board;
    if (!board?.isMyTurn || board.gameOver) return undefined;
    if (
      this.decisionRuntimeError &&
      isWasmDecisionEngine(this.settings.engine)
    ) {
      return undefined;
    }
    if (
      board.action !== "none" &&
      board.action !== "discard" &&
      this.decisionPendingKey &&
      isWasmDecisionEngine(this.settings.engine)
    ) {
      // Every spatial prompt is owned by the same state-locked planner. Do not
      // let a legacy coordinate score race settlement, road, city, or robber
      // search while that board signature is still being evaluated.
      return undefined;
    }
    const coachPlayer = this.userPlayer(state);
    const report =
      state && coachPlayer
        ? this.coachReport(state, coachPlayer)
        : undefined;
    const deepAction = this.preferredDeepAction(state, coachPlayer);
    const authoritativeDeep =
      Boolean(this.decisionAnalysis?.deepSearch) &&
      isSearchDecisionRuntime(this.decisionAnalysis?.runtime);
    const deepSpatialAction =
      deepAction?.kind === "build-road" || deepAction?.kind === "place-road"
        ? "road"
        : deepAction?.kind === "build-settlement" ||
            deepAction?.kind === "place-settlement"
          ? "settlement"
          : deepAction?.kind === "build-city"
            ? "city"
            : deepAction?.kind === "move-robber"
              ? "robber"
              : undefined;
    const activeAction =
      board.action && board.action !== "none" && board.action !== "discard"
        ? board.action
        : undefined;
    const proactiveAction = !activeAction
      ? deepSpatialAction
      : undefined;
    const action = activeAction ?? proactiveAction;
    if (!action) return undefined;
    const context = this.placementContext(board, state, report);
    context.legalVertexIds =
      action === "settlement"
        ? activeAction
          ? board.legalVertexIds
          : board.buildableSettlementIds
        : action === "city"
          ? activeAction
            ? board.legalVertexIds
            : board.buildableCityIds
          : undefined;
    context.legalEdgeIds =
      action === "road"
        ? activeAction
          ? board.legalEdgeIds
          : board.buildableRoadIds
        : undefined;
    context.initialPlacement = Boolean(activeAction && board.initialPlacement);
    context.requireConnection = !context.initialPlacement;
    const recommendations =
      action === "settlement"
        ? scoreSettlementPlacements(board, context)
        : action === "city"
          ? scoreCityPlacements(board, context)
          : action === "road"
            ? scoreRoadPlacements(board, context)
            : scoreRobberPlacements(board, context);
    if (
      this.freeRoadPlan &&
      this.freeRoadPlan.gameKey &&
      board.gameKey &&
      this.freeRoadPlan.gameKey !== board.gameKey
    ) {
      this.freeRoadPlan = undefined;
    }
    if (this.freeRoadPlan) {
      this.freeRoadPlan.edgeIds = this.freeRoadPlan.edgeIds.filter(
        (edgeId) => !board.edges.find((edge) => edge.id === edgeId)?.player,
      );
      if (!this.freeRoadPlan.edgeIds.length) this.freeRoadPlan = undefined;
    }
    const queuedFreeRoad =
      action === "road" && activeAction && this.freeRoadPlan?.edgeIds[0]
        ? recommendations.find(
            (candidate) => candidate.id === this.freeRoadPlan?.edgeIds[0],
          )
        : undefined;
    const searched =
      deepAction?.targetId && deepSpatialAction === action
        ? recommendations.find(
            (candidate) => candidate.id === deepAction.targetId,
          )
        : undefined;
    if (
      authoritativeDeep &&
      activeAction &&
      !queuedFreeRoad &&
      (
        deepSpatialAction !== action ||
        Boolean(deepAction?.targetId && !searched)
      )
    ) {
      // The authoritative target no longer maps to this exact legal set.
      // Wait for the changed signature to be replanned; never click the top
      // heuristic coordinate as a substitute.
      return undefined;
    }
    let recommendation =
      queuedFreeRoad ??
      searched ??
      (authoritativeDeep ? undefined : recommendations[0]);
    if (action === "road" && recommendations.length) {
      const continuing = !authoritativeDeep && this.roadPlan
        ? recommendations.find(
            (candidate) =>
              candidate.metrics?.targetId === this.roadPlan?.targetId &&
              candidate.metrics?.strategicallyUseful,
          )
        : undefined;
      recommendation =
        queuedFreeRoad ??
        searched ??
        (
          deepAction?.kind === "build-road" ||
          deepAction?.kind === "place-road"
            ? undefined
            : continuing ?? recommendations[0]
        );
      if (
        !authoritativeDeep &&
        recommendation?.metrics?.targetId &&
        recommendation.metrics.strategicallyUseful
      ) {
        this.roadPlan = {
          gameKey: board.gameKey,
          targetId: recommendation.metrics.targetId,
        };
      } else if (authoritativeDeep || !continuing) {
        this.roadPlan = undefined;
      }
      if (
        proactiveAction === "road" &&
        !recommendation?.metrics?.strategicallyUseful
      ) {
        return undefined;
      }
    }
    if (!recommendation) return undefined;
    if (
      deepAction?.targetId === recommendation.id &&
      this.decisionAnalysis?.deepSearch
    ) {
      const search = this.decisionAnalysis.deepSearch;
      const statistic = search.actions.find(
        (candidate) => candidate.action.targetId === recommendation?.id,
      );
      recommendation = {
        ...recommendation,
        ...(deepAction.player ? { targetPlayer: deepAction.player } : {}),
        reasons: [
          `Strategist selected this after ${search.nodes.toLocaleString()} search nodes at decision depth ${search.deepestDecisionDepth}`,
          statistic
            ? `Its relative strategic value is ${Math.round((statistic.value[search.rootIndex] ?? 0) * 100)} across the current belief set`
            : `It remained best across ${search.particles} legal hidden-card particles`,
          ...recommendation.reasons,
        ],
      };
    }
    return {
      action,
      recommendation,
      alternatives: recommendations
        .filter((candidate) => candidate.id !== recommendation.id)
        .slice(0, 2),
      proactive: Boolean(proactiveAction),
      ...(report ? { report } : {}),
    };
  }

  private nextClick(
    state: TrackerState | undefined,
    spatial: ReturnType<AssistantOverlay["spatialRecommendation"]>,
    report: CoachReport | undefined,
  ): NextClick | undefined {
    const board = this.board;
    if (
      !board ||
      board.gameOver ||
      this.resetGameScope !== undefined
    ) {
      return undefined;
    }
    const signatureBase = JSON.stringify({
      game: board.gameKey ?? location.pathname,
      turn: board.turn,
      currentPlayer: board.currentPlayer,
      action: board.action ?? "none",
      hasRolled: board.hasRolled,
      discardCount: board.discardCount,
      robberVictimSelection: board.robberVictimSelection,
      robberVictimPlayers: board.robberVictimPlayers,
      hand: RESOURCE_ORDER.map(
        (resource) => board.ownHand?.[resource] ?? 0,
      ),
      legalVertices: [...(board.legalVertexIds ?? [])].sort(),
      legalEdges: [...(board.legalEdgeIds ?? [])].sort(),
      buildableSettlements: [
        ...(board.buildableSettlementIds ?? []),
      ].sort(),
      buildableCities: [...(board.buildableCityIds ?? [])].sort(),
      buildableRoads: [...(board.buildableRoadIds ?? [])].sort(),
      trades: (board.activeTrades ?? []).map((trade) => ({
        id: trade.id,
        creator: trade.creator,
        executor: trade.tradeExecutor,
        incoming: trade.incoming,
        counter: trade.counterOffer,
        canAccept: trade.canAccept,
        give: localTradeBundles(trade).give,
        receive: localTradeBundles(trade).receive,
        accepted: trade.acceptedPlayers,
        pending: trade.pendingPlayers,
        rejected: trade.rejectedPlayers,
        complete: trade.responsesComplete,
        response: trade.myResponse,
      })),
    });
    const discard = this.discardRecommendation(state);
    if (discard) {
      return {
        kind: "discard",
        cards: discard.discard,
        label: `Discard ${discard.count} cards`,
        // Colonist temporarily removes selected cards from the displayed hand.
        // Keep one workflow identity for the complete discard transaction so a
        // selection-driven snapshot cannot restart the plan midway through.
        signature: `${board.gameKey ?? location.pathname}|${board.turn}|discard|${board.discardCount ?? discard.count}`,
        confidence: 1,
      };
    }

    if (
      board.robberVictimSelection &&
      board.robberVictimPlayers?.length
    ) {
      const candidates = board.robberVictimPlayers;
      const planned = this.robberVictimPlan?.player;
      const strategicSteal =
        report?.steal?.player &&
        candidates.includes(report.steal.player)
          ? report.steal.player
          : undefined;
      const player =
        (planned && candidates.includes(planned) ? planned : undefined) ??
        strategicSteal ??
        [...candidates].sort((left, right) => {
          const leftWin =
            report?.decisionAnalysis?.players.find(
              (candidate) => candidate.player === left,
            )?.probability ?? 0;
          const rightWin =
            report?.decisionAnalysis?.players.find(
              (candidate) => candidate.player === right,
            )?.probability ?? 0;
          const leftCards = board.players?.[left]?.handSize ?? 0;
          const rightCards = board.players?.[right]?.handSize ?? 0;
          return rightWin - leftWin || rightCards - leftCards;
        })[0]!;
      return {
        kind: "player",
        player,
        label: `Steal from ${player}`,
        signature: `${board.gameKey ?? location.pathname}|${board.turn}|robber-victim|${player}`,
        confidence: 0.96,
      };
    }

    if (state && board.activeTrades?.length) {
      for (let index = 0; index < board.activeTrades.length; index += 1) {
        const trade = board.activeTrades[index]!;
        if (
          !trade.incoming ||
          this.completedIncomingTradeIds.has(trade.id) ||
          (
            trade.myResponse &&
            trade.myResponse !== "pending"
          )
        ) {
          continue;
        }
        if (this.settings.disablePlayerTrades) {
          return {
            kind: "trade",
            offerIndex: index,
            tradeId: trade.id,
            verdict: "decline",
            label: "Decline this trade",
            signature: `${signatureBase}|trade|${trade.id}|decline|player-trades-disabled`,
            confidence: 1,
          };
        }
        if (!report) continue;
        const deepAction = this.preferredDeepAction(
          state,
          report.player,
          trade.id,
        );
        const deepVerdict =
          deepAction?.kind === "respond-trade"
            ? deepAction.accept
              ? "accept"
              : "decline"
            : deepAction?.kind === "counter-trade"
              ? "counter"
              : undefined;
        if (
          isFullySpecifiedTrade(trade) &&
          !deepVerdict &&
          (
            this.decisionPendingKey ||
            this.decisionAnalysis?.deepSearch
          ) &&
          isWasmDecisionEngine(this.settings.engine)
        ) {
          // A missing or stale searched response is an engine-error state, not
          // permission for the older material-gain heuristic to accept a deal.
          return undefined;
        }
        const fallbackVerdict = deepVerdict
          ? undefined
          : evaluateTradeOffer(state, board, report.player, trade, {
              primaryKind: report.primary.kind,
              primaryDeficit: report.primary.deficit,
              phase: report.phase,
            });
        let verdict = deepVerdict ?? fallbackVerdict?.kind ?? "decline";
        const prospectiveSignature = `${signatureBase}|trade|${trade.id}|${verdict}`;
        if (
          verdict === "counter" &&
          this.failedTradeActions.has(prospectiveSignature)
        ) {
          // A counter transaction that Colonist rejected or could not commit
          // must not loop forever. Declining is the safe protocol completion.
          verdict = "decline";
        }
        const counterGive =
          deepAction?.kind === "counter-trade" && deepAction.cards
            ? tupleResources(deepAction.cards)
            : fallbackVerdict?.counterGive;
        const counterReceive =
          deepAction?.kind === "counter-trade" &&
          deepAction.receiveCards
            ? tupleResources(deepAction.receiveCards)
            : fallbackVerdict?.counterReceive;
        return {
          kind: "trade",
          offerIndex: index,
          tradeId: trade.id,
          verdict,
          label:
            verdict === "accept"
              ? "Accept this trade"
              : verdict === "counter"
                ? "Counter this trade"
                : "Decline this trade",
          signature: `${signatureBase}|trade|${trade.id}|${verdict}`,
          confidence:
            deepVerdict
              ? 0.94
              : verdict === "accept"
                ? Math.min(
                    0.98,
                    0.82 + Math.max(0, fallbackVerdict?.score ?? 0) / 120,
                  )
                : verdict === "decline"
                  ? Math.min(
                      0.98,
                      0.84 + Math.max(0, -(fallbackVerdict?.score ?? 0)) / 130,
                    )
                  : 0.72,
          ...(counterGive && counterReceive
            ? {
                counterGive,
                counterReceive,
                existingGive: localTradeBundles(trade).give,
                existingReceive: localTradeBundles(trade).receive,
              }
            : {}),
        };
      }
    }

    const outgoingTrades = (board.activeTrades ?? []).filter(
      (trade) => !trade.incoming,
    );
    for (const trade of outgoingTrades) {
      if (this.settings.disablePlayerTrades) {
        return {
          kind: "trade-cancel",
          offerIndex: board.activeTrades!.indexOf(trade),
          tradeId: trade.id,
          label: "Cancel player trade",
          signature: `${signatureBase}|cancel-trade|${trade.id}|player-trades-disabled`,
          confidence: 1,
        };
      }
      if (trade.acceptedPlayers?.length) {
        const deepSearch = this.decisionAnalysis?.deepSearch;
        if (!deepSearch || this.decisionPendingKey) return undefined;
        const deepAction = deepSearch.chosen;
        if (deepAction?.kind === "cancel-trade") {
          return {
            kind: "trade-cancel",
            offerIndex: board.activeTrades!.indexOf(trade),
            tradeId: trade.id,
            label: "Cancel this accepted trade",
            signature: `${signatureBase}|cancel-trade|${trade.id}|rust`,
            confidence: 1,
          };
        }
        if (
          deepAction?.kind !== "confirm-trade" ||
          !deepAction.player ||
          !trade.acceptedPlayers.includes(deepAction.player)
        ) {
          return undefined;
        }
        const selected = deepAction.player;
        return {
          kind: "trade-partner",
          offerIndex: board.activeTrades!.indexOf(trade),
          tradeId: trade.id,
          acceptedIndex: trade.acceptedPlayers.indexOf(selected),
          player: selected,
          label: `Trade with ${selected}`,
          signature: `${signatureBase}|execute-trade|${trade.id}|${selected}`,
          confidence: 0.96,
        };
      }
      if (!trade.responsesComplete) {
        const firstSeen =
          this.outgoingTradeSeenAt.get(trade.id) ?? Date.now();
        if (
          outgoingTradeDisposition(
            trade.responsesComplete,
            firstSeen,
            Date.now(),
          ) === "wait"
        ) {
          return undefined;
        }
        return {
          kind: "trade-cancel",
          offerIndex: board.activeTrades!.indexOf(trade),
          tradeId: trade.id,
          label: "Cancel unanswered trade",
          signature: `${signatureBase}|cancel-trade|${trade.id}|timeout`,
          confidence: 1,
        };
      }
      const deepSearch = this.decisionAnalysis?.deepSearch;
      if (!deepSearch || this.decisionPendingKey) return undefined;
      if (deepSearch.chosen?.kind !== "cancel-trade") return undefined;
      return {
        kind: "trade-cancel",
        offerIndex: board.activeTrades!.indexOf(trade),
        tradeId: trade.id,
        label: "Close rejected trade",
        signature: `${signatureBase}|cancel-trade|${trade.id}|complete`,
        confidence: 1,
      };
    }

    const queuedPlacement = this.queuedPlacement;
    const queuedContinuation =
      spatial &&
      !spatial.proactive &&
      (
        (
          queuedPlacement?.gameKey === board.gameKey &&
          queuedPlacement?.action === spatial.action &&
          queuedPlacement?.targetId === spatial.recommendation.id
        ) ||
        (
          spatial.action === "road" &&
          this.freeRoadPlan?.edgeIds[0] === spatial.recommendation.id
        )
      );
    if (queuedContinuation) {
      const source =
        spatial.action === "road"
          ? board.edges.find(
              (edge) => edge.id === spatial.recommendation.id,
            )
          : board.vertices.find(
              (vertex) => vertex.id === spatial.recommendation.id,
            );
      if (source?.screen) {
        return {
          kind: "board",
          boardAction: spatial.action,
          targetId: spatial.recommendation.id,
          point: source.screen,
          label: `Place ${spatial.action} here`,
          signature: `${signatureBase}|board-continuation|${spatial.action}|${spatial.recommendation.id}`,
          confidence: 0.99,
        };
      }
      this.queuedPlacement = undefined;
    }

    if (shouldFastTrackRoll(board, visibleTurnControl())) {
      return {
        kind: "turn-control",
        control: "roll",
        label: "Roll dice",
        signature: `${signatureBase}|forced-roll`,
        confidence: 1,
      };
    }

    if (shouldFastTrackEndTurn(board, visibleTurnControl())) {
      return {
        kind: "turn-control",
        control: "end",
        label: "End turn",
        signature: `${signatureBase}|forced-end`,
        confidence: 1,
      };
    }

    if (
      this.decisionPendingKey &&
      isWasmDecisionEngine(this.settings.engine)
    ) {
      // Mandatory protocol actions above remain responsive. Strategic
      // placement/build/roll/end-turn fallbacks may not race the authoritative
      // deep request for this exact state signature.
      return undefined;
    }

    if (spatial && !spatial.proactive) {
      const source =
        spatial.action === "road"
          ? board.edges.find((edge) => edge.id === spatial.recommendation.id)
          : spatial.action === "robber"
            ? board.hexes.find((hex) => hex.id === spatial.recommendation.id)
            : board.vertices.find(
                (vertex) => vertex.id === spatial.recommendation.id,
              );
      if (source?.screen) {
        if (
          spatial.action === "robber" &&
          spatial.recommendation.targetPlayer
        ) {
          this.robberVictimPlan = {
            gameKey: board.gameKey,
            turn: board.turn,
            player: spatial.recommendation.targetPlayer,
          };
        }
        return {
          kind: "board",
          boardAction: spatial.action,
          targetId: spatial.recommendation.id,
          point: source.screen,
          label: `Place ${spatial.action} here`,
          signature: `${signatureBase}|board|${spatial.action}|${spatial.recommendation.id}`,
          confidence: board.legalEdgeIds || board.legalVertexIds ? 0.96 : 0.86,
          ...(spatial.action === "robber" &&
          spatial.recommendation.targetPlayer
            ? { followupPlayer: spatial.recommendation.targetPlayer }
            : {}),
        };
      }
    }

    const deep = this.decisionAnalysis?.deepSearch;
    const deepAction = this.preferredDeepAction(state, report?.player);
    if (board.isMyTurn && deep && deepAction) {
      const confidence = deep.tacticalProven
        ? 0.99
        : Math.min(
            0.97,
            0.72 +
              Math.log10(Math.max(10, deep.iterations)) * 0.07 +
              Math.min(0.08, deep.particles / 600),
          );
      const developmentByAction = {
        "play-knight": "knight",
        "play-monopoly": "monopoly",
        "play-road-building": "road-building",
        "play-year-of-plenty": "year-of-plenty",
      } as const;
      const developmentCard =
        developmentByAction[
          deepAction.kind as keyof typeof developmentByAction
        ];
      if (developmentCard) {
        const followupResources = [
          deepAction.resource,
          deepAction.otherResource,
        ].filter((resource): resource is Resource => Boolean(resource));
        return {
          kind: "development",
          card: developmentCard,
          label: `Play ${developmentCard.replaceAll("-", " ")} now`,
          signature: `${signatureBase}|deep-dev|${developmentCard}`,
          confidence,
          ...(followupResources.length ? { followupResources } : {}),
        };
      }
      const buildByAction = {
        "build-road": "road",
        "build-settlement": "settlement",
        "build-city": "city",
        "buy-development": "development",
      } as const;
      const build =
        buildByAction[deepAction.kind as keyof typeof buildByAction];
      if (build && board.action === "none") {
        return {
          kind: "build",
          build,
          label:
            build === "development"
              ? "Buy a development card"
              : `Choose build ${build}`,
          signature: `${signatureBase}|deep-build|${build}`,
          confidence,
        };
      }
      if (deepAction.kind === "roll") {
        return {
          kind: "turn-control",
          control: "roll",
          label: "Roll dice",
          signature: `${signatureBase}|deep-roll`,
          confidence: 1,
        };
      }
      if (deepAction.kind === "end-turn") {
        return {
          kind: "turn-control",
          control: "end",
          label: "End turn",
          signature: `${signatureBase}|deep-end`,
          confidence,
        };
      }
      if (deepAction.kind === "offer-trade") {
        if (this.settings.disablePlayerTrades) {
          this.decisionTraces.mappingFailure(
            decisionStateDigest(this.decisionKey),
            "rust-offer-trade-returned-while-player-trades-disabled",
          );
          return undefined;
        }
        if (!deepAction.cards || !deepAction.receiveCards) {
          this.decisionTraces.mappingFailure(
            decisionStateDigest(this.decisionKey),
            "rust-offer-trade-missing-card-bundle",
          );
          return undefined;
        }
        return {
          kind: "trade-builder",
          mode: "player",
          give: tupleResources(deepAction.cards),
          receive: tupleResources(deepAction.receiveCards),
          recipients: deepAction.recipients,
          label: "Open recommended player trade",
          signature: `${signatureBase}|deep-offer-trade|${deepAction.cards.join(",")}|${deepAction.receiveCards.join(",")}`,
          confidence,
        };
      }
      if (
        deepAction.kind === "maritime-trade" &&
        deepAction.resource &&
        deepAction.otherResource
      ) {
        const give = emptyResources();
        give[deepAction.resource] = Math.max(2, deepAction.ratio ?? 4);
        const receive = emptyResources();
        receive[deepAction.otherResource] = 1;
        return {
          kind: "trade-builder",
          mode: "bank",
          give,
          receive,
          label: "Open recommended bank trade",
          signature: `${signatureBase}|deep-bank-trade|${deepAction.resource}|${deepAction.otherResource}|${deepAction.ratio ?? 4}`,
          confidence,
        };
      }
      // The selected action could not be mapped to a validated live control.
      // The changed board signature must be searched again; no legacy policy
      // is allowed to silently choose a different action.
      this.decisionTraces.mappingFailure(
        decisionStateDigest(this.decisionKey),
        "rust-action-could-not-map-to-live-control",
      );
      return undefined;
    }
    if (board.isMyTurn && deep) {
      return undefined;
    }

    const development = report?.developmentTiming?.primary;
    if (board.isMyTurn && development) {
      return {
        kind: "development",
        card: development.card,
        label: development.title,
        signature: `${signatureBase}|dev|${development.card}`,
        confidence: Math.min(0.98, 0.84 + Math.max(0, development.score) / 220),
        ...(
          development.resources
            ? { followupResources: development.resources }
            : development.resource
              ? { followupResources: [development.resource] }
              : {}
        ),
      };
    }

    const turnControl = board.isMyTurn ? visibleTurnControl() : undefined;
    // Colonist keeps the dice control mounted while modal board protocols
    // such as robber placement are active. If the target coordinate is
    // temporarily unavailable, wait for the next board snapshot instead of
    // clicking a stale roll control and skipping the mandatory action.
    if (turnControl === "roll" && (board.action ?? "none") === "none") {
      return {
        kind: "turn-control",
        control: "roll",
        label: "Roll dice",
        signature: `${signatureBase}|roll`,
        confidence: 1,
      };
    }

    if (spatial?.proactive) {
      return {
        kind: "build",
        build: spatial.action as Exclude<BuildKind, "development">,
        label: `Choose build ${spatial.action}`,
        signature: `${signatureBase}|build|${spatial.action}|${spatial.recommendation.id}`,
        confidence: 0.91,
      };
    }
    if (
      board.isMyTurn &&
      report?.primary.affordableProbability === 1
    ) {
      return {
        kind: "build",
        build: report.primary.kind,
        label:
          report.primary.kind === "development"
            ? "Buy development card"
            : `Choose build ${report.primary.kind}`,
        signature: `${signatureBase}|build|${report.primary.kind}`,
        confidence: 0.86,
      };
    }
    // Colonist can leave the end-turn control visible while a mandatory
    // robber, discard, or placement prompt is still active. Never let that
    // stale control outrank the protocol action.
    if (turnControl === "end" && (board.action ?? "none") === "none") {
      return {
        kind: "turn-control",
        control: "end",
        label: "End turn",
        signature: `${signatureBase}|end`,
        confidence: 0.86,
      };
    }
    return undefined;
  }

  private renderBoardMarker(
    action: Exclude<BoardAction, "none" | "discard">,
    recommendation: PlacementRecommendation,
  ): string {
    const source =
      action === "road"
        ? this.board?.edges.find((edge) => edge.id === recommendation.id)
        : action === "robber"
          ? this.board?.hexes.find((hex) => hex.id === recommendation.id)
          : this.board?.vertices.find((vertex) => vertex.id === recommendation.id);
    if (!source?.screen) return "";
    const panel = this.host.getBoundingClientRect();
    const panelOverlap =
      panel.width > 0 &&
      source.screen.x >= panel.left - 56 &&
      source.screen.x <= panel.right + 56 &&
      source.screen.y >= panel.top - 56 &&
      source.screen.y <= panel.bottom + 56;
    const edgeClasses = [
      source.screen.x < 90 ? "near-left" : "",
      source.screen.x > window.innerWidth - 90 ? "near-right" : "",
      source.screen.y > window.innerHeight - 90 ? "near-bottom" : "",
      panelOverlap ? "panel-overlap" : "",
    ]
      .filter(Boolean)
      .join(" ");
    return `<div class="board-marker ${edgeClasses}" style="left:${source.screen.x}px;top:${source.screen.y}px" aria-hidden="true">
      <i></i><b>1</b><span><em>${this.pieceArt(action)}</em>BEST ${action.toUpperCase()}</span>
    </div>`;
  }

  private renderAdvice(
    state?: TrackerState,
    spatial?: ReturnType<AssistantOverlay["spatialRecommendation"]>,
    preparedReport?: CoachReport,
    next?: NextClick,
  ): string {
    if (this.board?.gameOver) {
      const winner = this.board.winner;
      const user = this.board.myPlayer ?? this.userPlayer(state);
      const won = Boolean(winner && user && winner === user);
      return `<section class="empty terminal-state">
        <span class="empty-mark">${assistantMark()}</span>
        <h1>${won ? "You won" : "Game complete"}</h1>
        <p>${winner ? `${escapeHtml(winner)} won this game.` : "Colonist has ended this game."} The assistant is stopped and will not issue or execute any more actions.</p>
      </section>`;
    }
    if (
      placementIsAwaitingSync(this.pendingPlacement, this.board)
    ) {
      return this.renderPlacementSync(this.pendingPlacement);
    }
    if (
      (this.decisionRuntimeError || this.decisionContextInvalidated) &&
      isWasmDecisionEngine(this.settings.engine)
    ) {
      const reloadRequired = this.decisionContextInvalidated;
      const detail = reloadRequired
        ? EXTENSION_CONTEXT_RELOAD_MESSAGE
        : this.decisionRuntimeError;
      return `<section class="decision pending-decision" aria-live="assertive">
        <div class="decision-meta"><span>STRATEGIST PAUSED</span><span>NO FALLBACK</span></div>
        <div class="decision-command">
          <span class="command-art">${assistantMark()}</span>
          <h1>${reloadRequired ? "Reload this Colonist tab" : "Waiting for a valid engine result"}</h1>
        </div>
        <p class="why">${escapeHtml(detail)}. No heuristic recommendation or autonomous action can replace the failed decision.</p>
        ${reloadRequired
          ? `<div class="board-confirm pending"><i></i><span>Reload the tab to reconnect this content script</span></div>`
          : `<div class="board-confirm pending"><i></i><span>The selected engine is paused after a transient failure</span></div><button class="reset-link" type="button" data-action="retry-engine">Retry selected engine</button>`}
      </section>`;
    }
    const discard = this.discardRecommendation(state);
    if (discard) return this.renderDiscardAdvice(discard);
    if (next?.kind === "trade") {
      return this.renderIncomingTradeAdvice(next);
    }
    if (next?.kind === "trade-cancel") {
      return this.renderTradeCancelAdvice(next);
    }
    if (next?.kind === "player") {
      return this.renderRobberVictimAdvice(next);
    }
    if (next?.kind === "turn-control") {
      return this.renderTurnControlAdvice(next);
    }
    if (
      this.board &&
      !this.board.isMyTurn &&
      !unansweredIncomingTrades(
        this.board.activeTrades,
        this.completedIncomingTradeIds,
      ).length &&
      !this.decisionPendingKey
    ) {
      return `<section class="empty compact-empty" aria-live="polite">
        <span class="empty-mark">${assistantMark()}</span>
        <h1>Waiting for ${escapeHtml(this.board.currentPlayer ?? "the next player")}</h1>
        <p>Strategist will evaluate the authoritative board when your turn or a trade response begins.</p>
      </section>`;
    }
    if (
      !next &&
      this.decisionPendingKey &&
      isWasmDecisionEngine(this.settings.engine)
    ) {
      const mandatory = Boolean(
        this.board?.action === "discard" ||
        this.board?.action === "robber" ||
        unansweredIncomingTrades(
          this.board?.activeTrades,
          this.completedIncomingTradeIds,
        ).length,
      );
      return `<section class="decision pending-decision" aria-live="polite">
        <div class="decision-meta"><span>${mandatory ? "EXACT DECISION" : this.board?.isMyTurn ? "PLANNING THIS TURN" : "PONDERING"}</span><span>STATE LOCKED</span></div>
        <div class="decision-command">
          <span class="command-art">${assistantMark()}</span>
          <h1>${mandatory ? "Comparing every legal choice" : this.board?.isMyTurn ? "Calculating the next action" : "Preparing your next turn"}</h1>
        </div>
        <p class="why">${mandatory ? "Autopilot is paused for this state until the exact mandatory solver returns" : "No fallback click can override the authoritative search for this board signature"}.</p>
        <div class="board-confirm pending"><i></i><span>${mandatory ? "A result should appear immediately after board synchronization" : "The best complete turn plan will replace this state"}</span></div>
      </section>`;
    }
    const pendingOutgoing = this.board?.activeTrades?.find(
      (trade) =>
        !trade.incoming &&
        !trade.acceptedPlayers?.length &&
        !trade.responsesComplete &&
        outgoingTradeDisposition(
          false,
          this.outgoingTradeSeenAt.get(trade.id) ?? Date.now(),
          Date.now(),
        ) === "wait",
    );
    if (!next && pendingOutgoing) {
      return this.renderOutgoingTradeWait(pendingOutgoing);
    }
    if (
      spatial &&
      (
        !next ||
        next.kind === "board" ||
        (
          next.kind === "build" &&
          next.build !== "development" &&
          next.build === spatial.action
        )
      )
    ) {
      return this.renderSpatialAdvice(spatial);
    }
    if (!state?.playerOrder.length) {
      return `<section class="empty">
        <span class="empty-mark">${assistantMark()}</span>
        <h1>Waiting for your game</h1>
        <p>Start, join, or replay a Colonist game. Your next move and live board marker will appear here.</p>
      </section>`;
    }
    const user = this.userPlayer(state);
    if (!user) {
      return `<section class="empty compact-empty">
        <span class="empty-mark">${assistantMark()}</span>
        <h1>Resolving your seat</h1>
        <p>Advice will start when Colonist exposes your current player identity. Card evidence can still be viewed safely.</p>
      </section>`;
    }
    const report =
      preparedReport ??
      this.coachReport(state, user);
    if (!report) return "";
    const deepDevelopment = this.preferredDeepAction(state, report?.player);
    if (
      this.board?.isMyTurn &&
      next?.kind === "trade-builder" &&
      (
        (
          deepDevelopment?.kind === "offer-trade" &&
          deepDevelopment.cards &&
          deepDevelopment.receiveCards
        ) ||
        (
          deepDevelopment?.kind === "maritime-trade" &&
          deepDevelopment.resource &&
          deepDevelopment.otherResource
        )
      )
    ) {
      return this.renderDeepTradeAdvice(deepDevelopment);
    }
    if (
      this.board?.isMyTurn &&
      next?.kind === "development" &&
      deepDevelopment?.kind.startsWith("play-")
    ) {
      return this.renderDeepDevelopmentAdvice(deepDevelopment);
    }
    if (
      this.board?.isMyTurn &&
      next?.kind === "development" &&
      report.developmentTiming?.primary
    ) {
      return this.renderDevelopmentAdvice(report);
    }
    return this.renderBuildAdvice(
      state,
      report,
      next?.kind === "build" ? next.build : undefined,
    );
  }

  private renderTurnControlAdvice(
    next: Extract<NextClick, { kind: "turn-control" }>,
  ): string {
    const roll = next.control === "roll";
    const icon = roll
      ? '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3.5" y="3.5" width="17" height="17" rx="2" fill="currentColor"/><circle cx="8" cy="8" r="1.35" fill="#0d1821"/><circle cx="16" cy="8" r="1.35" fill="#0d1821"/><circle cx="12" cy="12" r="1.35" fill="#0d1821"/><circle cx="8" cy="16" r="1.35" fill="#0d1821"/><circle cx="16" cy="16" r="1.35" fill="#0d1821"/></svg>'
      : '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 4h3v16H5zM10 5l9 7-9 7z" fill="currentColor"/></svg>';
    return `<section class="decision control-decision" aria-live="polite">
      <div class="decision-meta"><span>${roll ? "START YOUR TURN" : "TURN COMPLETE"}</span><span>EXACT NEXT CLICK</span></div>
      <div class="decision-command">
        <span class="command-art">${icon}</span>
        <h1>${roll ? "Roll the dice" : "End your turn"}</h1>
      </div>
      <p class="why">${roll ? "Roll before the engine evaluates spend, trade, and development-card lines from the resulting hand" : "No remaining legal conversion beats passing; the highlighted control ends this turn"}.</p>
      <div class="board-confirm"><i></i><span>${roll ? "The active dice face is highlighted" : "The pass-turn button is highlighted"}</span></div>
    </section>`;
  }

  private renderIncomingTradeAdvice(
    next: Extract<NextClick, { kind: "trade" }>,
  ): string {
    const trade = this.board?.activeTrades?.find(
      (candidate) => candidate.id === next.tradeId,
    );
    const creator = trade?.creator ?? "this player";
    const title =
      next.verdict === "accept"
        ? "Accept this offer"
        : next.verdict === "counter"
          ? "Send a counteroffer"
          : "Decline this offer";
    const detail =
      next.verdict === "accept"
        ? "The received value and tempo beat the opponent benefit in the current race."
        : next.verdict === "counter"
          ? "The original offer is close, but the highlighted counter sequence improves your conversion."
          : "The offer helps the opponent more than it advances your best reachable build.";
    return `<section class="decision trade-decision" aria-live="polite">
      <div class="decision-meta"><span>INCOMING TRADE</span><span>${next.verdict.toUpperCase()}</span></div>
      <div class="decision-command">
        <span class="command-art">${this.pieceArt("development")}</span>
        <h1>${title}</h1>
      </div>
      <div class="single-tactic"><span>FROM</span><strong>${escapeHtml(creator)}</strong></div>
      <p class="why">${detail}</p>
      <div class="board-confirm"><i></i><span>The exact ${next.verdict} control is highlighted on the offer</span></div>
    </section>`;
  }

  private renderTradeCancelAdvice(
    next: Extract<NextClick, { kind: "trade-cancel" }>,
  ): string {
    return `<section class="decision trade-decision" aria-live="polite">
      <div class="decision-meta"><span>OUTGOING TRADE</span><span>RECOVERY</span></div>
      <div class="decision-command">
        <span class="command-art">${this.pieceArt("development")}</span>
        <h1>${escapeHtml(next.label)}</h1>
      </div>
      <p class="why">This offer has no useful live response. Closing it releases Colonist's trade state; the same bundle stays blocked for this turn.</p>
      <div class="board-confirm"><i></i><span>The offer's cancel control is highlighted</span></div>
    </section>`;
  }

  private renderOutgoingTradeWait(
    trade: NonNullable<BoardSnapshot["activeTrades"]>[number],
  ): string {
    const pending = trade.pendingPlayers?.length ?? 0;
    return `<section class="decision trade-decision" aria-live="polite">
      <div class="decision-meta"><span>OUTGOING TRADE</span><span>WAITING</span></div>
      <div class="decision-command">
        <span class="command-art">${this.pieceArt("development")}</span>
        <h1>Waiting for ${pending === 1 ? "1 response" : `${pending || "the remaining"} responses`}</h1>
      </div>
      <p class="why">The counteroffer was sent successfully. No duplicate search or click will run while Colonist collects the replies.</p>
      <div class="board-confirm pending"><i></i><span>An acceptance will be confirmed immediately; a rejected or unanswered offer will close automatically</span></div>
    </section>`;
  }

  private renderRobberVictimAdvice(
    next: Extract<NextClick, { kind: "player" }>,
  ): string {
    return `<section class="decision control-decision" aria-live="polite">
      <div class="decision-meta"><span>ROBBER TARGET</span><span>EXACT NEXT CLICK</span></div>
      <div class="decision-command">
        <span class="command-art">${this.pieceArt("robber")}</span>
        <h1>Steal from ${escapeHtml(next.player)}</h1>
      </div>
      <p class="why">This victim best combines current win threat, steal value, and disruption of the strongest reachable build line.</p>
      <div class="board-confirm"><i></i><span>Select the highlighted player, then confirm the victim</span></div>
    </section>`;
  }

  private renderPlacementSync(
    pending: PendingBoardPlacement,
  ): string {
    const label =
      pending.action === "robber"
        ? "Robber move"
        : `${pending.action[0]!.toUpperCase()}${pending.action.slice(1)} placement`;
    return `<section class="decision sync-decision" aria-live="polite">
      <div class="decision-meta"><span>PLACEMENT SENT</span><span>SYNCING BOARD</span></div>
      <div class="decision-command">
        <span class="command-art">${this.pieceArt(pending.action)}</span>
        <h1>${escapeHtml(label)} received</h1>
      </div>
      <p class="why">The previous marker is cleared. Waiting for Colonist to expose the next legal action before calculating another target.</p>
      <div class="sync-status"><i></i><strong>Reading the updated board…</strong></div>
    </section>`;
  }

  private renderDeepTradeAdvice(
    action: NonNullable<
      NonNullable<DecisionAnalysis["deepSearch"]>["chosen"]
    >,
  ): string {
    const row = (cards: ResourceVector): string =>
      RESOURCE_ORDER.filter((resource) => cards[resource] > 0)
        .map(
          (resource) =>
            `<span class="trade-resource" title="${RESOURCE_LABELS[resource]}">
              ${this.resourceArt(resource)}
              <b>${cards[resource]}×</b>
              <small>${RESOURCE_LABELS[resource]}</small>
            </span>`,
        )
        .join("");
    const maritime = action.kind === "maritime-trade";
    const give = maritime
      ? (() => {
          const cards = emptyResources();
          if (action.resource) {
            cards[action.resource] = Math.max(2, action.ratio ?? 4);
          }
          return cards;
        })()
      : tupleResources(action.cards);
    const receive = maritime
      ? (() => {
          const cards = emptyResources();
          if (action.otherResource) cards[action.otherResource] = 1;
          return cards;
        })()
      : tupleResources(action.receiveCards);
    const recipients = maritime
      ? `Use your ${action.ratio ?? 4}:1 bank or port rate`
      : action.recipients?.length
        ? `Send to ${action.recipients.join(", ")}`
        : "Send to the table";
    const firstGive = RESOURCE_ORDER.find((resource) => give[resource] > 0);
    return `<section class="decision trade-decision" aria-live="polite">
      <div class="decision-meta"><span>DEEP SEARCH · ${maritime ? "BANK TRADE" : "PLAYER TRADE"}</span><span>NEXT SEQUENCE</span></div>
      <div class="decision-command">
        <span class="command-art">${firstGive ? this.resourceArt(firstGive) : this.pieceArt("development")}</span>
        <h1>${maritime ? "Trade with the bank" : "Send this offer"}</h1>
      </div>
      <div class="trade-flow">
        <div><em>YOU GIVE</em><span>${row(give)}</span></div>
        <i aria-hidden="true">→</i>
        <div><em>YOU GET</em><span>${row(receive)}</span></div>
      </div>
      <div class="trade-next"><span>${maritime ? "BANK / PORT" : "RECIPIENTS"}</span><strong>${escapeHtml(recipients)}</strong></div>
      <p class="why">Every click in this sequence is highlighted. The search chose it only because the modeled conversion beats building, another trade, or ending the turn.</p>
      <details class="more">
        <summary>Search evidence</summary>
        <p>${escapeHtml(this.decisionAnalysis?.model ?? "")}.</p>
      </details>
    </section>`;
  }

  private renderDeepDevelopmentAdvice(
    action: NonNullable<
      NonNullable<DecisionAnalysis["deepSearch"]>["chosen"]
    >,
  ): string {
    const cards = {
      "play-knight": {
        label: "Knight",
        art: "knight" as const,
        instruction: action.player
          ? `Move the robber, then steal from ${action.player}`
          : "Move the robber to the marked denial target",
      },
      "play-monopoly": {
        label: "Monopoly",
        art: "monopoly" as const,
        instruction: action.resource
          ? `Call ${RESOURCE_LABELS[action.resource]}`
          : "Call the modeled highest-value resource",
      },
      "play-road-building": {
        label: "Road Building",
        art: "roadBuilding" as const,
        instruction: "Place both roads on one coherent route",
      },
      "play-year-of-plenty": {
        label: "Year of Plenty",
        art: "yearOfPlenty" as const,
        instruction:
          action.resource && action.otherResource
            ? `Take ${RESOURCE_LABELS[action.resource]} + ${RESOURCE_LABELS[action.otherResource]}`
            : "Take the two cards that complete the modeled conversion",
      },
    };
    const card = cards[action.kind as keyof typeof cards];
    if (!card) return "";
    const search = this.decisionAnalysis?.deepSearch;
    return `<section class="decision development-decision" aria-live="polite">
      <div class="decision-meta"><span>DEEP SEARCH · PLAY NOW</span><span>${escapeHtml(search?.algorithm.toUpperCase() ?? "SEARCH")}</span></div>
      <div class="decision-command">
        <span class="command-art">${this.pieceArt(card.art)}</span>
        <h1>Play ${card.label} now</h1>
      </div>
      <div class="single-tactic"><span>NEXT</span><strong>${escapeHtml(card.instruction)}</strong></div>
      <p class="why">This line has the best modeled continuation from your exact hand and the current opponent-card belief set.</p>
      <details class="more">
        <summary>Search evidence</summary>
        <p>${escapeHtml(this.decisionAnalysis?.model ?? "")}.</p>
        ${search?.tacticalProven ? "<p>The current-turn solver proved this tactical line.</p>" : ""}
      </details>
    </section>`;
  }

  private renderDevelopmentAdvice(report: CoachReport): string {
    const recommendation = report.developmentTiming!.primary!;
    const artByCard: Record<typeof recommendation.card, PieceAsset> = {
      knight: "knight",
      monopoly: "monopoly",
      "road-building": "roadBuilding",
      "year-of-plenty": "yearOfPlenty",
      "victory-point": "victoryPoint",
    };
    const instruction = recommendation.resources
      ? `<div class="single-tactic"><span>TAKE</span><strong>${recommendation.resources
          .map((resource) => RESOURCE_LABELS[resource])
          .join(" + ")}</strong></div>`
      : recommendation.resource
        ? `<div class="single-tactic"><span>CALL</span><strong>${RESOURCE_LABELS[recommendation.resource]}</strong></div>`
        : recommendation.targetPlayer
          ? `<div class="single-tactic"><span>STEAL</span><strong>${escapeHtml(recommendation.targetPlayer)}</strong></div>`
          : recommendation.routeEdgeIds?.length
            ? `<div class="single-tactic"><span>ROUTE</span><strong>Place both free roads on the marked coherent route</strong></div>`
            : "";
    const otherCards = report.developmentTiming!.cards
      .filter((card) => card !== recommendation)
      .map(
        (card) =>
          `<p>${escapeHtml(card.title)} — ${escapeHtml(card.reason)}.</p>`,
      )
      .join("");
    return `<section class="decision development-decision" aria-live="polite">
      <div class="decision-meta"><span>DEVELOPMENT CARD · PLAY NOW</span><span>ONE CARD THIS TURN</span></div>
      <div class="decision-command">
        <span class="command-art">${this.pieceArt(artByCard[recommendation.card])}</span>
        <h1>${escapeHtml(recommendation.title)}</h1>
      </div>
      <p class="why">${escapeHtml(recommendation.reason)}. ${escapeHtml(recommendation.detail)}</p>
      ${instruction}
      <details class="more">
        <summary>Timing and held cards</summary>
        <p>${escapeHtml(recommendation.detail)}</p>
        ${otherCards}
      </details>
    </section>`;
  }

  private renderSpatialAdvice(
    spatial: NonNullable<ReturnType<AssistantOverlay["spatialRecommendation"]>>,
  ): string {
    const labels: Record<Exclude<BoardAction, "none" | "discard">, [string, string]> = {
      settlement: ["PLACE YOUR SETTLEMENT", "Build here"],
      city: ["PLACE YOUR CITY", "Upgrade here"],
      road: ["PLACE YOUR ROAD", "Build this road"],
      robber: ["MOVE THE ROBBER", "Block this hex"],
    };
    const [activeKicker, activeTitle] = labels[spatial.action];
    const kicker = spatial.proactive
      ? `READY TO BUILD · ${activeKicker.replace("PLACE YOUR ", "")}`
      : activeKicker;
    const title = spatial.proactive
      ? spatial.action === "city"
        ? "Upgrade here now"
        : spatial.action === "robber"
          ? "Move the robber here"
          : `Build a ${spatial.action} here`
      : activeTitle;
    const target = spatial.recommendation.targetPlayer
      ? `<div class="single-tactic"><span>STEAL</span><strong>${escapeHtml(spatial.recommendation.targetPlayer)}</strong></div>`
      : "";
    const metricLine = spatial.recommendation.metrics
      ? [
          spatial.recommendation.metrics.rawPips !== undefined
            ? `${spatial.recommendation.metrics.rawPips} raw pips`
            : "",
          spatial.recommendation.metrics.weightedPips !== undefined
            ? `${spatial.recommendation.metrics.weightedPips} weighted pips`
            : "",
          spatial.recommendation.metrics.strikeWays !== undefined
            ? `${spatial.recommendation.metrics.strikeWays}/36 strike rate`
            : "",
          spatial.recommendation.metrics.roadsRequired !== undefined
            ? `${spatial.recommendation.metrics.roadsRequired} road${spatial.recommendation.metrics.roadsRequired === 1 ? "" : "s"} to target`
            : "",
        ]
          .filter(Boolean)
          .join(" · ")
      : "";
    const alternatives = `<details class="more">
      <summary>Why this target wins</summary>
      ${this.decisionAnalysis?.deepSearch ? `<p>${escapeHtml(this.decisionAnalysis.model)}.</p>` : ""}
      ${metricLine ? `<p>${escapeHtml(metricLine)}.</p>` : ""}
      ${spatial.recommendation.reasons
        .slice(2)
        .map((reason) => `<p>${escapeHtml(reason)}.</p>`)
        .join("")}
      ${spatial.alternatives
        .map(
          (item, index) => `<div class="alternative">
            <b>${index + 2}</b>
            <span><strong>${escapeHtml(item.label)}</strong><small>${escapeHtml(item.reasons[0] ?? "")}</small></span>
          </div>`,
        )
        .join("")}
    </details>`;
    return `<section class="decision spatial-decision">
      <div class="decision-meta"><span>${kicker}</span><span>LIVE BOARD</span></div>
      <div class="decision-command">
        <span class="command-art">${this.pieceArt(spatial.action)}</span>
        <h1>${title}</h1>
      </div>
      <h2>${escapeHtml(spatial.recommendation.label)}</h2>
      <p class="why">${escapeHtml(spatial.recommendation.reasons[0] ?? "")}. ${escapeHtml(spatial.recommendation.reasons[1] ?? "")}.</p>
      <div class="board-confirm"><i></i><span>Marked directly on the board</span></div>
      ${target}
      ${alternatives}
    </section>`;
  }

  private discardRecommendation(
    state?: TrackerState,
  ): DiscardRecommendation | undefined {
    const board = this.board;
    if (
      !board?.myPlayer ||
      !board.ownHand ||
      !board.isMyTurn ||
      board.action !== "discard" ||
      !board.discardCount
    ) {
      return undefined;
    }
    const deepSearch = this.decisionAnalysis?.deepSearch;
    const deepDiscard = deepSearch?.chosen;
    if (
      deepSearch &&
      deepDiscard?.kind === "discard" &&
      deepDiscard.cards
    ) {
      const discard = Object.fromEntries(
        RESOURCE_ORDER.map((resource, index) => [
          resource,
          deepDiscard.cards?.[index] ?? 0,
        ]),
      ) as ResourceVector;
      if (resourceTotal(discard) === board.discardCount) {
        const keep = Object.fromEntries(
          RESOURCE_ORDER.map((resource) => [
            resource,
            Math.max(0, board.ownHand![resource] - discard[resource]),
          ]),
        ) as ResourceVector;
        return {
          count: board.discardCount,
          discard,
          keep,
          score: deepSearch.tacticalWinProbability,
          reasons: [
            deepSearch.exactDecision
              ? `The exact solver compared every legal discard across ${deepSearch.particles.toLocaleString()} weighted belief ${deepSearch.particles === 1 ? "world" : "worlds"}`
              : `Deep Search compared this discard inside ${deepSearch.rollouts.toLocaleString()} continuation rollouts`,
            "It preserves the strongest legal conversion and win-race branches",
          ],
        };
      }
    }
    if (
      this.decisionPendingKey &&
      isWasmDecisionEngine(this.settings.engine)
    ) {
      return undefined;
    }
    const report = state
      ? this.coachReport(state, board.myPlayer)
      : undefined;
    const profile = playerBoardProfile(board, board.myPlayer);
    return recommendDiscard(board.ownHand, board.discardCount, {
      goal: report?.primary.kind ?? "settlement",
      profile,
    });
  }

  private renderDiscardAdvice(recommendation: DiscardRecommendation): string {
    const cards = RESOURCE_ORDER.filter(
      (resource) => recommendation.discard[resource] > 0,
    )
      .map(
        (resource) => `<span class="discard-card" title="${RESOURCE_LABELS[resource]}">
          <i style="--resource:${RESOURCE_COLORS[resource]}">${this.resourceArt(resource)}</i>
          <b>${recommendation.discard[resource]}× ${RESOURCE_LABELS[resource]}</b>
        </span>`,
      )
      .join("");
    return `<section class="decision discard-decision" aria-live="polite">
      <div class="decision-meta"><span>SEVEN ROLLED</span><span>EXACT HAND</span></div>
      <div class="decision-command">
        <span class="command-art">${this.pieceArt("robber")}</span>
        <h1>Discard these ${recommendation.count}</h1>
      </div>
      <div class="discard-plan" aria-label="Recommended cards to discard">${cards}</div>
      <p class="why">${escapeHtml(recommendation.reasons[0] ?? "")}. ${escapeHtml(recommendation.reasons[1] ?? "")}.</p>
      <details class="more">
        <summary>How this was chosen</summary>
        <p>Compared every legal discard from your current hand.</p>
        <p>Scored build completion, replacement speed, port-ready sets, and remaining options.</p>
      </details>
    </section>`;
  }

  private renderBuildAdvice(
    state: TrackerState,
    report: CoachReport,
    forcedKind?: BuildKind,
  ): string {
    const matching = [report.primary, ...report.alternatives].find(
      (candidate) => candidate.kind === forcedKind,
    );
    const fallbackCost = forcedKind
      ? (BUILD_COSTS[forcedKind] as Partial<ResourceVector>)
      : undefined;
    const fallbackDeficit = Object.fromEntries(
      RESOURCE_ORDER.map((resource) => [
        resource,
        Math.max(
          0,
          (fallbackCost?.[resource] ?? 0) -
            (this.board?.ownHand?.[resource] ?? 0),
        ),
      ]),
    ) as ResourceVector;
    const primary =
      matching ??
      (forcedKind
        ? {
            ...report.primary,
            kind: forcedKind,
            label:
              forcedKind === "development"
                ? "Development Card"
                : `${forcedKind[0]!.toUpperCase()}${forcedKind.slice(1)}`,
            deficit: fallbackDeficit,
            affordableProbability:
              resourceTotal(fallbackDeficit) === 0 ? 1 : 0,
            reasons: [
              `${this.decisionEngineLabel()} selected this action from the current legal position`,
              this.decisionAnalysis?.model ??
                "It is the highest-value legal continuation now",
            ],
          }
        : report.primary);
    const affordable = primary.affordableProbability === 1;
    const title = affordable
      ? `Build a ${primary.label.toLowerCase()} now`
      : `Save for a ${primary.label.toLowerCase()}`;
    const cost = BUILD_COSTS[primary.kind] as Partial<ResourceVector>;
    const resourcePlan = RESOURCE_ORDER.filter((resource) => (cost[resource] ?? 0) > 0)
      .map((resource) => {
        const missing = primary.deficit[resource];
        return `<span class="${missing ? "missing" : "ready"}" title="${RESOURCE_LABELS[resource]}">
          <i style="--resource:${RESOURCE_COLORS[resource]}">${this.resourceArt(resource)}</i>
          <b>${missing ? `Need ${missing}` : "Ready"}</b>
        </span>`;
      })
      .join("");
    const heldDevelopmentCard = report.developmentTiming?.cards.find(
      (card) => !card.useNow && card.score >= 0,
    );
    const tradeVectorLabel = (vector: ResourceVector): string =>
      RESOURCE_ORDER.filter((resource) => vector[resource] > 0)
        .map(
          (resource) =>
            `${vector[resource] > 1 ? `${vector[resource]}× ` : ""}${RESOURCE_LABELS[resource].toLowerCase()}`,
        )
        .join(" + ");
    const tactic = report.alerts[0]
      ? `<div class="single-tactic"><span>WATCH</span><strong>${escapeHtml(report.alerts[0])}</strong></div>`
      : heldDevelopmentCard
        ? `<div class="single-tactic"><span>DEV</span><strong>${escapeHtml(heldDevelopmentCard.title)} — ${escapeHtml(heldDevelopmentCard.reason)}</strong></div>`
      : report.trade
      ? `<div class="single-tactic">
          <span>NEXT</span>
          <strong>Offer ${tradeVectorLabel(report.trade.give)} for ${tradeVectorLabel(report.trade.receive)} to ${escapeHtml(report.trade.partner)} · ${Math.round(report.trade.acceptanceProbability * 100)}% modeled acceptance</strong>
        </div>`
      : primary.reasons[2]
        ? `<div class="single-tactic"><span>WATCH</span><strong>${escapeHtml(primary.reasons[2])}</strong></div>`
        : "";
    return `<section class="decision">
      <div class="decision-meta">
        <span>YOUR NEXT MOVE</span>
        <span>${report.phase.toUpperCase()} · ${report.strategy.replaceAll("-", " ").toUpperCase()}</span>
      </div>
      <div class="decision-command">
        <span class="command-art">${this.pieceArt(primary.kind === "development" ? "development" : primary.kind)}</span>
        <h1>${escapeHtml(title)}</h1>
      </div>
      <p class="why">${escapeHtml(primary.reasons[1] ?? "")}.</p>
      <div class="resource-plan" aria-label="Resources for this goal">${resourcePlan}</div>
      ${tactic}
      <details class="more">
        <summary>Why this recommendation</summary>
        ${this.decisionAnalysis?.deepSearch ? `<p>${escapeHtml(this.decisionAnalysis.model)}.</p>` : ""}
        <p>${primary.confidence}% hand certainty across ${state.worlds.length} legal tracked state${state.worlds.length === 1 ? "" : "s"}.</p>
        ${primary.reasons.map((reason) => `<p>${escapeHtml(reason)}.</p>`).join("")}
        ${report.trade ? `<p>Trade model: ${escapeHtml(report.trade.reason)}.</p>` : ""}
        <p>${report.developmentDeck.remainingCards} development cards remain by public evidence; next-card prior is ${Math.round(report.developmentDeck.next.knight * 100)}% knight and ${Math.round(report.developmentDeck.next.victoryPoint * 100)}% victory point.</p>
        ${report.winWindow ? `<p>${escapeHtml(report.winWindow)}.</p>` : ""}
      </details>
    </section>`;
  }

  private renderCards(
    state?: TrackerState,
    displayedWinAnalysis?: DecisionAnalysis,
  ): string {
    if (!state?.playerOrder.length) {
      return `<section class="empty compact-empty"><h1>No cards tracked yet</h1><p>Public card evidence appears after the first game-log action.</p></section>`;
    }
    const warning =
      this.session?.partialHistory || state.warnings.length
        ? `<div class="notice">${warningIcon()}<span>History is incomplete. Ranges stay conservative.</span></div>`
        : "";
    const user = this.userPlayer(state);
    const headings = RESOURCE_ORDER.map(
      (resource) =>
        `<span class="resource-head" style="--resource:${RESOURCE_COLORS[resource]}" title="${RESOURCE_LABELS[resource]}">${this.resourceArt(resource)}</span>`,
    ).join("");
    const rows = state.playerOrder
      .map((player) =>
        this.renderPlayerRow(
          state,
          player,
          player === user,
          displayedWinAnalysis?.players.find(
            (estimate) => estimate.player === player,
          )?.probability,
        ),
      )
      .join("");
    const bankRow =
      this.board?.bankVisible && this.board.bank
        ? this.renderBankRow(this.board.bank)
        : "";
    const model = displayedWinAnalysis
      ? `<div class="model-strip"><span>${escapeHtml(displayedWinAnalysis.model)}</span><b>${displayedWinAnalysis.simulations ? `${displayedWinAnalysis.simulations} ROLLOUTS` : "DETERMINISTIC"}</b></div>`
      : "";
    return `${warning}${model}
      <header class="cards-heading">
        <span>PUBLIC EVIDENCE</span>
        <h1>Table cards</h1>
        <p>Exact counts or honest minimum–maximum ranges.</p>
      </header>
      <div class="matrix-head"><span>PLAYER</span>${headings}<span>Σ</span></div>
      <section class="player-matrix" aria-label="Tracked player resources">${rows}${bankRow}</section>
      <button class="reset-link" data-action="reset">Reset this session</button>`;
  }

  private renderSettings(): string {
    const { identity: buildIdentity, label: buildLabel, builtAt } =
      this.buildInfo;
    const runtime = this.runtimePresentation();
    return `<section class="settings-panel">
      <header class="settings-heading">
        <span>ASSISTANT SETTINGS</span>
        <h1>How it thinks</h1>
        <p>Changes apply immediately to this game.</p>
      </header>
      <label class="settings-field engine-field">
        <span><b>Decision engine</b><small>Choose full MaxN search or the fast weighted heuristic policy.</small></span>
        <select data-setting="engine" aria-label="Decision engine">
          <option value="deep-search"${this.settings.engine === "deep-search" ? " selected" : ""}>MaxN</option>
          <option value="weighted"${this.settings.engine === "weighted" ? " selected" : ""}>Weighted</option>
        </select>
      </label>
      <div class="runtime-field" data-runtime="${runtime.state}">
        <span><b>Engine runtime</b><small>${escapeHtml(runtime.detail)}</small></span>
        <strong><i></i>${escapeHtml(runtime.label)}</strong>
      </div>
      <label class="settings-field">
        <span><b>Highlight next click</b><small>Circle the exact board location or Colonist control.</small></span>
        <input type="checkbox" data-setting="highlightNextAction"${this.settings.highlightNextAction ? " checked" : ""}>
        <i aria-hidden="true"></i>
      </label>
      <label class="settings-field">
        <span><b>Disable player trades</b><small>Only bank and port trades are allowed.</small></span>
        <input type="checkbox" data-setting="disablePlayerTrades"${this.settings.disablePlayerTrades ? " checked" : ""}>
        <i aria-hidden="true"></i>
      </label>
      <label class="settings-field">
        <span><b>Record game</b><small>Keep a compact LLM-ready timeline with engine reasoning, belief evidence, timing, status, and execution results.</small></span>
        <input type="checkbox" data-setting="recordGame"${this.settings.recordGame ? " checked" : ""}>
        <i aria-hidden="true"></i>
      </label>
      <label class="settings-field">
        <span><b>Autopilot</b><small>Play recommended steps automatically in any Colonist game.</small></span>
        <input type="checkbox" data-setting="autonomousPrivateGames"${this.settings.autonomousPrivateGames ? " checked" : ""}>
        <i aria-hidden="true"></i>
      </label>
      <label class="settings-field">
        <span><b>Autopilot delay</b><small>Wait before each automatic click so play is easier to follow.</small></span>
        <select data-setting="autopilotDelaySeconds" aria-label="Autopilot delay">
          ${AUTOPILOT_DELAY_OPTIONS.map(
            (seconds) =>
              `<option value="${seconds}"${this.settings.autopilotDelaySeconds === seconds ? " selected" : ""}>${seconds === 0 ? "None" : `${seconds} second${seconds === 1 ? "" : "s"}`}</option>`,
          ).join("")}
        </select>
      </label>
      <div class="settings-version">
        <span>INSTALLED BUILD</span>
        <strong title="${escapeHtml(buildIdentity)}">${escapeHtml(buildLabel)}</strong>
      </div>
      ${builtAt ? `<div class="settings-version"><span>BUILT AT</span><strong>${escapeHtml(builtAt)}</strong></div>` : ""}
      <button class="reset-link" data-action="export-record">Export compact LLM record (.txt)</button>
      <button class="reset-link" data-action="reset">Reset this game session</button>
    </section>`;
  }

  private renderPlayerRow(
    state: TrackerState,
    player: string,
    isUser: boolean,
    winProbability?: number,
  ): string {
    const meta = state.players[player]!;
    const estimate = getPlayerEstimate(state, player);
    const resources = RESOURCE_ORDER.map((resource) => {
      const minimum = estimate.minimum[resource];
      const maximum = estimate.maximum[resource];
      const value = formatRange(minimum, maximum, estimate.approximate);
      return `<span class="resource-cell ${minimum === maximum ? "exact" : "range"}" title="${RESOURCE_LABELS[resource]}: ${value}">${value}</span>`;
    }).join("");
    const profile =
      this.board && this.board.vertices.some((vertex) => vertex.building?.player === player)
        ? playerBoardProfile(this.board, player)
        : undefined;
    const path =
      this.board && profile
        ? likelyUpgradePath(this.board, player, estimate.average)
        : undefined;
    const pathLabel = path
      ? `${path.kind === "development" ? "DEV" : path.kind.toUpperCase()} ${path.affordable ? "READY" : "PATH"}`
      : "";
    const bestPort = profile
      ? RESOURCE_ORDER.filter(
          (resource) => profile.tradeRatios[resource] === 2,
        ).sort(
          (left, right) =>
            profile.tradeRatios[left] - profile.tradeRatios[right],
        )[0]
      : undefined;
    const portLabel =
      bestPort && profile
        ? `${profile.tradeRatios[bestPort]}:1 ${RESOURCE_LABELS[bestPort].toUpperCase()}`
        : profile && RESOURCE_ORDER.every((resource) => profile.tradeRatios[resource] <= 3)
          ? "3:1 PORT"
          : "";
    const metaLabel = isUser
      ? this.board?.ownHand
        ? `YOU · EXACT${winProbability !== undefined ? ` · ${Math.round(winProbability * 100)}% WIN` : ""}`
        : `YOU${winProbability !== undefined ? ` · ${Math.round(winProbability * 100)}% WIN` : ""}`
      : [
          winProbability !== undefined
            ? `${Math.round(winProbability * 100)}% WIN`
            : "",
          pathLabel,
          portLabel,
          (this.board?.players?.[player]?.developmentCards ?? meta.devCards.length)
            ? `${this.board?.players?.[player]?.developmentCards ?? meta.devCards.length} DEV`
            : "",
        ]
          .filter(Boolean)
          .join(" · ");
    const awards = [
      this.board?.players?.[player]?.hasLargestArmy
        ? `<i class="award" title="Largest Army">${this.pieceArt("largestArmy")}</i>`
        : "",
      this.board?.players?.[player]?.hasLongestRoad
        ? `<i class="award" title="Longest Road">${this.pieceArt("longestRoad")}</i>`
        : "",
    ]
      .filter(Boolean)
      .join("");
    return `<article class="matrix-row ${isUser ? "is-user" : ""}" style="--player:${safeColor(meta.color)}">
      <span class="player-name"><i class="player-stripe"></i><b>${escapeHtml(player)}${awards ? `<span class="player-awards">${awards}</span>` : ""}</b><small>${escapeHtml(metaLabel)}</small></span>
      ${resources}
      <span class="total-cell">${formatRange(estimate.totalMinimum, estimate.totalMaximum, estimate.approximate)}</span>
    </article>`;
  }

  private renderBankRow(bank: ResourceVector): string {
    const resources = RESOURCE_ORDER.map(
      (resource) =>
        `<span class="resource-cell exact" title="${RESOURCE_LABELS[resource]} in bank: ${bank[resource]}">${bank[resource]}</span>`,
    ).join("");
    return `<article class="matrix-row bank-row">
      <span class="player-name"><b>BANK</b><small>PUBLIC</small></span>
      ${resources}
      <span class="total-cell">${resourceTotal(bank)}</span>
    </article>`;
  }
}
