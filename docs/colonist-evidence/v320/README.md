# Colonist client evidence snapshot — runtime v320

This directory preserves the client artifacts used for the September 2026 Colonist base-game compatibility audit. It exists so future engine work can re-check the shipped client instead of relying on an agent summary or on line numbers from a temporary prettified file.

## Snapshot identity

Passive browser inspection on 2026-09-03 recorded:

- `window.versionNumber = 320`
- `localStorage.versionno = "319"`
- `window.isProduction = true`
- `window.assetPath = "https://cdn.colonist.io/dist/"`
- `window.socketServerWSS = "wss://socket.svr.colonist.io/"`
- `window.webpackChunkkatan` present
- no active game was joined during the preserved runtime capture

The two version markers disagree. Treat the exact asset names and SHA-256 hashes in `manifest.tsv` as the authoritative identity of this snapshot; `window.versionNumber = 320` is the best runtime version label.

## What is preserved

- `raw/` — byte-for-byte public client assets used in the audit. Do not edit these files.
- `manifest.tsv` — repository-relative provenance, sizes and SHA-256 hashes.
- `manifest.original.tsv` — the original D2 audit manifest, retained for provenance comparison.
- `runtime/` — passive browser runtime observations. These prove what was visible in the inspected lobby state; they are not fabricated active-game captures.
- `network/` — passive protocol-observation status. No live gameplay frames were captured in D2.
- `generated/` — ignored output directory for reproducible formatting.

The raw set includes the audited first-party bundles, selected lazy chunks, English strings, and the site service worker. It is the audited client surface, not a claim that every possible feature-specific lazy chunk Colonist can ever load has been archived.

## Integrity

From the repository root:

```sh
scripts/colonist-evidence/verify-v320.sh
```

The verifier checks every manifest entry against its recorded byte count and SHA-256.

## Produce readable diagnostic copies

Raw minified files are evidence and remain immutable. Generate readable copies with the pinned formatter:

```sh
scripts/colonist-evidence/prettify-v320.sh
```

The script uses `prettier@3.8.1` and writes to `docs/colonist-evidence/v320/generated/pretty/`. Generated formatting is intentionally not versioned because it can be reproduced from the hashed raw bytes.

## Evidence discipline

Use these categories in future rule claims:

1. **Shipped client static** — schema, enum, validator, controller, UI or dataflow found in a hashed asset here.
2. **Live runtime** — value actually evaluated in the browser state and saved under `runtime/`.
3. **Live protocol** — message actually observed over the wire. D2 has no gameplay examples in this category.
4. **Official rule** — Colonist or CATAN published base-game rule documentation.
5. **Local engine** — current repository implementation.
6. **Inference / model choice** — conclusion not directly established by the above.

Never upgrade an absence search to “server-side implementation proven.” The safe wording is “not located in the inspected client surface” unless positive evidence identifies the execution owner.

## Related documents

- `docs/COLONIST_CLIENT_MAPPING_GUIDE.md` — how to navigate and de-obfuscate the bundles.
- `docs/COLONIST_BASE_GAME_RULE_FIDELITY.md` — rule-by-rule mapping to our engine and current gaps.
- `docs/COLONIST_5_8_PLAYER_SUPPORT.md` — future 5–8 player base-game migration map.
