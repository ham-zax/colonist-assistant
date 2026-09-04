import { describe, expect, it } from "vitest";

import {
  NATIVE_GPU_PROTOCOL_VERSION,
  NATIVE_GPU_STATE_SCHEMA_VERSION,
} from "../src/background/native-gpu";

describe("native GPU compatibility contract", () => {
  it("bumps only the state JSON schema for dice-mode metadata", () => {
    expect(NATIVE_GPU_PROTOCOL_VERSION).toBe(6);
    expect(NATIVE_GPU_STATE_SCHEMA_VERSION).toBe(3);
  });
});
