import {
  InvestigationLog,
  formatInvestigationLog,
  type InvestigationKind,
  type InvestigationSnapshot,
} from "../core/investigation-log";
import { isExtensionContextInvalidatedError } from "./extension-context";

export const INVESTIGATION_STORAGE_KEY = "colonistAssistantInvestigationLog";
const SAVE_DELAY_MS = 250;

const isStoredSnapshot = (value: unknown): value is InvestigationSnapshot => {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<InvestigationSnapshot>;
  return candidate.schema === "colonist-investigation/1" && Array.isArray(candidate.entries);
};

const safeGameKey = (gameKey: string | undefined): string =>
  (gameKey ?? "unknown-game")
    .replace(/[^a-z0-9]+/giu, "-")
    .replace(/^-+|-+$/gu, "")
    .slice(0, 48) || "unknown-game";

export class InvestigationRecorder {
  private readonly log: InvestigationLog;
  private readonly lastTransitionByChannel = new Map<string, string>();
  private enabled = false;
  private saveTimer: number | undefined;

  constructor(maxEntries?: number) {
    this.log = new InvestigationLog(maxEntries);
  }

  async initialize(enabled: boolean, gameKey?: string): Promise<void> {
    this.enabled = enabled;
    this.lastTransitionByChannel.clear();
    this.log.setGame(gameKey);
    try {
      const result = await chrome.storage.local.get(INVESTIGATION_STORAGE_KEY);
      const stored = result[INVESTIGATION_STORAGE_KEY];
      if (
        isStoredSnapshot(stored) &&
        (stored.gameKey?.trim() || undefined) === (gameKey?.trim() || undefined)
      ) {
        this.log.replace(stored);
      }
    } catch (error) {
      if (isExtensionContextInvalidatedError(error)) {
        this.enabled = false;
        return;
      }
      throw error;
    }
  }

  setEnabled(enabled: boolean): void {
    if (enabled === this.enabled) return;
    if (!enabled) void this.flush();
    this.enabled = enabled;
    if (enabled) {
      this.lastTransitionByChannel.clear();
      this.scheduleSave();
    }
  }

  isEnabled(): boolean {
    return this.enabled;
  }

  setGame(gameKey?: string): void {
    const before = this.log.snapshot().gameKey;
    this.log.setGame(gameKey);
    if (before !== this.log.snapshot().gameKey) {
      this.lastTransitionByChannel.clear();
      if (this.enabled) this.scheduleSave();
    }
  }

  record(kind: InvestigationKind, data: Record<string, unknown>): void {
    if (!this.enabled) return;
    const discriminator = data.action ?? data.phase ?? "state";
    const shouldDedupe = kind === "dice" &&
      (discriminator === "board-roll-duplicate" || discriminator === "source-reconciliation");
    if (shouldDedupe) {
      const channel = `${kind}:${String(discriminator)}`;
      const signature = JSON.stringify(data);
      if (this.lastTransitionByChannel.get(channel) === signature) return;
      this.lastTransitionByChannel.set(channel, signature);
    }
    this.log.record(kind, data);
    this.scheduleSave();
  }

  snapshot(): InvestigationSnapshot {
    return this.log.snapshot();
  }

  async flush(): Promise<void> {
    if (this.saveTimer !== undefined) {
      window.clearTimeout(this.saveTimer);
      this.saveTimer = undefined;
    }
    try {
      await chrome.storage.local.set({
        [INVESTIGATION_STORAGE_KEY]: this.log.snapshot(),
      });
    } catch (error) {
      if (isExtensionContextInvalidatedError(error)) {
        this.enabled = false;
        return;
      }
      throw error;
    }
  }

  async clear(): Promise<void> {
    const gameKey = this.log.snapshot().gameKey;
    this.lastTransitionByChannel.clear();
    this.log.setGame(undefined);
    this.log.setGame(gameKey);
    if (this.saveTimer !== undefined) {
      window.clearTimeout(this.saveTimer);
      this.saveTimer = undefined;
    }
    try {
      await chrome.storage.local.remove(INVESTIGATION_STORAGE_KEY);
    } catch (error) {
      if (isExtensionContextInvalidatedError(error)) {
        this.enabled = false;
        return;
      }
      throw error;
    }
  }

  download(): void {
    const snapshot = this.log.snapshot();
    const blob = new Blob([formatInvestigationLog(snapshot)], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    const stamp = new Date().toISOString().replace(/[:.]/gu, "-");
    anchor.href = url;
    anchor.download = `colonist-investigation-${safeGameKey(snapshot.gameKey)}-${stamp}.txt`;
    anchor.style.display = "none";
    document.documentElement.append(anchor);
    anchor.click();
    anchor.remove();
    globalThis.setTimeout(() => URL.revokeObjectURL(url), 0);
  }

  private scheduleSave(): void {
    if (this.saveTimer !== undefined) window.clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => {
      this.saveTimer = undefined;
      void this.flush();
    }, SAVE_DELAY_MS);
  }
}

export const investigationRecorder = new InvestigationRecorder();
