import { classifyIconSignature } from "../core/parser";
import type { ParsedLogSnapshot } from "../core/types";

const signatureForElement = (element: Element): string =>
  [
    element.getAttribute("src"),
    element.getAttribute("alt"),
    element.getAttribute("title"),
    element.getAttribute("aria-label"),
    element.getAttribute("data-tooltip-content"),
    element.getAttribute("class"),
    element.getAttribute("style"),
    element.querySelector("use")?.getAttribute("href"),
  ]
    .filter(Boolean)
    .join(" ");

const serializeNode = (node: Node): string => {
  if (node.nodeType === Node.TEXT_NODE) return node.nodeValue ?? "";
  if (!(node instanceof Element)) return "";
  const tag = node.tagName.toLowerCase();
  if (tag === "br") return " ";

  const token = classifyIconSignature(signatureForElement(node));
  if (token && (tag === "img" || tag === "svg" || !node.children.length)) {
    return ` :${token}: `;
  }
  return [...node.childNodes].map(serializeNode).join("");
};

const normalize = (value: string): string =>
  value.replace(/\u00a0/g, " ").replace(/\s+/g, " ").trim();

const readColor = (element: Element): string | undefined => {
  for (const candidate of element.querySelectorAll<HTMLElement>("[style*='color']")) {
    const color = candidate.style.color;
    if (color && color !== "inherit" && color !== "transparent") return color;
  }
  return undefined;
};

export const snapshotMessage = (
  element: Element,
  language: string,
): ParsedLogSnapshot | undefined => {
  const visibleText = normalize(element.textContent ?? "");
  const serialText = normalize(serializeNode(element));
  if (!visibleText && !serialText) return undefined;
  const rawIndex = element.getAttribute("data-index");
  const index = rawIndex !== null && Number.isFinite(Number(rawIndex)) ? Number(rawIndex) : undefined;
  return {
    ...(index !== undefined ? { index } : {}),
    visibleText,
    serialText,
    ...(readColor(element) ? { color: readColor(element) } : {}),
    language,
  };
};

const isNearChatInput = (element: Element): boolean => {
  let ancestor: Element | null = element;
  for (let depth = 0; ancestor && depth < 2; depth += 1, ancestor = ancestor.parentElement) {
    if (ancestor.querySelector("input[data-testid='game-chat-input']")) return true;
  }
  return false;
};

const GAME_TOKEN_PATTERN =
  /:(?:lumber|brick|wool|grain|ore|resource-back|dev-back|road|settlement|city|die-[1-6]|knight|monopoly|road-building|year-of-plenty):/;
const GAME_LOG_STARTUP_PATTERN =
  /^(?:happy settling!|bot is (?:selecting cards to discard|placing (?:a |an )?(?:road|settlement)) for\b)|\blist of commands:\s*\/help\b/iu;

const gameLogScore = (container: Element): number => {
  let score = 0;
  for (const entry of [...container.querySelectorAll("[data-index]")].slice(-16)) {
    const snapshot = snapshotMessage(entry, detectLanguage());
    if (!snapshot) continue;
    if (GAME_TOKEN_PATTERN.test(snapshot.serialText)) score += 3;
    if (GAME_LOG_STARTUP_PATTERN.test(snapshot.visibleText)) score += 2;
    if (
      /\b(?:placed|built|rolled|received|got|bought|stole|discarded|gave bank|took from bank)\b/iu.test(
        snapshot.visibleText,
      )
    ) {
      score += 1;
    }
  }
  return score;
};

export const findLogRoot = (): HTMLElement | undefined => {
  const legacy = document.querySelector("#game-log-text, [id*='game-log']");
  if (legacy) return legacy as HTMLElement;

  const virtualizers = [
    ...document.querySelectorAll<HTMLElement>(
      "[class*='virtualContainer'], [class*='virtual-container']",
    ),
  ].filter((container) => !isNearChatInput(container));
  return virtualizers
    .map((container) => ({ container, score: gameLogScore(container) }))
    .filter((candidate) => candidate.score > 0)
    .sort((left, right) => right.score - left.score)[0]?.container;
};

export const findMessageElements = (root: ParentNode = document): Element[] => {
  const indexed = [...root.querySelectorAll("[data-index]")].filter(
    (element) => !element.parentElement?.closest("[data-index]"),
  );
  if (indexed.length) return indexed;

  if (root instanceof Element) {
    return [...root.children].filter(
      (element) => !element.querySelector("[data-index]") && element.id !== "game-log-text",
    );
  }
  return [];
};

export const detectLanguage = (): string => {
  const htmlLanguage = document.documentElement.lang;
  if (htmlLanguage) return htmlLanguage;
  const prefix = location.pathname.split("/").filter(Boolean)[0];
  return prefix && /^[a-z]{2}(?:-[A-Z]{2})?$/.test(prefix) ? prefix : "en";
};

export const stableMessageId = (snapshot: ParsedLogSnapshot): string => {
  const fingerprint = hashString(`${snapshot.serialText}|${snapshot.visibleText}`);
  return snapshot.index === undefined ? `message:${fingerprint}` : `index:${snapshot.index}:${fingerprint}`;
};

export const hashString = (value: string): string => {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(36);
};
