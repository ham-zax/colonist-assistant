import { describe, expect, it } from "vitest";

import { InteractionRenderGate } from "../src/content/render-gate";

describe("overlay interaction render gate", () => {
  it("keeps an active settings interaction mounted throughout rerenders", () => {
    const gate = new InteractionRenderGate();

    expect(gate.tryRender()).toBe(true);
    gate.hold("settings-interaction");
    expect(gate.tryRender()).toBe(false);
    expect(gate.tryRender()).toBe(false);
    expect(gate.hasDeferredRender()).toBe(true);

    gate.release("settings-interaction");
    expect(gate.tryRender()).toBe(true);
    expect(gate.hasDeferredRender()).toBe(false);
  });

  it("requires every active interaction lock to release before rendering", () => {
    const gate = new InteractionRenderGate();

    gate.hold("settings-interaction");
    gate.hold("secondary");
    gate.release("settings-interaction");
    expect(gate.tryRender()).toBe(false);

    gate.release("secondary");
    expect(gate.tryRender()).toBe(true);
  });
});
