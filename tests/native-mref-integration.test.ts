// Run explicitly with COLONIST_NATIVE_BINARY pointing to the built CUDA host.
// D68 supplies a stable resource/board fixture; dice histories below are synthetic
// contract probes, not claims about that historical game's actual roll process.
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { createTrackerState } from "../src/core/tracker";
import { emptyResources } from "../src/core/resources";
import type { PlayerMeta, TrackerState } from "../src/core/types";
import type { BoardSnapshot } from "../src/core/placement";
import { buildDeepSearchRequest } from "../src/worker/deep-search";
import {
  appendPublicDiceRoll, buildLiveDecisionStochasticInput, createDiceHistoryState, observeLogCoverage,
  MREF_COLONIST_LINKED_2024_V1, M0_FAIR_IID_2D6_V1,
} from "../src/core/dice-history";
import { NATIVE_GPU_PROTOCOL_VERSION, NATIVE_GPU_STATE_SCHEMA_VERSION } from "../src/background/native-gpu";

interface Fixture {
  rootPlayer: string;
  board: BoardSnapshot;
  tracker: Pick<TrackerState, "worlds" | "playerOrder" | "eventCount" | "currentTurn" | "recentEvents"> & {
    players: Record<string, Pick<PlayerMeta, "devCards"> & {
      playedDevCards: Partial<PlayerMeta["playedDevCards"]>;
      policyPosterior: PlayerMeta["opponentModel"]["policyPosterior"];
    }>;
  };
}

const fixture = JSON.parse(readFileSync(new URL("./fixtures/gpu-strategic-strength-d68.json", import.meta.url), "utf8")) as Fixture;
const hydrate = (): TrackerState => {
  const state = createTrackerState();
  for (const name of fixture.tracker.playerOrder) {
    const source = fixture.tracker.players[name]!;
    state.players[name] = {
      name, color: "", devCards: structuredClone(source.devCards),
      playedDevCards: { knight: 0, monopoly: 0, "road-building": 0, "year-of-plenty": 0, "victory-point": 0, unknown: 0, ...source.playedDevCards },
      builds: { road: 0, settlement: 0, city: 0, development: 0 },
      resourcesGained: emptyResources(), productionGained: emptyResources(), resourcesSpent: emptyResources(),
      opponentModel: { tradeAccepts: 0, tradeRejects: 0, offersMade: 0, countersMade: 0, policyPosterior: structuredClone(source.policyPosterior) },
    };
  }
  return { ...state, ...structuredClone(fixture.tracker), players: state.players };
};

type Envelope = { id: number; error?: string; runtime?: string; stateSchemaVersion?: number; response?: Record<string, unknown> };
class Host {
  readonly child;
  private buffer = Buffer.alloc(0);
  private readonly pending = new Map<number, { resolve: (value: Envelope) => void; reject: (error: Error) => void }>();
  constructor(binary: string) {
    this.child = spawn(binary, [], { stdio: ["pipe", "pipe", "pipe"] });
    let stderr = "";
    this.child.stderr.on("data", (chunk: Buffer) => { stderr += chunk.toString(); });
    this.child.stdout.on("data", (chunk: Buffer) => {
      this.buffer = Buffer.concat([this.buffer, chunk]);
      while (this.buffer.length >= 4) {
        const length = this.buffer.readUInt32LE(0);
        if (this.buffer.length < length + 4) break;
        const value = JSON.parse(this.buffer.subarray(4, length + 4).toString()) as Envelope;
        this.buffer = this.buffer.subarray(length + 4);
        this.pending.get(value.id)?.resolve(value);
        this.pending.delete(value.id);
      }
    });
    const fail = (error: Error) => { for (const pending of this.pending.values()) pending.reject(error); this.pending.clear(); };
    this.child.on("error", fail);
    this.child.on("exit", (code) => fail(new Error(`native host exit ${code}: ${stderr}`)));
  }
  send(value: Record<string, unknown> & { id: number }): Promise<Envelope> {
    return new Promise((resolve, reject) => {
      this.pending.set(value.id, { resolve, reject });
      const payload = Buffer.from(JSON.stringify(value));
      const header = Buffer.alloc(4);
      header.writeUInt32LE(payload.length);
      this.child.stdin.write(Buffer.concat([header, payload]));
    });
  }
  close(): void { this.child.kill(); }
}

describe("real native Mref integration", () => {
  it.skipIf(!process.env.COLONIST_NATIVE_BINARY)("executes M0, complete Mref and partial Mref without relabeling, and rejects unavailable evidence", async () => {
    const host = new Host(process.env.COLONIST_NATIVE_BINARY!);
    try {
      const oldHello = await host.send({ id: 1, type: "hello", protocolVersion: NATIVE_GPU_PROTOCOL_VERSION, stateSchemaVersion: 3 });
      expect(oldHello.error).toMatch(/protocol mismatch/);
      const hello = await host.send({ id: 2, type: "hello", protocolVersion: NATIVE_GPU_PROTOCOL_VERSION, stateSchemaVersion: NATIVE_GPU_STATE_SCHEMA_VERSION });
      expect(hello.error).toBeUndefined();
      expect(hello.runtime).toBe("gpu-native");
      expect(hello.stateSchemaVersion).toBe(4);
      let id = 3;
      for (const mode of ["m0", "complete", "partial"] as const) {
        const board = structuredClone(fixture.board);
        board.diceMode = mode === "m0" ? "random" : "balanced";
        const history = createDiceHistoryState();
        if (mode === "partial") history.missingPrefixRolls = 8;
        const index = mode === "partial" ? 9 : 0;
        observeLogCoverage(history, [index]);
        appendPublicDiceRoll(history, { actor: board.playerOrder![0]!, total: 8, dice: [3, 5], eventId: "public-roll", logIndex: index });
        const stochastic = buildLiveDecisionStochasticInput(board.diceMode, history, board.playerOrder);
        const { request } = buildDeepSearchRequest(hydrate(), board, fixture.rootPlayer, {}, false, 8, stochastic);
        request.effort = { ...request.effort!, decisionTimeMs: 4000, gpu: { rootCap: 12, rolloutBudget: 384, rolloutSteps: 96 } };
        request.timeBudgetMs = 4000;
        const result = await host.send({ id: id++, type: "analyze", request });
        expect(result.error).toBeUndefined();
        const response = result.response!;
        expect(response.algorithm).toBe("gpu-root-rollout");
        expect(response.stochasticModel).toBe(mode === "m0" ? M0_FAIR_IID_2D6_V1 : MREF_COLONIST_LINKED_2024_V1);
        expect(response.rollouts).toBeGreaterThan(0);
        expect(response.chosen).toBeDefined();
        if (mode === "partial") expect(response.stochasticBeliefParticleCount).toBeGreaterThan(1);
        console.log(JSON.stringify({ mode, model: response.stochasticModel, particles: response.stochasticBeliefParticleCount, rollouts: response.rollouts, chosen: response.chosen }));
        if (mode === "complete") {
          const invalid = structuredClone(request);
          invalid.stochastic = { ...invalid.stochastic!, provenance: "gapped", gaps: [{ afterOrdinal: 0 }] };
          const rejected = await host.send({ id: id++, type: "analyze", request: invalid });
          expect(rejected.error).toBeTruthy();
          expect(rejected.response).toBeUndefined();
        }
      }
    } finally { host.close(); }
  }, 300_000);
});
