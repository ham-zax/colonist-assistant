import { describe, expect, it } from "vitest";

import { explainDeepSearchDecision } from "../src/core/engine";
import { actionStats, makeDeepSearch } from "../scripts/ui-preview/fixtures";

describe("player-facing decision rationale", () => {
  const city = { kind: "build-city", targetId: "v2" } as const;
  const dev = { kind: "buy-development" } as const;

  it("describes a close search gap without raw values", () => {
    const rationale = explainDeepSearchDecision(
      makeDeepSearch(city, [actionStats(city, 0.412), actionStats(dev, 0.406)]),
    );
    expect(rationale?.plain).toBe("A close call over buy a development card");
    expect(rationale?.plain).not.toMatch(/\d\.\d{3}/);
    // Technical detail is kept for the "Why" section and records.
    expect(rationale?.summary).toContain("Deep MaxN");
  });

  it("calls out a clear margin", () => {
    const rationale = explainDeepSearchDecision(
      makeDeepSearch(city, [actionStats(city, 0.5), actionStats(dev, 0.3)]),
    );
    expect(rationale?.plain).toBe(
      "Clearly better than the next option, buy a development card",
    );
  });
});
