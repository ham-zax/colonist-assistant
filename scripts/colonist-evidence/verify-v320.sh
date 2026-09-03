#!/usr/bin/env sh
set -eu

repo_root=$(git rev-parse --show-toplevel)
evidence="$repo_root/docs/colonist-evidence/v320"
manifest="$evidence/manifest.tsv"

fail=0
while IFS="$(printf '\t')" read -r asset url retrieved status bytes sha loaded raw pretty; do
  [ "$asset" = "Asset" ] && continue
  file="$evidence/$raw"
  if [ ! -f "$file" ]; then
    printf 'MISSING  %s\n' "$raw" >&2
    fail=1
    continue
  fi
  actual_bytes=$(wc -c < "$file" | tr -d ' ')
  actual_sha=$(sha256sum "$file" | awk '{print $1}')
  if [ "$actual_bytes" != "$bytes" ]; then
    printf 'SIZE    %s expected=%s actual=%s\n' "$raw" "$bytes" "$actual_bytes" >&2
    fail=1
  fi
  if [ "$actual_sha" != "$sha" ]; then
    printf 'SHA256  %s expected=%s actual=%s\n' "$raw" "$sha" "$actual_sha" >&2
    fail=1
  fi
done < "$manifest"

if [ "$fail" -ne 0 ]; then
  exit 1
fi

printf 'Colonist v320 evidence hashes and sizes match manifest.tsv\n'
