import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import {
  optimizeWasmFile,
  preflightWasmOpt,
  resolveWasmOpt,
  wasmOptArgs,
} from "../scripts/wasm-opt.mjs";

const scratch = (name, bytes) => {
  const dir = mkdtempSync(join(tmpdir(), "wasm-opt-"));
  const file = join(dir, name);
  writeFileSync(file, bytes);
  return file;
};

describe("wasm-opt opt-in", () => {
  it("stays disabled by default so the normal build never spawns Binaryen", () => {
    expect(resolveWasmOpt([]).enabled).toBe(false);
    expect(resolveWasmOpt(["--wasm-opt-bin=/tmp/wasm-opt"]).enabled).toBe(false);
  });

  it("enables via --wasm-opt with executable path overrides", () => {
    expect(resolveWasmOpt(["--wasm-opt"])).toMatchObject({ enabled: true, bin: "wasm-opt" });
    expect(resolveWasmOpt(["--wasm-opt", "--wasm-opt-bin=/tmp/wasm-opt"]).bin).toBe(
      "/tmp/wasm-opt",
    );
    expect(resolveWasmOpt(["--wasm-opt", "--wasm-opt-bin", "/tmp/wasm-opt"]).bin).toBe(
      "/tmp/wasm-opt",
    );
    expect(resolveWasmOpt(["--wasm-opt"], { WASM_OPT_BIN: "/tmp/env-opt" }).bin).toBe(
      "/tmp/env-opt",
    );
  });

  it("preflight fails helpfully for a missing binary", async () => {
    await expect(preflightWasmOpt("/definitely/not/wasm-opt")).rejects.toThrow(
      /wasm-opt requested but .* unavailable.*--wasm-opt-bin/s,
    );
  });

  it("preflight reports the tool version", async () => {
    const run = async () => "wasm-opt version 123 (Binaryen)\nextra line\n";
    await expect(preflightWasmOpt("wasm-opt", run)).resolves.toBe(
      "wasm-opt version 123 (Binaryen)",
    );
  });

  it("uses the Binaryen 133 validated flags: nontrapping-float required, no --enable-atomics", () => {
    const portable = wasmOptArgs(false);
    const threaded = wasmOptArgs(true);
    // i32.trunc_sat_f64_u in the packaged modules fails validation without this.
    for (const args of [portable, threaded]) {
      for (const required of [
        "-O3",
        "--enable-bulk-memory",
        "--enable-mutable-globals",
        "--enable-sign-ext",
        "--enable-nontrapping-float-to-int",
        "--enable-reference-types",
        "--enable-multivalue",
      ]) {
        expect(args).toContain(required);
      }
      // Official version 133 help has no --enable-atomics; --enable-threads
      // covers atomics. --detect-features is deprecated/no-op on modules
      // without a target_features section.
      for (const banned of [
        "--enable-atomics",
        "--detect-features",
        "--all-features",
        "--fast-math",
        "--traps-never-happen",
      ]) {
        expect(args).not.toContain(banned);
      }
      expect(args.some((flag) => flag.includes("minify"))).toBe(false);
    }
    expect(portable).not.toContain("--enable-threads");
    expect(threaded).toContain("--enable-threads");
  });

  it("replaces the artifact on success and logs before/after bytes", async () => {
    const file = scratch("colonist_search_bg.wasm", Buffer.from([0, 1, 2, 3]));
    const calls = [];
    const logs = [];
    const original = console.log;
    console.log = (message) => logs.push(message);
    try {
      await optimizeWasmFile("wasm-opt", file, {
        threaded: false,
        version: "wasm-opt version 123",
        run: async (bin, args) => {
          calls.push([bin, args]);
          writeFileSync(args[args.length - 1], Buffer.from([9, 9]));
        },
      });
    } finally {
      console.log = original;
    }
    expect(readFileSync(file)).toEqual(Buffer.from([9, 9]));
    const [, args] = calls[0];
    expect(args[0]).toBe("-O3");
    expect(args).toContain("--enable-nontrapping-float-to-int");
    expect(args.slice(-3)).toEqual([file, "-o", `${file}.wasm-opt.tmp`]);
    expect(logs.join("\n")).toMatch(/4 -> 2 bytes/);
  });

  it("leaves the original safe and cleans the temp file on failure", async () => {
    const before = Buffer.from([0, 1, 2, 3]);
    const file = scratch("colonist_search_bg.wasm", before);
    await expect(
      optimizeWasmFile("wasm-opt", file, {
        threaded: true,
        version: "wasm-opt version 123",
        run: async (bin, args) => {
          writeFileSync(args[args.length - 1], Buffer.from([9]));
          throw new Error("boom");
        },
      }),
    ).rejects.toThrow(/wasm-opt failed/);
    expect(readFileSync(file)).toEqual(before);
    await expect(
      import("node:fs/promises").then((fs) => fs.stat(`${file}.wasm-opt.tmp`)),
    ).rejects.toThrow();
  });

  it("cleans the temp output when the rename itself fails", async () => {
    const before = Buffer.from([0, 1, 2, 3]);
    const file = scratch("colonist_search_bg.wasm", before);
    const realFs = await import("node:fs/promises");
    const unlinked = [];
    const filesystem = {
      ...realFs,
      rename: async () => {
        throw new Error("rename boom");
      },
      unlink: async (path) => {
        unlinked.push(path);
        return realFs.unlink(path);
      },
    };
    await expect(
      optimizeWasmFile("wasm-opt", file, {
        threaded: false,
        version: "wasm-opt version 123",
        filesystem,
        run: async (bin, args) => {
          writeFileSync(args[args.length - 1], Buffer.from([9, 9]));
        },
      }),
    ).rejects.toThrow(/rename boom/);
    expect(readFileSync(file)).toEqual(before);
    expect(unlinked).toEqual([`${file}.wasm-opt.tmp`]);
  });
});
