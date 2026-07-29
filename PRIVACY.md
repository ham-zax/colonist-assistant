# Privacy Policy — Colonist Assistant

Last updated: July 28, 2026

Colonist Assistant does not collect, transmit, sell, or share personal data. It has no server, analytics, telemetry, advertising, remote code, or extension backend.

## Data the extension processes

While you are on `colonist.io`, the extension processes visible public game-log entries and the game information already available to you. This can include player display names, public in-game actions, tiles, ports, roads, buildings, robber position, public hand and development-card totals, public victory/trophy state and trade ratios, the current turn/action, legal placement targets, your own visible resource hand, and bank counts when the room setting makes them public. A narrowly scoped page bridge sanitizes that game slice before the extension uses it.

For recognizable in-game iconography, the bridge resolves current card and piece image URLs already published by Colonist. The content script accepts only `https://colonist.io/dist/assets/*` or `https://cdn.colonist.io/dist/assets/*` SVG/PNG URLs. Your browser may load those static images from Colonist’s CDN. They are not an analytics channel and no game/session data is placed in the URL.

Colonist Assistant does not read opponents’ hidden resource identities, hidden development-card identities, cookies, account tokens, or network messages. Processing happens entirely inside your browser.

## Data stored locally

Colonist Assistant uses Chrome’s storage APIs for:

- extension preferences and overlay position;
- parsed events from the current game;
- derived possible hand states and a short session summary.

This information stays in your Chrome profile on your device. The extension developer cannot access it.

## Permissions

- `storage`: saves preferences and the current session locally.
- `https://colonist.io/*`: lets the content script read the public game log, receive the sanitized board/self-hand snapshot, and render the overlay only on Colonist.

Colonist Assistant does not request cookies, browsing history, tabs, identity, downloads, web request, or access to all websites.

## Deletion and retention

Use **Reset session** to clear the active in-game model. Chrome removes all extension storage when the extension is uninstalled. Session records are not transmitted or retained anywhere else.

## Changes

Material changes to this policy will be included with the extension source and store update. Because Colonist Assistant has no backend, a policy change cannot silently alter server-side data handling.

## Contact

For privacy questions, use the project’s public issue tracker after the source repository is published.
