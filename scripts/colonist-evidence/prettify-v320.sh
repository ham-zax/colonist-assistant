#!/usr/bin/env sh
set -eu

repo_root=$(git rev-parse --show-toplevel)
evidence="$repo_root/docs/colonist-evidence/v320"
raw="$evidence/raw"
out="$evidence/generated/pretty"

rm -rf "$out"
mkdir -p "$out"

for file in "$raw"/*.js; do
  name=$(basename "$file")
  cp "$file" "$out/$name.pretty.js"
done

for file in "$raw"/*.json; do
  name=$(basename "$file")
  cp "$file" "$out/$name.pretty.json"
done

# Invoke the pinned formatter once for the whole snapshot. Re-running npx once
# per bundle is unnecessarily slow and can exceed remote harness call limits.
npx --yes prettier@3.8.1 --write "$out"/*.pretty.js "$out"/*.pretty.json >/dev/null

printf 'Generated diagnostic formatting under %s\n' "$out"
printf 'Raw evidence was not modified.\n'
