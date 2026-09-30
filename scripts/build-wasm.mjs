import { access, mkdir, readdir, readFile, writeFile } from "node:fs/promises";
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

const run = (command, arguments_, cwd, env = process.env) =>
  new Promise((resolve, reject) => {
    const child = spawn(command, arguments_, { cwd, env, stdio: "inherit" });
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
    "--locked",
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

// Keep native and portable WASM builds on stable. Only the threaded artifact
// rebuilds std with atomics on this tested, pinned nightly.
const threadedTarget = join(engine, "target", "wasm-threads");
const threadedOutput = join(root, "src", "generated", "wasm-threads");
await run(
  cargo,
  [
    "+nightly-2025-11-15",
    "build",
    "--locked",
    "-p",
    "colonist-catan-wasm",
    "--features",
    "wasm-threads",
    "--target",
    "wasm32-unknown-unknown",
    "--target-dir",
    threadedTarget,
    "--release",
    "-Z",
    "build-std=panic_abort,std",
  ],
  engine,
  {
    ...process.env,
    RUSTFLAGS: [
      "-C target-feature=+atomics,+bulk-memory",
      "-C link-arg=--shared-memory",
      "-C link-arg=--max-memory=1073741824",
      "-C link-arg=--import-memory",
      "-C link-arg=--export=__wasm_init_tls",
      "-C link-arg=--export=__tls_size",
      "-C link-arg=--export=__tls_align",
      "-C link-arg=--export=__tls_base",
    ].join(" "),
  },
);
await mkdir(threadedOutput, { recursive: true });
await run(
  wasmBindgen,
  [
    join(threadedTarget, "wasm32-unknown-unknown", "release", "colonist_catan_wasm.wasm"),
    "--target",
    "web",
    "--out-dir",
    threadedOutput,
    "--out-name",
    "colonist_search",
    "--no-typescript",
  ],
  root,
);
// Upstream's bundlerless helper uses blob workers to support remote CDNs.
// MV3 allows packaged worker scripts under 'self', so use its packaged URL.
// Guard the rewrite so an upstream change fails visibly instead of shipping
// a helper that silently violates the extension's CSP.
const snippets = join(threadedOutput, "snippets");
let adaptedWorkers = 0;
for (const entry of await readdir(snippets)) {
  if (!entry.startsWith("wasm-bindgen-rayon-")) continue;
  const helper = join(snippets, entry, "src", "workerHelpers.no-bundler.js");
  const original = await readFile(helper, "utf8");
  const blobWorker = /let scriptBlob = await fetch\(import\.meta\.url\)\.then\(r => r\.blob\(\)\);\s*let url = URL\.createObjectURL\(scriptBlob\);\s*const worker = new Worker\(url,/;
  if (!blobWorker.test(original)) {
    throw new Error("wasm-bindgen-rayon worker helper changed; review the packaged worker adaptation");
  }
  const packaged = original
    .replace(blobWorker, "const worker = new Worker(new URL(import.meta.url),")
    .replace("URL.revokeObjectURL(url);", "")
    .replace("await pkg.default(data.module, data.memory);", "await pkg.default({ module_or_path: data.module, memory: data.memory });")
    .replace(/      \/\/ Self-spawn[\s\S]*?security error\./, "      // Spawn the packaged helper under the extension's 'self' CSP.")
    .replace(/\r\n/g, "\n")
    .replace(/[ \t]+$/gm, "");
  await writeFile(helper, packaged);
  adaptedWorkers += 1;
}
if (adaptedWorkers !== 1) {
  throw new Error(`Expected one wasm-bindgen-rayon helper, found ${adaptedWorkers}`);
}
console.log("Built packaged threaded Rust/WASM search engine");
