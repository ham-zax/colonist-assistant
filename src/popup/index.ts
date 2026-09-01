import type { SessionSummary } from "../content/session";
import {
  DEFAULT_SETTINGS,
  normalizeAutopilotDelaySeconds,
  RESET_NONCE_KEY,
  SETTINGS_KEY,
  type AssistantSettings,
} from "../content/settings";
import { latestSummaryKey } from "../content/session";
import { clearCurrentGameStorage } from "../core/local-data";

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
    engine: "deep-search",
    autopilotDelaySeconds: normalizeAutopilotDelaySeconds(
      (sync[SETTINGS_KEY] as Partial<AssistantSettings> | undefined)
        ?.autopilotDelaySeconds,
    ),
  };
  const summary = local[LATEST_SUMMARY_KEY] as SessionSummary | undefined;

  const status = document.querySelector("#status");
  if (status) {
    status.innerHTML = summary
      ? `<strong>${summary.playerCount} players · ${summary.eventCount} events</strong><span>${summary.possibilities} possible state${summary.possibilities === 1 ? "" : "s"} · ${relativeTime(summary.updatedAt)}</span>`
      : "<strong>Waiting for a game</strong><span>Open Colonist and start a friendly match.</span>";
    status.classList.toggle("active", Boolean(summary));
  }

  for (const input of document.querySelectorAll<HTMLInputElement>(
    "input[data-setting]",
  )) {
    const key = input.dataset.setting as keyof AssistantSettings;
    input.checked = Boolean(settings[key]);
    input.addEventListener("change", () => {
      settings = { ...settings, [key]: input.checked };
      void chrome.storage.sync.set({ [SETTINGS_KEY]: settings });
    });
  }

  for (const select of document.querySelectorAll<HTMLSelectElement>(
    "select[data-setting]",
  )) {
    if (select.dataset.setting !== "autopilotDelaySeconds") continue;
    select.value = String(settings.autopilotDelaySeconds);
    select.addEventListener("change", () => {
      settings = {
        ...settings,
        autopilotDelaySeconds: normalizeAutopilotDelaySeconds(
          Number(select.value),
        ),
      };
      void chrome.storage.sync.set({ [SETTINGS_KEY]: settings });
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
