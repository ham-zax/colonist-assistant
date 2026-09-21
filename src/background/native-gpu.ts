import type { NativeGpuBuildIdentity } from "../core/engine";
import type { WasmSearchResponse } from "../generated/wasm/colonist_search.js";
import { M0_FAIR_IID_2D6_V1, MREF_COLONIST_LINKED_2024_V1 } from "../core/dice-history";

export const NATIVE_GPU_HOST = "io.colonist_assistant.gpu";
export const NATIVE_GPU_PROTOCOL_VERSION = 7;
export const NATIVE_GPU_STATE_SCHEMA_VERSION = 3;
export const NATIVE_GPU_HANDSHAKE_TIMEOUT_MS = 2_000;
export const NATIVE_GPU_ROLLOUT_ALGORITHM = "gpu-root-rollout";
export const NATIVE_GPU_EXACT_ALGORITHM =
  "deep-maxn-cuda-exact-fixed-work-v1";
export const NATIVE_GPU_STOCHASTIC_MODELS: readonly string[] = [
  M0_FAIR_IID_2D6_V1, MREF_COLONIST_LINKED_2024_V1,
];
const canonicalNativeGpuStochasticModel = (model: string | undefined): string =>
  model === "fair-iid-2d6" ? M0_FAIR_IID_2D6_V1 : model ?? M0_FAIR_IID_2D6_V1;
export const nativeGpuSupportsStochasticModel = (
  model: string | undefined,
  advertisedModels: readonly string[] = NATIVE_GPU_STOCHASTIC_MODELS,
): boolean => {
  const effective = canonicalNativeGpuStochasticModel(model);
  return NATIVE_GPU_STOCHASTIC_MODELS.includes(effective) && advertisedModels.includes(effective);
};
const EXPECTED_ENGINE_REVISION = "deep-maxn-v14";

export interface NativeGpuExactCapability {
  algorithm: string;
  available: boolean;
  supportsDeadline: boolean;
  supportsCancellation: boolean;
  supportsOpening: boolean;
  supportsTypedErrors?: boolean;
  fixedWorkParityOnly: boolean;
  unavailableReason?: string;
}

export interface NativeGpuCapabilities {
  algorithms: readonly string[];
  exactMaxn: NativeGpuExactCapability;
}

export interface NativeGpuStatus {
  runtime: "gpu-native";
  stochasticModels: readonly string[];
  capabilities: NativeGpuCapabilities;
  engineRevision: string;
  build?: NativeGpuBuildIdentity;
  device: {
    backend: string;
    ordinal: number;
    name: string;
    computeCapability: [number, number];
  };
}

type NativeGpuErrorKind =
  | "compatibility"
  | "backend-unavailable"
  | "request"
  | "cancelled";

interface NativeGpuResponse {
  id: number;
  runtime?: "gpu-native";
  protocolVersion?: number;
  stateSchemaVersion?: number;
  stochasticModels?: string[];
  capabilities?: NativeGpuCapabilities;
  engineRevision?: string;
  build?: NativeGpuBuildIdentity;
  device?: NativeGpuStatus["device"];
  response?: WasmSearchResponse;
  errorKind?: NativeGpuErrorKind;
  error?: string;
}

interface PendingNativeRequest {
  resolve: (response: NativeGpuResponse) => void;
  reject: (error: Error) => void;
}

export class NativeGpuUnavailableError extends Error {
  constructor(
    message: string,
    readonly reason: "transport" | "compatibility" | "backend-unavailable",
  ) {
    super(message);
    this.name = "NativeGpuUnavailableError";
  }
}

class NativeGpuCompatibilityError extends NativeGpuUnavailableError {
  constructor(message: string) {
    super(message, "compatibility");
    this.name = "NativeGpuCompatibilityError";
  }
}

const nativeGpuResponseError = (response: NativeGpuResponse): Error => {
  const message = response.error ?? "Native GPU request failed";
  if (response.errorKind === "compatibility") {
    return new NativeGpuUnavailableError(message, "compatibility");
  }
  if (response.errorKind === "backend-unavailable") {
    return new NativeGpuUnavailableError(message, "backend-unavailable");
  }
  return new Error(message);
};

export const isNativeGpuUnavailableError = (
  error: unknown,
): error is NativeGpuUnavailableError => error instanceof NativeGpuUnavailableError;

const isNativeGpuCapabilities = (
  value: NativeGpuCapabilities | undefined,
): value is NativeGpuCapabilities =>
  Boolean(
    value &&
      Array.isArray(value.algorithms) &&
      value.algorithms.every((algorithm) => typeof algorithm === "string") &&
      value.exactMaxn &&
      typeof value.exactMaxn.algorithm === "string" &&
      typeof value.exactMaxn.available === "boolean" &&
      typeof value.exactMaxn.supportsDeadline === "boolean" &&
      typeof value.exactMaxn.supportsCancellation === "boolean" &&
      typeof value.exactMaxn.supportsOpening === "boolean" &&
      (value.exactMaxn.supportsTypedErrors === undefined ||
        typeof value.exactMaxn.supportsTypedErrors === "boolean") &&
      typeof value.exactMaxn.fixedWorkParityOnly === "boolean" &&
      (value.exactMaxn.unavailableReason === undefined ||
        typeof value.exactMaxn.unavailableReason === "string"),
  );

export const nativeGpuSupportsExactMaxn = (
  status: NativeGpuStatus,
): boolean =>
  status.capabilities.exactMaxn.available &&
  status.capabilities.exactMaxn.algorithm === NATIVE_GPU_EXACT_ALGORITHM &&
  status.capabilities.algorithms.includes(NATIVE_GPU_EXACT_ALGORITHM);

export const nativeGpuSupportsProductionExactMaxn = (
  status: NativeGpuStatus,
): boolean =>
  nativeGpuSupportsExactMaxn(status) &&
  status.capabilities.exactMaxn.supportsDeadline &&
  status.capabilities.exactMaxn.supportsCancellation &&
  status.capabilities.exactMaxn.supportsTypedErrors === true &&
  !status.capabilities.exactMaxn.fixedWorkParityOnly;

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
  private connectionGeneration = 0;

  async status(): Promise<NativeGpuStatus | undefined> {
    if (this.statusValue && this.port) return this.statusValue;
    if (this.unavailable && !this.everReady) return undefined;
    if (this.fatalError) {
      // Retry a previously healthy companion. The background decision router
      // decides whether a transport-only reconnect failure can safely preserve
      // the same stochastic model and algorithm on CPU/WASM.
      this.fatalError = undefined;
      this.connectPromise = undefined;
    }
    this.connectPromise ??= this.connect();
    return this.connectPromise;
  }

  release(): void {
    this.connectionGeneration += 1;
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

  async analyzeRollout(
    request: unknown,
    decisionId?: number,
  ): Promise<WasmSearchResponse> {
    const requestedStrategyPolicy = (request as { strategyPolicy?: unknown } | null)
      ?.strategyPolicy;
    if (requestedStrategyPolicy !== undefined) {
      throw new NativeGpuCompatibilityError(
        `GPU rollout does not support strategy policy ${String(requestedStrategyPolicy)}`,
      );
    }
    return this.analyzeWithType(
      "analyze",
      NATIVE_GPU_ROLLOUT_ALGORITHM,
      request,
      decisionId,
      false,
    );
  }

  async analyzeExact(
    request: unknown,
    decisionId?: number,
    signal?: AbortSignal,
  ): Promise<WasmSearchResponse> {
    return this.analyzeWithType(
      "analyze-exact",
      NATIVE_GPU_EXACT_ALGORITHM,
      request,
      decisionId,
      true,
      signal,
    );
  }

  private async analyzeWithType(
    type: "analyze" | "analyze-exact",
    expectedAlgorithm: string,
    request: unknown,
    decisionId: number | undefined,
    requireExactCapability: boolean,
    signal?: AbortSignal,
  ): Promise<WasmSearchResponse> {
    signal?.throwIfAborted();
    const status = await this.status();
    signal?.throwIfAborted();
    if (!status) throw new Error("GPU companion is not installed");
    if (requireExactCapability && !nativeGpuSupportsExactMaxn(status)) {
      throw new NativeGpuCompatibilityError(
        status.capabilities.exactMaxn.unavailableReason ??
          "GPU companion does not expose exact MaxN",
      );
    }
    const requestedModel = (request as { stochastic?: { model?: string } } | null)
      ?.stochastic?.model;
    const expectedStochasticModel = canonicalNativeGpuStochasticModel(requestedModel);
    if (!nativeGpuSupportsStochasticModel(requestedModel, status.stochasticModels)) {
      throw new NativeGpuCompatibilityError(
        `GPU companion does not support stochastic model ${requestedModel}`,
      );
    }
    if (this.activeAnalyzeId !== undefined) {
      this.cancelAnalyze(this.activeAnalyzeId, "GPU search superseded by a newer decision");
    }
    const { id, response } = this.beginRequest({ type, request });
    this.activeAnalyzeId = id;
    this.activeDecisionId = decisionId;
    const abort = () => this.cancelAnalyze(id, "GPU search cancelled as stale");
    signal?.addEventListener("abort", abort, { once: true });
    try {
      const result = await response;
      if (result.error) throw nativeGpuResponseError(result);
      if (!result.response) {
        throw new NativeGpuUnavailableError(
          "GPU companion returned no search response",
          "backend-unavailable",
        );
      }
      if (result.response.algorithm !== expectedAlgorithm) {
        throw new NativeGpuCompatibilityError(
          `GPU companion returned algorithm ${result.response.algorithm}; expected ${expectedAlgorithm}`,
        );
      }
      if (result.response.stochasticModel !== expectedStochasticModel) {
        if (expectedStochasticModel === MREF_COLONIST_LINKED_2024_V1) {
          throw new NativeGpuCompatibilityError(
            "GPU companion returned mismatched Mref stochastic authority",
          );
        }
        throw new NativeGpuCompatibilityError(
          `GPU companion returned stochastic model ${String(result.response.stochasticModel)}; expected ${expectedStochasticModel}`,
        );
      }
      return result.response;
    } finally {
      signal?.removeEventListener("abort", abort);
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
    const generation = ++this.connectionGeneration;
    try {
      const port = chrome.runtime.connectNative(NATIVE_GPU_HOST);
      this.port = port;
      port.onMessage.addListener((message: unknown) => this.onMessage(message));
      port.onDisconnect.addListener(() => this.onDisconnect(port));
      const hello = await this.beginRequest({
        type: "hello",
        protocolVersion: NATIVE_GPU_PROTOCOL_VERSION,
        stateSchemaVersion: NATIVE_GPU_STATE_SCHEMA_VERSION,
      }, NATIVE_GPU_HANDSHAKE_TIMEOUT_MS).response;
      if (generation !== this.connectionGeneration || this.port !== port) {
        throw new NativeGpuUnavailableError(
          "GPU companion disconnected during initialization",
          "transport",
        );
      }
      if (hello.error) {
        if (hello.errorKind === "backend-unavailable") {
          throw nativeGpuResponseError(hello);
        }
        // Handshake errors cannot be request-semantic failures. Older
        // companions predate errorKind, so an untyped hello error is safely
        // treated as compatibility/unavailability and falls back to CPU/WASM.
        throw new NativeGpuCompatibilityError(hello.error);
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
      if (
        !Array.isArray(hello.stochasticModels) ||
        !hello.stochasticModels.every((model) => typeof model === "string")
      ) {
        throw new NativeGpuCompatibilityError(
          "Native GPU stochastic capabilities are invalid",
        );
      }
      if (!isNativeGpuCapabilities(hello.capabilities)) {
        throw new NativeGpuCompatibilityError(
          "Native GPU algorithm capabilities are invalid",
        );
      }
      const status: NativeGpuStatus = {
        runtime: hello.runtime,
        stochasticModels: hello.stochasticModels,
        capabilities: hello.capabilities,
        engineRevision: hello.engineRevision,
        ...(hello.build ? { build: hello.build } : {}),
        device: hello.device,
      };
      this.statusValue = status;
      this.everReady = true;
      this.unavailable = false;
      return status;
    } catch (error) {
      // A released handshake may settle after a new connection has started.
      // Its completion must not clear or poison the new connection's state.
      if (generation !== this.connectionGeneration) throw error;
      this.connectPromise = undefined;
      this.closePort();
      const detail =
        error instanceof Error ? error.message : "GPU companion connection failed";
      if (error instanceof NativeGpuUnavailableError) {
        if (error.reason === "transport" && !this.everReady) {
          this.unavailable = true;
          return undefined;
        }
        this.fatalError = error;
        throw error;
      }
      if (this.everReady) {
        this.fatalError = new NativeGpuUnavailableError(detail, "transport");
        throw this.fatalError;
      }
      this.unavailable = true;
      return undefined;
    }
  }

  private beginRequest(payload: Record<string, unknown>, timeoutMs?: number): {
    id: number;
    response: Promise<NativeGpuResponse>;
  } {
    const id = this.nextId++;
    if (!this.port) {
      return {
        id,
        response: Promise.reject(
          new NativeGpuUnavailableError("GPU companion is disconnected", "transport"),
        ),
      };
    }
    const response = new Promise<NativeGpuResponse>((resolve, reject) => {
      const timer = timeoutMs === undefined ? undefined : globalThis.setTimeout(() => {
        this.pending.delete(id);
        reject(
          new NativeGpuUnavailableError("GPU companion handshake timed out", "transport"),
        );
      }, timeoutMs);
      const clearTimer = () => {
        if (timer !== undefined) globalThis.clearTimeout(timer);
      };
      const pending: PendingNativeRequest = {
        resolve: (value) => { clearTimer(); resolve(value); },
        reject: (error) => { clearTimer(); reject(error); },
      };
      this.pending.set(id, pending);
      try {
        this.port!.postMessage({ ...payload, id });
      } catch (error) {
        this.pending.delete(id);
        pending.reject(
          new NativeGpuUnavailableError(
            error instanceof Error
              ? error.message
              : "GPU companion message could not be sent",
            "transport",
          ),
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
    const error = new NativeGpuUnavailableError(detail, "transport");
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
