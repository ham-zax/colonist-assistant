// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const harness = vi.hoisted(() => ({
  calls: [] as string[],
  startResults: [] as Array<"resolve" | "reject">,
  intervalCallback: undefined as (() => void) | undefined,
}));

vi.mock("../src/content/board", () => ({
  readPublicBoardSnapshot: () => ({
    gameKey: "/|boot-fixture|1",
    diceMode: "balanced",
    initialPlacement: false,
    gameplayRollCount: 10,
    turn: 13,
  }),
  installPublicBoardBridge: () => {
    harness.calls.push("bridge-install");
    return () => harness.calls.push("bridge-remove");
  },
}));

vi.mock("../src/content/dom", () => ({
  findLogRoot: () => undefined,
}));

vi.mock("../src/content/settings", () => ({
  readSettings: async () => ({ enabled: true, investigationLog: false }),
  RESET_NONCE_KEY: "reset",
  SETTINGS_KEY: "settings",
}));

vi.mock("../src/core/local-data", () => ({
  clearCurrentGameStorage: async () => {},
}));

vi.mock("../src/content/investigation-recorder", () => ({
  investigationRecorder: {
    initialize: async () => {},
    setGame: () => {},
    setEnabled: () => {},
    isEnabled: () => false,
    flush: async () => {},
    record: (_kind: string, data: { phase?: string }) => {
      if (data.phase === "session-start-failed") {
        harness.calls.push("session-start-failed");
      }
    },
  },
}));

vi.mock("../src/content/overlay", () => ({
  AssistantOverlay: class {
    setSettings(): void {}
    updateBoard(): void {
      harness.calls.push("overlay-board");
    }
    update(session?: unknown): void {
      harness.calls.push(session ? "overlay-session" : "overlay-session-clear");
    }
    async clearStoredSessionData(): Promise<void> {}
    destroy(): void {
      harness.calls.push("overlay-destroy");
    }
  },
}));

vi.mock("../src/content/session", () => ({
  GameSession: class {
    gameKey?: string;
    private readonly onUpdate: (session: unknown) => void;

    constructor(
      _root: HTMLElement,
      onUpdate: (session: unknown) => void,
      gameKey?: string,
    ) {
      harness.calls.push("session-construct");
      this.onUpdate = onUpdate;
      this.gameKey = gameKey;
    }

    async start(): Promise<void> {
      harness.calls.push("session-start");
      const result = harness.startResults.shift() ?? "resolve";
      if (result === "reject") throw new Error("fixture restore failure");
      this.onUpdate(this);
      harness.calls.push("session-started");
    }

    setInitialPlacement(): void {}
    setMyPlayer(): void {}
    setGameKey(gameKey: string): void {
      this.gameKey = gameKey;
    }
    reconcileBoardSnapshot(): void {}
    observeBoardDiceSnapshot(): void {
      harness.calls.push("board-dice");
    }
    attachRoot(): void {
      harness.calls.push("session-attach-root");
    }
    stop(): void {
      harness.calls.push("session-stop");
    }
    async clearStoredData(): Promise<void> {}
  },
}));

describe("content session boot ordering", () => {
  beforeEach(() => {
    vi.resetModules();
    harness.calls.length = 0;
    harness.startResults.length = 0;
    harness.intervalCallback = undefined;
    document.body.innerHTML = '<div id="game-canvas"></div>';
    vi.stubGlobal("chrome", {
      storage: {
        onChanged: { addListener: vi.fn() },
      },
    });
    vi.spyOn(window, "setInterval").mockImplementation((callback: TimerHandler) => {
      harness.intervalCallback = callback as () => void;
      return 1 as unknown as ReturnType<typeof window.setInterval>;
    });
    vi.spyOn(window, "clearInterval").mockImplementation(() => {});
  });

  afterEach(() => {
    window.dispatchEvent(new Event("pagehide"));
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    document.body.replaceChildren();
  });

  it("publishes the initial board only after a session has attached", async () => {
    harness.startResults.push("resolve");

    await import("../src/content/index");
    await vi.waitFor(() => expect(harness.calls).toContain("overlay-board"));

    expect(harness.calls.indexOf("session-start")).toBeGreaterThanOrEqual(0);
    expect(harness.calls.indexOf("overlay-session")).toBeGreaterThan(
      harness.calls.indexOf("session-start"),
    );
    expect(harness.calls.indexOf("overlay-board")).toBeGreaterThan(
      harness.calls.indexOf("overlay-session"),
    );
    expect(harness.calls.indexOf("bridge-install")).toBeGreaterThan(
      harness.calls.indexOf("overlay-board"),
    );
  });

  it("does not poison the root when session startup fails", async () => {
    harness.startResults.push("reject", "resolve");

    await import("../src/content/index");
    await vi.waitFor(() => expect(harness.calls).toContain("session-start-failed"));

    expect(harness.calls.filter((call) => call === "session-construct")).toHaveLength(1);
    expect(harness.intervalCallback).toBeDefined();

    harness.intervalCallback!();
    await vi.waitFor(() =>
      expect(harness.calls.filter((call) => call === "session-construct")).toHaveLength(2),
    );
    await vi.waitFor(() => expect(harness.calls).toContain("overlay-session"));

    expect(harness.calls.filter((call) => call === "session-start")).toHaveLength(2);
  });
});
