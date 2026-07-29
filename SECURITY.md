# Security

## Data boundary

Colonist Assistant is a Manifest V3 extension scoped to
`https://colonist.io/*`. It has no backend, telemetry, analytics, advertising,
remote JavaScript, or cross-site host access.

The page bridge emits a validated, JSON-serializable game snapshot. The content
script rejects malformed topology, out-of-range counters, unknown action
states, duplicate IDs, invalid screen coordinates, and non-Colonist asset
URLs. Search runs in the extension-origin background service worker, with a
lightweight content-script fallback if that service is unavailable. The WASM
module is packaged with the extension and covered by the extension-page CSP.

Autonomous clicks require all of the following:

- the user enabled private-game autopilot;
- Colonist identifies the room as private;
- the recommendation meets the confidence threshold;
- the action signature has not already been clicked.

## Reporting

After the source repository is published, report vulnerabilities privately to
the maintainer rather than opening a public exploit report. Include the
extension version, affected URL, reproduction steps, and impact. Do not include
Colonist credentials or session tokens.

## Supported version

Only the latest released version receives security fixes.
