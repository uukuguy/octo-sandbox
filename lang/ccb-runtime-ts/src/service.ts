/**
 * CcbRuntimeService — gRPC service implementation for ccb-runtime.
 *
 * Implements all 16 RuntimeService methods from the EAASP L1 contract
 * (ADR-V2-017). This is a scaffold — actual ccb subprocess wiring lands
 * in a follow-up task; Send() returns a single "done" chunk for now.
 */

import type {
  Capabilities,
  ConnectMCPRequest,
  ConnectMCPResponse,
  DisconnectMcpRequest,
  Empty,
  EventStreamEntry,
  HealthResponse,
  InitializeRequest,
  InitializeResponse,
  LoadSkillRequest,
  LoadSkillResponse,
  StateResponse,
  StopAck,
  StopEvent,
  TelemetryRequest,
  ToolCallAck,
  ToolCallEvent,
  ToolResultAck,
  ToolResultEvent,
  SendRequest,
  SendResponse,
} from "./proto/types.js";
import { ChunkType } from "./proto/types.js";

const RUNTIME_ID = "eaasp-ccb-runtime";

export class CcbRuntimeService {
  private currentSessionId: string | null = null;
  private deploymentMode: string;

  constructor(deploymentMode = "shared") {
    this.deploymentMode = deploymentMode;
  }

  initialize(req: InitializeRequest): InitializeResponse {
    const sessionId = req.payload?.sessionId ?? crypto.randomUUID();
    this.currentSessionId = sessionId;
    return { sessionId, runtimeId: RUNTIME_ID };
  }

  /**
   * Send returns an async generator of SendResponse chunks.
   * Stub: immediately yields a "done" chunk (no subprocess).
   */
  async *send(req: SendRequest): AsyncGenerator<SendResponse> {
    // ADR-V2-021: chunk_type is the proto ChunkType enum (int on wire).
    // The upstream ccb runtime is still a stub — only text/done/error
    // semantics are emitted here; TOOL_START / TOOL_RESULT will be wired
    // when real Anthropic TS SDK streaming lands (§S1.T7 scope note).
    if (!this.currentSessionId) {
      yield {
        chunkType: ChunkType.ERROR,
        content: "no active session; call Initialize first",
        toolName: "",
        toolId: "",
        isError: true,
      };
      return;
    }
    // Stub: echo content back as a text chunk then done.
    if (req.message?.content) {
      yield {
        chunkType: ChunkType.TEXT_DELTA,
        content: req.message.content,
        toolName: "",
        toolId: "",
        isError: false,
      };
    }
    yield {
      chunkType: ChunkType.DONE,
      content: "end_turn",
      toolName: "",
      toolId: "",
      isError: false,
    };
  }

  loadSkill(_req: LoadSkillRequest): LoadSkillResponse {
    return { success: true, error: "" };
  }

  onToolCall(req: ToolCallEvent): ToolCallAck {
    console.debug(`[ccb-runtime] OnToolCall stub — allow: ${req.toolName}`);
    return { decision: "allow", mutatedInputJson: "", reason: "" };
  }

  onToolResult(req: ToolResultEvent): ToolResultAck {
    console.debug(`[ccb-runtime] OnToolResult stub — allow: ${req.toolName}`);
    return { decision: "allow", reason: "" };
  }

  onStop(req: StopEvent): StopAck {
    console.debug(`[ccb-runtime] OnStop stub — allow: ${req.sessionId}`);
    return { decision: "allow", reason: "" };
  }

  getState(): StateResponse {
    const sessionId = this.currentSessionId ?? "";
    return {
      sessionId,
      stateData: new Uint8Array(),
      runtimeId: RUNTIME_ID,
      stateFormat: "ccb-stub-v1",
      createdAt: new Date().toISOString(),
    };
  }

  connectMcp(req: ConnectMCPRequest): ConnectMCPResponse {
    const connected = (req.servers ?? []).map((s) => s.name);
    return { success: true, connected, failed: [] };
  }

  emitTelemetry(_req: TelemetryRequest): Empty {
    return {};
  }

  getCapabilities(): Capabilities {
    return {
      runtimeId: RUNTIME_ID,
      model: "",
      contextWindow: 0,
      tools: [],
      supportsNativeHooks: false,
      supportsNativeMcp: false,
      supportsNativeSkills: false,
      costPer1kTokens: 0,
      credentialMode: 0,
      strengths: ["ccb-bun-typescript"],
      limitations: ["stub-send"],
      tier: "aligned",
      deploymentMode: this.deploymentMode,
    };
  }

  terminate(): Empty {
    this.currentSessionId = null;
    return {};
  }

  restoreState(req: StateResponse): Empty {
    this.currentSessionId = req.sessionId;
    return {};
  }

  health(): HealthResponse {
    return { healthy: true, runtimeId: RUNTIME_ID, checks: {} };
  }

  disconnectMcp(_req: DisconnectMcpRequest): Empty {
    return {};
  }

  pauseSession(): StateResponse {
    return this.getState();
  }

  resumeSession(req: StateResponse): Empty {
    return this.restoreState(req);
  }

  emitEvent(_req: EventStreamEntry): Empty {
    return {};
  }
}
