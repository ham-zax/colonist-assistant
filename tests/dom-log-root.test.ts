// @vitest-environment jsdom

import { afterEach, describe, expect, it } from "vitest";
import { findLogRoot } from "../src/content/dom";

afterEach(() => {
  document.body.replaceChildren();
});

const indexedMessage = (index: number, text: string): HTMLDivElement => {
  const row = document.createElement("div");
  row.dataset.index = String(index);
  row.textContent = text;
  return row;
};

describe("game log root discovery", () => {
  it("attaches to a bot-game setup log before the first gameplay event", () => {
    const root = document.createElement("div");
    root.className = "virtualContainer-game-log-fixture";
    root.append(
      indexedMessage(0, "Happy settling!"),
      indexedMessage(1, "Bot is placing a settlement for Katey"),
    );
    document.body.append(root);

    expect(findLogRoot()).toBe(root);
  });
});
