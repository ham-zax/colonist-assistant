import localeTemplates from "../generated/log-templates.json";
import {
  BUILD_COSTS,
  emptyResources,
  RESOURCE_ORDER,
  resourceTotal,
  type Resource,
  type ResourceVector,
} from "./resources";
import type {
  DevCardKind,
  ParsedLogSnapshot,
  ParseResult,
  TrackerEvent,
} from "./types";

type LocaleName = keyof typeof localeTemplates;
type TemplateName = keyof (typeof localeTemplates)["en"];
type Groups = Record<string, string | undefined>;

const TOKEN_PATTERN = ":[a-z0-9_-]+:";
const PLAYER_FIELDS = new Set([
  "playerName",
  "playerNameThief",
  "playerNameVictim",
  "acceptingPlayerName",
]);
const NUMBER_FIELDS = new Set(["amountStolen", "count", "bankCount"]);
const regexCache = new Map<string, RegExp>();

const escapeRegex = (value: string): string =>
  value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

const decodeTemplateText = (template: string): string =>
  template
    .replace(/<[^>]+>/g, "")
    .replaceAll("&nbsp;", " ")
    .replaceAll("&#39;", "'")
    .replaceAll("&amp;", "&");

const compileTemplate = (locale: string, name: string, template: string): RegExp => {
  const cacheKey = `${locale}:${name}`;
  const cached = regexCache.get(cacheKey);
  if (cached) return cached;

  const source = decodeTemplateText(template);
  let pattern = "^\\s*";
  let cursor = 0;
  const placeholders = /\{\{(\w+)(?:,[^}]*)?\}\}/g;
  let match: RegExpExecArray | null;
  while ((match = placeholders.exec(source))) {
    const literal = source.slice(cursor, match.index);
    pattern += escapeRegex(literal).replace(/\s+/g, "\\s*");
    const field = match[1]!;
    if (PLAYER_FIELDS.has(field)) {
      pattern += `(?<${field}>.+?)`;
    } else if (NUMBER_FIELDS.has(field)) {
      pattern += `(?<${field}>\\d+)`;
    } else {
      pattern += `(?<${field}>.*?)`;
    }
    cursor = match.index + match[0].length;
  }
  pattern += escapeRegex(source.slice(cursor)).replace(/\s+/g, "\\s*");
  pattern += "\\s*$";
  const compiled = new RegExp(pattern, "iu");
  regexCache.set(cacheKey, compiled);
  return compiled;
};

const normalizeLanguage = (language?: string): LocaleName => {
  const requested = (language || "en").replace("_", "-").toLowerCase();
  const locales = Object.keys(localeTemplates) as LocaleName[];
  const exact = locales.find((locale) => locale.toLowerCase() === requested);
  if (exact) return exact;
  const base = requested.split("-")[0];
  return locales.find((locale) => locale.toLowerCase() === base) ?? "en";
};

const matchTemplate = (
  snapshot: ParsedLogSnapshot,
  name: TemplateName,
): Groups | undefined => {
  const locale = normalizeLanguage(snapshot.language);
  const pack = localeTemplates[locale] as Partial<Record<TemplateName, string>>;
  const fallback = localeTemplates.en as Record<TemplateName, string>;
  const template = pack[name] ?? fallback[name];
  if (!template) return undefined;
  const match = compileTemplate(locale, name, template).exec(snapshot.serialText);
  return match?.groups as Groups | undefined;
};

const normalizeName = (value?: string): string =>
  (value ?? "").replace(new RegExp(TOKEN_PATTERN, "giu"), "").replace(/\s+/g, " ").trim();

const tokenValues = (value: string): string[] =>
  [...value.matchAll(/:([a-z0-9_-]+):/giu)].map((match) => match[1]!.toLowerCase());

export const classifyIconSignature = (signature: string): string | undefined => {
  const value = signature.toLowerCase().replace(/%20/g, "_");
  const rules: Array<[RegExp, string]> = [
    [/(?:card|resource)[_-]?(?:lumber|wood)|(?:lumber|wood)[_-]card/, "lumber"],
    [/(?:card|resource)[_-]?(?:brick|clay)|(?:brick|clay)[_-]card/, "brick"],
    [/(?:card|resource)[_-]?(?:wool|sheep)|(?:wool|sheep)[_-]card/, "wool"],
    [/(?:card|resource)[_-]?(?:grain|wheat)|(?:grain|wheat)[_-]card/, "grain"],
    [/(?:card|resource)[_-]?(?:ore|stone)|(?:ore|stone)[_-]card/, "ore"],
    [/(?:resource.*back|back.*resource|rescard.*back|card_resourceback)/, "resource-back"],
    [/(?:development|devcard|dev_card).*back|back.*(?:development|devcard)/, "dev-back"],
    [/(?:year[_-]?of[_-]?plenty|plenty)/, "year-of-plenty"],
    [/(?:road[_-]?building)/, "road-building"],
    [/(?:monopoly)/, "monopoly"],
    [/(?:victory[_-]?point|chapel|library|market|university)/, "victory-point"],
    [/(?:knight|soldier)/, "knight"],
    [/(?:settlement)/, "settlement"],
    [/(?:city)(?!.*improvement)/, "city"],
    [/(?:road)(?!.*building)/, "road"],
  ];
  for (const [pattern, result] of rules) {
    if (pattern.test(value)) return result;
  }
  const dice = value.match(/(?:dice|die)[_-]?(?:white[_-]?|red[_-]?)?([1-6])(?:\D|$)/);
  if (dice?.[1]) return `die-${dice[1]}`;
  return undefined;
};

const resourceFromToken = (token: string): Resource | undefined =>
  RESOURCE_ORDER.find((resource) => token === resource);

const vectorFromText = (value: string): ResourceVector => {
  const vector = emptyResources();
  for (const token of tokenValues(value)) {
    const resource = resourceFromToken(token);
    if (resource) vector[resource] += 1;
  }
  return vector;
};

const firstResource = (value: string): Resource | undefined => {
  for (const token of tokenValues(value)) {
    const resource = resourceFromToken(token);
    if (resource) return resource;
  }
  const lower = value.toLowerCase();
  const textRules: Array<[RegExp, Resource]> = [
    [/\b(?:lumber|wood)\b/, "lumber"],
    [/\b(?:brick|clay)\b/, "brick"],
    [/\b(?:wool|sheep)\b/, "wool"],
    [/\b(?:grain|wheat)\b/, "grain"],
    [/\b(?:ore|stone)\b/, "ore"],
  ];
  return textRules.find(([pattern]) => pattern.test(lower))?.[1];
};

const countToken = (value: string, token: string): number =>
  tokenValues(value).filter((candidate) => candidate === token).length;

const cardFromText = (value: string): DevCardKind => {
  const lower = value.toLowerCase();
  if (lower.includes("road-building") || /road[\s_-]*building/.test(lower)) {
    return "road-building";
  }
  if (lower.includes("year-of-plenty") || /year[\s_-]*of[\s_-]*plenty/.test(lower)) {
    return "year-of-plenty";
  }
  if (lower.includes("monopoly")) return "monopoly";
  if (lower.includes("victory-point") || /victory[\s_-]*point/.test(lower)) {
    return "victory-point";
  }
  if (lower.includes("knight") || lower.includes("soldier")) return "knight";
  return "unknown";
};

const colorize = <T extends TrackerEvent>(event: T, snapshot: ParsedLogSnapshot): T =>
  ({ ...event, ...(snapshot.color ? { color: snapshot.color } : {}) }) as T;

const exact = (event: TrackerEvent): ParseResult => ({ event, confidence: "exact" });
const uncertain = (event: TrackerEvent): ParseResult => ({ event, confidence: "uncertain" });

const gainFrom = (
  snapshot: ParsedLogSnapshot,
  groups: Groups,
  field: string,
  reason: Extract<TrackerEvent, { type: "gain" }>["reason"],
): ParseResult | undefined => {
  const player = normalizeName(groups.playerName);
  const cards = vectorFromText(groups[field] ?? snapshot.serialText);
  if (!player || !resourceTotal(cards)) return undefined;
  return exact(colorize({ type: "gain", player, cards, reason }, snapshot));
};

const parseDice = (value: string): [number, number] | undefined => {
  const dice = tokenValues(value)
    .filter((token) => token.startsWith("die-"))
    .map((token) => Number(token.slice(4)))
    .filter((die) => die >= 1 && die <= 6);
  return dice.length >= 2 ? [dice[0]!, dice[1]!] : undefined;
};

export const parseLogSnapshot = (snapshot: ParsedLogSnapshot): ParseResult | undefined => {
  const text = snapshot.serialText;

  let groups = matchTemplate(snapshot, "stolenResourceThief");
  if (groups) {
    const victim = normalizeName(groups.playerName);
    const cardsText = groups.cardString ?? text;
    const cards = vectorFromText(cardsText);
    if (victim && resourceTotal(cards)) {
      return exact(colorize({ type: "transfer", from: victim, to: "You", cards, reason: "robbery" }, snapshot));
    }
  }

  groups = matchTemplate(snapshot, "stolenResourceVictim");
  if (groups) {
    const thief = normalizeName(groups.playerName);
    const cardsText = groups.cardString ?? text;
    const cards = vectorFromText(cardsText);
    if (thief && resourceTotal(cards)) {
      return exact(colorize({ type: "transfer", from: "You", to: thief, cards, reason: "robbery" }, snapshot));
    }
  }

  groups = matchTemplate(snapshot, "stolenResourceClosed");
  if (groups) {
    const thief = normalizeName(groups.playerNameThief);
    const victim = normalizeName(groups.playerNameVictim);
    const cardsText = groups.cardString ?? text;
    const cards = vectorFromText(cardsText);
    if (thief && victim && resourceTotal(cards)) {
      return exact(colorize({ type: "transfer", from: victim, to: thief, cards, reason: "robbery" }, snapshot));
    }
    const unknownCount = countToken(cardsText, "resource-back") || 1;
    if (thief && victim) {
      return uncertain(
        colorize(
          { type: "unknown-transfer", from: victim, to: thief, count: unknownCount },
          snapshot,
        ),
      );
    }
  }

  groups = matchTemplate(snapshot, "playerTradedWithBank");
  if (groups) {
    const player = normalizeName(groups.playerName);
    const given = vectorFromText(groups.givenCardString ?? "");
    const received = vectorFromText(groups.receivedCardString ?? "");
    if (player && resourceTotal(given) && resourceTotal(received)) {
      return exact(colorize({ type: "trade", player, given, received, bank: true }, snapshot));
    }
  }

  groups = matchTemplate(snapshot, "playerTradedWithPlayer");
  if (groups) {
    const player = normalizeName(groups.playerName);
    const acceptingPlayer = normalizeName(groups.acceptingPlayerName);
    const given = vectorFromText(groups.givenCardString ?? "");
    const received = vectorFromText(groups.receivedCardString ?? "");
    if (player && acceptingPlayer && resourceTotal(given) && resourceTotal(received)) {
      return exact(
        colorize(
          { type: "trade", player, acceptingPlayer, given, received, bank: false },
          snapshot,
        ),
      );
    }
  }

  groups = matchTemplate(snapshot, "playerDiscarded");
  if (groups) {
    const player = normalizeName(groups.playerName);
    const cardText = groups.cardString ?? text;
    const cards = vectorFromText(cardText);
    if (player && resourceTotal(cards)) {
      return exact(
        colorize(
          { type: "trade", player, given: cards, received: emptyResources(), bank: true },
          snapshot,
        ),
      );
    }
    const count = countToken(cardText, "resource-back");
    if (player && count) {
      return uncertain(colorize({ type: "unknown-discard", player, count }, snapshot));
    }
  }

  groups = matchTemplate(snapshot, "playerReceivedStartingResources");
  if (groups) {
    const result = gainFrom(snapshot, groups, "cardsString", "starting");
    if (result) return result;
  }

  groups = matchTemplate(snapshot, "playerGotCards");
  if (groups) {
    const result = gainFrom(snapshot, groups, "cardsString", "production");
    if (result) return result;
  }

  groups = matchTemplate(snapshot, "playerSelectedFromGoldTile");
  if (groups) {
    const result = gainFrom(snapshot, groups, "cardsString", "gold");
    if (result) return result;
  }

  groups = matchTemplate(snapshot, "playerSelectedFromAqueduct");
  if (groups) {
    const result = gainFrom(snapshot, groups, "cardsString", "other");
    if (result) return result;
  }

  groups = matchTemplate(snapshot, "playerTookFromBank");
  if (groups) {
    const result = gainFrom(snapshot, groups, "cardString", "bank");
    if (result) return result;
  }

  groups = matchTemplate(snapshot, "playerReceived");
  if (groups) {
    const result = gainFrom(snapshot, groups, "pieceString", "other");
    if (result) return result;
  }

  groups = matchTemplate(snapshot, "playerBoughtCard");
  if (groups) {
    const player = normalizeName(groups.playerName);
    if (player && (countToken(text, "dev-back") || /bought|comprou|compró|acheté/iu.test(snapshot.visibleText))) {
      return exact(colorize({ type: "buy-dev", player }, snapshot));
    }
  }

  groups = matchTemplate(snapshot, "playerBuiltPiece");
  if (groups) {
    const player = normalizeName(groups.playerName);
    const pieceText = groups.pieceString ?? text;
    const piece = tokenValues(pieceText).find((token) =>
      ["road", "settlement", "city"].includes(token),
    ) as "road" | "settlement" | "city" | undefined;
    if (player && piece) {
      const cost = { ...emptyResources(), ...BUILD_COSTS[piece] };
      return exact(colorize({ type: "spend", player, cost, reason: piece }, snapshot));
    }
  }

  groups = matchTemplate(snapshot, "playerPlacedPiece");
  if (groups) {
    const player = normalizeName(groups.playerName);
    const pieceText = groups.pieceString ?? text;
    const piece = tokenValues(pieceText).find((token) =>
      ["road", "settlement", "city"].includes(token),
    ) as "road" | "settlement" | "city" | undefined;
    if (player && piece) {
      // Opening placements are free, but they still need to update public
      // piece counts and placement synchronization.
      return exact(
        colorize(
          {
            type: "spend",
            player,
            cost: emptyResources(),
            reason: piece,
          },
          snapshot,
        ),
      );
    }
    if (player) return exact(colorize({ type: "discover", player }, snapshot));
  }

  groups = matchTemplate(snapshot, "playerPlayedDevelopmentCard");
  if (groups) {
    const player = normalizeName(groups.playerName);
    if (player) {
      return exact(
        colorize(
          { type: "play-dev", player, card: cardFromText(groups.cardImage ?? text) },
          snapshot,
        ),
      );
    }
  }

  groups = matchTemplate(snapshot, "playerStoleUsingMonopoly");
  if (groups) {
    const player = normalizeName(groups.playerName);
    const resource = firstResource(groups.cardString ?? text);
    const amount = Number(groups.amountStolen);
    if (player && resource) {
      return exact(
        colorize(
          {
            type: "monopoly",
            player,
            resource,
            ...(Number.isFinite(amount) ? { amount } : {}),
          },
          snapshot,
        ),
      );
    }
  }

  groups = matchTemplate(snapshot, "playerRolledDice");
  if (groups) {
    const player = normalizeName(groups.playerName);
    const dice = parseDice(groups.diceString ?? text);
    if (player) {
      return exact(colorize({ type: "roll", player, ...(dice ? { dice } : {}) }, snapshot));
    }
  }

  return parseEnglishFallback(snapshot);
};

const parseEnglishFallback = (
  snapshot: ParsedLogSnapshot,
): ParseResult | undefined => {
  const visible = snapshot.visibleText.replace(/\s+/g, " ").trim();
  const serial = snapshot.serialText;
  let match = visible.match(/^(.+?)\s+(?:got|received starting resources)\b/i);
  if (match) {
    const cards = vectorFromText(serial);
    if (resourceTotal(cards)) {
      return exact(
        colorize(
          {
            type: "gain",
            player: normalizeName(match[1]),
            cards,
            reason: visible.includes("starting") ? "starting" : "production",
          },
          snapshot,
        ),
      );
    }
  }
  match = visible.match(/^(.+?)\s+built a\s+(road|settlement|city)\b/i);
  if (match) {
    const reason = match[2]!.toLowerCase() as "road" | "settlement" | "city";
    return exact(
      colorize(
        {
          type: "spend",
          player: normalizeName(match[1]),
          cost: { ...emptyResources(), ...BUILD_COSTS[reason] },
          reason,
        },
        snapshot,
      ),
    );
  }
  match = visible.match(/^(.+?)\s+bought\b/i);
  if (match && countToken(serial, "dev-back")) {
    return exact(colorize({ type: "buy-dev", player: normalizeName(match[1]) }, snapshot));
  }
  match = visible.match(/^(.+?)\s+rolled\b/i);
  if (match) {
    const dice = parseDice(serial);
    return exact(
      colorize(
        { type: "roll", player: normalizeName(match[1]), ...(dice ? { dice } : {}) },
        snapshot,
      ),
    );
  }
  return undefined;
};

export const templatesForLanguage = (language?: string): Record<string, string> =>
  localeTemplates[normalizeLanguage(language)] as Record<string, string>;
