// Assemble a disposable extension; browser access stays with the browser skill.
import { cp, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { dirname, join, resolve, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outsideRepository = (output) => {
  const fromRoot = relative(root, output);
  if (!fromRoot.startsWith("..") && !fromRoot.startsWith("/")) throw new Error("Private benchmark artifacts must be outside the repository");
};
const [input, outputArgument, extractionOutput] = process.argv.slice(2);
if (input === "--extract") {
  if (!outputArgument || !extractionOutput) throw new Error("usage: prepare-threading-browser.mjs --extract results.json output-directory");
  const result = JSON.parse(await readFile(outputArgument, "utf8"));
  if (result.progress !== "done") throw new Error(`Benchmark is incomplete: ${result.progress}`);
  if (!["fixed", "live-current", "live-original"].includes(result.mode) || !Array.isArray(result.runs) || result.runs.some((run) => !Number.isInteger(run.threads) || run.threads < 0 || run.threads > 8 || !Array.isArray(run.rows))) {
    throw new Error("Expected completed benchmark output with mode and runs");
  }
  const output = resolve(extractionOutput);
  outsideRepository(output);
  await mkdir(output, { recursive: true });
  for (const run of result.runs) {
    const path = join(output, `${result.mode}-threads-${run.threads}.jsonl`);
    await writeFile(path, run.rows.map((row) => JSON.stringify(row)).join("\n") + "\n");
    console.log(path);
  }
} else {
  if (!input) throw new Error("usage: prepare-threading-browser.mjs requests.json [temporary-extension-directory]");
  const fixtures = JSON.parse(await readFile(input, "utf8"));
  if (!Array.isArray(fixtures) || !fixtures.length || fixtures.some((item) => typeof item.id !== "string" || !item.request?.state)) {
    throw new Error("Expected nonempty [{id, request: {state, effort, ...}}] fixtures");
  }
  const output = outputArgument ? resolve(outputArgument) : await mkdtemp(join(tmpdir(), "colonist-threading-browser-"));
  outsideRepository(output);
  await mkdir(output, { recursive: true });
  await cp(join(root, "scripts", "threading-browser"), output, { recursive: true });
  for (const variant of ["wasm", "wasm-threads"]) {
    await cp(join(root, "src", "generated", variant), join(output, variant), { recursive: true });
  }
  await writeFile(join(output, "requests.json"), JSON.stringify(fixtures));
  const extensionId = [...createHash("sha256").update(output).digest("hex").slice(0, 32)]
    .map((digit) => String.fromCharCode(97 + parseInt(digit, 16))).join("");
  console.log(JSON.stringify({ extensionDirectory: output, extensionId, benchmarkUrl: `chrome-extension://${extensionId}/benchmark.html` }));
}
