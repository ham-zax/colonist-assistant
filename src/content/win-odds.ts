import type { DecisionAnalysis } from "../core/engine";
import type { TrackerState } from "../core/types";

const ROOT_ID = "colonist-assistant-win-odds";
const FONT_STYLE_ID = "colonist-assistant-document-font";

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

export const renderWinOdds = (
  analysis: DecisionAnalysis | undefined,
  state: TrackerState | undefined,
): void => {
  const existing = document.getElementById(ROOT_ID);
  if (!analysis || !state?.playerOrder.length) {
    existing?.remove();
    return;
  }
  const root = ensureRoot();
  const claimed = new Set<HTMLElement>();
  root.replaceChildren();
  for (const estimate of analysis.players) {
    const panel = findPlayerPanel(estimate.player, claimed);
    if (!panel) continue;
    claimed.add(panel);
    const rect = panel.getBoundingClientRect();
    const badge = document.createElement("span");
    badge.dataset.player = estimate.player;
    badge.textContent = `${Math.round(estimate.probability * 100)}% WIN`;
    badge.title = `${estimate.player}: ${Math.round(estimate.probability * 100)}% stabilized model estimate, not yet calibrated · ${estimate.etaTurns} turn ETA · ${estimate.confidence} hand-evidence confidence · ${analysis.model}`;
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
    root.append(badge);
  }
};

export const destroyWinOdds = (): void => {
  document.getElementById(ROOT_ID)?.remove();
};
