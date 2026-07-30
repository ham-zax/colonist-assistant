import type {
  ActiveTradeOffer,
} from "../core/placement";
import type {
  TradeVerdict,
} from "../core/trades";

const STYLE_ID = "colonist-assistant-trade-verdict-styles";
const VERDICT_CLASS = "ca-trade-verdict";

const ensureStyles = (): void => {
  if (document.getElementById(STYLE_ID)) return;
  const style = document.createElement("style");
  style.id = STYLE_ID;
  const fontUrl = chrome.runtime.getURL(
    "assets/fonts/ArchivoNarrow-Variable.ttf",
  );
  style.textContent = `
    @font-face {
      font-family: "Colonist Assistant Archivo";
      src: url("${fontUrl}") format("truetype");
      font-style: normal;
      font-weight: 400 700;
      font-display: swap;
    }
    .${VERDICT_CLASS} {
      display: grid;
      grid-template-columns: 8px minmax(58px, auto) minmax(0, 1fr);
      gap: 8px;
      align-items: center;
      min-height: 42px;
      margin-top: 5px;
      padding: 7px 10px;
      border-top: 1px solid #2b404e;
      border-bottom: 1px solid #2b404e;
      color: #f1f4ef;
      background: #101e28;
      font-family: "Colonist Assistant Archivo", ui-sans-serif, system-ui, sans-serif;
      line-height: 1.2;
      pointer-events: none;
    }
    .${VERDICT_CLASS}::before {
      width: 7px;
      height: 7px;
      border-radius: 50%;
      background: var(--ca-trade-verdict-color);
      content: "";
    }
    .${VERDICT_CLASS} strong {
      color: var(--ca-trade-verdict-color);
      font-size: 10.5px;
      font-weight: 800;
      letter-spacing: .075em;
    }
    .${VERDICT_CLASS} span {
      min-width: 0;
      overflow-wrap: anywhere;
      color: #c4d1d8;
      font-size: 11.5px;
      font-weight: 650;
      line-height: 1.25;
    }
    .${VERDICT_CLASS}[data-verdict="accept"] {
      --ca-trade-verdict-color: #7ad7a2;
    }
    .${VERDICT_CLASS}[data-verdict="counter"] {
      --ca-trade-verdict-color: #f1c84b;
    }
    .${VERDICT_CLASS}[data-verdict="decline"] {
      --ca-trade-verdict-color: #ef7c72;
    }
    @media (prefers-reduced-motion: reduce) {
      .${VERDICT_CLASS} { transition: none; }
    }
  `;
  (document.head ?? document.documentElement).append(style);
};

export const clearTradeVerdicts = (): void => {
  document
    .querySelectorAll(`.${VERDICT_CLASS}`)
    .forEach((element) => element.remove());
};

export const renderTradeVerdicts = (
  activeTrades: ActiveTradeOffer[],
  verdicts: Map<string, TradeVerdict>,
): void => {
  clearTradeVerdicts();
  if (!activeTrades.length || !verdicts.size) return;
  ensureStyles();
  const tradeContainers = [
    ...document.querySelectorAll<HTMLElement>(
      '[class*="gameTradeOffersWrapper-"] [class*="tradeContainer-"]',
    ),
  ].filter(
    (container, index, items) =>
      items.findIndex((candidate) => candidate === container) === index,
  );
  const unmatched = new Set(tradeContainers);
  for (const trade of activeTrades) {
    const verdict = verdicts.get(trade.id);
    const namedMatches = [...unmatched].filter((container) => {
      const text = container.textContent?.toLocaleLowerCase() ?? "";
      return [trade.creator, trade.tradeExecutor]
        .filter(Boolean)
        .some((player) => text.includes(player.toLocaleLowerCase()));
    });
    // Colonist does not currently expose its internal offer id in the DOM.
    // A unique player-name match is safe; a single remaining one-to-one pair
    // is also safe. With multiple ambiguous offers, omit the cosmetic badge
    // instead of attaching advice to the wrong transaction by array index.
    const container =
      namedMatches.length === 1
        ? namedMatches[0]
        : unmatched.size === 1 &&
            activeTrades.filter((candidate) => verdicts.has(candidate.id))
              .length === 1
          ? [...unmatched][0]
          : undefined;
    if (!verdict || !container) continue;
    unmatched.delete(container);
    const badge = document.createElement("div");
    badge.className = VERDICT_CLASS;
    badge.dataset.verdict = verdict.kind;
    badge.dataset.tradeId = verdict.tradeId;
    badge.setAttribute("role", "status");
    badge.setAttribute(
      "aria-label",
      `${verdict.label}: ${verdict.reason}`,
    );
    badge.title = verdict.detail;
    const label = document.createElement("strong");
    label.textContent = verdict.label;
    const reason = document.createElement("span");
    reason.textContent =
      verdict.kind === "counter" ? verdict.detail : verdict.reason;
    badge.append(label, reason);
    container.append(badge);
  }
};

export const destroyTradeVerdicts = (): void => {
  clearTradeVerdicts();
  document.getElementById(STYLE_ID)?.remove();
};
