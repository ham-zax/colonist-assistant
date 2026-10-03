import { describe, expect, it } from "vitest";

import { hasWonTheGamePhrase, isTerminalGameHeading, resolveWonTheGameWinner } from "../src/core/game-over";

describe("terminal game heading detection", () => {
  it.each([
    "Victory",
    "Victory!",
    "Victory!!!",
    "Defeat",
    "Defeat!!",
    "Game Over",
    "Well Played!",
    "You Won",
    "You Lost!",
  ])("accepts the terminal heading %s", (heading) => {
    expect(isTerminalGameHeading(heading)).toBe(true);
  });

  it.each([
    "Victory Points",
    "Victory Points!!!",
    "Public Victory Points",
    "Public Victory Points!",
    "How victory points work",
    "Largest Army",
    "",
  ])("rejects the in-game heading %s", (heading) => {
    expect(isTerminalGameHeading(heading)).toBe(false);
  });
});

describe("won-the-game winner resolution", () => {
  it("resolves the clean banner to the canonical roster spelling", () => {
    expect(
      resolveWonTheGameWinner("hamzax won the game!", ["DigitEffect", "hamzax"]),
    ).toBe("hamzax");
  });

  it("ignores preceding award chatter from the live log shape", () => {
    expect(
      resolveWonTheGameWinner(
        "Longest Road passed from DigitEffect to hamzax (+2 VPs)hamzax won the game!",
        ["DigitEffect", "hamzax"],
      ),
    ).toBe("hamzax");
  });

  it("tolerates trophy glyphs and flattened DOM without separators", () => {
    expect(
      resolveWonTheGameWinner("🏆hamzax won the game!", ["hamzax"]),
    ).toBe("hamzax");
    expect(
      resolveWonTheGameWinner("Victoryhamzax won the game!", ["hamzax"]),
    ).toBe("hamzax");
  });

  it("normalizes case but returns canonical roster spelling", () => {
    expect(
      resolveWonTheGameWinner("HAMZAX WON THE GAME!", ["hamzax"]),
    ).toBe("hamzax");
  });

  it("prefers the full name when a roster name suffixes another player", () => {
    expect(
      resolveWonTheGameWinner("hamzax won the game!", ["ax", "hamzax"]),
    ).toBe("hamzax");
    expect(
      resolveWonTheGameWinner("ax won the game!", ["ax", "hamzax"]),
    ).toBe("ax");
    // A stranger's longer name never resolves to its roster-name tail.
    expect(
      resolveWonTheGameWinner("hamzax won the game!", ["ax"]),
    ).toBeUndefined();
    // Glue to Colonist's own terminal heading still resolves.
    expect(
      resolveWonTheGameWinner("Victoryax won the game!", ["ax"]),
    ).toBe("ax");
  });

  it("resolves long and special-character names", () => {
    const name = "xX_Hamza-99_the-Settler!";
    expect(
      resolveWonTheGameWinner(`🏆 ${name} won the game!`, ["Codi", name]),
    ).toBe(name);
  });

  it("detects the raw phrase independently of roster resolution", () => {
    expect(hasWonTheGamePhrase("🏆Stranger won the game!")).toBe(true);
    expect(hasWonTheGamePhrase("hamzax won the game!")).toBe(true);
    expect(hasWonTheGamePhrase("no banner here")).toBe(false);
    expect(hasWonTheGamePhrase(null)).toBe(false);
  });

  it("never returns arbitrary preceding text for unknown names", () => {
    expect(
      resolveWonTheGameWinner(
        "Longest Road passed from DigitEffect to Stranger (+2 VPs)Stranger won the game!",
        ["DigitEffect", "hamzax"],
      ),
    ).toBeUndefined();
    expect(resolveWonTheGameWinner("no banner here", ["hamzax"])).toBeUndefined();
    expect(resolveWonTheGameWinner("hamzax won the game!", [])).toBeUndefined();
    expect(resolveWonTheGameWinner(null, ["hamzax"])).toBeUndefined();
  });
});
