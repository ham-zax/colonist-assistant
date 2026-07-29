import type { DecisionAnalysis } from "../core/engine";
import type { TrackerState } from "../core/types";

const ROOT_ID = "colonist-assistant-win-odds";
const FONT_STYLE_ID = "colonist-assistant-document-font";
const STALE_AFTER_MS = 15_000;
const REPOSITION_DELAY_MS = 48;

let lastAnalysis: DecisionAnalysis | undefined;
let lastState: TrackerState | undefined;
let staleTimer: number | undefined;
let repositionTimer: number | undefined;
let observer: MutationObserver | undefined;

const ensureFont = (): void => {
  if (document.getElementById(FONT_STYLE_ID)) return;
  const style = document.createElement("style");
  style.id = FONT_STYLE_ID;
  style.textContent = `@font-face{font-family:"Colonist Assistant Archivo";src:url("${chrome.runtime.getURL("assets/fonts/ArchivoNarrow-Variable.ttf")}") format("truetype");font-style:normal;font-weight:400 700;font-display:swap}`;
  (document.head ?? document.documentElement).append(style);
};

const visible = (element: HTMLElement): boolean => {
  const style = getComputedStyle(element);
  const rect = element.getBoundingClientRect();
  return (
    style.display !== "none" &&
    style.visibility !== "hidden" &&
    Number(style.opacity) > 0 &&
    rect.width > 0 &&
    rect.height > 0
  );
};

const textWithoutAssistant = (element: HTMLElement): string =>
  (element.textContent ?? "").replace(/\s+/gu, " ").trim();

const findPlayerPanel = (
  player: string,
  claimed: Set<HTMLElement>,
): HTMLElement | undefined => {
  const candidates: Array<{ element: HTMLElement; score: number }> = [];
  for (const element of document.querySelectorAll<HTMLElement>(
    "div, section, article, span",
  )) {
    if (
      element.closest(
        `#${ROOT_ID}, #colonist-assistant-root, [data-colonist-assistant-trade-verdict]`,
      ) ||
      !visible(element)
    ) {
      continue;
    }
    const text = textWithoutAssistant(element);
    if (!text.includes(player) || text.length > 220) continue;
    let panel: HTMLElement | null = element;
    for (let depth = 0; panel && depth < 5; depth += 1) {
      const rect = panel.getBoundingClientRect();
      const panelText = textWithoutAssistant(panel);
      if (
        panelText.includes(player) &&
        rect.width >= 160 &&
        rect.width <= 520 &&
        rect.height >= 38 &&
        rect.height <= 180
      ) {
        const rightSideBonus = rect.left > window.innerWidth * 0.48 ? 30 : 0;
        const exactBonus = text === player ? 26 : 0;
        const compactness = Math.max(
          0,
          30 - Math.abs(rect.width * rect.height - 24_000) / 1_800,
        );
        candidates.push({
          element: panel,
          score:
            rightSideBonus +
            exactBonus +
            compactness -
            depth * 3 -
            panelText.length * 0.03,
        });
      }
      panel = panel.parentElement;
    }
  }
  return candidates
    .filter((candidate) => !claimed.has(candidate.element))
    .sort((left, right) => right.score - left.score)[0]?.element;
};

const ensureRoot = (): HTMLDivElement => {
  ensureFont();
  const existing = document.getElementById(ROOT_ID);
  if (existing instanceof HTMLDivElement) return existing;
  const root = document.createElement("div");
  root.id = ROOT_ID;
  root.style.cssText =
    'position:fixed;inset:0;z-index:2147482988;pointer-events:none;font-family:"Colonist Assistant Archivo",ui-sans-serif,system-ui,sans-serif;';
  document.documentElement.append(root);
  return root;
};

const knownPlayerMutation = (
  records: MutationRecord[],
  players: Set<string>,
): boolean =>
  records.some((record) => {
    const target =
      record.target instanceof HTMLElement
        ? record.target
        : record.target.parentElement;
    if (target?.closest(`#${ROOT_ID}`)) return false;
    const candidates = [
      ...record.addedNodes,
      ...record.removedNodes,
      ...(target ? [target] : []),
    ];
    return candidates.some((node) => {
      const text = (node.textContent ?? "").replace(/\s+/gu, " ").trim();
      if (!text || text.length > 500) return false;
      return [...players].some((player) => text.includes(player));
    });
  });

const scheduleReposition = (): void => {
  if (repositionTimer !== undefined || !lastAnalysis || !lastState) return;
  repositionTimer = window.setTimeout(() => {
    repositionTimer = undefined;
    if (lastAnalysis && lastState) {
      renderCurrentWinOdds(lastAnalysis, lastState);
    }
  }, REPOSITION_DELAY_MS);
};

const ensureObserver = (): void => {
  if (observer) return;
  observer = new MutationObserver((records) => {
    const players = new Set(
      lastAnalysis?.players.map((estimate) => estimate.player) ?? [],
    );
    if (players.size && knownPlayerMutation(records, players)) {
      scheduleReposition();
    }
  });
  observer.observe(document.documentElement, {
    childList: true,
    subtree: true,
  });
  window.addEventListener("resize", scheduleReposition, { passive: true });
  window.addEventListener("scroll", scheduleReposition, {
    capture: true,
    passive: true,
  });
};

const removeStaleBadges = (
  root: HTMLDivElement,
  players: Set<string>,
): void => {
  for (const badge of root.querySelectorAll<HTMLElement>("[data-player]")) {
    if (!badge.dataset.player || !players.has(badge.dataset.player)) {
      badge.remove();
    }
  }
};

const badgeFor = (
  root: HTMLDivElement,
  player: string,
): HTMLSpanElement | undefined =>
  [...root.querySelectorAll<HTMLSpanElement>("span[data-player]")].find(
    (badge) => badge.dataset.player === player,
  );

const renderCurrentWinOdds = (
  analysis: DecisionAnalysis,
  state: TrackerState,
): void => {
  if (!state.playerOrder.length || !analysis.players.length) return;
  const root = ensureRoot();
  const currentPlayers = new Set(
    analysis.players.map((estimate) => estimate.player),
  );
  removeStaleBadges(root, currentPlayers);
  const claimed = new Set<HTMLElement>();
  for (const estimate of analysis.players) {
    const panel = findPlayerPanel(estimate.player, claimed);
    const existingBadge = badgeFor(root, estimate.player);
    if (existingBadge) {
      existingBadge.textContent = `${Math.round(estimate.probability * 100)}% WIN`;
      existingBadge.title = `${estimate.player}: ${Math.round(estimate.probability * 100)}% stabilized model estimate, not yet calibrated · ${estimate.etaTurns} turn ETA · ${estimate.confidence} hand-evidence confidence · ${analysis.model}`;
    }
    // Colonist briefly unmounts or empties player panels during React commits.
    // Keep the last valid badge instead of flashing every player's odds off.
    if (!panel) continue;
    claimed.add(panel);
    const rect = panel.getBoundingClientRect();
    const badge = existingBadge ?? document.createElement("span");
    badge.dataset.player = estimate.player;
    if (!existingBadge) {
      badge.textContent = `${Math.round(estimate.probability * 100)}% WIN`;
      badge.title = `${estimate.player}: ${Math.round(estimate.probability * 100)}% stabilized model estimate, not yet calibrated · ${estimate.etaTurns} turn ETA · ${estimate.confidence} hand-evidence confidence · ${analysis.model}`;
    }
    const left = Math.max(4, Math.min(window.innerWidth - 72, rect.right - 72));
    const top = Math.max(4, Math.min(window.innerHeight - 24, rect.top + 5));
    badge.style.cssText = [
      "position:fixed",
      `left:${Math.round(left)}px`,
      `top:${Math.round(top)}px`,
      "display:grid",
      "min-width:66px",
      "height:20px",
      "place-items:center",
      "padding:0 6px",
      "border:1px solid rgba(241,200,75,.7)",
      "background:rgba(13,24,33,.94)",
      "color:#f1c84b",
      "box-shadow:0 4px 14px rgba(3,10,15,.34)",
      "font-size:10px",
      "font-weight:800",
      "letter-spacing:.045em",
      "font-variant-numeric:tabular-nums",
      "white-space:nowrap",
    ].join(";");
    if (!existingBadge) root.append(badge);
  }
};

export const renderWinOdds = (
  analysis: DecisionAnalysis | undefined,
  state: TrackerState | undefined,
): void => {
  if (!analysis || !state?.playerOrder.length) {
    // A missing scan is not proof that the game ended. Retain the last valid
    // model briefly; explicit lifecycle events still call destroyWinOdds().
    if (
      document.getElementById(ROOT_ID) &&
      staleTimer === undefined
    ) {
      staleTimer = window.setTimeout(destroyWinOdds, STALE_AFTER_MS);
    }
    return;
  }
  if (staleTimer !== undefined) {
    window.clearTimeout(staleTimer);
    staleTimer = undefined;
  }
  lastAnalysis = analysis;
  lastState = state;
  ensureObserver();
  renderCurrentWinOdds(analysis, state);
};

export const destroyWinOdds = (): void => {
  if (staleTimer !== undefined) window.clearTimeout(staleTimer);
  if (repositionTimer !== undefined) window.clearTimeout(repositionTimer);
  staleTimer = undefined;
  repositionTimer = undefined;
  lastAnalysis = undefined;
  lastState = undefined;
  observer?.disconnect();
  observer = undefined;
  window.removeEventListener("resize", scheduleReposition);
  window.removeEventListener("scroll", scheduleReposition, true);
  document.getElementById(ROOT_ID)?.remove();
};
