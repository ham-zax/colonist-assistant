import { access, mkdir } from "node:fs/promises";
import { join } from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const cargoHome = process.env.CARGO_HOME ?? join(process.env.HOME ?? "", ".cargo");

const executable = async (preferred, fallback) => {
  try {
    await access(preferred);
    return preferred;
  } catch {
    return fallback;
  }
};

const run = (command, arguments_, cwd) =>
  new Promise((resolve, reject) => {
    const child = spawn(command, arguments_, { cwd, stdio: "inherit" });
    child.once("error", reject);
    child.once("exit", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} exited with code ${code}`));
    });
  });

const cargo = await executable(join(cargoHome, "bin", "cargo"), "cargo");
const wasmBindgen = await executable(
  join(cargoHome, "bin", "wasm-bindgen"),
  "wasm-bindgen",
);
const engine = join(root, "engine");
const output = join(root, "src", "generated", "wasm");

await run(
  cargo,
  [
    "build",
    "-p",
    "colonist-catan-wasm",
    "--target",
    "wasm32-unknown-unknown",
    "--release",
  ],
  engine,
);
await mkdir(output, { recursive: true });
await run(
  wasmBindgen,
  [
    join(
      engine,
      "target",
      "wasm32-unknown-unknown",
      "release",
      "colonist_catan_wasm.wasm",
    ),
    "--target",
    "web",
    "--out-dir",
    output,
    "--out-name",
    "colonist_search",
    "--no-typescript",
  ],
  root,
);

console.log("Built packaged Rust/WASM search engine");
