import { describe, expect, it } from "vitest";

import {
  bumpManagerGeneration,
  isGameGenerationRollover,
  readLiveGameProgress,
  readManagerGeneration,
  updateLiveGameProgress,
  writeLiveGameProgress,
} from "../src/page/game-generation";

const memoryStorage = (): Pick<Storage, "getItem" | "setItem"> => {
  const values = new Map<string, string>();
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => {
      values.set(key, value);
    },
  };
};

describe("page manager generation", () => {
  it("survives a same-tab reload and remains monotonic across game replacement", () => {
    const storage = memoryStorage();

    expect(readManagerGeneration(storage)).toBe(1);
    expect(bumpManagerGeneration(storage, 1)).toBe(2);

    // A fresh bridge instance in the same tab must reuse the persisted game
    // generation instead of reverting the same live game to generation 1.
    expect(readManagerGeneration(storage)).toBe(2);
    expect(bumpManagerGeneration(storage, 2)).toBe(3);
    expect(readManagerGeneration(storage)).toBe(3);
  });

  it("detects rollover across game replacement even with intermediate asynchronous reset frames", () => {
    // Game 1 reaches turn 98 with 62 pieces
    let progress = updateLiveGameProgress(undefined, {
      completedTurns: 98,
      placedPieces: 62,
      initialPlacement: false,
      gameplayRollCount: 104,
      gameOver: false,
      playerRosterKey: "hamzax,Merras5380,Neda1311,Ruqaiyahhhhh",
      victoryTarget: 12,
    });
    expect(progress.maxCompletedTurns).toBe(98);
    expect(progress.maxPlacedPieces).toBe(62);
    expect(progress.hasSeenGameplay).toBe(true);

    // Intermediate frame 1: game resets turns to 0, but tile pieces haven't cleared yet (placedPieces = 62)
    const intermediateFrame = {
      completedTurns: 0,
      placedPieces: 62,
      initialPlacement: true,
      gameplayRollCount: 0,
      gameOver: false,
      playerRosterKey: "hamzax,Merras5380,Ruqaiyahhhhh",
      victoryTarget: 13,
    };
    expect(isGameGenerationRollover(progress, intermediateFrame)).toBe(true);

    // Even if intermediate frame was a partial 4P snapshot where only turns dropped:
    const turnDropOnly = {
      completedTurns: 0,
      placedPieces: 62,
      initialPlacement: false,
      gameplayRollCount: undefined,
      gameOver: false,
      playerRosterKey: "hamzax,Merras5380,Neda1311,Ruqaiyahhhhh",
      victoryTarget: 12,
    };
    expect(isGameGenerationRollover(progress, turnDropOnly)).toBe(true);

    // New 3P game in setup (from live report)
    const newGameSetup = {
      completedTurns: 1,
      placedPieces: 2,
      initialPlacement: true,
      gameplayRollCount: 0,
      gameOver: false,
      playerRosterKey: "hamzax,Merras5380,Ruqaiyahhhhh",
      victoryTarget: 13,
    };
    expect(isGameGenerationRollover(progress, newGameSetup)).toBe(true);
  });

  it("detects game over transition followed by new game", () => {
    let progress = updateLiveGameProgress(undefined, {
      completedTurns: 40,
      placedPieces: 30,
      initialPlacement: false,
      gameplayRollCount: 30,
      gameOver: true,
      playerRosterKey: "Alice,Bob",
      victoryTarget: 10,
    });
    expect(progress.hasSeenGameOver).toBe(true);

    // New game begins
    expect(
      isGameGenerationRollover(progress, {
        completedTurns: 0,
        placedPieces: 1,
        initialPlacement: true,
        gameplayRollCount: 0,
        gameOver: false,
        playerRosterKey: "Alice,Bob",
        victoryTarget: 10,
      }),
    ).toBe(true);
  });

  it("does not trigger rollover during ordinary ongoing game progress", () => {
    let progress = updateLiveGameProgress(undefined, {
      completedTurns: 0,
      placedPieces: 1,
      initialPlacement: true,
      gameplayRollCount: 0,
      gameOver: false,
      playerRosterKey: "Alice,Bob",
      victoryTarget: 10,
    });

    // Player 1 places road (placedPieces: 2, completedTurns: 1)
    const turn1 = {
      completedTurns: 1,
      placedPieces: 2,
      initialPlacement: true,
      gameplayRollCount: 0,
      gameOver: false,
      playerRosterKey: "Alice,Bob",
      victoryTarget: 10,
    };
    expect(isGameGenerationRollover(progress, turn1)).toBe(false);
    progress = updateLiveGameProgress(progress, turn1);

    // Player 2 places settlement and road
    const turn2 = {
      completedTurns: 3,
      placedPieces: 4,
      initialPlacement: true,
      gameplayRollCount: 0,
      gameOver: false,
      playerRosterKey: "Alice,Bob",
      victoryTarget: 10,
    };
    expect(isGameGenerationRollover(progress, turn2)).toBe(false);
  });

  it("persists and restores live game progress in storage", () => {
    const storage = memoryStorage();
    expect(readLiveGameProgress(storage)).toBeUndefined();

    const progress = {
      maxCompletedTurns: 98,
      maxPlacedPieces: 62,
      hasSeenGameplay: true,
      hasSeenGameOver: true,
      playerRosterKey: "Alice,Bob",
      victoryTarget: 10,
    };
    writeLiveGameProgress(storage, progress);
    expect(readLiveGameProgress(storage)).toEqual(progress);
  });

  it("does not trigger rollover when a player leaves or roster changes midgame", () => {
    let progress = updateLiveGameProgress(undefined, {
      completedTurns: 15,
      placedPieces: 20,
      initialPlacement: false,
      gameplayRollCount: 15,
      gameOver: false,
      playerRosterKey: "Alice,Bob,Carol,Dave",
      victoryTarget: 10,
    });
    expect(progress.hasSeenGameplay).toBe(true);

    // Dave disconnects/leaves midgame; roster changes to Alice,Bob,Carol
    const midgamePlayerDrop = {
      completedTurns: 16,
      placedPieces: 20,
      initialPlacement: false,
      gameplayRollCount: 16,
      gameOver: false,
      playerRosterKey: "Alice,Bob,Carol",
      victoryTarget: 10,
    };
    expect(isGameGenerationRollover(progress, midgamePlayerDrop)).toBe(false);
  });

  it("does not trigger rollover if initialPlacement briefly glitches to true during midgame", () => {
    let progress = updateLiveGameProgress(undefined, {
      completedTurns: 18,
      placedPieces: 24,
      initialPlacement: false,
      gameplayRollCount: 18,
      gameOver: false,
      playerRosterKey: "Alice,Bob",
      victoryTarget: 10,
    });

    // Transient initialPlacement flip during midgame
    const midgameGlitch = {
      completedTurns: 18,
      placedPieces: 24,
      initialPlacement: true,
      gameplayRollCount: 0,
      gameOver: false,
      playerRosterKey: "Alice,Bob",
      victoryTarget: 10,
    };
    expect(isGameGenerationRollover(progress, midgameGlitch)).toBe(false);
  });

  it("survives midgame page reload without bumping, but detects new game in same room after reload", () => {
    const storage = memoryStorage();
    // Simulate Game 1 reached turn 25
    const game1Progress = {
      maxCompletedTurns: 25,
      maxPlacedPieces: 30,
      hasSeenGameplay: true,
      hasSeenGameOver: false,
      playerRosterKey: "Alice,Bob,Carol,Dave",
      victoryTarget: 10,
    };
    writeLiveGameProgress(storage, game1Progress);

    // Tab reloads midgame; progress is restored from storage
    const restored = readLiveGameProgress(storage);
    expect(restored).toEqual(game1Progress);

    // Midgame state re-hydrates from server at turn 25 -> must NOT rollover
    const rehydratedMidgame = {
      completedTurns: 25,
      placedPieces: 30,
      initialPlacement: false,
      gameplayRollCount: 25,
      gameOver: false,
      playerRosterKey: "Alice,Bob,Carol,Dave",
      victoryTarget: 10,
    };
    expect(isGameGenerationRollover(restored, rehydratedMidgame)).toBe(false);

    // In contrast, if a new game starts in setup in the same room -> MUST rollover
    const newGameInSameRoom = {
      completedTurns: 0,
      placedPieces: 0,
      initialPlacement: true,
      gameplayRollCount: 0,
      gameOver: false,
      playerRosterKey: "Alice,Bob,Carol,Dave",
      victoryTarget: 10,
    };
    expect(isGameGenerationRollover(restored, newGameInSameRoom)).toBe(true);
  });

  it("does not overwrite resolved roster with empty string or unresolved placeholders", () => {
    let progress = updateLiveGameProgress(undefined, {
      completedTurns: 5,
      placedPieces: 10,
      initialPlacement: false,
      gameplayRollCount: 5,
      gameOver: false,
      playerRosterKey: "Alice,Bob",
      victoryTarget: 10,
    });
    expect(progress.playerRosterKey).toBe("Alice,Bob");

    // Frame with empty roster key
    progress = updateLiveGameProgress(progress, {
      completedTurns: 6,
      placedPieces: 11,
      initialPlacement: false,
      gameplayRollCount: 6,
      gameOver: false,
      playerRosterKey: "",
      victoryTarget: 10,
    });
    expect(progress.playerRosterKey).toBe("Alice,Bob");

    // Frame with unresolved placeholders
    progress = updateLiveGameProgress(progress, {
      completedTurns: 7,
      placedPieces: 12,
      initialPlacement: false,
      gameplayRollCount: 7,
      gameOver: false,
      playerRosterKey: "Player 1,Player 2",
      victoryTarget: 10,
    });
    expect(progress.playerRosterKey).toBe("Alice,Bob");
  });

  it("does not infer a new game from midgame roster changes", () => {
    const progress = updateLiveGameProgress(undefined, {
      completedTurns: 20,
      placedPieces: 25,
      initialPlacement: false,
      gameplayRollCount: 20,
      gameOver: false,
      playerRosterKey: "Alice,Bob,Carol,Dave",
      victoryTarget: 10,
    });

    // Names alone cannot distinguish replacement players from a new game.
    const novelRosterFrame = {
      completedTurns: 15,
      placedPieces: 20,
      initialPlacement: false,
      gameplayRollCount: 15,
      gameOver: false,
      playerRosterKey: "Alice,Bob,Eve,Frank",
      victoryTarget: 10,
    };
    expect(isGameGenerationRollover(progress, novelRosterFrame)).toBe(false);

    // Midgame departure alone (subset, no novel players) must NOT rollover
    const departureFrame = {
      completedTurns: 21,
      placedPieces: 26,
      initialPlacement: false,
      gameplayRollCount: 21,
      gameOver: false,
      playerRosterKey: "Alice,Bob,Carol",
      victoryTarget: 10,
    };
    expect(isGameGenerationRollover(progress, departureFrame)).toBe(false);
  });

  it("preserves generation across stale midgame turn counts", () => {
    const progress = updateLiveGameProgress(undefined, {
      completedTurns: 50,
      placedPieces: 40,
      initialPlacement: false,
      gameplayRollCount: 50,
      gameOver: false,
      playerRosterKey: "Alice,Bob",
      victoryTarget: 10,
    });

    // Turn count drops by 5 turns midgame (e.g. new game joined at turn 45)
    const turnDropFrame = {
      completedTurns: 45,
      placedPieces: 35,
      initialPlacement: false,
      gameplayRollCount: 45,
      gameOver: false,
      playerRosterKey: "Alice,Bob",
      victoryTarget: 10,
    };
    expect(isGameGenerationRollover(progress, turnDropFrame)).toBe(false);

    // Minor fluctuation / 1-turn skew does NOT trigger rollover
    const sameTurnFrame = {
      completedTurns: 49,
      placedPieces: 39,
      initialPlacement: false,
      gameplayRollCount: 49,
      gameOver: false,
      playerRosterKey: "Alice,Bob",
      victoryTarget: 10,
    };
    expect(isGameGenerationRollover(progress, sameTurnFrame)).toBe(false);
  });
});
