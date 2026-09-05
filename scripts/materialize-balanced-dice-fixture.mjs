#!/usr/bin/env node

import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { spawn } from "node:child_process";
import process from "node:process";
import { build } from "esbuild";

const root = resolve(import.meta.dirname, "..");
const temporary = await mkdtemp(resolve(tmpdir(), "balanced-dice-materializer-"));
const bundle = resolve(temporary, "materialize-balanced-dice-fixture.mjs");

try {
  await build({
    entryPoints: [resolve(root, "scripts/materialize-balanced-dice-fixture.ts")],
    outfile: bundle,
    bundle: true,
    platform: "node",
    format: "esm",
    target: "node22",
  });
  await new Promise((resolvePromise, reject) => {
    const child = spawn(process.execPath, [bundle, ...process.argv.slice(2)], {
      cwd: root,
      env: { ...process.env, COLONIST_REPO_ROOT: root },
      stdio: "inherit",
    });
    child.once("error", reject);
    child.once("exit", (code) => {
      if (code === 0) resolvePromise();
      else reject(new Error(`balanced-dice materializer exited with code ${code}`));
    });
  });
} finally {
  await rm(temporary, { recursive: true, force: true });
}
