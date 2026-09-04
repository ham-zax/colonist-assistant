import type { NativeGpuBuildIdentity } from "../core/engine";
import type { WasmSearchResponse } from "../generated/wasm/colonist_search.js";

export const NATIVE_GPU_HOST = "io.colonist_assistant.gpu";
export const NATIVE_GPU_PROTOCOL_VERSION = 6;
export const NATIVE_GPU_STATE_SCHEMA_VERSION = 3;
const EXPECTED_ENGINE_REVISION = "deep-maxn-v10";

export interface NativeGpuStatus {
  runtime: "gpu-native";
  engineRevision: string;
  build?: NativeGpuBuildIdentity;
  device: {
    backend: string;
    ordinal: number;
    name: string;
    computeCapability: [number, number];
  };
}

interface NativeGpuResponse {
  id: number;
  runtime?: "gpu-native";
  protocolVersion?: number;
  stateSchemaVersion?: number;
  engineRevision?: string;
  build?: NativeGpuBuildIdentity;
  device?: NativeGpuStatus["device"];
  response?: WasmSearchResponse;
  error?: string;
}

interface PendingNativeRequest {
  resolve: (response: NativeGpuResponse) => void;
  reject: (error: Error) => void;
}

class NativeGpuCompatibilityError extends Error {}

const isNativeGpuBuildIdentity = (
  value: NativeGpuBuildIdentity | undefined,
): value is NativeGpuBuildIdentity =>
  Boolean(
    value &&
      typeof value.gitSha === "string" &&
      typeof value.dirty === "boolean" &&
      Number.isSafeInteger(value.builtAtUnixMs) &&
      value.builtAtUnixMs > 0 &&
      /^[0-9a-f]{64}$/u.test(value.ptxSha256),
  );

export class NativeGpuClient {
  private port?: chrome.runtime.Port;
  private nextId = 1;
  private readonly pending = new Map<number, PendingNativeRequest>();
  private statusValue?: NativeGpuStatus;
  private connectPromise?: Promise<NativeGpuStatus | undefined>;
  private unavailable = false;
  private fatalError?: Error;
  private everReady = false;
  private activeAnalyzeId?: number;
  private activeDecisionId?: number;

  async status(): Promise<NativeGpuStatus | undefined> {
    if (this.statusValue && this.port) return this.statusValue;
    if (this.unavailable && !this.everReady) return undefined;
    if (this.fatalError) {
      // A companion that was previously healthy must stay the selected
      // Strategist backend. Retry the native connection, but never reinterpret
      // a post-ready failure as permission to fall back to WASM.
      this.fatalError = undefined;
      this.connectPromise = undefined;
    }
    this.connectPromise ??= this.connect();
    return this.connectPromise;
  }

  release(): void {
    const port = this.port;
    this.port = undefined;
    this.statusValue = undefined;
    this.connectPromise = undefined;
    this.unavailable = false;
    this.fatalError = undefined;
    this.everReady = false;
    if (this.activeAnalyzeId !== undefined && port) {
      try {
        port.postMessage({ type: "cancel", id: this.activeAnalyzeId });
      } catch {
        // Disconnecting the port below also terminates any remaining native work.
      }
    }
    this.activeAnalyzeId = undefined;
    this.activeDecisionId = undefined;
    const error = new Error("GPU companion released");
    for (const pending of this.pending.values()) pending.reject(error);
    this.pending.clear();
    try {
      port?.disconnect();
    } catch {
      // The native port may already be closing.
    }
  }

  async analyze(request: unknown, decisionId?: number): Promise<WasmSearchResponse> {
    const status = await this.status();
    if (!status) throw new Error("GPU companion is not installed");
    if (this.activeAnalyzeId !== undefined) {
      this.cancelAnalyze(this.activeAnalyzeId, "GPU search superseded by a newer decision");
    }
    const { id, response } = this.beginRequest({ type: "analyze", request });
    this.activeAnalyzeId = id;
    this.activeDecisionId = decisionId;
    try {
      const result = await response;
      if (result.error) throw new Error(result.error);
      if (!result.response) {
        throw new Error("GPU companion returned no search response");
      }
      return result.response;
    } finally {
      if (this.activeAnalyzeId === id) {
        this.activeAnalyzeId = undefined;
        this.activeDecisionId = undefined;
      }
    }
  }

  cancelDecision(decisionId: number): void {
    if (
      this.activeAnalyzeId !== undefined &&
      this.activeDecisionId === decisionId
    ) {
      this.cancelAnalyze(this.activeAnalyzeId, "GPU search cancelled as stale");
    }
  }

  private async connect(): Promise<NativeGpuStatus | undefined> {
    try {
      const port = chrome.runtime.connectNative(NATIVE_GPU_HOST);
      this.port = port;
      port.onMessage.addListener((message: unknown) => this.onMessage(message));
      port.onDisconnect.addListener(() => this.onDisconnect(port));
      const hello = await this.request({
        type: "hello",
        protocolVersion: NATIVE_GPU_PROTOCOL_VERSION,
        stateSchemaVersion: NATIVE_GPU_STATE_SCHEMA_VERSION,
      });
      if (hello.error) {
        if (hello.error.startsWith("GPU companion protocol mismatch:")) {
          throw new NativeGpuCompatibilityError(hello.error);
        }
        throw new Error(hello.error);
      }
      if (
        hello.runtime !== "gpu-native" ||
        hello.protocolVersion !== NATIVE_GPU_PROTOCOL_VERSION ||
        hello.stateSchemaVersion !== NATIVE_GPU_STATE_SCHEMA_VERSION ||
        hello.engineRevision !== EXPECTED_ENGINE_REVISION ||
        !hello.device
      ) {
        throw new NativeGpuCompatibilityError(
          `GPU companion is incompatible with this extension (expected protocol/state/engine ${NATIVE_GPU_PROTOCOL_VERSION}/${NATIVE_GPU_STATE_SCHEMA_VERSION}/${EXPECTED_ENGINE_REVISION})`,
        );
      }
      if (hello.build !== undefined && !isNativeGpuBuildIdentity(hello.build)) {
        throw new NativeGpuCompatibilityError("Native GPU build identity is invalid");
      }
      const status: NativeGpuStatus = {
        runtime: hello.runtime,
        engineRevision: hello.engineRevision,
        ...(hello.build ? { build: hello.build } : {}),
        device: hello.device,
      };
      this.statusValue = status;
      this.everReady = true;
      this.unavailable = false;
      return status;
    } catch (error) {
      this.connectPromise = undefined;
      const detail =
        error instanceof Error ? error.message : "GPU companion connection failed";
      if (error instanceof NativeGpuCompatibilityError) {
        this.closePort();
        this.fatalError = error;
        throw error;
      }
      if (this.everReady) {
        this.fatalError = new Error(detail);
        throw this.fatalError;
      }
      this.closePort();
      this.unavailable = true;
      return undefined;
    }
  }

  private request(payload: Record<string, unknown>): Promise<NativeGpuResponse> {
    return this.beginRequest(payload).response;
  }

  private beginRequest(payload: Record<string, unknown>): {
    id: number;
    response: Promise<NativeGpuResponse>;
  } {
    const id = this.nextId++;
    if (!this.port) {
      return {
        id,
        response: Promise.reject(new Error("GPU companion is disconnected")),
      };
    }
    const response = new Promise<NativeGpuResponse>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      try {
        this.port!.postMessage({ ...payload, id });
      } catch (error) {
        this.pending.delete(id);
        reject(
          error instanceof Error
            ? error
            : new Error("GPU companion message could not be sent"),
        );
      }
    });
    return { id, response };
  }

  private cancelAnalyze(id: number, reason: string): void {
    if (this.activeAnalyzeId !== id) return;
    const pending = this.pending.get(id);
    if (pending) {
      this.pending.delete(id);
      pending.reject(new Error(reason));
    }
    try {
      this.port?.postMessage({ type: "cancel", id });
    } catch {
      // A disconnect will reject/clear every remaining request separately.
    }
    if (this.activeAnalyzeId === id) {
      this.activeAnalyzeId = undefined;
      this.activeDecisionId = undefined;
    }
  }

  private onMessage(message: unknown): void {
    if (!message || typeof message !== "object") return;
    const response = message as NativeGpuResponse;
    if (typeof response.id !== "number") return;
    const pending = this.pending.get(response.id);
    if (!pending) return;
    this.pending.delete(response.id);
    pending.resolve(response);
  }

  private onDisconnect(port: chrome.runtime.Port): void {
    if (this.port !== port) return;
    const detail =
      chrome.runtime.lastError?.message ?? "GPU companion disconnected";
    this.port = undefined;
    this.connectPromise = undefined;
    this.statusValue = undefined;
    this.activeAnalyzeId = undefined;
    this.activeDecisionId = undefined;
    const error = new Error(detail);
    for (const pending of this.pending.values()) pending.reject(error);
    this.pending.clear();
    if (this.everReady) this.fatalError = error;
    else this.unavailable = true;
  }

  private closePort(): void {
    const port = this.port;
    this.port = undefined;
    if (!port) return;
    try {
      port.disconnect();
    } catch {
      // The port may already have disconnected while Chrome reported a missing host.
    }
  }
}
