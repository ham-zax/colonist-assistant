export const OVERLAY_STYLES = `
  :host {
    --ca-bg: #0d1821;
    --ca-chrome: #101e28;
    --ca-raised: #12222e;
    --ca-ink: #f1f4ef;
    --ca-copy: #b4c7d5;
    --ca-quiet: #8fa4b3;
    --ca-line: #2b404e;
    --ca-line-strong: #496171;
    --ca-accent: #f1c84b;
    --ca-accent-hover: #ffe18a;
    --ca-success: #7ad7a2;
    --ca-danger: #ef7c72;
    all: initial;
    color: var(--ca-ink);
    font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
    font-size: 13px;
    line-height: 1.4;
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
    width: var(--ca-interface-width, 392px);
    max-height: var(--ca-interface-max-height, min(72vh, 650px));
    flex-direction: column;
    overflow: hidden;
    border-radius: 12px;
    border: 1px solid var(--ca-line);
    background: var(--ca-bg);
    box-shadow: 0 2px 8px rgba(0, 0, 0, .28), 0 20px 56px rgba(3, 10, 15, .54);
    zoom: var(--ca-interface-scale, 1.15);
  }
  .assistant.collapsed { width: min(286px, var(--ca-interface-width, 286px)); }
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
    padding: 0 8px;
    margin-left: auto;
    color: var(--ca-quiet);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: .05em;
    flex-shrink: 0;
  }
  .status i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }
  .status.live { color: var(--ca-accent); }
  .meta-engine-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 22px;
    padding: 0 8px;
    border: 1px solid var(--ca-line);
    border-radius: 999px;
    background: var(--ca-bg);
    color: var(--ca-copy);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: .03em;
    cursor: default;
    white-space: nowrap;
    text-transform: uppercase;
    transition: background-color .15s ease, color .15s ease, border-color .15s ease;
  }
  .meta-engine-chip i {
    width: 6px;
    height: 6px;
    flex: 0 0 6px;
    border-radius: 50%;
    background: var(--ca-success);
  }
  .meta-engine-chip.searching i,
  .meta-engine-chip.slow i {
    background: var(--ca-accent);
    animation: ca-search-pulse 1.25s cubic-bezier(.16, 1, .3, 1) infinite alternate;
  }
  .meta-engine-chip.connecting i {
    background: var(--ca-quiet);
  }
  .meta-engine-chip.error i {
    background: var(--ca-danger);
  }
  .meta-engine-chip.searching,
  .meta-engine-chip.slow {
    color: var(--ca-accent);
    border-color: rgba(241, 200, 75, .3);
  }
  .meta-engine-chip.error {
    color: var(--ca-danger);
    border-color: rgba(224, 86, 76, .35);
  }
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
    transition: color .15s ease, background-color .15s ease;
  }
  .icon-button { width: 34px; flex: 0 0 34px; border-radius: 6px; }
  .icon-button:hover { color: var(--ca-ink); background: #172a37; }
  .icon-button.active {
    color: var(--ca-accent);
    background: #172a37;
  }
  .icon-button svg { width: 16px; height: 16px; }
  button:focus-visible,
  summary:focus-visible {
    outline: 2px solid var(--ca-accent);
    outline-offset: -2px;
  }
  .body {
    display: flex;
    min-height: 0;
    max-height: calc(var(--ca-interface-max-height, min(72vh, 650px)) - 56px);
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
    font-size: 11px;
    text-align: left;
  }
  .model-strip span {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  @keyframes ca-search-pulse {
    from { opacity: .56; }
    to { opacity: 1; }
  }
  .model-strip b {
    flex: 0 0 auto;
    color: var(--ca-accent);
    font-size: 11px;
    letter-spacing: .04em;
  }
  .decision {
    padding: 16px 17px 18px;
    background: var(--ca-raised);
    border-bottom: 1px solid var(--ca-line);
  }
  .decision-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 12px;
    color: var(--ca-quiet);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: .05em;
  }
  .decision-meta span {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .decision-meta span:first-child { color: var(--ca-accent); }
  .decision h1 {
    max-width: 340px;
    margin: 0;
    color: var(--ca-accent);
    font-size: 26px;
    font-weight: 700;
    line-height: 1.1;
    letter-spacing: -.02em;
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
    width: 36px;
    height: 36px;
    flex: 0 0 36px;
    place-items: center;
    margin-top: -2px;
    border-radius: 8px;
    border: 1px solid rgba(241, 200, 75, .24);
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
    margin: 11px 0 14px;
    color: var(--ca-copy);
    font-size: 13px;
    line-height: 1.45;
  }
  .resource-plan {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 13px 0 0;
  }
  .resource-plan > span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 28px;
    padding: 3px 7px 3px 0;
  }
  .resource-plan i {
    display: grid;
    width: 16px;
    height: 20px;
    place-items: center;
    color: var(--resource);
  }
  .resource-plan svg { width: 14px; height: 14px; }
  .resource-plan .native-card-art {
    width: 14px;
    height: 20px;
    object-fit: contain;
  }
  .resource-plan b {
    color: var(--ca-copy);
    font-family: ui-monospace, "SFMono-Regular", Consolas, monospace;
    font-size: 11px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .resource-plan .missing b { color: var(--ca-accent); }
  .resource-plan .ready b { color: var(--ca-success); }
  .trade-flow {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 30px minmax(0, 1fr);
    min-height: 80px;
    margin: 14px 0 0;
    border: 1px solid var(--ca-line);
    border-radius: 8px;
    background: var(--ca-bg);
    overflow: hidden;
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
    font-size: 11px;
    font-style: normal;
    font-weight: 700;
    letter-spacing: .04em;
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
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }
  .trade-resource small {
    min-width: 0;
    align-self: start;
    overflow: hidden;
    color: var(--ca-copy);
    font-size: 11px;
    line-height: 1.2;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .trade-next {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    column-gap: 12px;
    min-height: 40px;
    align-items: center;
    margin: 12px 0 0;
    padding: 8px 0;
    border-top: 1px solid var(--ca-line);
  }
  .trade-next span {
    display: block;
    color: var(--ca-accent);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: .05em;
  }
  .trade-next strong {
    display: block;
    min-width: 0;
    line-height: 1.25;
    overflow-wrap: anywhere;
    font-size: 12.5px;
  }
  .trade-decision .why { margin-top: 13px; }
  .discard-plan {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 14px 0 0;
  }
  .discard-card {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 44px;
    padding: 6px 8px 6px 0;
  }
  .discard-card i {
    display: grid;
    width: 24px;
    height: 32px;
    flex: 0 0 24px;
    place-items: center;
    color: var(--resource);
  }
  .discard-card svg { width: 16px; height: 16px; }
  .discard-card .native-card-art {
    width: 20px;
    height: 28px;
    object-fit: contain;
  }
  .discard-card b {
    min-width: 0;
    overflow-wrap: anywhere;
    color: var(--ca-ink);
    font-family: ui-monospace, "SFMono-Regular", Consolas, monospace;
    font-size: 11.5px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .discard-decision .why { margin-top: 14px; }
  .single-tactic {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px;
    align-items: baseline;
    margin: 12px 0 0;
    padding: 8px 0;
    border-top: 1px solid var(--ca-line);
  }
  .single-tactic span {
    color: var(--ca-accent);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: .05em;
  }
  .single-tactic strong { font-size: 12.5px; font-weight: 700; }
  .more {
    margin: 14px 0 0;
    border-top: 1px solid var(--ca-line);
  }
  .more summary {
    min-height: 38px;
    padding: 6px 0;
    align-content: center;
    color: var(--ca-quiet);
    font-size: 11.5px;
    font-weight: 650;
    cursor: pointer;
    list-style-position: inside;
  }
  .more summary:hover { color: var(--ca-ink); }
  .more > p {
    margin: 0;
    padding: 0 0 10px 14px;
    color: var(--ca-quiet);
    font-size: 12px;
  }
  .alternative {
    display: grid;
    grid-template-columns: 24px 1fr;
    gap: 8px;
    min-height: 44px;
    padding: 6px 0 6px 14px;
    border-top: 1px solid var(--ca-line);
  }
  .alternative > b {
    color: var(--ca-quiet);
    font-family: ui-monospace, "SFMono-Regular", Consolas, monospace;
    font-size: 11px;
  }
  .alternative > span { display: grid; }
  .alternative strong { font-size: 12.5px; }
  .alternative small { margin-top: 2px; color: var(--ca-quiet); font-size: 11px; }
  .board-confirm {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 12px 0 0;
    min-height: 36px;
    padding: 8px 0;
    border-top: 1px solid var(--ca-line);
    color: var(--ca-success);
    font-size: 12px;
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
    min-height: 40px;
    align-items: center;
    gap: 10px;
    margin: 12px 0 0;
    padding: 8px 0;
    border-top: 1px solid var(--ca-line);
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
    font-size: 11px;
    font-weight: 700;
    letter-spacing: .04em;
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
  .cards-heading {
    display: flex;
    min-height: 42px;
    align-items: center;
    padding: 0 14px;
  }
  .cards-heading h2 {
    margin: 0;
    font-size: 16px;
    line-height: 1.2;
  }
  .matrix-head,
  .matrix-row {
    display: grid;
    grid-template-columns: minmax(72px, 1.65fr) repeat(5, minmax(24px, 1fr)) minmax(30px, .85fr);
    align-items: center;
  }
  .matrix-head {
    min-height: 32px;
    padding: 0 8px 0 12px;
    border-top: 1px solid var(--ca-line);
    border-bottom: 1px solid var(--ca-line);
    color: var(--ca-quiet);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: .04em;
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
  .dice-distribution {
    padding: 13px 12px 12px;
    border-bottom: 1px solid var(--ca-line);
    background: var(--ca-chrome);
  }
  .dice-distribution header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }
  .dice-distribution header span {
    color: var(--ca-accent);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: .05em;
  }
  .dice-distribution header b {
    color: var(--ca-quiet);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: .035em;
  }
  .dice-bars {
    display: grid;
    height: 94px;
    grid-template-columns: repeat(11, minmax(0, 1fr));
    align-items: stretch;
    gap: 4px;
    margin-top: 10px;
  }
  .dice-column {
    display: grid;
    min-width: 0;
    grid-template-rows: 17px 58px 19px;
    color: var(--ca-copy);
    text-align: center;
  }
  .dice-count,
  .dice-column > b {
    align-content: center;
    font-family: ui-monospace, "SFMono-Regular", Consolas, monospace;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .dice-count { color: var(--ca-quiet); }
  .dice-column > b { font-weight: 700; }
  .dice-track {
    position: relative;
    display: block;
    overflow: hidden;
    border: 1px solid var(--ca-line);
    background: var(--ca-bg);
  }
  .dice-track .dice-benchmark {
    position: absolute;
    right: 1px;
    bottom: 0;
    left: 1px;
    height: var(--expected-height, 0%);
    max-height: calc(100% - 2px);
    border-top: 1.5px dashed var(--ca-quiet);
    background: rgba(143, 164, 179, .12);
    pointer-events: none;
    transition: height .2s ease, opacity .2s ease;
  }
  .dice-distribution.low-sample .dice-benchmark {
    opacity: .35;
  }
  .dice-legend {
    margin: 7px 0 0;
    color: var(--ca-quiet);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: .035em;
    text-align: right;
  }
  .dice-track i {
    position: absolute;
    right: 2px;
    bottom: 2px;
    left: 2px;
    height: var(--roll-height);
    max-height: calc(100% - 4px);
    background: var(--ca-success);
    transition: height .2s ease;
  }
  .dice-column.has-rolls .dice-track i { min-height: 3px; }
  .dice-column.is-high-yield {
    color: #f08272;
  }
  .dice-column.is-high-yield > b {
    color: #f08272;
    text-shadow: 0 0 8px rgba(240, 130, 114, .3);
  }
  .dice-column.is-high-yield .dice-track i {
    background: #f08272;
  }
  .dice-column.is-seven { color: var(--ca-danger); }
  .dice-column.is-seven > b { color: var(--ca-danger); }
  .dice-column.is-seven .dice-track i { background: var(--ca-danger); }
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
    font-size: 12px;
    font-weight: 700;
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
    min-height: 14px;
    overflow: hidden;
    color: var(--ca-quiet);
    font-size: 11px;
    letter-spacing: .03em;
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
    font-size: 11.5px;
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
    font-size: 11.5px;
  }
  .notice svg { width: 14px; height: 14px; flex: 0 0 14px; }
  .settings-panel { min-height: 330px; }
  .settings-heading {
    padding: 18px 17px 16px;
    border-bottom: 1px solid var(--ca-line);
  }
  .settings-heading > span {
    color: var(--ca-accent);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: .05em;
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
    font-size: 12px;
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
    font-size: 12.5px;
    font-weight: 700;
  }
  .runtime-field small {
    margin-top: 2px;
    color: var(--ca-quiet);
    font-size: 11px;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  .runtime-field > strong {
    display: inline-flex;
    max-width: 118px;
    align-items: center;
    gap: 6px;
    color: var(--ca-success);
    font-size: 11px;
    letter-spacing: .03em;
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
    font-size: 12.5px;
    font-weight: 700;
  }
  .settings-field small {
    margin-top: 2px;
    color: var(--ca-quiet);
    font-size: 11px;
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
    font-size: 11.5px;
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
  .settings-version {
    display: flex;
    min-height: 44px;
    align-items: center;
    justify-content: space-between;
    padding: 0 17px;
    border-bottom: 1px solid var(--ca-line);
    color: var(--ca-quiet);
    font-size: 11px;
    letter-spacing: .04em;
  }
  .settings-version strong {
    color: var(--ca-success);
    font-family: ui-monospace, "SFMono-Regular", Consolas, monospace;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .reset-link {
    width: 100%;
    min-height: 42px;
    border: 0;
    color: var(--ca-quiet);
    background: transparent;
    font-size: 11.5px;
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
  .empty p { max-width: 300px; margin: 8px 0 0; color: var(--ca-copy); font-size: 12.5px; }
  .compact-empty { min-height: 190px; }
  /* Keep the card ledger visible even when an error or explanation is long. */
  .overview {
    display: flex;
    flex-direction: column;
    min-height: 0;
    max-height: calc(var(--ca-interface-max-height, min(72vh, 650px)) - 56px);
  }
  .advice-pane {
    flex: 1 1 auto;
    min-height: 110px;
    overflow: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--ca-line-strong) var(--ca-raised);
  }
  .cards-pane { flex: 0 0 auto; }
  .overview .decision { padding: 13px 14px 14px; }
  .overview .decision-meta { margin-bottom: 9px; }
  .overview .decision h1 { font-size: 23px; }
  .overview .why { margin: 9px 0 11px; overflow-wrap: anywhere; }
  .overview .empty { min-height: 130px; padding: 18px 14px; }
  .overview .empty-mark { display: none; }
  .overview .empty h1 { font-size: 21px; }
  .overview .cards-heading { min-height: 36px; justify-content: space-between; gap: 8px; }
  .cards-heading > span { color: var(--ca-quiet); font-size: 10px; letter-spacing: .045em; }
  .overview .matrix-row { min-height: 44px; }
  .overview .matrix-head { min-height: 30px; }
  .dice-details { border-bottom: 1px solid var(--ca-line); }
  .dice-details > summary {
    padding: 10px 14px;
    color: var(--ca-copy);
    font-size: 12px;
    cursor: pointer;
  }
  .dice-details > summary:hover { color: var(--ca-accent); }
  .dice-details[open] > summary { color: var(--ca-accent); }
  .dice-details .dice-distribution { border-top: 1px solid var(--ca-line); }
  @media (max-width: 700px) {
    .assistant {
      width: var(--ca-interface-width, calc(100vw - 16px));
    }
    .status { display: none; }
    .topbar .product-name { flex: 1; }
    .matrix-head,
    .matrix-row { grid-template-columns: minmax(68px, 1.65fr) repeat(5, minmax(22px, 1fr)) minmax(26px, .85fr); }
  }
  @media (prefers-reduced-motion: reduce) {
    *, *::before, *::after { animation: none !important; transition: none !important; }
  }
  @keyframes live-signal {
    0%, 70%, 100% { opacity: 1; transform: scale(1); }
    82% { opacity: .35; transform: scale(.72); }
  }
`;
