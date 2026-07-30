# Security

## Data boundary

Colonist Assistant is a Manifest V3 extension scoped to
`https://colonist.io/*`. It has no backend, telemetry, analytics, advertising,
remote JavaScript, or cross-site host access.

The page bridge emits a validated, JSON-serializable game snapshot. The content
script rejects malformed topology, out-of-range counters, unknown action
states, duplicate IDs, invalid screen coordinates, and non-Colonist asset
URLs. Search runs in the extension-origin background service worker, with a
packaged Rust/WASM Strategist covered by the extension-page CSP. If the
service worker or WASM engine is unavailable, the extension fails closed,
shows an engine error, and does not substitute a content-script action policy.

Autonomous clicks require all of the following:

- the user enabled autopilot;
- the validated public board identifies a private game or an all-bot match;
- the current game, turn, phase, state signature, and legal target set still
  match the state used to select the action;
- the action signature has not already been completed; duplicate dispatches
  are blocked except for bounded, legality-validated retries while waiting for
  an authoritative board commit; and
- every multi-click continuation observes the expected authoritative commit.

On a state or commit mismatch, the workflow cancels and replans. It does not
continue a stale sequence or replace Strategist with a heuristic action.

## Reporting

After the source repository is published, report vulnerabilities privately to
the maintainer rather than opening a public exploit report. Include the
extension version, affected URL, reproduction steps, and impact. Do not include
Colonist credentials or session tokens.

## Supported version

Only the latest released version receives security fixes.
