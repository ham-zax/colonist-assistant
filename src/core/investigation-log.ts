export type InvestigationKind =
  | "board"
  | "dom"
  | "dice"
  | "decision"
  | "restore"
  | "system";

export interface InvestigationEntry {
  seq: number;
  at: number;
  kind: InvestigationKind;
  data: Record<string, unknown>;
  /** Identical follow-up observations folded into this entry. */
  repeat?: number;
  lastAt?: number;
}

export interface InvestigationExportMeta {
  build?: string;
  exportedAt?: number;
  saveError?: string;
}

export interface InvestigationSnapshot {
  schema: "colonist-investigation/1";
  gameKey?: string;
  entries: InvestigationEntry[];
}

const DEFAULT_MAX_ENTRIES = 1200;

export class InvestigationLog {
  private gameKey?: string;
  private entries: InvestigationEntry[] = [];
  private sequence = 0;

  constructor(private readonly maxEntries = DEFAULT_MAX_ENTRIES) {
    if (!Number.isInteger(maxEntries) || maxEntries < 1) {
      throw new Error("Investigation log maxEntries must be a positive integer");
    }
  }

  setGame(gameKey: string | undefined): void {
    const normalized = gameKey?.trim() || undefined;
    if (normalized === this.gameKey) return;
    this.gameKey = normalized;
    this.entries = [];
    this.sequence = 0;
  }

  replace(snapshot: InvestigationSnapshot): void {
    this.gameKey = snapshot.gameKey?.trim() || undefined;
    this.entries = snapshot.entries
      .slice(-this.maxEntries)
      .map((entry) => ({
        seq: entry.seq,
        at: entry.at,
        kind: entry.kind,
        data: { ...entry.data },
        ...(entry.repeat ? { repeat: entry.repeat, lastAt: entry.lastAt } : {}),
      }));
    this.sequence = this.entries.at(-1)?.seq ?? 0;
  }

  /** Fold an identical repeat into an existing entry; false if it was evicted. */
  bump(seq: number, at = Date.now()): boolean {
    const entry = this.entries.find((candidate) => candidate.seq === seq);
    if (!entry) return false;
    entry.repeat = (entry.repeat ?? 0) + 1;
    entry.lastAt = at;
    return true;
  }

  record(kind: InvestigationKind, data: Record<string, unknown>, at = Date.now()): number {
    this.sequence += 1;
    this.entries.push({
      seq: this.sequence,
      at,
      kind,
      data: { ...data },
    });
    if (this.entries.length > this.maxEntries) {
      this.entries.splice(0, this.entries.length - this.maxEntries);
    }
    return this.sequence;
  }

  snapshot(): InvestigationSnapshot {
    return {
      schema: "colonist-investigation/1",
      ...(this.gameKey ? { gameKey: this.gameKey } : {}),
      entries: this.entries.map((entry) => ({
        ...entry,
        data: { ...entry.data },
      })),
    };
  }
}

export const formatInvestigationLog = (
  snapshot: InvestigationSnapshot,
  meta: InvestigationExportMeta = {},
): string => {
  const lines = [
    `@schema=${snapshot.schema}`,
    `@gameKey=${JSON.stringify(snapshot.gameKey ?? null)}`,
    ...(meta.build ? [`@build=${JSON.stringify(meta.build)}`] : []),
    ...(meta.exportedAt !== undefined ? [`@exported=${meta.exportedAt}`] : []),
    ...(meta.saveError ? [`@saveError=${JSON.stringify(meta.saveError)}`] : []),
    `@entries=${snapshot.entries.length}`,
    `@repeat=${JSON.stringify("repeat/lastAt count identical follow-up observations folded into the entry")}`,
    "@format=jsonl",
    "",
  ];
  for (const entry of snapshot.entries) {
    lines.push(JSON.stringify(entry));
  }
  return `${lines.join("\n")}\n`;
};
