import type {
  BoardAction,
  BoardPoint,
  KnownDevelopmentCard,
} from "../core/placement";
import type { BuildKind, Resource, ResourceVector } from "../core/resources";
import {
  findLogRoot,
  findMessageElements,
} from "./dom";

export type NextClick =
  | {
      kind: "board";
      boardAction: "road" | "settlement" | "city" | "robber";
      targetId: string;
      point: BoardPoint;
      label: string;
      signature: string;
      confidence: number;
      followupPlayer?: string;
    }
  | {
      kind: "build";
      build: BuildKind;
      label: string;
      signature: string;
      confidence: number;
    }
  | {
      kind: "development";
      card: KnownDevelopmentCard;
      label: string;
      signature: string;
      confidence: number;
      followupResources?: Resource[];
    }
  | {
      kind: "trade";
      offerIndex: number;
      tradeId: string;
      tradeCreator?: string;
      tradeExecutor?: string;
      tradeCreatorGive?: ResourceVector;
      tradeCreatorReceive?: ResourceVector;
      verdict: "accept" | "counter" | "decline";
      label: string;
      signature: string;
      confidence: number;
      counterGive?: ResourceVector;
      counterReceive?: ResourceVector;
      existingGive?: ResourceVector;
      existingReceive?: ResourceVector;
    }
  | {
      kind: "trade-builder";
      mode: "player" | "bank";
      give: ResourceVector;
      receive: ResourceVector;
      recipients?: string[];
      label: string;
      signature: string;
      confidence: number;
    }
  | {
      kind: "trade-partner";
      offerIndex: number;
      tradeId: string;
      tradeCreator?: string;
      tradeExecutor?: string;
      tradeCreatorGive?: ResourceVector;
      tradeCreatorReceive?: ResourceVector;
      acceptedIndex: number;
      player: string;
      label: string;
      signature: string;
      confidence: number;
    }
  | {
      kind: "trade-cancel";
      offerIndex: number;
      tradeId: string;
      tradeCreator?: string;
      tradeExecutor?: string;
      tradeCreatorGive?: ResourceVector;
      tradeCreatorReceive?: ResourceVector;
      exhaustDomesticOffers?: boolean;
      label: string;
      signature: string;
      confidence: number;
    }
  | {
      kind: "discard";
      cards: ResourceVector;
      label: string;
      signature: string;
      confidence: number;
    }
  | {
      kind: "turn-control";
      control: "roll" | "end" | "confirm";
      label: string;
      signature: string;
      confidence: number;
    }
  | {
      kind: "resource";
      resource: Resource;
      label: string;
      signature: string;
      confidence: number;
    }
  | {
      kind: "player";
      player: string;
      label: string;
      signature: string;
      confidence: number;
    };

export interface ActionExecutionDiagnostic {
  actionKind: string;
  tradeId?: string;
  offerIndex?: number;
  visibleTradeCount?: number;
  visibleTradeFingerprints?: string[];
  domesticTradeExhausted?: boolean;
}

export interface ActionGuideOptions {
  highlight: boolean;
  autonomous: boolean;
  /** Pause before the first automatic click for a recommendation. */
  autopilotDelayMs?: number;
  validate?: () => boolean;
  /// Board placement commands can remain legal while the overlay temporarily
  /// renders a pending-search state. Keep their bounded commit retries tied to
  /// the live board phase and legal target, not to the current overlay card.
  validateBoardContinuation?: () => boolean;
  /// Multi-click workflows legitimately change the original action's phase
  /// after their first click (for example, confirming Year of Plenty before
  /// choosing two resources). The owner must validate that the transaction,
  /// turn, and modal workflow still belong to this action without requiring
  /// the original first click to remain legal.
  validateContinuation?: () => boolean;
  /// Confirms the exact expected road/building/robber mutation after a canvas
  /// command stops being legal. Dispatch alone is not a successful commit.
  validateBoardCommit?: () => boolean;
  /// Trade submission is not complete merely because the Send button was
  /// clicked. The owner must observe the resulting outgoing offer or exact
  /// bank-hand transfer before the workflow may report success.
  validateTransactionCommit?: () => boolean;
  /// Ordinary DOM controls can also be swallowed while Colonist replaces its
  /// React tree. When supplied, dispatch is only tentative until the owner
  /// observes the exact state mutation caused by this action.
  validateControlCommit?: () => boolean;
  onExecutionStart?: (result: { signature: string }) => void;
  onExecution?: (result: {
    succeeded: boolean;
    signature: string;
    reason?: string;
    diagnostic?: ActionExecutionDiagnostic;
  }) => void;
}

const ROOT_ID = "colonist-assistant-action-guide";
const FONT_STYLE_ID = "colonist-assistant-document-font";
let lastClickSignature = "";
const followupTimers = new Set<number>();
let pendingAutopilotSignature = "";
let boardFollowupCleanup: (() => void) | undefined;
let workflowSignature = "";
let workflowAction: NextClick | undefined;
let workflowGeneration = 0;
let workflowOptions: ActionGuideOptions | undefined;
let workflowCurrentElement: HTMLElement | undefined;
let currentGuideOptions: ActionGuideOptions | undefined;
let currentGuideAction: NextClick | undefined;
let manualExecutionCleanup: (() => void) | undefined;
let activeBoardFollowupSignature = "";
let tradePreflightSignature = "";
const boardCommandAttempts = new Map<string, number>();
let activeBoardCommand:
  | {
      action: Extract<NextClick, { kind: "board" }>;
      options: ActionGuideOptions;
      attempt: number;
      generation: number;
    }
  | undefined;
let boardCommandGeneration = 0;
const controlResolutionAttempts = new Map<string, number>();
const buildControlCommitAttempts = new Map<string, number>();
const reportedMissingControls = new Set<string>();

const normalized = (value: string): string =>
  value.toLowerCase().replace(/\s+/gu, " ").trim();

const visible = (element: HTMLElement): boolean => {
  const rect = element.getBoundingClientRect();
  const style = getComputedStyle(element);
  return (
    rect.width >= 12 &&
    rect.height >= 12 &&
    rect.bottom > 0 &&
    rect.right > 0 &&
    rect.top < innerHeight &&
    rect.left < innerWidth &&
    style.display !== "none" &&
    style.visibility !== "hidden" &&
    Number(style.opacity) > 0 &&
    !element.hasAttribute("disabled") &&
    element.getAttribute("aria-disabled") !== "true"
  );
};

const CONTROL_SELECTOR =
  "button, [role='button'], input[type='button'], input[type='submit'], [class*='actionButton-'], [class*='tradeButton-'], [class*='confirmButton-']";

const allControls = (): HTMLElement[] =>
  [
    ...document.querySelectorAll<HTMLElement>(CONTROL_SELECTOR),
  ].filter(
    (element) =>
      !element.closest(
        `#${ROOT_ID}, #colonist-assistant-root, [data-colonist-assistant-trade-verdict]`,
      ) && visible(element),
  );

const controlText = (element: HTMLElement): string =>
  normalized(
    [
      element.textContent ?? "",
      element.getAttribute("aria-label") ?? "",
      element.getAttribute("title") ?? "",
      element.querySelector("img")?.getAttribute("alt") ?? "",
      element.querySelector("img")?.getAttribute("src") ?? "",
    ].join(" "),
  );

const findControl = (
  preferred: string[],
  rejected: string[] = [],
  root: ParentNode = document,
): HTMLElement | undefined => {
  const controls = (
    root === document
      ? allControls()
      : [...root.querySelectorAll<HTMLElement>(CONTROL_SELECTOR)].filter(
          visible,
        )
  ).map((element) => {
    const text = controlText(element);
    const hits = preferred.reduce(
      (sum, token) => sum + (text.includes(normalized(token)) ? 1 : 0),
      0,
    );
    const rejects = rejected.reduce(
      (sum, token) => sum + (text.includes(normalized(token)) ? 1 : 0),
      0,
    );
    return {
      element,
      score:
        hits * 18 -
        rejects * 30 -
        Math.min(9, text.length / 45) +
        (preferred.some((token) => text === normalized(token)) ? 12 : 0),
    };
  });
  return controls
    .filter((candidate) => candidate.score > 0)
    .sort((left, right) => right.score - left.score)[0]?.element;
};

const findRollDiceControl = (): HTMLElement | undefined => {
  const rollGroup = document.querySelector<HTMLElement>("#roll-dice-button");
  if (rollGroup) {
    const diceWrappers = [
      ...rollGroup.querySelectorAll<HTMLElement>("[class*='diceWrapper-']"),
    ];
    const dice = diceWrappers.filter(
      (element) =>
        visible(element) &&
        !normalized(element.className).includes("inactive-") &&
        !element.querySelector("[class*='inactive-']"),
    );
    if (dice[0]) return dice[0];
    // Colonist keeps the dice group mounted after a roll while marking only
    // the child images inactive. The visible parent is then a dead target.
    if (
      diceWrappers.length === 0 &&
      visible(rollGroup) &&
      !normalized(rollGroup.className).includes("inactive-")
    ) {
      return rollGroup;
    }
    return undefined;
  }
  return findControl(
    ["roll dice", "roll"],
    ["end turn", "pass turn", "finish turn"],
  );
};

const findEndTurnControl = (): HTMLElement | undefined => {
  const exact = document.querySelector<HTMLElement>(
    "#action-button-pass-turn",
  );
  if (exact && visible(exact)) return exact;
  return findControl(
    ["end turn", "pass turn", "finish turn"],
    ["roll dice", "roll"],
  );
};

export const visibleTurnControl = (): "roll" | "end" | undefined => {
  if (findRollDiceControl()) return "roll";
  if (findEndTurnControl()) return "end";
  return undefined;
};

const buildTokens: Record<BuildKind, string[]> = {
  road: ["build road", "road_blue", "road_red", "road_green", "road_orange"],
  settlement: ["build settlement", "settlement_blue", "settlement_red", "settlement_green"],
  city: ["build city", "city_blue", "city_red", "city_green"],
  development: ["buy development", "development card", "card_devcardback"],
};

const developmentTokens: Record<KnownDevelopmentCard, string[]> = {
  knight: ["card_knight", "play knight", "knight"],
  monopoly: ["card_monopoly", "play monopoly", "monopoly"],
  "road-building": ["card_roadbuilding", "road building"],
  "year-of-plenty": ["card_yearofplenty", "year of plenty"],
  "victory-point": ["card_vp", "victory point"],
};

const resourceAsset: Record<Resource, string> = {
  lumber: "card_lumber",
  brick: "card_brick",
  wool: "card_wool",
  grain: "card_grain",
  ore: "card_ore",
};

const resourceCardEnum: Record<Resource, number> = {
  lumber: 1,
  brick: 2,
  wool: 3,
  grain: 4,
  ore: 5,
};

const nearestClickable = (element: Element | null): HTMLElement | undefined => {
  const clickable = element?.closest<HTMLElement>(
    "button, [role='button'], [tabindex], [class*='actionButton-'], [class*='tradeButton-'], [class*='confirmButton-'], [class*='cardContainer-']",
  );
  return clickable && visible(clickable) ? clickable : undefined;
};

const activeColonistControl = (
  element: HTMLElement | undefined,
): HTMLElement | undefined => {
  if (!element) return undefined;
  const control = element.matches("[class*='actionButton-'], [class*='tradeButton-']")
    ? element
    : element.querySelector<HTMLElement>(
        "[class*='actionButton-'], [class*='tradeButton-']",
      ) ?? element;
  if (
    !control ||
    !visible(control) ||
    control.matches("[disabled], [aria-disabled='true']") ||
    normalized(control.className).includes("disabled") ||
    control.querySelector("[class*='foregroundDisabled-']")
  ) {
    return undefined;
  }
  return control;
};

const findPieceBuildControl = (
  build: "road" | "settlement",
): HTMLElement | undefined => {
  for (const image of document.querySelectorAll<HTMLImageElement>("img[src]")) {
    if (
      image.closest("#colonist-assistant-root") ||
      !new RegExp(`(?:^|/)${build}_[a-z]+[.]`, "iu").test(image.src)
    ) {
      continue;
    }
    const control = activeColonistControl(
      image.closest<HTMLElement>("[class*='actionButton-']") ?? undefined,
    );
    if (control) return control;
  }
  return undefined;
};

const findDevelopmentCard = (
  card: KnownDevelopmentCard,
): HTMLElement | undefined => {
  const control = findControl(developmentTokens[card]);
  if (control) return control;
  for (const image of document.querySelectorAll<HTMLImageElement>("img[src]")) {
    if (
      !image.closest("#colonist-assistant-root") &&
      developmentTokens[card].some((token) =>
        normalized(image.src).includes(normalized(token)),
      )
    ) {
      const clickable = nearestClickable(image);
      if (clickable) return clickable;
    }
  }
  return undefined;
};

const tradeExecutionDiagnostic = (
  action: NextClick,
): ActionExecutionDiagnostic => {
  const diagnostic: ActionExecutionDiagnostic = { actionKind: action.kind };
  if (
    action.kind !== "trade" &&
    action.kind !== "trade-partner" &&
    action.kind !== "trade-cancel"
  ) {
    return diagnostic;
  }
  const nestedOffers = [
    ...document.querySelectorAll<HTMLElement>(
      "[class*='gameTradeOffersWrapper-'] [class*='tradeContainer-']",
    ),
  ].filter(visible);
  const offers = nestedOffers.length
    ? nestedOffers
    : [
        ...document.querySelectorAll<HTMLElement>("[class*='tradeContainer-']"),
      ].filter(visible);
  return {
    actionKind: action.kind,
    tradeId: action.tradeId,
    offerIndex: action.offerIndex,
    visibleTradeCount: offers.length,
    visibleTradeFingerprints: offers.slice(0, 8).map((offer, index) => {
      const text = normalized(offer.textContent ?? "").slice(0, 120);
      return `${index}:${text || "<no-text>"}`;
    }),
  };
};

const visibleTradeContainers = (): HTMLElement[] => {
  const nestedOffers = [
    ...document.querySelectorAll<HTMLElement>(
      "[class*='gameTradeOffersWrapper-'] [class*='tradeContainer-']",
    ),
  ].filter(visible);
  return nestedOffers.length
    ? nestedOffers
    : [
        ...document.querySelectorAll<HTMLElement>(
          "[class*='tradeContainer-']",
        ),
      ].filter(visible);
};

const tradeResourceCount = (
  root: ParentNode,
  resource: Resource,
): number => {
  const counts = [...root.querySelectorAll<HTMLImageElement>("img[src]")]
    .filter((image) => normalized(image.src).includes(resourceAsset[resource]))
    .map((image) => {
      const stack =
        image.closest<HTMLElement>(
          "[class*='cardStackContainer-'], [class*='cardContainer-'], [data-card-enum], button, [role='button']",
        ) ?? image;
      const badge = stack.querySelector<HTMLElement>(
        "[class*='countBadge-'], [class*='cardCount-'], [class*='amount-']",
      );
      const value = Number.parseInt(
        normalized(badge?.textContent ?? "").match(/\d+/u)?.[0] ?? "1",
        10,
      );
      return Number.isFinite(value) ? Math.max(1, value) : 1;
    });
  return counts.length ? Math.max(...counts) : 0;
};

const tradeResourceCounts = (root: ParentNode): Record<Resource, number> =>
  Object.fromEntries(
    (Object.keys(resourceAsset) as Resource[]).map((resource) => [
      resource,
      tradeResourceCount(root, resource),
    ]),
  ) as Record<Resource, number>;

const sameResourceCounts = (
  actual: Record<Resource, number>,
  expected: ResourceVector,
): boolean =>
  (Object.keys(resourceAsset) as Resource[]).every(
    (resource) => actual[resource] === expected[resource],
  );

const tradeContainerResourceMatch = (
  offer: HTMLElement,
  give?: ResourceVector,
  receive?: ResourceVector,
): boolean | undefined => {
  if (!give || !receive) return undefined;
  const offered = offer.querySelector<HTMLElement>(
    "[class*='proposalOfferedHalfContainer-']",
  );
  const wanted = offer.querySelector<HTMLElement>(
    "[class*='proposalWantedHalfContainer-']",
  );
  if (offered && wanted) {
    const offeredCounts = tradeResourceCounts(offered);
    const wantedCounts = tradeResourceCounts(wanted);
    const observed =
      Object.values(offeredCounts).some((count) => count > 0) ||
      Object.values(wantedCounts).some((count) => count > 0);
    if (!observed) return undefined;
    return sameResourceCounts(offeredCounts, give) &&
      sameResourceCounts(wantedCounts, receive);
  }
  const actual = tradeResourceCounts(offer);
  if (Object.values(actual).every((count) => count === 0)) return undefined;
  return (Object.keys(resourceAsset) as Resource[]).every(
    (resource) => actual[resource] === give[resource] + receive[resource],
  );
};

// `offerIndex` remains on NextClick for board/record diagnostics only. Colonist
// does not promise that its store order matches visible DOM container order.
const findTradeContainer = (
  _offerIndex: number,
  tradeCreator?: string,
  tradeExecutor?: string,
  tradeCreatorGive?: ResourceVector,
  tradeCreatorReceive?: ResourceVector,
): HTMLElement | undefined => {
  const offers = visibleTradeContainers();
  let candidates = offers;
  for (const player of [tradeCreator, tradeExecutor]) {
    if (!player) continue;
    const wanted = normalized(player);
    const named = candidates.filter((offer) =>
      normalized(offer.textContent ?? "").includes(wanted),
    );
    if (named.length) candidates = named;
  }
  if (tradeCreatorGive && tradeCreatorReceive) {
    const resourceEvidence = candidates.map((offer) => ({
      offer,
      match: tradeContainerResourceMatch(
        offer,
        tradeCreatorGive,
        tradeCreatorReceive,
      ),
    }));
    const resourceMatches = resourceEvidence
      .filter((candidate) => candidate.match === true)
      .map((candidate) => candidate.offer);
    if (resourceMatches.length) {
      candidates = resourceMatches;
    } else if (resourceEvidence.some((candidate) => candidate.match === false)) {
      candidates = [];
    }
  }
  return candidates.length === 1 ? candidates[0] : undefined;
};

const findTradeControl = (
  offerIndex: number,
  verdict: "accept" | "counter" | "decline",
  tradeCreator?: string,
  tradeExecutor?: string,
  tradeCreatorGive?: ResourceVector,
  tradeCreatorReceive?: ResourceVector,
): HTMLElement | undefined => {
  const offer = findTradeContainer(
    offerIndex,
    tradeCreator,
    tradeExecutor,
    tradeCreatorGive,
    tradeCreatorReceive,
  );
  if (!offer) return undefined;
  if (verdict === "accept") {
    // Open-ended Colonist offers can have no genuine accept control. Never
    // reinterpret an unrelated visible trade button as permission to accept.
    return findControl(["accept", "check", "yes"], ["counter", "reject"], offer);
  }
  if (verdict === "counter") {
    return findControl(["counter", "edit"], ["accept", "reject"], offer);
  }
  return findControl(
    ["decline", "reject", "cancel", "close", "no"],
    ["accept", "counter"],
    offer,
  );
};

const findTradeCancelControl = (
  offerIndex: number,
  tradeCreator?: string,
  tradeExecutor?: string,
  tradeCreatorGive?: ResourceVector,
  tradeCreatorReceive?: ResourceVector,
): HTMLElement | undefined => {
  const offer = findTradeContainer(
    offerIndex,
    tradeCreator,
    tradeExecutor,
    tradeCreatorGive,
    tradeCreatorReceive,
  );
  if (!offer) return undefined;

  const explicit = findControl(
    ["cancel offer", "cancel trade", "close offer", "cancel", "close"],
    ["accept", "counter"],
    offer,
  );
  const activeExplicit = activeColonistControl(explicit);
  if (activeExplicit) return activeExplicit;

  // Completed outgoing offers render one enabled X for cancelling alongside
  // per-player response buttons. Rejected responses also show X icons, but
  // those controls are inert and carry Colonist's nested disabled foreground.
  // Only select an X that resolves to an active trade control; if more than
  // one remains, do not guess which destructive control the site currently
  // means.
  const activeXControls = [
    ...offer.querySelectorAll<HTMLImageElement>("img[src*='icon_x']"),
  ]
    .map((image) =>
      activeColonistControl(
        image.closest<HTMLElement>("[class*='tradeButton-']") ?? undefined,
      ),
    )
    .filter((element): element is HTMLElement => Boolean(element))
    .filter((element, index, all) => all.indexOf(element) === index);
  return activeXControls.length === 1 ? activeXControls[0] : undefined;
};

const findResourceCard = (resource: Resource): HTMLElement | undefined => {
  const images = [...document.querySelectorAll<HTMLImageElement>("img[src]")].filter(
    (image) =>
      !image.closest("#colonist-assistant-root") &&
      normalized(image.src).includes(resourceAsset[resource]) &&
      visible(image),
  );
  return images
    .map((image) => nearestClickable(image))
    .filter((element): element is HTMLElement => Boolean(element))
    .sort((left, right) => {
      const leftRect = left.getBoundingClientRect();
      const rightRect = right.getBoundingClientRect();
      return rightRect.top - leftRect.top || rightRect.left - leftRect.left;
    })[0];
};

const resourceEvidence = (
  element: Element,
  resource: Resource,
): boolean =>
  normalized(
    [
      element.getAttribute("src") ?? "",
      element.getAttribute("aria-label") ?? "",
      element.getAttribute("title") ?? "",
      element.getAttribute("alt") ?? "",
      element.textContent ?? "",
    ].join(" "),
  ).includes(resourceAsset[resource]) ||
  normalized(
    [
      element.getAttribute("aria-label") ?? "",
      element.getAttribute("title") ?? "",
      element.getAttribute("alt") ?? "",
      element.textContent ?? "",
    ].join(" "),
  ).includes(resource);

const findResourceInRoot = (
  root: ParentNode | undefined,
  resource: Resource,
): HTMLElement | undefined => {
  if (!root) return undefined;
  const exact = root.querySelector<HTMLElement>(
    `[data-card-enum="${resourceCardEnum[resource]}"]`,
  );
  if (exact && visible(exact)) return exact;
  const candidates = [
    ...root.querySelectorAll<HTMLElement>(
      "img[src], [aria-label], [title], [role='button'], button",
    ),
  ].filter((element) => resourceEvidence(element, resource));
  for (const candidate of candidates) {
    const clickable = nearestClickable(candidate);
    if (clickable) return clickable;
  }
  return undefined;
};

const findTradePanelControl = (): HTMLElement | undefined =>
  document.querySelector<HTMLElement>("#action-button-trade") ??
  findControl(
    ["make trade", "create trade", "offer trade", "trade"],
    ["accept", "decline", "reject", "history"],
  );

const findTradeResourceChoice = (
  resource: Resource,
  side: "give" | "receive",
): HTMLElement | undefined => {
  const root =
    side === "give"
      ? document.querySelector<HTMLElement>("#player-card-inventory")
      : document.querySelector<HTMLElement>(
          "[class*='wantedCardSelectorContainer-']",
        );
  return findResourceInRoot(root ?? undefined, resource);
};

const findTradeSubmit = (
  mode: "player" | "bank",
): HTMLElement | undefined => {
  const exact = document.querySelector<HTMLElement>(
    mode === "bank"
      ? "#action-button-trade-bank"
      : "#action-button-trade-players",
  );
  if (exact && visible(exact)) return exact;
  return mode === "bank"
    ? findControl(["trade with bank", "bank trade"], ["player"])
    : findControl(["trade with players", "send offer", "offer"], ["bank"]);
};

const findTradePartnerControl = (
  offerIndex: number,
  _acceptedIndex: number,
  player: string,
  tradeCreator?: string,
  tradeExecutor?: string,
  tradeCreatorGive?: ResourceVector,
  tradeCreatorReceive?: ResourceVector,
): HTMLElement | undefined => {
  const offer = findTradeContainer(
    offerIndex,
    tradeCreator,
    tradeExecutor,
    tradeCreatorGive,
    tradeCreatorReceive,
  );
  if (!offer) return undefined;
  // Colonist's player-response controls are divs, so disabled responses do
  // not carry a native `disabled` attribute. The accepted responses are the
  // enabled controls whose icon is the green check. Select among those
  // directly; otherwise the first rejected player's inert X can be chosen.
  const accepted = [
    ...offer.querySelectorAll<HTMLImageElement>("img[src*='icon_check']"),
  ]
    .map((image) =>
      image.closest<HTMLElement>("[class*='tradeButton-']"),
    )
    .filter((element): element is HTMLElement =>
      Boolean(
        element &&
          visible(element) &&
          !/disabled/iu.test(element.outerHTML),
      ),
    );
  const wanted = normalized(player);
  const named = accepted.filter((element) => {
    const participant =
      element.closest<HTMLElement>("[class*='player-'], [class*='response-']") ??
      element.parentElement;
    return normalized(participant?.textContent ?? element.textContent ?? "").includes(wanted);
  });
  if (named.length === 1) return named[0];
  if (accepted.length === 1) return accepted[0];
  return undefined;
};

const modalRoots = (): HTMLElement[] =>
  [
    ...document.querySelectorAll<HTMLElement>(
      "[role='dialog'], [class*='modal-'], [class*='popup-'], [class*='gameActionBox-'], [class*='actionBoxContainer-'], [class*='actionBox-'], [class*='selectResource-'], [class*='chooseResource-'], [class*='selectPlayer-']",
    ),
  ].filter(visible);

const findDiscardRoot = (): HTMLElement | undefined => {
  const actionBoxes = [
    ...document.querySelectorAll<HTMLElement>(
      "[class*='gameActionBox-'], [class*='actionBoxContainer-'], [class*='actionBox-']",
    ),
  ].filter(visible);
  return (
    actionBoxes.find(
      (root) =>
        root.querySelector("[data-card-enum], img[src*='card_']") &&
        (
          normalized(root.textContent ?? "").includes("discard") ||
          root.querySelector("button, [role='button']")
        ),
    ) ??
    modalRoots().find((root) =>
      normalized(root.textContent ?? "").includes("discard"),
    )
  );
};

const findDiscardCard = (resource: Resource): HTMLElement | undefined => {
  const inventory = document.querySelector<HTMLElement>(
    "#player-card-inventory",
  );
  const discardRoot = findDiscardRoot();
  return (
    findResourceInRoot(inventory ?? undefined, resource) ??
    findResourceInRoot(discardRoot, resource) ??
    findResourceCard(resource)
  );
};

const findResourceChoice = (resource: Resource): HTMLElement | undefined => {
  for (const root of modalRoots()) {
    const choice = findResourceInRoot(root, resource);
    if (choice) return choice;
  }
  return undefined;
};

const findPlayerChoice = (player: string): HTMLElement | undefined => {
  const wanted = normalized(player);
  for (const root of modalRoots()) {
    const candidates = [
      ...root.querySelectorAll<HTMLElement>(
        "button, [role='button'], [tabindex], [class*='player']",
      ),
    ].filter(visible);
    const exact = candidates.find(
      (candidate) => normalized(candidate.textContent ?? "") === wanted,
    );
    if (exact) {
      return (
        exact.closest<HTMLElement>("[class*='playerOption-']") ??
        nearestClickable(exact) ??
        exact
      );
    }
    const containing = candidates.find((candidate) =>
      normalized(candidate.textContent ?? "").includes(wanted),
    );
    if (containing) {
      return (
        containing.closest<HTMLElement>("[class*='playerOption-']") ??
        nearestClickable(containing) ??
        containing
      );
    }
  }
  return undefined;
};

const findConfirmationControl = (
  root: ParentNode = document,
): HTMLElement | undefined => {
  const exact = [
    ...root.querySelectorAll<HTMLElement>("[class*='confirmButton-']"),
  ].filter(visible).at(-1);
  return (
    exact ??
    findControl(
      ["confirm", "select", "okay", "continue", "done", "icon_check", "check"],
      ["cancel", "close"],
      root,
    )
  );
};

const resolveElement = (action: NextClick): HTMLElement | undefined => {
  if (action.kind === "build") {
    const exact =
      action.build === "city"
        ? document.querySelector<HTMLElement>("#action-button-build-city")
        : action.build === "development"
          ? document.querySelector<HTMLElement>(
              "#action-button-buy-dev-card",
            )
          : action.build === "road"
            ? document.querySelector<HTMLElement>(
                "#action-button-build-road",
              ) ?? findPieceBuildControl("road")
            : document.querySelector<HTMLElement>(
                "#action-button-build-settlement",
              ) ?? findPieceBuildControl("settlement");
    return (
      activeColonistControl(exact ?? undefined) ??
      findControl(buildTokens[action.build])
    );
  }
  if (action.kind === "development") {
    return findDevelopmentCard(action.card);
  }
  if (action.kind === "trade") {
    return findTradeControl(
      action.offerIndex,
      action.verdict,
      action.tradeCreator,
      action.tradeExecutor,
      action.tradeCreatorGive,
      action.tradeCreatorReceive,
    );
  }
  if (action.kind === "trade-builder") {
    return findTradePanelControl();
  }
  if (action.kind === "trade-partner") {
    return findTradePartnerControl(
      action.offerIndex,
      action.acceptedIndex,
      action.player,
      action.tradeCreator,
      action.tradeExecutor,
      action.tradeCreatorGive,
      action.tradeCreatorReceive,
    );
  }
  if (action.kind === "trade-cancel") {
    return findTradeCancelControl(
      action.offerIndex,
      action.tradeCreator,
      action.tradeExecutor,
      action.tradeCreatorGive,
      action.tradeCreatorReceive,
    );
  }
  if (action.kind === "discard") {
    const resource = (Object.keys(action.cards) as Resource[]).find(
      (candidate) => action.cards[candidate] > 0,
    );
    return resource ? findDiscardCard(resource) : undefined;
  }
  if (action.kind === "turn-control") {
    if (action.control === "roll") return findRollDiceControl();
    if (action.control === "end") return findEndTurnControl();
    return findControl(
      ["confirm", "select", "okay", "continue", "done"],
      ["cancel", "close"],
      modalRoots()[0] ?? document,
    );
  }
  if (action.kind === "resource") {
    return findResourceChoice(action.resource);
  }
  if (action.kind === "player") {
    return findPlayerChoice(action.player);
  }
  return undefined;
};

const ensureRoot = (): HTMLDivElement => {
  if (!document.getElementById(FONT_STYLE_ID)) {
    const style = document.createElement("style");
    style.id = FONT_STYLE_ID;
    style.textContent = `@font-face{font-family:"Colonist Assistant Archivo";src:url("${chrome.runtime.getURL("assets/fonts/ArchivoNarrow-Variable.ttf")}") format("truetype");font-style:normal;font-weight:400 700;font-display:swap}`;
    (document.head ?? document.documentElement).append(style);
  }
  const existing = document.getElementById(ROOT_ID);
  if (existing instanceof HTMLDivElement) return existing;
  const root = document.createElement("div");
  root.id = ROOT_ID;
  root.style.cssText =
    'position:fixed;inset:0;z-index:2147482997;pointer-events:none;font-family:"Colonist Assistant Archivo",ui-sans-serif,system-ui,sans-serif;';
  document.documentElement.append(root);
  return root;
};

const drawHighlight = (
  action: NextClick,
  element?: HTMLElement,
): void => {
  const root = ensureRoot();
  root.replaceChildren();
  const rect =
    action.kind === "board"
      ? {
          left: action.point.x - 25,
          top: action.point.y - 25,
          width: 50,
          height: 50,
          right: action.point.x + 25,
          bottom: action.point.y + 25,
        }
      : element?.getBoundingClientRect();
  if (!rect) return;
  const ring = document.createElement("div");
  ring.style.cssText = [
    "position:fixed",
    `left:${Math.round(rect.left - 5)}px`,
    `top:${Math.round(rect.top - 5)}px`,
    `width:${Math.round(rect.width + 10)}px`,
    `height:${Math.round(rect.height + 10)}px`,
    "border:3px solid #f1c84b",
    "border-radius:12px",
    "box-shadow:0 0 0 2px rgba(13,24,33,.85),0 0 0 7px rgba(241,200,75,.18)",
    "animation:ca-next-click-pulse 1.5s ease-in-out infinite",
  ].join(";");
  const label = document.createElement("span");
  label.textContent = `NEXT · ${action.label}`;
  const labelTop =
    rect.top >= 34 ? rect.top - 30 : Math.min(innerHeight - 25, rect.bottom + 8);
  label.style.cssText = [
    "position:fixed",
    `left:${Math.max(4, Math.min(innerWidth - 190, Math.round(rect.left)))}px`,
    `top:${Math.round(labelTop)}px`,
    "max-width:186px",
    "min-height:22px",
    "padding:4px 8px",
    "background:#f1c84b",
    "color:#0d1821",
    "font-size:10px",
    "font-weight:900",
    "letter-spacing:.055em",
    "line-height:14px",
  ].join(";");
  const style = document.createElement("style");
  style.textContent =
    "@keyframes ca-next-click-pulse{0%,100%{opacity:1;transform:scale(1)}50%{opacity:.58;transform:scale(1.035)}}@media(prefers-reduced-motion:reduce){#colonist-assistant-action-guide div{animation:none!important}}";
  root.append(style, ring, label);
};

const executeBoardAction = (
  action: Extract<NextClick, { kind: "board" }>,
  attempt: number,
): boolean => {
  try {
    window.postMessage(
      {
        source: "colonist-assistant-content",
        type: "execute-board-action",
        action: action.boardAction,
        targetId: action.targetId,
        signature: action.signature,
        attempt,
      },
      window.location.origin,
    );
    return true;
  } catch {
    return false;
  }
};

const requestBoardRefresh = (): void => {
  window.dispatchEvent(
    new CustomEvent("colonist-assistant-board-refresh"),
  );
};

const clearBoardCommand = (signature?: string): void => {
  if (
    signature &&
    activeBoardCommand?.action.signature !== signature
  ) {
    return;
  }
  const activeSignature = activeBoardCommand?.action.signature;
  activeBoardCommand = undefined;
  boardCommandGeneration += 1;
  if (activeSignature) boardCommandAttempts.delete(activeSignature);
  if (!signature || lastClickSignature === signature) {
    lastClickSignature = "";
  }
};

const boardCommandStillLegal = (
  command: NonNullable<typeof activeBoardCommand>,
): boolean => {
  const validate =
    command.options.validateBoardContinuation ??
    command.options.validate;
  return validate ? validate() : true;
};

const scheduleBoardCommandRetry = (
  command: NonNullable<typeof activeBoardCommand>,
): void => {
  later(() => {
    if (
      activeBoardCommand?.generation !== command.generation ||
      activeBoardCommand.action.signature !== command.action.signature
    ) {
      return;
    }
    if (!command.options.autonomous) {
      clearBoardCommand(command.action.signature);
      return;
    }
    if (!boardCommandStillLegal(command)) {
      // A changed phase/target is success only when the authoritative
      // snapshot contains the exact expected mutation.
      const { action, options } = command;
      const committed = options.validateBoardCommit?.();
      clearBoardCommand(action.signature);
      if (committed !== undefined) {
        options.onExecution?.({
          succeeded: committed,
          signature: action.signature,
          ...(!committed
            ? {
                reason:
                  "Board state changed without the expected placement commit",
              }
            : {}),
        });
      }
      return;
    }
    if (command.attempt >= 5) {
      const { action, options } = command;
      clearBoardCommand(action.signature);
      options.onExecution?.({
        succeeded: false,
        signature: action.signature,
        reason:
          "Colonist did not commit board placement after bounded validated retries",
      });
      requestBoardRefresh();
      return;
    }
    command.attempt += 1;
    boardCommandAttempts.set(command.action.signature, command.attempt);
    command.options.onExecutionStart?.({ signature: command.action.signature });
    if (!executeBoardAction(command.action, command.attempt)) {
      const { action, options } = command;
      clearBoardCommand(action.signature);
      options.onExecution?.({
        succeeded: false,
        signature: action.signature,
        reason: "Colonist board command could not be dispatched",
      });
      return;
    }
    scheduleBoardCommandRetry(command);
  }, 1_400);
};

const validatedClick = (
  element: HTMLElement,
  options: ActionGuideOptions,
  signature: string,
  reportExecution = true,
): boolean => {
  if (options.validate && !options.validate()) {
    if (lastClickSignature === signature) lastClickSignature = "";
    options.onExecution?.({
      succeeded: false,
      signature,
      reason: "State signature or legal target set changed before execution",
    });
    return false;
  }
  options.onExecutionStart?.({ signature });
  try {
    element.click();
  } catch {
    if (lastClickSignature === signature) lastClickSignature = "";
    options.onExecution?.({
      succeeded: false,
      signature,
      reason: "Colonist control could not be dispatched",
    });
    return false;
  }
  if (reportExecution) {
    options.onExecution?.({ succeeded: true, signature });
  }
  return true;
};

const awaitControlCommit = (
  action: NextClick,
  options: ActionGuideOptions,
  attempt = 0,
): void => {
  const committed = options.validateControlCommit;
  if (!committed) return;
  later(() => {
    if (committed()) {
      options.onExecution?.({
        succeeded: true,
        signature: action.signature,
      });
      requestBoardRefresh();
      return;
    }
    const stillCurrent = currentGuideAction?.signature === action.signature;
    const stillLegal = options.validate ? options.validate() : true;
    if (!stillCurrent || !stillLegal) {
      if (lastClickSignature === action.signature) lastClickSignature = "";
      options.onExecution?.({
        succeeded: false,
        signature: action.signature,
        reason: "Colonist state changed without the expected control commit",
      });
      requestBoardRefresh();
      return;
    }
    if (attempt >= 23) {
      if (lastClickSignature === action.signature) lastClickSignature = "";
      options.onExecution?.({
        succeeded: false,
        signature: action.signature,
        reason: "Colonist did not commit the recommended control",
      });
      requestBoardRefresh();
      return;
    }
    requestBoardRefresh();
    awaitControlCommit(action, options, attempt + 1);
  }, 140);
};

const installManualExecutionObserver = (
  action: NextClick,
  element: HTMLElement | undefined,
  options: ActionGuideOptions,
): void => {
  manualExecutionCleanup?.();
  manualExecutionCleanup = undefined;
  if (options.autonomous || !element || action.kind === "board") return;

  const handler = () => {
    manualExecutionCleanup = undefined;
    if (options.validate && !options.validate()) {
      options.onExecution?.({
        succeeded: false,
        signature: action.signature,
        reason: "State signature or legal target set changed before execution",
      });
      requestBoardRefresh();
      return;
    }
    lastClickSignature = action.signature;
    options.onExecutionStart?.({ signature: action.signature });
    options.onExecution?.({
      succeeded: true,
      signature: action.signature,
    });
    requestBoardRefresh();
  };
  element.addEventListener("click", handler, { once: true });
  manualExecutionCleanup = () => element.removeEventListener("click", handler);
};

const maybeAutoclick = (
  action: NextClick,
  element: HTMLElement | undefined,
  options: ActionGuideOptions,
): void => {
  if (
    !options.autonomous ||
    action.kind === "discard" ||
    action.kind === "trade-builder" ||
    (action.kind === "trade" && action.verdict === "counter")
  ) {
    return;
  }
  if (
    action.signature === lastClickSignature ||
    action.signature === pendingAutopilotSignature
  ) {
    return;
  }
  const delayMs = Math.max(0, options.autopilotDelayMs ?? 0);
  if (delayMs > 0) {
    pendingAutopilotSignature = action.signature;
    later(() => {
      if (pendingAutopilotSignature !== action.signature) return;
      pendingAutopilotSignature = "";
      const activeOptions = currentGuideOptions;
      if (
        !activeOptions?.autonomous ||
        currentGuideAction?.signature !== action.signature
      ) {
        return;
      }
      if (action.signature === lastClickSignature) return;
      dispatchAutoclick(
        action,
        resolveElement(action),
        activeOptions,
      );
    }, delayMs);
    return;
  }
  dispatchAutoclick(action, element, options);
};

const dispatchAutoclick = (
  action: NextClick,
  element: HTMLElement | undefined,
  options: ActionGuideOptions,
): void => {
  if (action.kind === "board") {
    lastClickSignature = action.signature;
    if (options.validate && !options.validate()) {
      lastClickSignature = "";
      options.onExecution?.({
        succeeded: false,
        signature: action.signature,
        reason: "Board state changed before the requested placement",
      });
      return;
    }
    const generation = ++boardCommandGeneration;
    const command = {
      action,
      options,
      attempt: 1,
      generation,
    };
    activeBoardCommand = command;
    boardCommandAttempts.set(action.signature, 1);
    options.onExecutionStart?.({ signature: action.signature });
    if (!executeBoardAction(action, 1)) {
      clearBoardCommand(action.signature);
      options.onExecution?.({
        succeeded: false,
        signature: action.signature,
        reason: "Colonist board command could not be dispatched",
      });
      return;
    }
    if (action.followupPlayer) {
      activeBoardFollowupSignature = action.signature;
      const stillCurrent = () =>
        activeBoardFollowupSignature === action.signature;
      later(
        () =>
          pollForFollowup(
            {
              kind: "player",
              player: action.followupPlayer!,
              label: `Steal from ${action.followupPlayer}`,
              signature: `robber-player|${action.signature}|${action.followupPlayer}`,
              confidence: 0.94,
            },
            options,
            () =>
              pollForConfirmation(
                "Confirm player",
                options,
                undefined,
                0,
                stillCurrent,
              ),
            0,
            stillCurrent,
          ),
        220,
      );
    }
    scheduleBoardCommandRetry(command);
  } else {
    if (element) {
      controlResolutionAttempts.delete(action.signature);
      reportedMissingControls.delete(action.signature);
      lastClickSignature = action.signature;
    }
    const buildCommitAttempt =
      action.kind === "build" && action.build !== "development"
        ? buildControlCommitAttempts.get(action.signature) ?? 0
        : 0;
    const waitForControlCommit = Boolean(options.validateControlCommit);
    if (
      element &&
      validatedClick(
        element,
        options,
        action.signature,
        waitForControlCommit ? false : buildCommitAttempt === 0,
      )
    ) {
      requestBoardRefresh();
      if (waitForControlCommit) {
        awaitControlCommit(action, options);
      }
      if (action.kind === "build" && action.build !== "development") {
        const attempt = buildCommitAttempt + 1;
        buildControlCommitAttempts.set(action.signature, attempt);
        // Opening a road/settlement/city placement mode is idempotent, but
        // Colonist can ignore a click while React replaces the active action
        // bar. Re-resolve and retry only while the exact recommendation and
        // validation state are still current. Development purchases are
        // intentionally excluded because repeating one could buy two cards.
        later(() => {
          const activeOptions = currentGuideOptions;
          if (
            !activeOptions?.autonomous ||
            currentGuideAction?.signature !== action.signature ||
            lastClickSignature !== action.signature ||
            (activeOptions.validate && !activeOptions.validate())
          ) {
            buildControlCommitAttempts.delete(action.signature);
            return;
          }
          const fresh = resolveElement(action);
          if (!fresh) {
            requestBoardRefresh();
            return;
          }
          if (attempt >= 5) {
            if (!reportedMissingControls.has(action.signature)) {
              reportedMissingControls.add(action.signature);
              activeOptions.onExecution?.({
                succeeded: false,
                signature: action.signature,
                reason:
                  "Colonist did not enter placement mode after bounded build-control retries",
              });
            }
            // Do not leave an ignored React control permanently latched.
            // Releasing the signature lets the refreshed board re-resolve a
            // newly mounted control and, if necessary, execute a fresh deep
            // decision rather than waiting forever behind the stale element.
            buildControlCommitAttempts.delete(action.signature);
            lastClickSignature = "";
            requestBoardRefresh();
            return;
          }
          lastClickSignature = "";
          // Retries already waited for Colonist; do not re-apply the user delay.
          dispatchAutoclick(action, fresh, {
            ...activeOptions,
            autopilotDelayMs: 0,
          });
        }, 900);
      }
    } else if (!element) {
      const attempt =
        (controlResolutionAttempts.get(action.signature) ?? 0) + 1;
      controlResolutionAttempts.set(action.signature, attempt);
      if (attempt <= 6) {
        later(() => {
          if (
            currentGuideOptions?.autonomous &&
            lastClickSignature !== action.signature
          ) {
            requestBoardRefresh();
          }
        }, 180 + attempt * 80);
      } else if (!reportedMissingControls.has(action.signature)) {
        reportedMissingControls.add(action.signature);
        options.onExecution?.({
          succeeded: false,
          signature: action.signature,
          reason: "Recommended Colonist control was not present after bounded retries",
          diagnostic: tradeExecutionDiagnostic(action),
        });
      }
    }
  }
};

const later = (callback: () => void, delay: number): void => {
  const timer = window.setTimeout(() => {
    followupTimers.delete(timer);
    callback();
  }, delay);
  followupTimers.add(timer);
};

interface WorkflowStep {
  label: string;
  resolve: () => HTMLElement | undefined;
  /** The step has already reached its intended state and can be skipped. */
  ready?: () => boolean;
  /** Verify that Colonist committed the click before advancing. */
  complete?: () => boolean;
  /**
   * Re-resolve and re-click an idempotent control when Colonist swallows the
   * first click while replacing its React action bar.
   */
  retryOnIncomplete?: boolean;
  /** Re-run this step until `ready` is true (used to clear stale drafts). */
  repeatUntilReady?: boolean;
  settleMs?: number;
}

const resourceSteps = (
  cards: ResourceVector,
  side: "give" | "receive",
  resolver: (
    resource: Resource,
    side: "give" | "receive",
  ) => HTMLElement | undefined,
  verb: string,
): WorkflowStep[] =>
  (Object.keys(cards) as Resource[]).flatMap((resource) =>
    Array.from({ length: cards[resource] }, (_, index) => ({
      label: `${verb} ${resource}${cards[resource] > 1 ? ` ${index + 1}/${cards[resource]}` : ""}`,
      resolve: () => resolver(resource, side),
      settleMs: 150,
    })),
  );

export const tradePanelIsOpen = (): boolean =>
  [
    document.querySelector<HTMLElement>(
      "[class*='proposalWantedHalfContainer-']",
    ),
    document.querySelector<HTMLElement>(
      "[class*='proposalOfferedHalfContainer-']",
    ),
  ].some((element) => Boolean(element && visible(element))) &&
  Boolean(
    [
      document.querySelector<HTMLElement>("#action-button-trade-players"),
      document.querySelector<HTMLElement>("#action-button-trade-bank"),
    ].some((element) => Boolean(element && visible(element))),
  );

const selectedTradeCards = (): HTMLElement[] =>
  [
    ...new Set(
      [
        ...document.querySelectorAll<HTMLImageElement>(
          "[class*='proposalWantedHalfContainer-'] img[src*='card_'], [class*='proposalOfferedHalfContainer-'] img[src*='card_']",
        ),
      ]
        .map((image) => nearestClickable(image))
        .filter((element): element is HTMLElement => Boolean(element)),
    ),
  ];

const tradeDraftIsEmpty = (): boolean => selectedTradeCards().length === 0;

const proposalResourceCount = (
  side: "give" | "receive",
  resource: Resource,
): number => {
  const selector =
    side === "give"
      ? "[class*='proposalOfferedHalfContainer-']"
      : "[class*='proposalWantedHalfContainer-']";
  const root = document.querySelector<HTMLElement>(selector);
  if (!root) return 0;
  const matches = [
    ...root.querySelectorAll<HTMLElement>(
      `[data-card-enum="${resourceCardEnum[resource]}"], img[src], [aria-label], [title]`,
    ),
  ].filter((element) => resourceEvidence(element, resource));
  const counts = matches.map((element) => {
    const stack =
      element.closest<HTMLElement>(
        "[class*='cardStackContainer-'], [class*='cardContainer-'], [data-card-enum], button, [role='button']",
      ) ?? element;
    const badge = stack.querySelector<HTMLElement>(
      "[class*='countBadge-'], [class*='cardCount-'], [class*='amount-']",
    );
    const value = Number.parseInt(
      normalized(badge?.textContent ?? "").match(/\d+/u)?.[0] ?? "1",
      10,
    );
    return Number.isFinite(value) ? Math.max(1, value) : 1;
  });
  // Colonist animates old and new stacks at the same time. The largest stack
  // count is the committed draft state; summing would double-count animation.
  return counts.length ? Math.max(...counts) : 0;
};

const tradeResourceSteps = (
  cards: ResourceVector,
  side: "give" | "receive",
  verb: string,
): WorkflowStep[] =>
  (Object.keys(cards) as Resource[]).flatMap((resource) =>
    Array.from({ length: cards[resource] }, (_, index) => {
      const expected = index + 1;
      const complete = () =>
        proposalResourceCount(side, resource) >= expected;
      return {
        label: `${verb} ${resource}${cards[resource] > 1 ? ` ${expected}/${cards[resource]}` : ""}`,
        resolve: () => findTradeResourceChoice(resource, side),
        ready: complete,
        complete,
        settleMs: 320,
      };
    }),
  );

const TRADE_FAILURE_PATTERN =
  /no one has (?:the )?wanted resource|no player has (?:the )?wanted resource|(?:nobody|none of the players?) (?:has|have) (?:the )?(?:wanted|requested|required) resource|(?:players?|opponents?) (?:do not|don't|does not|doesn't) have enough (?:cards|resources)|insufficient (?:cards|resources)(?: for (?:this )?trade)?|identical trade|trade (?:offer )?limit|too many (?:identical )?trades|cannot (?:make|send|offer) (?:this )?trade|invalid trade|not enough (?:cards|resources)/iu;

const DOMESTIC_TRADE_EXHAUSTION_PATTERN =
  /no one has (?:the )?wanted resource|no player has (?:the )?wanted resource|(?:nobody|none of the players?) (?:has|have) (?:the )?(?:wanted|requested|required) resource|identical trade|trade (?:offer )?limit|too many (?:identical )?trades/iu;

const tradeFailureLogKeys = (): Set<string> => {
  const root = findLogRoot();
  if (!root) return new Set();
  return new Set(
    findMessageElements(root)
      .slice(-16)
      .map(
        (element) =>
          `${element.getAttribute("data-index") ?? "unindexed"}|${normalized(element.textContent ?? "")}`,
      )
      .filter((key) => TRADE_FAILURE_PATTERN.test(key)),
  );
};

const floatingTradeFailures = (): Array<{
  element: HTMLElement;
  key: string;
  text: string;
}> =>
  [
    ...document.querySelectorAll<HTMLElement>(
      "[role='alert'], [aria-live='assertive'], [class*='toast'], [class*='snackbar'], [class*='notification'], [class*='errorMessage'], [class*='error-message'], [class*='floatingText'], [class*='floating-text'], [class*='insufficient']",
    ),
  ]
    .filter(visible)
    .flatMap((element) => {
      const text = element.textContent?.trim();
      return text && TRADE_FAILURE_PATTERN.test(text)
        ? [{ element, key: normalized(text), text }]
        : [];
    });

const visibleTradeFailure = (
  ignoredLogKeys: ReadonlySet<string> = new Set(),
  ignoredFloating: Map<HTMLElement, string> = new Map(),
): string | undefined => {
  for (const [element, key] of ignoredFloating) {
    if (
      !document.contains(element) ||
      !visible(element) ||
      normalized(element.textContent ?? "") !== key
    ) {
      ignoredFloating.delete(element);
    }
  }
  const floatingFailure = floatingTradeFailures().find(
    ({ element, key }) => ignoredFloating.get(element) !== key,
  )?.text;
  if (floatingFailure) return floatingFailure;

  const root = findLogRoot();
  if (!root) return undefined;
  return findMessageElements(root)
    .slice(-16)
    .map((element) => ({
      key: `${element.getAttribute("data-index") ?? "unindexed"}|${normalized(element.textContent ?? "")}`,
      text: element.textContent?.trim(),
    }))
    .find(
      ({ key, text }) =>
        !ignoredLogKeys.has(key) &&
        Boolean(text && TRADE_FAILURE_PATTERN.test(text)),
    )?.text;
};

const closeTradePanelStep = (label: string): WorkflowStep => ({
  label,
  resolve: findTradePanelControl,
  ready: () => !tradePanelIsOpen(),
  complete: () => !tradePanelIsOpen(),
  retryOnIncomplete: true,
  settleMs: 320,
});

const clearTradeDraftStep = (): WorkflowStep => ({
  label: "Clear previous trade draft",
  resolve: () => selectedTradeCards()[0],
  ready: tradeDraftIsEmpty,
  repeatUntilReady: true,
  settleMs: 220,
});

const tradeWorkflow = (
  action: Extract<NextClick, { kind: "trade-builder" }>,
): WorkflowStep[] => [
  {
    ...closeTradePanelStep("Close previous trade draft"),
  },
  {
    label: "Open trade builder",
    resolve: findTradePanelControl,
    ready: tradePanelIsOpen,
    complete: tradePanelIsOpen,
    retryOnIncomplete: true,
    settleMs: 320,
  },
  clearTradeDraftStep(),
  ...tradeResourceSteps(action.give, "give", "Offer"),
  ...tradeResourceSteps(action.receive, "receive", "Request"),
  {
    label:
      action.mode === "bank"
        ? "Confirm bank trade"
        : "Send this offer",
    resolve: () => findTradeSubmit(action.mode),
    settleMs: 460,
  },
  closeTradePanelStep("Close completed trade panel"),
];

const counterWorkflow = (
  action: Extract<NextClick, { kind: "trade" }>,
): WorkflowStep[] => {
  if (
    action.verdict !== "counter" ||
    !action.counterGive ||
    !action.counterReceive
  ) {
    return [];
  }
  return [
    {
      label: "Open counteroffer",
      resolve: () =>
        findTradeControl(
          action.offerIndex,
          "counter",
          action.tradeCreator,
          action.tradeExecutor,
          action.tradeCreatorGive,
          action.tradeCreatorReceive,
        ),
      ready: tradePanelIsOpen,
      complete: tradePanelIsOpen,
      retryOnIncomplete: true,
      settleMs: 340,
    },
    clearTradeDraftStep(),
    ...tradeResourceSteps(action.counterGive, "give", "Offer"),
    ...tradeResourceSteps(action.counterReceive, "receive", "Request"),
    {
      label: "Send counteroffer",
      resolve: () => findTradeSubmit("player"),
      settleMs: 460,
    },
    closeTradePanelStep("Close completed counteroffer"),
  ];
};

const discardWorkflow = (
  action: Extract<NextClick, { kind: "discard" }>,
): WorkflowStep[] => {
  const selectionProgress = (): { selected: number; required: number } | undefined => {
    for (const root of modalRoots()) {
      const match = normalized(root.textContent ?? "").match(
        /(?:^|\D)(\d{1,3})\s*\/\s*(\d{1,3})(?:\D|$)/u,
      );
      if (match) {
        return { selected: Number(match[1]), required: Number(match[2]) };
      }
    }
    return undefined;
  };
  const selectionSteps = resourceSteps(
    action.cards,
    "give",
    (resource) => findDiscardCard(resource),
    "Discard",
  ).map((step, index) => {
    const committed = () =>
      (selectionProgress()?.selected ?? 0) >= index + 1;
    return { ...step, ready: committed, complete: committed };
  });
  return [
    ...selectionSteps,
    {
      label: "Confirm discard",
      resolve: () => {
        const discardRoot = findDiscardRoot();
        return findConfirmationControl(discardRoot ?? document);
      },
      complete: () => !findDiscardRoot(),
      retryOnIncomplete: true,
      settleMs: 240,
    },
  ];
};

const developmentWorkflow = (
  action: Extract<NextClick, { kind: "development" }>,
): WorkflowStep[] => {
  const actionPanelOpen = (): boolean =>
    modalRoots().some((root) =>
      Boolean(root.querySelector("[class*='confirmButton-']")),
    );
  const steps: WorkflowStep[] = [
    {
      label: `Play ${action.card.replaceAll("-", " ")}`,
      resolve: () => findDevelopmentCard(action.card),
      ready: actionPanelOpen,
      settleMs: 180,
    },
    {
      label: `Confirm ${action.card.replaceAll("-", " ")}`,
      resolve: () =>
        findConfirmationControl(modalRoots()[0] ?? document),
      settleMs: 240,
    },
  ];
  if (action.followupResources?.length) {
    steps.push(
      ...action.followupResources.map((resource, index) => ({
        label: `Choose ${resource}${action.followupResources!.length > 1 ? ` ${index + 1}/${action.followupResources!.length}` : ""}`,
        resolve: () => findResourceChoice(resource),
        settleMs: 170,
      })),
      {
        label: "Confirm resource choice",
        resolve: () =>
          findConfirmationControl(modalRoots()[0] ?? document),
        settleMs: 240,
      },
    );
  }
  return steps;
};

const playerWorkflow = (
  action: Extract<NextClick, { kind: "player" }>,
): WorkflowStep[] => [
  {
    label: `Steal from ${action.player}`,
    resolve: () => findPlayerChoice(action.player),
    settleMs: 180,
  },
  {
    label: "Confirm victim",
    resolve: () =>
      findConfirmationControl(modalRoots()[0] ?? document),
    settleMs: 240,
  },
];

const cancelWorkflow = (): void => {
  workflowGeneration += 1;
  workflowSignature = "";
  workflowAction = undefined;
  workflowOptions = undefined;
  workflowCurrentElement = undefined;
};

const cancelAutonomousContinuations = (): void => {
  for (const timer of followupTimers) window.clearTimeout(timer);
  followupTimers.clear();
  pendingAutopilotSignature = "";
  clearBoardCommand();
  cancelWorkflow();
  tradePreflightSignature = "";
  activeBoardFollowupSignature = "";
  boardFollowupCleanup?.();
  controlResolutionAttempts.clear();
  buildControlCommitAttempts.clear();
  reportedMissingControls.clear();
};

export const activeWorkflowAction = (
  boardAction?: BoardAction,
  robberVictimSelection = false,
): NextClick | undefined => {
  if (!workflowSignature || !workflowAction) return undefined;
  const stillOwnsBoardPhase =
    workflowAction.kind === "discard"
      ? boardAction === "discard"
      : workflowAction.kind === "player"
        ? boardAction === "none" && robberVictimSelection
        : workflowAction.kind === "trade-builder"
          ? boardAction === "none" || tradePanelIsOpen()
          : workflowAction.kind === "development"
            ? boardAction === "none" ||
              boardAction === "road" ||
              boardAction === "robber"
            : boardAction === "none";
  if (stillOwnsBoardPhase) return workflowAction;
  cancelWorkflow();
  document.getElementById(ROOT_ID)?.remove();
  return undefined;
};

const startWorkflow = (
  action: NextClick,
  steps: WorkflowStep[],
  options: ActionGuideOptions,
): void => {
  if (
    !steps.length ||
    workflowSignature === action.signature ||
    lastClickSignature === action.signature
  ) {
    return;
  }
  cancelWorkflow();
  workflowSignature = action.signature;
  workflowAction = action;
  workflowOptions = options;
  const generation = workflowGeneration;
  const tradeTransaction =
    action.kind === "trade-builder" ||
    (action.kind === "trade" && action.verdict === "counter");
  const ignoredTradeFailureLogKeys = tradeTransaction
    ? tradeFailureLogKeys()
    : new Set<string>();
  const ignoredTradeFailures = tradeTransaction
    ? new Map(
        floatingTradeFailures().map(({ element, key }) => [element, key]),
      )
    : new Map<HTMLElement, string>();

  const fail = (reason: string, tradeFailure?: string): void => {
    const activeOptions = workflowOptions ?? options;
    activeOptions.onExecution?.({
      succeeded: false,
      signature: action.signature,
      reason,
      diagnostic: {
        ...tradeExecutionDiagnostic(action),
        ...(tradeFailure && DOMESTIC_TRADE_EXHAUSTION_PATTERN.test(tradeFailure)
          ? { domesticTradeExhausted: true }
          : {}),
      },
    });
    if (tradeTransaction) {
      const closeTradePanel = (attempt = 0): void => {
        if (!tradePanelIsOpen()) {
          requestBoardRefresh();
          return;
        }
        const close = findTradePanelControl();
        if (close) close.click();
        if (attempt < 2) {
          later(() => closeTradePanel(attempt + 1), 240);
        } else {
          requestBoardRefresh();
        }
      };
      later(() => closeTradePanel(), 120);
    }
    cancelWorkflow();
    document.getElementById(ROOT_ID)?.remove();
    requestBoardRefresh();
  };

  const run = (index: number, attempts = 0): void => {
    if (
      generation !== workflowGeneration ||
      workflowSignature !== action.signature
    ) {
      return;
    }
    const tradeFailure = tradeTransaction
      ? visibleTradeFailure(
          ignoredTradeFailureLogKeys,
          ignoredTradeFailures,
        )
      : undefined;
    if (tradeFailure) {
      fail(`Colonist rejected the trade workflow: ${tradeFailure}`, tradeFailure);
      return;
    }
    const step = steps[index];
    if (!step) {
      const completedOptions = workflowOptions ?? options;
      if (
        tradeTransaction &&
        completedOptions.validateTransactionCommit &&
        !completedOptions.validateTransactionCommit()
      ) {
        if (attempts < 24) {
          requestBoardRefresh();
          later(() => run(index, attempts + 1), 140);
        } else {
          fail("Colonist did not commit the submitted trade");
        }
        return;
      }
      lastClickSignature = action.signature;
      completedOptions.onExecution?.({
        succeeded: true,
        signature: action.signature,
      });
      cancelWorkflow();
      document.getElementById(ROOT_ID)?.remove();
      // The final panel close does not reliably mutate the observed board.
      // Force a fresh snapshot so the next selected-engine action starts
      // immediately instead of waiting for an unrelated game animation.
      requestBoardRefresh();
      return;
    }
    if (step.ready?.()) {
      later(() => run(index + 1), 90);
      return;
    }
    const element = step.resolve();
    if (!element) {
      if (attempts < 30) {
        later(() => run(index, attempts + 1), 180);
      } else {
        fail(`Workflow control not found: ${step.label}`);
      }
      return;
    }
    workflowCurrentElement = element;
    const currentOptions = workflowOptions ?? options;
    if (currentOptions.highlight) {
      drawHighlight(
        {
          kind: "turn-control",
          control: "confirm",
          label: step.label,
          signature: `${action.signature}|step|${index}`,
          confidence: action.confidence,
        },
        element,
      );
    }
    let advanced = false;
    const advance = () => {
      if (advanced) return;
      advanced = true;
      workflowCurrentElement = undefined;
      requestBoardRefresh();
      const verify = (
        verificationAttempts = 0,
        recommitAttempts = 0,
      ): void => {
        if (
          generation !== workflowGeneration ||
          workflowSignature !== action.signature
        ) {
          return;
        }
        const failure = tradeTransaction
          ? visibleTradeFailure(
              ignoredTradeFailureLogKeys,
              ignoredTradeFailures,
            )
          : undefined;
        if (failure) {
          fail(`Colonist rejected the trade workflow: ${failure}`, failure);
          return;
        }
        if (step.repeatUntilReady) {
          later(
            () => run(step.ready?.() ? index + 1 : index),
            step.settleMs ?? 220,
          );
          return;
        }
        if (!step.complete || step.complete()) {
          later(() => run(index + 1), step.settleMs ?? 220);
          return;
        }
        if (
          step.retryOnIncomplete &&
          verificationAttempts > 0 &&
          verificationAttempts % 8 === 0 &&
          recommitAttempts < 2
        ) {
          const retryOptions = workflowOptions ?? options;
          const validateRetry =
            index === 0 || !retryOptions.validateContinuation
              ? retryOptions.validate
              : retryOptions.validateContinuation;
          if (validateRetry && !validateRetry()) {
            fail(
              "State signature or legal target set changed before workflow retry",
            );
            return;
          }
          const fresh = step.resolve();
          if (fresh) {
            retryOptions.onExecutionStart?.({ signature: action.signature });
            fresh.click();
            requestBoardRefresh();
            later(
              () =>
                verify(
                  verificationAttempts + 1,
                  recommitAttempts + 1,
                ),
              180,
            );
            return;
          }
        }
        if (verificationAttempts < 24) {
          later(
            () =>
              verify(
                verificationAttempts + 1,
                recommitAttempts,
              ),
            120,
          );
          return;
        }
        fail(`Colonist did not commit workflow step: ${step.label}`);
      };
      verify();
    };
    element.addEventListener(
      "click",
      () => {
        currentOptions.onExecutionStart?.({ signature: action.signature });
        advance();
      },
      { once: true },
    );
    if (currentOptions.autonomous) {
      const startDelay =
        index === 0 ? Math.max(0, currentOptions.autopilotDelayMs ?? 0) : 0;
      later(() => {
        if (
          generation !== workflowGeneration ||
          workflowSignature !== action.signature
        ) {
          return;
        }
        if (step.ready?.()) {
          workflowCurrentElement = undefined;
          advance();
          return;
        }
        // Colonist frequently replaces React controls while the workflow is
        // waiting for its first-click delay or a modal animation. A detached
        // node still accepts HTMLElement.click() without reaching the live UI,
        // so resolve the authoritative control again immediately before the
        // automatic click.
        const freshElement = step.resolve();
        if (!freshElement) {
          workflowCurrentElement = undefined;
          requestBoardRefresh();
          later(() => run(index, attempts + 1), 180);
          return;
        }
        workflowCurrentElement = freshElement;
        if (currentOptions.highlight && freshElement !== element) {
          drawHighlight(
            {
              kind: "turn-control",
              control: "confirm",
              label: step.label,
              signature: `${action.signature}|step|${index}`,
              confidence: action.confidence,
            },
            freshElement,
          );
        }
        if (
          !validatedClick(
            freshElement,
            index === 0 || !currentOptions.validateContinuation
              ? currentOptions
              : {
                  ...currentOptions,
                  validate: currentOptions.validateContinuation,
                },
            `${action.signature}|step|${index}`,
            false,
          )
        ) {
          cancelWorkflow();
          return;
        }
        // React can replace the clicked card/control while handling the same
        // event. In that case a listener attached to the old node is not a
        // reliable transaction clock even though Colonist accepted the
        // click. Autopilot owns this click, so advance idempotently here too.
        advance();
      }, startDelay + (tradeTransaction ? 280 : 160));
    }
  };

  run(0);
};

const pollForFollowup = (
  action: Extract<NextClick, { kind: "resource" | "player" }>,
  options: ActionGuideOptions,
  onComplete?: () => void,
  attempts = 0,
  stillCurrent: () => boolean = () => true,
): void => {
  if (!stillCurrent()) return;
  const element = resolveElement(action);
  if (!element && attempts < 28) {
    later(
      () =>
        pollForFollowup(
          action,
          options,
          onComplete,
          attempts + 1,
          stillCurrent,
        ),
      140,
    );
    return;
  }
  if (!element) return;
  const activeOptions = currentGuideOptions ?? options;
  if (activeOptions.highlight) drawHighlight(action, element);
  if (activeOptions.autonomous) {
    later(() => {
      if (!stillCurrent()) return;
      const freshElement = resolveElement(action);
      if (!freshElement) {
        pollForFollowup(
          action,
          activeOptions,
          onComplete,
          attempts + 1,
          stillCurrent,
        );
        return;
      }
      if (validatedClick(freshElement, activeOptions, action.signature)) {
        requestBoardRefresh();
        onComplete?.();
      }
    }, 180);
  } else if (onComplete) {
    element.addEventListener("click", onComplete, { once: true });
  }
};

const pollForConfirmation = (
  label: string,
  options: ActionGuideOptions,
  onComplete?: () => void,
  attempts = 0,
  stillCurrent: () => boolean = () => true,
): void => {
  if (!stillCurrent()) return;
  const resolveConfirmation = (): HTMLElement | undefined => {
    const root = modalRoots()[0];
    return (
      findConfirmationControl(root ?? document) ??
      (
        root
          ? [
              ...root.querySelectorAll<HTMLElement>(
                "button:not([disabled]), [role='button']:not([aria-disabled='true'])",
              ),
            ].filter(visible).at(-1)
          : undefined
      )
    );
  };
  const element = resolveConfirmation();
  if (!element && attempts < 35) {
    later(
      () =>
        pollForConfirmation(
          label,
          options,
          onComplete,
          attempts + 1,
          stillCurrent,
        ),
      140,
    );
    return;
  }
  if (!element) return;
  const action: NextClick = {
    kind: "turn-control",
    control: "confirm",
    label,
    signature: `followup-confirm|${label}`,
    confidence: 1,
  };
  const activeOptions = currentGuideOptions ?? options;
  if (activeOptions.highlight) drawHighlight(action, element);
  if (activeOptions.autonomous) {
    later(() => {
      if (!stillCurrent()) return;
      const freshElement = resolveConfirmation();
      if (!freshElement) {
        pollForConfirmation(
          label,
          activeOptions,
          onComplete,
          attempts + 1,
          stillCurrent,
        );
        return;
      }
      if (validatedClick(freshElement, activeOptions, action.signature)) {
        requestBoardRefresh();
        onComplete?.();
      }
    }, 160);
  } else if (onComplete) {
    element.addEventListener("click", onComplete, { once: true });
  }
};

const installFollowupGuide = (
  action: NextClick,
  element: HTMLElement | undefined,
  options: ActionGuideOptions,
): void => {
  if (action.kind === "board") {
    boardFollowupCleanup?.();
    if (action.followupPlayer) {
      activeBoardFollowupSignature = action.signature;
    }
    const handler = (event: PointerEvent) => {
      if (
        event.button !== 0 ||
        Math.hypot(
          event.clientX - action.point.x,
          event.clientY - action.point.y,
        ) > 44
      ) {
        return;
      }
      document.removeEventListener("pointerdown", handler, true);
      boardFollowupCleanup = undefined;
      options.onExecutionStart?.({ signature: action.signature });
      executeBoardAction(action, 1);
      if (action.followupPlayer) {
        const stillCurrent = () =>
          activeBoardFollowupSignature === action.signature;
        later(
          () =>
            pollForFollowup(
              {
                kind: "player",
                player: action.followupPlayer!,
                label: `Steal from ${action.followupPlayer}`,
                signature: `robber-player|${action.followupPlayer}`,
                confidence: 0.94,
              },
              options,
              () =>
                pollForConfirmation(
                  "Confirm player",
                  options,
                  undefined,
                  0,
                  stillCurrent,
                ),
              0,
              stillCurrent,
            ),
          180,
        );
      }
    };
    document.addEventListener("pointerdown", handler, true);
    const cleanup = () => {
      document.removeEventListener("pointerdown", handler, true);
      if (boardFollowupCleanup === cleanup) boardFollowupCleanup = undefined;
    };
    boardFollowupCleanup = cleanup;
    return;
  }
};

const requiresClosedTradePanel = (action: NextClick): boolean =>
  action.kind !== "trade-builder" &&
  !(action.kind === "trade" && action.verdict === "counter");

const renderTradePanelPreflight = (
  action: NextClick,
  options: ActionGuideOptions,
): boolean => {
  if (!tradePanelIsOpen() || !requiresClosedTradePanel(action)) return false;
  const close = findTradePanelControl();
  if (!close) return false;
  const signature = `close-trade-panel|${action.signature}`;
  if (options.highlight) {
    drawHighlight(
      {
        kind: "turn-control",
        control: "confirm",
        label: `Close trade panel, then ${action.label.toLowerCase()}`,
        signature,
        confidence: 1,
      },
      close,
    );
  }
  const finish = () => {
    tradePreflightSignature = "";
    requestBoardRefresh();
  };
  close.addEventListener("click", () => later(finish, 320), { once: true });
  if (options.autonomous && tradePreflightSignature !== signature) {
    tradePreflightSignature = signature;
    later(() => {
      if (
        tradePreflightSignature === signature &&
        tradePanelIsOpen()
      ) {
        const freshClose = findTradePanelControl();
        if (freshClose) {
          if (freshClose !== close) {
            freshClose.addEventListener("click", () => later(finish, 320), {
              once: true,
            });
          }
          freshClose.click();
        } else {
          finish();
        }
      } else {
        finish();
      }
    }, Math.max(0, options.autopilotDelayMs ?? 0) + 280);
  }
  return true;
};

export const renderActionGuide = (
  action: NextClick | undefined,
  options: ActionGuideOptions,
): void => {
  const activatingAutopilot =
    !currentGuideOptions?.autonomous && options.autonomous;
  const deactivatingAutopilot =
    currentGuideOptions?.autonomous === true && !options.autonomous;
  if (deactivatingAutopilot) cancelAutonomousContinuations();
  currentGuideOptions = options;
  currentGuideAction = action;
  manualExecutionCleanup?.();
  manualExecutionCleanup = undefined;
  if (
    pendingAutopilotSignature &&
    pendingAutopilotSignature !== action?.signature
  ) {
    pendingAutopilotSignature = "";
  }
  if (
    activeBoardFollowupSignature &&
    (
      action?.kind !== "board" ||
      action.signature !== activeBoardFollowupSignature
    )
  ) {
    activeBoardFollowupSignature = "";
  }
  if (activatingAutopilot) {
    lastClickSignature = "";
    pendingAutopilotSignature = "";
  }
  if (!action) {
    if (
      !activeBoardCommand ||
      !activeBoardCommand.options.validateBoardContinuation ||
      !boardCommandStillLegal(activeBoardCommand)
    ) {
      const command = activeBoardCommand;
      if (command) {
        const committed = command.options.validateBoardCommit?.();
        clearBoardCommand(command.action.signature);
        if (committed !== undefined) {
          command.options.onExecution?.({
            succeeded: committed,
            signature: command.action.signature,
            ...(!committed
              ? {
                  reason:
                    "Board state changed without the expected placement commit",
                }
              : {}),
          });
        }
      } else {
        clearBoardCommand();
      }
    }
    cancelWorkflow();
    boardFollowupCleanup?.();
    document.getElementById(ROOT_ID)?.remove();
    return;
  }
  if (workflowSignature === action.signature) {
    // Colonist can publish a new hand snapshot before the transaction panel
    // finishes closing. Refresh validation and settings, but attribute the
    // workflow result to the trace which actually started it.
    workflowOptions = {
      ...options,
      onExecutionStart:
        workflowOptions?.onExecutionStart ?? options.onExecutionStart,
      onExecution:
        workflowOptions?.onExecution ?? options.onExecution,
      validateTransactionCommit:
        workflowOptions?.validateTransactionCommit ??
        options.validateTransactionCommit,
    };
    if (activatingAutopilot && workflowCurrentElement) {
      const element = workflowCurrentElement;
      later(
        () =>
          validatedClick(
            element,
            workflowOptions ?? options,
            action.signature,
            false,
          ),
        80,
      );
    }
    return;
  }
  if (workflowSignature && workflowSignature !== action.signature) {
    cancelWorkflow();
  }
  if (renderTradePanelPreflight(action, options)) {
    return;
  }
  if (action.kind === "discard") {
    startWorkflow(action, discardWorkflow(action), options);
    return;
  }
  if (action.kind === "development") {
    startWorkflow(action, developmentWorkflow(action), options);
    return;
  }
  if (action.kind === "player") {
    startWorkflow(action, playerWorkflow(action), options);
    return;
  }
  if (action.kind === "trade-builder") {
    startWorkflow(action, tradeWorkflow(action), options);
    return;
  }
  if (
    action.kind === "trade" &&
    action.verdict === "counter" &&
    action.counterGive &&
    action.counterReceive
  ) {
    startWorkflow(action, counterWorkflow(action), options);
    return;
  }
  if (action.kind !== "board" || !action.followupPlayer) {
    boardFollowupCleanup?.();
  }
  const element = resolveElement(action);
  if (options.highlight) drawHighlight(action, element);
  else document.getElementById(ROOT_ID)?.remove();
  installFollowupGuide(action, element, options);
  installManualExecutionObserver(action, element, options);
  maybeAutoclick(action, element, options);
};

export const destroyActionGuide = (): void => {
  document.getElementById(ROOT_ID)?.remove();
  lastClickSignature = "";
  pendingAutopilotSignature = "";
  cancelWorkflow();
  currentGuideOptions = undefined;
  currentGuideAction = undefined;
  manualExecutionCleanup?.();
  manualExecutionCleanup = undefined;
  tradePreflightSignature = "";
  for (const timer of followupTimers) window.clearTimeout(timer);
  followupTimers.clear();
  boardCommandAttempts.clear();
  activeBoardCommand = undefined;
  boardCommandGeneration += 1;
  controlResolutionAttempts.clear();
  buildControlCommitAttempts.clear();
  reportedMissingControls.clear();
  activeBoardFollowupSignature = "";
  boardFollowupCleanup?.();
};
