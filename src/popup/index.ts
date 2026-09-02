import type { SessionSummary } from "../content/session";
import {
  DEFAULT_SETTINGS,
  normalizeAutopilotDelaySeconds,
  normalizeDecisionEngine,
  RESET_NONCE_KEY,
  SETTINGS_KEY,
  type AssistantSettings,
} from "../content/settings";
import { latestSummaryKey } from "../content/session";
import {
  ACTIVE_GAME_RECORD_STORAGE_KEY,
  clearCurrentGameStorage,
  LATEST_GAME_RECORD_STORAGE_KEY,
} from "../core/local-data";
import { downloadRecordedGame, readRecordedGame } from "../core/game-record";

const LATEST_SUMMARY_KEY = latestSummaryKey;

const relativeTime = (timestamp: number): string => {
  const seconds = Math.max(0, Math.floor((Date.now() - timestamp) / 1000));
  if (seconds < 10) return "just now";
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  return `${Math.floor(minutes / 60)}h ago`;
};

const boot = async (): Promise<void> => {
  const version = document.querySelector("#version");
  if (version) {
    const manifest = chrome.runtime.getManifest();
    const fullBuild = manifest.version_name ?? `v${manifest.version}`;
    version.textContent = manifest.version_name
      ? manifest.version_name.split(" · ").slice(0, 2).join(" · ")
      : fullBuild;
    version.setAttribute("title", fullBuild);
  }
  const [sync, local] = await Promise.all([
    chrome.storage.sync.get(SETTINGS_KEY),
    chrome.storage.local.get(LATEST_SUMMARY_KEY),
  ]);
  let settings: AssistantSettings = {
    ...DEFAULT_SETTINGS,
    ...(sync[SETTINGS_KEY] as Partial<AssistantSettings> | undefined),
    engine: normalizeDecisionEngine(
      (sync[SETTINGS_KEY] as Partial<AssistantSettings> | undefined)?.engine,
    ),
    autopilotDelaySeconds: normalizeAutopilotDelaySeconds(
      (sync[SETTINGS_KEY] as Partial<AssistantSettings> | undefined)
        ?.autopilotDelaySeconds,
    ),
  };
  const summary = local[LATEST_SUMMARY_KEY] as SessionSummary | undefined;
  let recordedGame = await readRecordedGame();
  const syncSettingControls = (): void => {
    for (const input of document.querySelectorAll<HTMLInputElement>(
      "input[data-setting]",
    )) {
      const key = input.dataset.setting as keyof AssistantSettings;
      input.checked = Boolean(settings[key]);
    }
    for (const select of document.querySelectorAll<HTMLSelectElement>(
      "select[data-setting]",
    )) {
      if (select.dataset.setting === "engine") {
        select.value = settings.engine;
      } else if (select.dataset.setting === "autopilotDelaySeconds") {
        select.value = String(settings.autopilotDelaySeconds);
      }
    }
  };

  const status = document.querySelector("#status");
  if (status) {
    status.innerHTML = summary
      ? `<strong>${summary.playerCount} players · ${summary.eventCount} events</strong><span>${summary.possibilities} possible state${summary.possibilities === 1 ? "" : "s"} · ${relativeTime(summary.updatedAt)}</span>`
      : "<strong>Waiting for a game</strong><span>Open Colonist and start a friendly match.</span>";
    status.classList.toggle("active", Boolean(summary));
  }

  syncSettingControls();
  for (const input of document.querySelectorAll<HTMLInputElement>(
    "input[data-setting]",
  )) {
    const key = input.dataset.setting as keyof AssistantSettings;
    input.addEventListener("change", () => {
      settings = { ...settings, [key]: input.checked };
      void chrome.storage.sync.set({ [SETTINGS_KEY]: settings });
    });
  }

  for (const select of document.querySelectorAll<HTMLSelectElement>(
    "select[data-setting]",
  )) {
    select.addEventListener("change", () => {
      if (select.dataset.setting === "engine") {
        settings = { ...settings, engine: normalizeDecisionEngine(select.value) };
      } else if (select.dataset.setting === "autopilotDelaySeconds") {
        settings = {
          ...settings,
          autopilotDelaySeconds: normalizeAutopilotDelaySeconds(
            Number(select.value),
          ),
        };
      } else {
        return;
      }
      void chrome.storage.sync.set({ [SETTINGS_KEY]: settings });
    });
  }

  chrome.storage.onChanged.addListener((changes, area) => {
    if (area !== "sync" || !changes[SETTINGS_KEY]?.newValue) return;
    const changed = changes[SETTINGS_KEY].newValue as Partial<AssistantSettings>;
    settings = {
      ...DEFAULT_SETTINGS,
      ...settings,
      ...changed,
      engine: normalizeDecisionEngine(changed.engine ?? settings.engine),
      autopilotDelaySeconds: normalizeAutopilotDelaySeconds(
        changed.autopilotDelaySeconds ?? settings.autopilotDelaySeconds,
      ),
    };
    syncSettingControls();
  });

  const exportRecord = document.querySelector<HTMLButtonElement>("#export-record");
  const syncExportRecordControl = (): void => {
    if (!exportRecord) return;
    exportRecord.disabled = !recordedGame;
    exportRecord.title = recordedGame
      ? `${recordedGame.decisions.length} decisions · ${recordedGame.events.length} game events`
      : "Enable Record game and play a match first";
  };
  syncExportRecordControl();
  if (exportRecord) {
    exportRecord.addEventListener("click", () => {
      void (async () => {
        recordedGame = await readRecordedGame();
        syncExportRecordControl();
        if (!recordedGame) return;
        downloadRecordedGame(recordedGame);
      })();
    });
    chrome.storage.onChanged.addListener((changes, area) => {
      if (
        area !== "local" ||
        (
          !changes[ACTIVE_GAME_RECORD_STORAGE_KEY] &&
          !changes[LATEST_GAME_RECORD_STORAGE_KEY]
        )
      ) {
        return;
      }
      void readRecordedGame().then((record) => {
        recordedGame = record;
        syncExportRecordControl();
      });
    });
  }

  document.querySelector("#reset")?.addEventListener("click", () => {
    void (async () => {
      await clearCurrentGameStorage();
      await chrome.storage.sync.set({ [RESET_NONCE_KEY]: Date.now() });
      window.close();
    })();
  });
};

void boot();
