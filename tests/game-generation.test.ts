import { describe, expect, it } from "vitest";

import {
  bumpManagerGeneration,
  readManagerGeneration,
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
});
