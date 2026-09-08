import { describe, expect, it } from "vitest";

import {
  NATIVE_GPU_PROTOCOL_VERSION,
  NATIVE_GPU_STATE_SCHEMA_VERSION,
} from "../src/background/native-gpu";

describe("native GPU compatibility contract", () => {
  it("requires protocol 7 with state schema 3", () => {
    expect(NATIVE_GPU_PROTOCOL_VERSION).toBe(7);
    expect(NATIVE_GPU_STATE_SCHEMA_VERSION).toBe(3);
  });
});
