import { describe, expect, it } from "vitest";

import { InteractionRenderGate } from "../src/content/render-gate";

describe("overlay interaction render gate", () => {
  it("keeps the native engine selector mounted throughout its open interaction", () => {
    const gate = new InteractionRenderGate();

    expect(gate.tryRender()).toBe(true);
    gate.hold("engine-select");
    expect(gate.tryRender()).toBe(false);
    expect(gate.tryRender()).toBe(false);
    expect(gate.hasDeferredRender()).toBe(true);

    gate.release("engine-select");
    expect(gate.tryRender()).toBe(true);
    expect(gate.hasDeferredRender()).toBe(false);
  });

  it("requires every active interaction lock to release before rendering", () => {
    const gate = new InteractionRenderGate();

    gate.hold("engine-select");
    gate.hold("secondary");
    gate.release("engine-select");
    expect(gate.tryRender()).toBe(false);

    gate.release("secondary");
    expect(gate.tryRender()).toBe(true);
  });
});
