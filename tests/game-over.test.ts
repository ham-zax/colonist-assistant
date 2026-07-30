import { describe, expect, it } from "vitest";

import { isTerminalGameHeading } from "../src/core/game-over";

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
