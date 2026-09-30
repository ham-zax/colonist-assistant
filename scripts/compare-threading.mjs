// node scripts/compare-threading.mjs sequential.jsonl candidate.jsonl [...]
import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";

const [baselinePath, ...candidatePaths] = process.argv.slice(2);
if (!baselinePath || !candidatePaths.length) {
  throw new Error("usage: compare-threading.mjs sequential.jsonl candidate.jsonl [...]");
}
const readRows = async (path) => (await readFile(path, "utf8"))
  .trim().split("\n").map((line) => JSON.parse(line));
const reference = new Map((await readRows(baselinePath))
  .filter((row) => row.id && row.rep === 0).map((row) => [row.id, row.sig]));
if (!reference.size) throw new Error("baseline has no request signatures");
let failed = false;
for (const path of candidatePaths) {
  const rows = await readRows(path);
  const candidateRows = rows.filter((row) => row.id);
  const repetitions = [...new Set(candidateRows.map((row) => row.rep))];
  const candidate = new Map(candidateRows.map((row) => [`${row.rep}:${row.id}`, row.sig]));
  const mismatch = [];
  if (!repetitions.length) throw new Error(`${path} has no request signatures`);
  for (const rep of repetitions) {
    for (const [id, expected] of reference) {
      try { assert.deepStrictEqual(candidate.get(`${rep}:${id}`), expected); }
      catch { mismatch.push(`${id}@rep${rep}`); }
    }
  }
  const extra = [...new Set(candidateRows.map((row) => row.id))].filter((id) => !reference.has(id));
  if (candidateRows.length !== candidate.size || extra.length || mismatch.length) failed = true;
  const times = rows.filter((row) => row.sumMs !== undefined).map((row) => row.sumMs);
  console.log(JSON.stringify({ path, compared: reference.size, mismatch, extra,
    duplicateIds: candidateRows.length !== candidate.size,
    ...(times.length ? { minMs: Math.min(...times) } : {}),
  }));
}
if (failed) process.exitCode = 1;
