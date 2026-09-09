import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const output = resolve(root, process.argv[2] ?? "benchmark-results/adaptive-3p-pilot");
mkdirSync(output, { recursive: true });
const common = [
  "--players", "3", "--blocks", "1", "--seed", "2026090901",
  "--candidate", "maxn", "--baseline", "weighted",
  "--threads", "3", "--max-turns", "600",
  "--maxn-depth", "3", "--maxn-branch", "12", "--maxn-nodes", "1500",
  "--belief-particles", "8", "--strategic-particles", "8",
  "--maxn-time-ms", "0", "--opening-nodes", "12000", "--opening-time-ms", "0",
  "--trade-response-nodes", "1500", "--trade-response-time-ms", "0",
  "--validate", "--quiet", "--json",
];
for (const trades of ["on", "off"]) {
  for (const policy of ["baseline", "adaptive-candidate-admission-v1"]) {
    const name = `${trades}-${policy}`;
    const args = [...common, "--strategy-policy", policy,
      "--checkpoint-output", resolve(output, `${name}.jsonl`),
      ...(trades === "off" ? ["--no-player-trades"] : [])];
    writeFileSync(resolve(output, `${name}-command.json`), JSON.stringify(args, null, 2) + "\n");
    console.log(`Starting ${name}: three rotated seats, one matched block`);
    const run = spawnSync(resolve(root, "engine/target/release/colonist-arena"), args,
      { cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
    writeFileSync(resolve(output, `${name}.json`), run.stdout ?? "");
    writeFileSync(resolve(output, `${name}.stderr.txt`), run.stderr ?? "");
    if (run.error || run.status !== 0) throw run.error ?? new Error(`${name} exited ${run.status}`);
    console.log(`Completed ${name}`);
  }
}
