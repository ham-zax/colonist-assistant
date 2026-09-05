import { describe, expect, it } from "vitest";

import { parsePublicBoardMessage } from "../src/content/board";
import { observeColonistDiceMode } from "../src/page/dice-mode";
import {
  NATIVE_GPU_STOCHASTIC_MODEL,
  nativeGpuSupportsStochasticModel,
} from "../src/background/native-gpu";

const bridgeMessage = (payload: Record<string, unknown>) => ({
  source: "colonist-assistant-public-board",
  type: "snapshot",
  payload: {
    hexes: [],
    vertices: [],
    edges: [],
    ...payload,
  },
});

describe("Colonist dice-mode boundary", () => {
  it("keeps current native GPU stochastic capability M0-only", () => {
    expect(NATIVE_GPU_STOCHASTIC_MODEL).toBe("m0-fair-iid-2d6-v1");
    expect(nativeGpuSupportsStochasticModel(undefined)).toBe(true);
    expect(nativeGpuSupportsStochasticModel("m0-fair-iid-2d6-v1")).toBe(true);
    expect(nativeGpuSupportsStochasticModel("mref-colonist-linked-2024-v1")).toBe(false);
  });
  it("maps the shipped numeric diceSetting contract without defaulting missing state to Random", () => {
    const absentSettings: { diceSetting?: unknown } = {};

    expect(observeColonistDiceMode(0)).toEqual({ mode: "random" });
    expect(observeColonistDiceMode(1)).toEqual({ mode: "balanced" });
    expect(observeColonistDiceMode(2)).toEqual({
      mode: "unsupported",
      rawUnsupportedSetting: 2,
    });
    expect(observeColonistDiceMode(3)).toEqual({
      mode: "unsupported",
      rawUnsupportedSetting: 3,
    });
    expect(observeColonistDiceMode(absentSettings.diceSetting)).toEqual({
      mode: "unknown",
    });
    expect(observeColonistDiceMode(undefined)).toEqual({ mode: "unknown" });
    expect(observeColonistDiceMode(null)).toEqual({ mode: "unknown" });
    expect(observeColonistDiceMode(99)).toEqual({
      mode: "unsupported",
      rawUnsupportedSetting: 99,
    });
  });

  it("normalizes legacy board ingress to Unknown and preserves all canonical modes", () => {
    expect(parsePublicBoardMessage(bridgeMessage({}))).toEqual(
      expect.objectContaining({ diceMode: "unknown" }),
    );

    for (const diceMode of ["random", "balanced", "unsupported"] as const) {
      const parsed = parsePublicBoardMessage(
        bridgeMessage({
          diceMode,
          ...(diceMode === "unsupported" ? { diceModeRaw: 2 } : {}),
        }),
      );
      expect(parsed).toEqual(
        expect.objectContaining({
          diceMode,
          ...(diceMode === "unsupported" ? { diceModeRaw: 2 } : {}),
        }),
      );
    }
  });

  it("rejects invalid canonical strings and contradictory unsupported raw evidence", () => {
    expect(
      parsePublicBoardMessage(bridgeMessage({ diceMode: "sometimes" })),
    ).toBeUndefined();
    expect(
      parsePublicBoardMessage(
        bridgeMessage({ diceMode: "unsupported", diceModeRaw: 0 }),
      ),
    ).toBeUndefined();
    expect(
      parsePublicBoardMessage(
        bridgeMessage({ diceMode: "unsupported", diceModeRaw: 1 }),
      ),
    ).toBeUndefined();
    expect(
      parsePublicBoardMessage(
        bridgeMessage({ diceMode: "random", diceModeRaw: 2 }),
      ),
    ).toBeUndefined();
  });
});
