// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";

import { emptyResources } from "../src/core/resources";
import { renderTradeVerdicts } from "../src/content/trade-verdicts";

afterEach(() => {
  document.body.replaceChildren();
  document.head
    .querySelector("#colonist-assistant-trade-verdict-styles")
    ?.remove();
  vi.unstubAllGlobals();
});

describe("trade verdict identity mapping", () => {
  it("matches simultaneous offers by player identity rather than object order", () => {
    vi.stubGlobal("chrome", {
      runtime: { getURL: (path: string) => `extension://${path}` },
    });
    const wrapper = document.createElement("div");
    wrapper.className = "gameTradeOffersWrapper-fixture";
    const alice = document.createElement("div");
    alice.className = "tradeContainer-fixture";
    alice.textContent = "Alice offers";
    const bob = document.createElement("div");
    bob.className = "tradeContainer-fixture";
    bob.textContent = "Bob offers";
    wrapper.append(alice, bob);
    document.body.append(wrapper);
    const vector = emptyResources();
    const trades = [
      {
        id: "bob-offer",
        creator: "Bob",
        tradeExecutor: "Bob",
        give: vector,
        receive: vector,
        incoming: true,
        counterOffer: false,
        canAccept: true,
      },
      {
        id: "alice-offer",
        creator: "Alice",
        tradeExecutor: "Alice",
        give: vector,
        receive: vector,
        incoming: true,
        counterOffer: false,
        canAccept: true,
      },
    ];

    renderTradeVerdicts(
      trades,
      new Map([
        [
          "bob-offer",
          {
            tradeId: "bob-offer",
            kind: "accept",
            score: 1,
            label: "ACCEPT",
            reason: "Bob result",
            detail: "Bob detail",
          },
        ],
        [
          "alice-offer",
          {
            tradeId: "alice-offer",
            kind: "decline",
            score: -1,
            label: "DECLINE",
            reason: "Alice result",
            detail: "Alice detail",
          },
        ],
      ]),
    );

    expect(
      alice.querySelector<HTMLElement>(".ca-trade-verdict")?.dataset.tradeId,
    ).toBe("alice-offer");
    expect(
      bob.querySelector<HTMLElement>(".ca-trade-verdict")?.dataset.tradeId,
    ).toBe("bob-offer");
  });

  it("omits badges when multiple containers cannot be identified safely", () => {
    vi.stubGlobal("chrome", {
      runtime: { getURL: (path: string) => `extension://${path}` },
    });
    const wrapper = document.createElement("div");
    wrapper.className = "gameTradeOffersWrapper-fixture";
    for (let index = 0; index < 2; index += 1) {
      const container = document.createElement("div");
      container.className = "tradeContainer-fixture";
      container.textContent = "Trade offer";
      wrapper.append(container);
    }
    document.body.append(wrapper);
    const vector = emptyResources();

    renderTradeVerdicts(
      [
        {
          id: "one",
          creator: "Alice",
          tradeExecutor: "Alice",
          give: vector,
          receive: vector,
          incoming: true,
          counterOffer: false,
          canAccept: true,
        },
        {
          id: "two",
          creator: "Bob",
          tradeExecutor: "Bob",
          give: vector,
          receive: vector,
          incoming: true,
          counterOffer: false,
          canAccept: true,
        },
      ],
      new Map([
        [
          "one",
          {
            tradeId: "one",
            kind: "accept",
            score: 1,
            label: "ACCEPT",
            reason: "one",
            detail: "one",
          },
        ],
        [
          "two",
          {
            tradeId: "two",
            kind: "decline",
            score: -1,
            label: "DECLINE",
            reason: "two",
            detail: "two",
          },
        ],
      ]),
    );

    expect(document.querySelector(".ca-trade-verdict")).toBeNull();
  });
});
