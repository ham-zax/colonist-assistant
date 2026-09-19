import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";

function canonicalize(value) {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicalize(value[key])]),
    );
  }
  return value;
}

export function canonicalJson(value) {
  return JSON.stringify(canonicalize(value));
}

export function jevRequestCacheKey(request) {
  return createHash("sha256").update(canonicalJson(request)).digest("hex");
}

export function readCachedJevResponse(cacheDirectory, cacheKey) {
  const cachePath = path.join(cacheDirectory, `${cacheKey}.json`);
  if (!fs.existsSync(cachePath)) return null;
  const record = JSON.parse(fs.readFileSync(cachePath, "utf8"));
  if (record.schemaVersion !== 1 || record.cacheKey !== cacheKey || !record.response) {
    throw new Error(`Invalid Jev cache record: ${cachePath}`);
  }
  if (jevRequestCacheKey(record.request) !== cacheKey) {
    throw new Error(`Jev cache request hash mismatch: ${cachePath}`);
  }
  return { cachePath, record };
}

export function writeCachedJevResponse(cacheDirectory, cacheKey, request, response) {
  fs.mkdirSync(cacheDirectory, { recursive: true });
  const cachePath = path.join(cacheDirectory, `${cacheKey}.json`);
  const temporaryPath = `${cachePath}.${process.pid}.tmp`;
  const record = {
    schemaVersion: 1,
    cacheKey,
    request,
    response,
  };
  fs.writeFileSync(temporaryPath, `${JSON.stringify(record)}\n`, { flag: "wx" });
  fs.renameSync(temporaryPath, cachePath);
  return cachePath;
}
