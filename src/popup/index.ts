import type { SessionSummary } from "../content/session";
import {
  DEFAULT_SETTINGS,
  RESET_NONCE_KEY,
  SETTINGS_KEY,
  type AssistantSettings,
} from "../content/settings";
import { latestSummaryKey } from "../content/session";

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
    version.textContent = `v${chrome.runtime.getManifest().version}`;
  }
  const [sync, local] = await Promise.all([
    chrome.storage.sync.get(SETTINGS_KEY),
    chrome.storage.local.get(LATEST_SUMMARY_KEY),
  ]);
  let settings: AssistantSettings = {
    ...DEFAULT_SETTINGS,
    ...(sync[SETTINGS_KEY] as Partial<AssistantSettings> | undefined),
  };
  const summary = local[LATEST_SUMMARY_KEY] as SessionSummary | undefined;

  const status = document.querySelector("#status");
  if (status) {
    status.innerHTML = summary
      ? `<strong>${summary.playerCount} players · ${summary.eventCount} events</strong><span>${summary.possibilities} possible state${summary.possibilities === 1 ? "" : "s"} · ${relativeTime(summary.updatedAt)}</span>`
      : "<strong>Waiting for a game</strong><span>Open Colonist and start a friendly match.</span>";
    status.classList.toggle("active", Boolean(summary));
  }

  for (const input of document.querySelectorAll<HTMLInputElement>("[data-setting]")) {
    const key = input.dataset.setting as keyof AssistantSettings;
    input.checked = Boolean(settings[key]);
    input.addEventListener("change", () => {
      settings = { ...settings, [key]: input.checked };
      void chrome.storage.sync.set({ [SETTINGS_KEY]: settings });
    });
  }

  const engine = document.querySelector<HTMLSelectElement>("#engine");
  if (engine) {
    engine.value = settings.engine;
    engine.addEventListener("change", () => {
      settings = {
        ...settings,
        engine: engine.value as AssistantSettings["engine"],
      };
      void chrome.storage.sync.set({ [SETTINGS_KEY]: settings });
    });
  }

  document.querySelector("#reset")?.addEventListener("click", () => {
    void chrome.storage.sync.set({ [RESET_NONCE_KEY]: Date.now() });
    window.close();
  });
};

void boot();
