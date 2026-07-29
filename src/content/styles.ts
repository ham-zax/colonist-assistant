export const OVERLAY_STYLES = `
  @font-face {
    font-family: "Archivo Narrow";
    src: url("__CA_FONT_URL__") format("truetype");
    font-style: normal;
    font-weight: 400 700;
    font-display: swap;
  }
  :host {
    --ca-bg: #0d1821;
    --ca-chrome: #101e28;
    --ca-raised: #12222e;
    --ca-ink: #f1f4ef;
    --ca-copy: #9bb0bd;
    --ca-quiet: #6f8594;
    --ca-line: #2b404e;
    --ca-line-strong: #496171;
    --ca-accent: #f1c84b;
    --ca-accent-hover: #ffe18a;
    --ca-success: #7ad7a2;
    --ca-danger: #ef7c72;
    all: initial;
    color: var(--ca-ink);
    font-family: "Archivo Narrow", "Arial Narrow", system-ui, sans-serif;
    font-size: 12.5px;
    line-height: 1.38;
  }
  * { box-sizing: border-box; }
  button { color: inherit; font: inherit; }
  svg,
  img { display: block; }
  .native-card-art,
  .native-piece-art {
    display: block;
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .assistant {
    position: relative;
    z-index: 2147483000;
    display: flex;
    width: 392px;
    max-height: min(72vh, 650px);
    flex-direction: column;
    overflow: hidden;
    border-radius: 12px;
    background: var(--ca-bg);
    box-shadow: 0 18px 54px rgba(3, 10, 15, .42);
  }
  .assistant.collapsed { width: 286px; }
  .topbar {
    display: flex;
    align-items: center;
    min-height: 56px;
    padding: 0 8px 0 13px;
    border-bottom: 1px solid var(--ca-line);
    background: var(--ca-chrome);
    cursor: grab;
    user-select: none;
  }
  .topbar:active { cursor: grabbing; }
  .brand-mark {
    width: 26px;
    height: 26px;
    flex: 0 0 26px;
    margin-right: 9px;
    color: var(--ca-accent);
  }
  .brand-mark svg { width: 100%; height: 100%; }
  .product-name {
    min-width: 0;
    overflow: hidden;
    font-size: 15px;
    font-weight: 700;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .status {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
    padding: 0 8px;
    color: var(--ca-quiet);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: .06em;
  }
  .status i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }
  .status.live { color: var(--ca-accent); }
  .view-button,
  .icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 38px;
    padding: 0;
    border: 0;
    color: var(--ca-copy);
    background: transparent;
    cursor: pointer;
  }
  .view-button {
    gap: 4px;
    min-width: 56px;
    padding: 0 6px;
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: .055em;
  }
  .icon-button { width: 34px; flex: 0 0 34px; }
  .view-button:hover,
  .icon-button:hover { color: var(--ca-ink); background: #172a37; }
  .view-button.active,
  .icon-button.active {
    color: var(--ca-accent);
    background: #172a37;
  }
  .view-button svg { width: 15px; height: 15px; }
  .icon-button svg { width: 16px; height: 16px; }
  button:focus-visible,
  summary:focus-visible {
    outline: 2px solid var(--ca-accent);
    outline-offset: -2px;
  }
  .body {
    display: flex;
    min-height: 0;
    max-height: calc(min(72vh, 650px) - 56px);
    flex: 1;
    flex-direction: column;
  }
  .collapsed .body { display: none; }
  .panel {
    min-height: 0;
    flex: 1;
    overflow: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--ca-line-strong) var(--ca-bg);
  }
  .panel::-webkit-scrollbar { width: 7px; }
  .panel::-webkit-scrollbar-track { background: var(--ca-bg); }
  .panel::-webkit-scrollbar-thumb { background: var(--ca-line-strong); }
  .model-strip {
    display: flex;
    width: 100%;
    min-height: 34px;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 0 13px;
    border: 0;
    border-bottom: 1px solid var(--ca-line);
    color: var(--ca-copy);
    background: var(--ca-raised);
    font-size: 9.5px;
    text-align: left;
  }
  .model-strip span {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .engine-strip > span {
    display: inline-flex;
    align-items: baseline;
    gap: 7px;
  }
  .engine-strip small {
    color: var(--ca-success);
    font-size: 8.5px;
    font-weight: 700;
    letter-spacing: .065em;
  }
  .engine-strip.error small { color: var(--ca-danger); }
  .engine-strip.connecting small { color: var(--ca-quiet); }
  .engine-strip.searching small,
  .engine-strip.slow small {
    color: var(--ca-accent);
    animation: ca-search-pulse 1.25s cubic-bezier(.16, 1, .3, 1) infinite alternate;
  }
  @keyframes ca-search-pulse {
    from { opacity: .56; }
    to { opacity: 1; }
  }
  @media (prefers-reduced-motion: reduce) {
    .engine-strip.searching small,
    .engine-strip.slow small { animation: none; }
  }
  .model-strip b {
    flex: 0 0 auto;
    color: var(--ca-accent);
    font-size: 10px;
    letter-spacing: .06em;
  }
  .engine-strip { cursor: pointer; }
  .engine-strip:hover {
    color: var(--ca-ink);
    background: #172a37;
  }
  .engine-strip:focus-visible {
    outline: 2px solid var(--ca-accent);
    outline-offset: -2px;
  }
  .decision { padding: 19px 17px 0; }
  .decision-meta {
    display: flex;
    justify-content: space-between;
    gap: 14px;
    margin-bottom: 10px;
    color: var(--ca-quiet);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: .075em;
  }
  .decision-meta span {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .decision-meta span:last-child { text-align: right; }
  .decision-meta span:first-child { color: var(--ca-accent); }
  .decision h1 {
    max-width: 340px;
    margin: 0;
    color: var(--ca-accent);
    font-size: 27px;
    font-weight: 700;
    line-height: 1;
    letter-spacing: -.025em;
    text-wrap: balance;
  }
  .decision-command {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  .decision-command h1 { flex: 1; min-width: 0; }
  .command-art {
    display: grid;
    width: 34px;
    height: 34px;
    flex: 0 0 34px;
    place-items: center;
    margin-top: -2px;
    color: var(--ca-accent);
    background: rgba(241, 200, 75, .08);
  }
  .command-art svg,
  .command-art .native-card-art,
  .command-art .native-piece-art {
    width: 27px;
    height: 27px;
    object-fit: contain;
  }
  .decision h2 {
    margin: 11px 0 0;
    color: var(--ca-ink);
    font-size: 15px;
    font-weight: 700;
    line-height: 1.25;
  }
  .why {
    margin: 11px 0 16px;
    color: var(--ca-copy);
    font-size: 12px;
    line-height: 1.48;
  }
  .resource-plan {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 1fr;
    margin: 0 -17px;
    border-top: 1px solid var(--ca-line);
    border-bottom: 1px solid var(--ca-line);
  }
  .resource-plan > span {
    display: grid;
    min-height: 54px;
    place-items: center;
    align-content: center;
    gap: 3px;
    border-right: 1px solid var(--ca-line);
  }
  .resource-plan > span:last-child { border-right: 0; }
  .resource-plan i {
    display: grid;
    width: 24px;
    height: 32px;
    place-items: center;
    color: var(--resource);
  }
  .resource-plan svg { width: 18px; height: 18px; }
  .resource-plan .native-card-art {
    width: 24px;
    height: 32px;
    object-fit: contain;
  }
  .resource-plan b {
    color: var(--ca-copy);
    font-family: ui-monospace, "SFMono-Regular", Consolas, monospace;
    font-size: 9.5px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .resource-plan .missing b { color: var(--ca-accent); }
  .resource-plan .ready b { color: var(--ca-success); }
  .trade-flow {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 30px minmax(0, 1fr);
    min-height: 82px;
    margin: 16px -17px 0;
    border-top: 1px solid var(--ca-line);
    border-bottom: 1px solid var(--ca-line);
    background: var(--ca-raised);
  }
  .trade-flow > div {
    display: grid;
    min-width: 0;
    align-content: center;
    gap: 7px;
    padding: 10px 12px;
  }
  .trade-flow > div > em {
    color: var(--ca-quiet);
    font-size: 9px;
    font-style: normal;
    font-weight: 700;
    letter-spacing: .075em;
  }
  .trade-flow > div > span {
    display: flex;
    min-width: 0;
    flex-wrap: wrap;
    gap: 8px;
  }
  .trade-flow > i {
    display: grid;
    place-items: center;
    border-right: 1px solid var(--ca-line);
    border-left: 1px solid var(--ca-line);
    color: var(--ca-accent);
    font-size: 17px;
    font-style: normal;
  }
  .trade-resource {
    display: grid;
    min-width: 64px;
    grid-template-columns: 25px minmax(0, 1fr);
    grid-template-rows: auto auto;
    align-items: center;
    column-gap: 7px;
  }
  .trade-resource svg,
  .trade-resource .native-card-art {
    width: 25px;
    height: 35px;
    grid-row: 1 / span 2;
    object-fit: contain;
  }
  .trade-resource b {
    align-self: end;
    color: var(--ca-ink);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }
  .trade-resource small {
    min-width: 0;
    align-self: start;
    overflow: hidden;
    color: var(--ca-copy);
    font-size: 9.5px;
    line-height: 1.1;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .trade-next {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    column-gap: 16px;
    min-height: 44px;
    align-items: center;
    margin: 0 -17px;
    padding: 0 17px;
    border-bottom: 1px solid var(--ca-line);
  }
  .trade-next span {
    display: block;
    color: var(--ca-accent);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: .075em;
  }
  .trade-next strong {
    display: block;
    min-width: 0;
    line-height: 1.25;
    overflow-wrap: anywhere;
    font-size: 11.5px;
  }
  .trade-decision .why { margin-top: 13px; }
  .discard-plan {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(104px, 1fr));
    margin: 16px -17px 0;
    border-top: 1px solid var(--ca-line);
    border-bottom: 1px solid var(--ca-line);
  }
  .discard-card {
    display: flex;
    min-width: 0;
    min-height: 54px;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    border-right: 1px solid var(--ca-line);
  }
  .discard-card:last-child { border-right: 0; }
  .discard-card i {
    display: grid;
    width: 28px;
    height: 39px;
    flex: 0 0 28px;
    place-items: center;
    color: var(--resource);
  }
  .discard-card svg { width: 19px; height: 19px; }
  .discard-card .native-card-art {
    width: 28px;
    height: 39px;
    object-fit: contain;
  }
  .discard-card b {
    min-width: 0;
    overflow-wrap: anywhere;
    color: var(--ca-ink);
    font-family: ui-monospace, "SFMono-Regular", Consolas, monospace;
    font-size: 10px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .discard-decision .why { margin-top: 14px; }
  .single-tactic {
    display: grid;
    grid-template-columns: 47px 1fr;
    gap: 10px;
    align-items: baseline;
    margin: 0 -17px;
    padding: 12px 17px;
    border-bottom: 1px solid var(--ca-line);
  }
  .single-tactic span {
    color: var(--ca-accent);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: .08em;
  }
  .single-tactic strong { font-size: 12px; font-weight: 700; }
  .more {
    margin: 0 -17px;
    border-bottom: 1px solid var(--ca-line);
  }
  .more summary {
    min-height: 42px;
    padding: 0 17px;
    align-content: center;
    color: var(--ca-copy);
    font-size: 11px;
    font-weight: 650;
    cursor: pointer;
    list-style-position: inside;
  }
  .more summary:hover { color: var(--ca-ink); }
  .more > p {
    margin: 0;
    padding: 0 17px 11px 32px;
    color: var(--ca-quiet);
    font-size: 10.5px;
  }
  .alternative {
    display: grid;
    grid-template-columns: 24px 1fr;
    gap: 8px;
    min-height: 48px;
    padding: 8px 17px 8px 32px;
    border-top: 1px solid var(--ca-line);
  }
  .alternative > b {
    color: var(--ca-quiet);
    font-family: ui-monospace, "SFMono-Regular", Consolas, monospace;
    font-size: 10px;
  }
  .alternative > span { display: grid; }
  .alternative strong { font-size: 11.5px; }
  .alternative small { margin-top: 2px; color: var(--ca-quiet); font-size: 10px; }
  .board-confirm {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 -17px;
    min-height: 43px;
    padding: 0 17px;
    border-top: 1px solid var(--ca-line);
    border-bottom: 1px solid var(--ca-line);
    color: var(--ca-success);
    font-size: 11px;
    font-weight: 650;
  }
  .board-confirm i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
  }
  .sync-decision { padding-bottom: 0; }
  .sync-status {
    display: flex;
    min-height: 48px;
    align-items: center;
    gap: 10px;
    margin: 0 -17px;
    padding: 0 17px;
    border-top: 1px solid var(--ca-line);
    border-bottom: 1px solid var(--ca-line);
    color: var(--ca-success);
  }
  .sync-status i {
    width: 8px;
    height: 8px;
    flex: 0 0 8px;
    border-radius: 50%;
    background: currentColor;
    animation: live-signal 1.15s cubic-bezier(.16, 1, .3, 1) infinite;
  }
  .sync-status strong {
    color: var(--ca-copy);
    font-size: 11px;
    font-weight: 650;
  }
  .board-marker {
    position: fixed;
    z-index: 2147482999;
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    pointer-events: none;
    transform: translate(-50%, -50%);
  }
  .board-marker i {
    position: absolute;
    inset: 0;
    border: 2px solid var(--ca-accent);
    border-radius: 50%;
    background: rgba(13, 24, 33, .64);
    animation: marker-pulse 1.8s cubic-bezier(.16, 1, .3, 1) infinite;
  }
  .board-marker b {
    position: relative;
    color: var(--ca-bg);
    font-family: ui-monospace, "SFMono-Regular", Consolas, monospace;
    font-size: 13px;
    line-height: 28px;
    text-align: center;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--ca-accent);
  }
  .board-marker span {
    position: absolute;
    top: 46px;
    left: 50%;
    padding: 3px 6px;
    color: var(--ca-bg);
    background: var(--ca-accent);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: .055em;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
    transform: translateX(-50%);
  }
  .board-marker span em {
    display: grid;
    width: 13px;
    height: 13px;
    place-items: center;
    color: var(--ca-bg);
  }
  .board-marker span em svg,
  .board-marker span em img {
    width: 13px;
    height: 13px;
    object-fit: contain;
  }
  .board-marker.near-left span { left: 0; transform: none; }
  .board-marker.near-right span { right: 0; left: auto; transform: none; }
  .board-marker.near-bottom span { top: auto; bottom: 46px; }
  .board-marker.panel-overlap span {
    top: 50%;
    right: 46px;
    bottom: auto;
    left: auto;
    transform: translateY(-50%);
  }
  @keyframes marker-pulse {
    0%, 55%, 100% { opacity: .95; transform: scale(.86); }
    72% { opacity: .3; transform: scale(1.16); }
  }
  .cards-heading { padding: 17px 14px 14px; }
  .cards-heading > span {
    color: var(--ca-accent);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: .075em;
  }
  .cards-heading h1 {
    margin: 3px 0 0;
    font-size: 23px;
    line-height: 1.05;
  }
  .cards-heading p { margin: 5px 0 0; color: var(--ca-copy); font-size: 11px; }
  .matrix-head,
  .matrix-row {
    display: grid;
    grid-template-columns: minmax(88px, 1fr) repeat(5, 36px) 39px;
    align-items: center;
  }
  .matrix-head {
    min-height: 32px;
    padding: 0 8px 0 12px;
    border-top: 1px solid var(--ca-line);
    border-bottom: 1px solid var(--ca-line);
    color: var(--ca-quiet);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: .055em;
    text-align: center;
  }
  .matrix-head > span:first-child { text-align: left; }
  .resource-head {
    display: grid;
    min-width: 18px;
    min-height: 24px;
    place-items: center;
    color: var(--resource);
  }
  .resource-head svg { width: 15px; height: 15px; }
  .resource-head .native-card-art {
    width: 18px;
    height: 24px;
    object-fit: contain;
  }
  .matrix-row {
    min-height: 52px;
    padding: 0 8px 0 12px;
    border-bottom: 1px solid var(--ca-line);
  }
  .matrix-row.is-user { background: var(--ca-raised); }
  .matrix-row.bank-row {
    color: var(--ca-copy);
    background: var(--ca-chrome);
  }
  .bank-row .player-name { padding-left: 0; }
  .bank-row .resource-cell { color: var(--ca-copy); }
  .player-name {
    position: relative;
    display: grid;
    min-width: 0;
    padding-left: 10px;
  }
  .player-name > .player-stripe {
    position: absolute;
    top: 4px;
    bottom: 4px;
    left: 0;
    width: 2px;
    background: var(--player);
  }
  .player-name b {
    display: flex;
    align-items: center;
    gap: 4px;
    overflow: hidden;
    font-size: 11.5px;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .player-awards { display: inline-flex; gap: 2px; }
  .player-name .award {
    display: grid;
    width: 13px;
    height: 13px;
    place-items: center;
    color: var(--ca-accent);
  }
  .player-name .award svg,
  .player-name .award img {
    width: 13px;
    height: 13px;
    object-fit: contain;
  }
  .player-name small {
    min-height: 12px;
    overflow: hidden;
    color: var(--ca-quiet);
    font-size: 10px;
    letter-spacing: .05em;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .resource-cell,
  .total-cell {
    display: grid;
    min-height: 30px;
    place-items: center;
    color: var(--ca-ink);
    font-family: ui-monospace, "SFMono-Regular", Consolas, monospace;
    font-size: 11px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .resource-cell.range {
    color: var(--ca-accent);
    text-decoration: underline dashed;
    text-underline-offset: 3px;
  }
  .total-cell { color: var(--ca-copy); border-left: 1px solid var(--ca-line); }
  .notice {
    display: flex;
    gap: 8px;
    padding: 9px 13px;
    border-bottom: 1px solid #5b4545;
    color: #ffd5cd;
    background: #2b2023;
    font-size: 10.5px;
  }
  .notice svg { width: 14px; height: 14px; flex: 0 0 14px; }
  .settings-panel { min-height: 330px; }
  .settings-heading {
    padding: 18px 17px 16px;
    border-bottom: 1px solid var(--ca-line);
  }
  .settings-heading > span {
    color: var(--ca-accent);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: .075em;
  }
  .settings-heading h1 {
    margin: 3px 0 0;
    font-size: 23px;
    line-height: 1.05;
    letter-spacing: -.02em;
  }
  .settings-heading p {
    margin: 6px 0 0;
    color: var(--ca-copy);
    font-size: 11px;
  }
  .settings-field {
    display: flex;
    min-height: 66px;
    align-items: center;
    gap: 14px;
    padding: 10px 17px;
    border-bottom: 1px solid var(--ca-line);
    cursor: pointer;
  }
  .runtime-field {
    display: flex;
    min-height: 62px;
    align-items: center;
    gap: 12px;
    padding: 9px 17px;
    border-bottom: 1px solid var(--ca-line);
  }
  .runtime-field > span {
    display: grid;
    min-width: 0;
    flex: 1;
  }
  .runtime-field b {
    font-size: 12px;
    font-weight: 700;
  }
  .runtime-field small {
    margin-top: 2px;
    color: var(--ca-quiet);
    font-size: 10.5px;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  .runtime-field > strong {
    display: inline-flex;
    max-width: 118px;
    align-items: center;
    gap: 6px;
    color: var(--ca-success);
    font-size: 9.5px;
    letter-spacing: .04em;
    text-align: right;
  }
  .runtime-field > strong i {
    width: 7px;
    height: 7px;
    flex: 0 0 7px;
    border-radius: 50%;
    background: currentColor;
  }
  .runtime-field[data-runtime="error"] > strong {
    color: var(--ca-danger);
  }
  .runtime-field[data-runtime="connecting"] > strong {
    color: var(--ca-quiet);
  }
  .runtime-field[data-runtime="searching"] > strong {
    color: var(--ca-accent);
  }
  .runtime-field[data-runtime="slow"] > strong {
    color: var(--ca-accent-hover);
  }
  .settings-field > span {
    display: grid;
    min-width: 0;
    flex: 1;
  }
  .settings-field b {
    font-size: 12px;
    font-weight: 700;
  }
  .settings-field small {
    margin-top: 2px;
    color: var(--ca-quiet);
    font-size: 10.5px;
    line-height: 1.35;
  }
  .settings-field select {
    width: 140px;
    min-height: 34px;
    padding: 0 8px;
    border: 1px solid var(--ca-line-strong);
    border-radius: 0;
    color: var(--ca-ink);
    background: var(--ca-chrome);
    font: inherit;
    font-size: 11px;
  }
  .settings-field select:focus-visible {
    outline: 2px solid var(--ca-accent);
    outline-offset: 2px;
  }
  .settings-field input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .settings-field > i {
    position: relative;
    width: 36px;
    height: 21px;
    flex: 0 0 36px;
    border: 1px solid var(--ca-line-strong);
    border-radius: 999px;
  }
  .settings-field > i::after {
    content: "";
    position: absolute;
    top: 3px;
    left: 3px;
    width: 13px;
    height: 13px;
    border-radius: 50%;
    background: var(--ca-copy);
    transition:
      transform .18s cubic-bezier(.16, 1, .3, 1),
      background .18s ease;
  }
  .settings-field input:checked + i {
    border-color: var(--ca-accent);
  }
  .settings-field input:checked + i::after {
    background: var(--ca-accent);
    transform: translateX(15px);
  }
  .settings-field input:focus-visible + i {
    outline: 2px solid var(--ca-accent);
    outline-offset: 2px;
  }
  .engine-field {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 140px;
  }
  .settings-version {
    display: flex;
    min-height: 44px;
    align-items: center;
    justify-content: space-between;
    padding: 0 17px;
    border-bottom: 1px solid var(--ca-line);
    color: var(--ca-quiet);
    font-size: 10px;
    letter-spacing: .06em;
  }
  .settings-version strong {
    color: var(--ca-success);
    font-family: ui-monospace, "SFMono-Regular", Consolas, monospace;
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
  }
  .reset-link {
    width: 100%;
    min-height: 42px;
    border: 0;
    color: var(--ca-quiet);
    background: transparent;
    font-size: 10.5px;
    cursor: pointer;
  }
  .reset-link:hover { color: var(--ca-ink); text-decoration: underline; text-underline-offset: 3px; }
  .empty {
    display: grid;
    min-height: 250px;
    align-content: center;
    justify-items: start;
    padding: 30px 25px;
  }
  .empty-mark { width: 34px; height: 34px; margin-bottom: 20px; color: var(--ca-accent); }
  .empty-mark svg { width: 100%; height: 100%; }
  .empty h1 { max-width: 300px; margin: 0; font-size: 23px; line-height: 1.05; }
  .empty p { max-width: 300px; margin: 8px 0 0; color: var(--ca-copy); font-size: 12px; }
  .compact-empty { min-height: 190px; }
  @media (max-width: 700px) {
    .assistant {
      width: min(392px, calc(100vw - 16px));
      max-height: 68vh;
    }
    .body { max-height: calc(68vh - 56px); }
    .status { display: none; }
    .matrix-head,
    .matrix-row { grid-template-columns: minmax(82px, 1fr) repeat(5, 34px) 37px; }
  }
  @media (prefers-reduced-motion: reduce) {
    *, *::before, *::after { animation: none !important; transition: none !important; }
  }
  @keyframes live-signal {
    0%, 70%, 100% { opacity: 1; transform: scale(1); }
    82% { opacity: .35; transform: scale(.72); }
  }
`;
