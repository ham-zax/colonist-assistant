const TERMINAL_HEADING =
  /^(?:victory|defeat|game over|well played|you won|you lost)[!.]*$/iu;

/**
 * Match only an actual end-of-game heading. Colonist keeps labels such as
 * "Victory Points" visible throughout play, so substring matching can stop
 * the executor and live benchmark many turns too early.
 */
export const isTerminalGameHeading = (
  value: string | null | undefined,
): boolean => TERMINAL_HEADING.test(value?.trim() ?? "");

const WON_THE_GAME = /won\s+the\s+game/i;

/**
 * Independent terminal signal: the raw "won the game" phrase appeared. This
 * stays true even when the named winner is outside the observed roster, so
 * terminal detection never depends on winner identity resolution.
 */
export const hasWonTheGamePhrase = (
  bodyText: string | null | undefined,
): boolean => (bodyText != null ? WON_THE_GAME.test(bodyText) : false);

const escapeRegExp = (value: string): string =>
  value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
// Colonist's own endgame headings can glue directly onto the winner name in
// flattened body text ("Victoryhamzax won the game!"). Those tokens are never
// player names (they already signal terminal state above), so a match glued
// to one of them still belongs to the roster name.
const GLUED_TERMINAL_HEADING = /(?:victory|defeat|game over|well played|you won|you lost)[!.]*\s*$/iu;

/**
 * Resolve a "X won the game" banner to a canonical observed roster name.
 *
 * The live banner is read from flattened `body.textContent`, so the text
 * immediately before "won the game" can be award chatter ("Longest Road
 * passed from A to hamzax (+2 VPs)"), trophy glyphs, or adjacent panels with
 * no separating whitespace. Returning that raw prefix as the winner poisons
 * downstream alias mapping (it resolves to unknown and the frame records no
 * winner). Instead, match the roster itself: longest roster name first so a
 * name that suffixes another player's name ("hamzax" vs "ax") resolves to the
 * full name, case-insensitively, returning canonical roster spelling.
 * Returns undefined when no roster name is present — never arbitrary text.
 */
export const resolveWonTheGameWinner = (
  bodyText: string | null | undefined,
  roster: readonly string[],
): string | undefined => {
  if (!bodyText || roster.length === 0) return undefined;
  const match = WON_THE_GAME.exec(bodyText);
  if (!match || match.index === undefined) return undefined;
  const windowSize = 120 + Math.max(0, ...roster.map((name) => name.length));
  const before = bodyText.slice(
    Math.max(0, match.index - windowSize),
    match.index,
  );
  const ordered = [...roster]
    .filter((name) => name.length > 0)
    .sort((left, right) => right.length - left.length);
  for (const name of ordered) {
    // Case-insensitive regex search keeps indices in the original string.
    // Lowercasing both sides instead would misalign indices for names whose
    // case mapping changes length (e.g. U+0130 İ folds to two characters).
    const occurrences = new RegExp(escapeRegExp(name), "giu");
    let found: RegExpExecArray | null;
    let nearest: string | undefined;
    while ((found = occurrences.exec(before)) !== null) {
      if (found[0].length === 0) {
        occurrences.lastIndex += 1;
        continue;
      }
      // The occurrence must run up to the banner (only glyphs, punctuation,
      // or whitespace between it and "won") and must not sit inside a longer
      // word (so roster "ax" never matches the tail of "hamzax"). The single
      // exception is glue to Colonist's own terminal heading, which is never
      // a player name. Later occurrences end nearer the banner and win.
      const end = found.index + found[0].length;
      const gap = before.slice(end);
      const prev = found.index === 0 ? "" : before[found.index - 1]!;
      const boundaryClean =
        prev === "" ||
        !/[\p{L}\p{N}_]/u.test(prev) ||
        GLUED_TERMINAL_HEADING.test(before.slice(0, found.index));
      if (!/[\p{L}\p{N}]/u.test(gap) && boundaryClean) {
        nearest = name;
      }
    }
    if (nearest !== undefined) return nearest;
  }
  return undefined;
};
