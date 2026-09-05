import { describe, expect, it } from "vitest";

import {
  InvestigationLog,
  formatInvestigationLog,
} from "../src/core/investigation-log";

describe("InvestigationLog", () => {
  it("keeps a bounded game-scoped diagnostic history and resets on game change", () => {
    const log = new InvestigationLog(3);
    log.setGame("game-a");
    log.record("board", { gameplayRollCount: 1 });
    log.record("dom", { logIndex: 12 });
    log.record("dice", { action: "ambiguity-added", logIndex: 12 });
    log.record("decision", { accepted: false });

    expect(log.snapshot()).toMatchObject({
      gameKey: "game-a",
      entries: [
        { seq: 2, kind: "dom", data: { logIndex: 12 } },
        { seq: 3, kind: "dice", data: { action: "ambiguity-added", logIndex: 12 } },
        { seq: 4, kind: "decision", data: { accepted: false } },
      ],
    });

    log.setGame("game-b");
    expect(log.snapshot()).toMatchObject({ gameKey: "game-b", entries: [] });
  });

  it("formats an export that preserves ordered causal entries", () => {
    const log = new InvestigationLog();
    log.setGame("grid7603");
    log.record("board", { gameplayRollCount: 8, hasRolled: true, lastRoll: 10 });
    log.record("decision", { expectedRollCount: 8, status: "accepted" });

    const text = formatInvestigationLog(log.snapshot());
    expect(text).toContain("@schema=colonist-investigation/1");
    expect(text).toContain("@gameKey=\"grid7603\"");
    expect(text).toContain('"kind":"board"');
    expect(text).toContain('"kind":"decision"');
    expect(text.indexOf('"kind":"board"')).toBeLessThan(text.indexOf('"kind":"decision"'));
  });
});
