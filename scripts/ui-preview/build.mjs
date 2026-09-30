import { build } from "esbuild";
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// Builds a static, browser-openable preview of the real Colonist Ally overlay.
// Open scripts/ui-preview/out/index.html directly (file://); no server needed.
const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..");
const outdir = join(here, "out");

await rm(outdir, { recursive: true, force: true });
await mkdir(outdir, { recursive: true });

await build({
  entryPoints: [join(here, "preview.ts")],
  outfile: join(outdir, "preview.js"),
  bundle: true,
  charset: "utf8",
  legalComments: "none",
  minify: false,
  sourcemap: true,
  target: ["chrome120"],
  format: "iife",
});

// The overlay resolves its font through chrome.runtime.getURL(path), which the
// preview stub maps to a path relative to preview.html.
await mkdir(join(outdir, "assets"), { recursive: true });
await cp(join(root, "assets", "fonts"), join(outdir, "assets", "fonts"), {
  recursive: true,
});

// Scenario names/descriptions come from the same module the page uses, so the
// index can never drift from the registry. Bundle it once for Node to import.
const registryFile = join(outdir, ".registry.mjs");
await build({
  stdin: {
    contents: `export { SCENARIO_NAMES, SCENARIO_DESCRIPTIONS } from ${JSON.stringify(join(here, "scenarios.ts"))};`,
    resolveDir: here,
    loader: "ts",
  },
  outfile: registryFile,
  bundle: true,
  format: "esm",
  platform: "node",
  logLevel: "silent",
  // The registry only needs names; the overlay is never constructed in Node.
  external: [],
});
const { SCENARIO_NAMES, SCENARIO_DESCRIPTIONS } = await import(
  pathToFileURL(registryFile).href
);
await rm(registryFile);

await writeFile(
  join(outdir, "preview.html"),
  `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Colonist Ally preview</title>
</head>
<body>
<script src="preview.js"></script>
</body>
</html>
`,
);

const links = SCENARIO_NAMES.map(
  (name) =>
    `<li><a href="preview.html?state=${name}">${name}</a><span>${SCENARIO_DESCRIPTIONS[name]}</span></li>`,
).join("\n");
const packageJson = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
await writeFile(
  join(outdir, "index.html"),
  `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Colonist Ally UI preview</title>
<style>
  body { margin: 0; padding: 32px 16px; background: #0d1821; color: #e8eef3; font: 15px/1.5 system-ui, sans-serif; }
  main { max-width: 640px; margin: 0 auto; }
  h1 { font-size: 22px; margin: 0 0 4px; }
  p { color: #9fb1bf; margin: 0 0 20px; }
  ul { list-style: none; margin: 0; padding: 0; }
  li { display: flex; gap: 14px; align-items: baseline; padding: 8px 0; border-top: 1px solid #22323f; }
  a { flex: 0 0 140px; color: #f1c84b; font-weight: 700; text-decoration: none; }
  a:hover { text-decoration: underline; }
  span { color: #9fb1bf; }
</style>
</head>
<body>
<main>
<h1>Colonist Ally UI preview</h1>
<p>Real overlay renderer, stubbed inputs (v${packageJson.version}). Use a 1280x800 window. Each link opens one scenario.</p>
<ul>
${links}
</ul>
</main>
</body>
</html>
`,
);

console.log(`UI preview built: ${pathToFileURL(join(outdir, "index.html")).href}`);
