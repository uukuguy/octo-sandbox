/**
 * Hand-written TypeScript interfaces mirroring proto/eaasp/runtime/v2/runtime.proto.
 * Used by the ccb-runtime gRPC service implementation.
 *
 * ADR-V2-021 (Phase 3.5 chunk_type contract freeze):
 *   `SendResponse.chunk_type` is now a closed proto enum (ChunkType).
 *   Mirrored here as a numeric enum so values match the wire ints 1:1.
 *   UNSPECIFIED (0) is forbidden for emission.
 */

export enum ChunkType {
  UNSPECIFIED = 0,
  TEXT_DELTA = 1,
  THINKING = 2,
  TOOL_START = 3,
  TOOL_RESULT = 4,
  DONE = 5,
  ERROR = 6,
  WORKFLOW_CONTINUATION = 7,
}

export interface InitializeRequest {
  payload?: SessionPayload;
}

export interface InitializeResponse {
  sessionId: string;
  runtimeId: string;
}

export interface SessionPayload {
  sessionId?: string;
  systemPrompt?: string;
  skillInstructions?: SkillInstructions;
}

export interface SkillInstructions {
  skillId?: string;
  content?: string;
  requiredTools?: string[];
}

export interface SendRequest {
  sessionId: string;
  message?: UserMessage;
}

export interface UserMessage {
  content: string;
  messageType?: string;
  metadata?: Record<string, string>;
}

export interface SendResponse {
  chunkType: ChunkType;
  content: string;
  toolName: string;
  toolId: string;
  isError: boolean;
  error?: RuntimeError;
}

export interface RuntimeError {
  code: string;
  message: string;
}

export interface LoadSkillRequest {
  sessionId: string;
  skill?: SkillInstructions;
}

export interface LoadSkillResponse {
  success: boolean;
  error: string;
}

export interface ToolCallEvent {
  sessionId: string;
  toolName: string;
  toolId: string;
  inputJson: string;
}

export interface ToolCallAck {
  decision: string;
  mutatedInputJson: string;
  reason: string;
}

export interface ToolResultEvent {
  sessionId: string;
  toolName: string;
  toolId: string;
  output: string;
  isError: boolean;
}

export interface ToolResultAck {
  decision: string;
  reason: string;
}

export interface StopEvent {
  sessionId: string;
  reason: string;
}

export interface StopAck {
  decision: string;
  reason: string;
}

export interface StateResponse {
  sessionId: string;
  stateData: Uint8Array;
  runtimeId: string;
  stateFormat: string;
  createdAt: string;
}

export interface ConnectMCPRequest {
  sessionId: string;
  servers?: McpServerConfig[];
}

export interface McpServerConfig {
  name: string;
  transport: string;
  command?: string;
  args?: string[];
  url?: string;
  env?: Record<string, string>;
}

export interface ConnectMCPResponse {
  success: boolean;
  connected: string[];
  failed: string[];
}

export interface DisconnectMcpRequest {
  sessionId: string;
  serverName: string;
}

export interface TelemetryRequest {
  sessionId: string;
  events?: TelemetryEvent[];
}

export interface TelemetryEvent {
  eventType: string;
  payloadJson: string;
  timestamp: string;
}

export interface HealthResponse {
  healthy: boolean;
  runtimeId: string;
  checks: Record<string, string>;
}

export interface Capabilities {
  runtimeId: string;
  model: string;
  contextWindow: number;
  tools: string[];
  supportsNativeHooks: boolean;
  supportsNativeMcp: boolean;
  supportsNativeSkills: boolean;
  costPer1kTokens: number;
  credentialMode: number;
  strengths: string[];
  limitations: string[];
  tier: string;
  deploymentMode: string;
}

export interface EventStreamEntry {
  sessionId: string;
  eventId: string;
  eventType: number;
  payloadJson: string;
  timestamp: string;
}

export type Empty = Record<string, never>;
