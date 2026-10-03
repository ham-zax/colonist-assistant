import { rename, stat, unlink } from "node:fs/promises";
import { spawn } from "node:child_process";

// Opt-in Binaryen wasm-opt pass for the packaged browser WASM artifacts.
// The default build never reaches Binaryen: build-wasm.mjs only calls into
// this module when `--wasm-opt` is present on the command line.

export const resolveWasmOpt = (argv = process.argv.slice(2), env = process.env) => {
  const enabled = argv.includes("--wasm-opt");
  let bin = env.WASM_OPT_BIN ?? "wasm-opt";
  for (const argument of argv) {
    if (argument.startsWith("--wasm-opt-bin=")) {
      bin = argument.slice("--wasm-opt-bin=".length);
    }
  }
  const flag = argv.indexOf("--wasm-opt-bin");
  if (flag !== -1 && flag + 1 < argv.length) {
    bin = argv[flag + 1];
  }
  return { enabled, bin };
};

const capture = (command, arguments_) =>
  new Promise((resolve, reject) => {
    const child = spawn(command, arguments_, { stdio: ["ignore", "pipe", "pipe"] });
    let output = "";
    child.stdout.on("data", (chunk) => {
      output += chunk;
    });
    child.stderr.on("data", (chunk) => {
      output += chunk;
    });
    child.once("error", reject);
    child.once("close", (code) => {
      if (code === 0) resolve(output.trim());
      else reject(new Error(`${command} ${arguments_.join(" ")} exited with code ${code}`));
    });
  });

// Runs `<bin> --version` and returns the first output line. Throws a helpful
// error naming the override knobs when the binary cannot be executed.
export const preflightWasmOpt = async (bin, run = capture) => {
  try {
    const output = await run(bin, ["--version"]);
    return output.split("\n")[0].trim();
  } catch (error) {
    throw new Error(
      `wasm-opt requested but '${bin}' is unavailable (${error.message}). ` +
        "Install Binaryen's wasm-opt or pass --wasm-opt-bin=<path> (or WASM_OPT_BIN=<path>).",
    );
  }
};

// Validated against official Binaryen version 133. Neither packaged module
// carries a target_features custom section, so features must be enabled
// explicitly (--detect-features is deprecated/no-op there); bare -O3 fails
// validation on i32.trunc_sat_f64_u. Per the version 133 help, --enable-atomics
// does not exist: --enable-threads covers atomics. No --all-features,
// --fast-math, --traps-never-happen, or import/export minification, so
// import/export names, memory sharing, TLS exports, and full math/trap
// semantics survive. The enabled WASM features fit the extension's Chrome
// 120 minimum.
export const wasmOptArgs = (threaded) => [
  "-O3",
  "--enable-bulk-memory",
  "--enable-mutable-globals",
  "--enable-sign-ext",
  "--enable-nontrapping-float-to-int",
  "--enable-reference-types",
  "--enable-multivalue",
  ...(threaded ? ["--enable-threads"] : []),
];

const runWasmOpt = (bin, arguments_) =>
  new Promise((resolve, reject) => {
    const child = spawn(bin, arguments_, { stdio: "inherit" });
    child.once("error", reject);
    child.once("exit", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`wasm-opt exited with code ${code}`));
    });
  });

// Optimizes `file` via a sibling temporary output, renaming over the
// original only on success. Any failure before the rename completes --
// optimizer spawn/output errors as well as rename failures -- removes
// the temporary file and leaves the original bytes untouched. Sizes are
// logged for provenance; they are not a speed claim.
export const optimizeWasmFile = async (
  bin,
  file,
  { threaded, version, run = runWasmOpt, filesystem = { rename, stat, unlink } } = {},
) => {
  const before = (await filesystem.stat(file)).size;
  const temporary = `${file}.wasm-opt.tmp`;
  let replaced = false;
  try {
    try {
      await run(bin, [...wasmOptArgs(threaded), file, "-o", temporary]);
    } catch (error) {
      throw new Error(`wasm-opt failed on ${file}: ${error.message}`);
    }
    await filesystem.rename(temporary, file);
    replaced = true;
  } finally {
    if (!replaced) await filesystem.unlink(temporary).catch(() => {});
  }
  const after = (await filesystem.stat(file)).size;
  const label = threaded ? "threaded" : "portable";
  console.log(`Optimized ${label} WASM (${version}): ${file} ${before} -> ${after} bytes`);
};
