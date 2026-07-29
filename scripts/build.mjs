import { build } from "esbuild";
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outdir = join(root, "dist");

await import("./build-wasm.mjs");
await rm(outdir, { recursive: true, force: true });
await mkdir(outdir, { recursive: true });

const shared = {
  bundle: true,
  charset: "utf8",
  legalComments: "none",
  minify: false,
  sourcemap: true,
  target: ["chrome120"],
};

await Promise.all([
  build({
    ...shared,
    entryPoints: [join(root, "src/content/index.ts")],
    outfile: join(outdir, "content.js"),
    format: "iife",
  }),
  build({
    ...shared,
    entryPoints: [join(root, "src/popup/index.ts")],
    outfile: join(outdir, "popup.js"),
    format: "iife",
  }),
  build({
    ...shared,
    entryPoints: [join(root, "src/page/bridge.ts")],
    outfile: join(outdir, "page-bridge.js"),
    format: "iife",
  }),
  build({
    ...shared,
    entryPoints: [join(root, "src/background/index.ts")],
    outfile: join(outdir, "background.js"),
    format: "esm",
  }),
]);

await cp(join(root, "static"), outdir, { recursive: true });
await cp(join(root, "assets"), join(outdir, "assets"), { recursive: true });
await cp(join(root, "PRIVACY.md"), join(outdir, "PRIVACY.md"));
await cp(
  join(root, "src", "generated", "wasm", "colonist_search_bg.wasm"),
  join(outdir, "colonist_search_bg.wasm"),
);

const manifestPath = join(outdir, "manifest.json");
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const packageJson = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
manifest.version = packageJson.version;
await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

console.log(`Built Colonist Assistant ${packageJson.version} in ${outdir}`);
