/**
 * Tauri System Commands — non-node Tauri IPC wrappers.
 *
 * Node-CRUD operations were removed in C1a (#1251); use backendAdapter directly.
 */

import type {
  AcpAgentInfo,
  AgentSession,
  AgentTurnResult,
  LocalAgentStatus
} from '$lib/types/agent-types';
import { invoke } from '@tauri-apps/api/core';

// ============================================================================
// Environment Detection
// ============================================================================

/** Check if running in a Tauri desktop environment. */
function isTauri(): boolean {
  return (
    typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
  );
}

// ============================================================================
// Local Agent Commands (Issue #1008)
// ============================================================================

/**
 * Get the current local agent status.
 */
export async function localAgentStatus(): Promise<LocalAgentStatus> {
  if (!isTauri()) return { status: 'idle' };
  return invoke<LocalAgentStatus>('local_agent_status');
}

/**
 * Create a new local agent conversation session.
 * @returns Session ID
 */
export async function localAgentNewSession(modelId: string): Promise<string> {
  if (!isTauri()) return `mock-session-${Date.now()}`;
  return invoke<string>('local_agent_new_session', { modelId });
}

/**
 * Send a user message and run one agent turn.
 * Streaming chunks are delivered via Tauri events (local-agent://chunk).
 * @returns Final turn result when generation completes.
 */
export async function localAgentSend(sessionId: string, message: string): Promise<AgentTurnResult> {
  if (!isTauri()) {
    return {
      response: 'Mock response (Tauri not available)',
      tool_calls_made: [],
      usage: { prompt_tokens: 0, completion_tokens: 0 }
    };
  }
  return invoke<AgentTurnResult>('local_agent_send', { sessionId, message });
}

/**
 * Cancel an in-progress generation for the given session.
 */
export async function localAgentCancel(sessionId: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>('local_agent_cancel', { sessionId });
}

/**
 * End a local agent session, freeing all resources.
 */
export async function localAgentEndSession(sessionId: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>('local_agent_end_session', { sessionId });
}

/**
 * Get all active local agent sessions.
 */
export async function localAgentGetSessions(): Promise<AgentSession[]> {
  if (!isTauri()) return [];
  return invoke<AgentSession[]>('local_agent_get_sessions');
}

// ============================================================================
// Chat Model Management Commands (Issue #1008)
// ============================================================================

/** Lifecycle status of a catalog model (tagged union from the daemon). */
export interface ChatModelStatus {
  status: 'not_downloaded' | 'downloading' | 'verifying' | 'ready' | 'loaded' | 'error';
  [key: string]: unknown;
}

/**
 * A model entry from `chat_model_list`. Built-in (GGUF) and Ollama models are
 * merged into one list, distinguished by `backend`. Shape matches the daemon's
 * `chat_model_list` command output (camelCase).
 */
export interface ChatModelEntry {
  id: string;
  name: string;
  backend: 'gguf' | 'ollama';
  status: ChatModelStatus;
  sizeBytes: number;
  quantization: string;
  minMemoryGb: number;
}

/**
 * List all models in the local catalog (built-in GGUF + Ollama when running).
 */
export async function chatModelList(): Promise<ChatModelEntry[]> {
  if (!isTauri()) return [];
  return invoke<ChatModelEntry[]>('chat_model_list');
}

/**
 * Get the recommended model ID based on system RAM.
 */
export async function chatModelRecommended(): Promise<string> {
  if (!isTauri()) return 'ministral-3b-q4km';
  return invoke<string>('chat_model_recommended');
}

/**
 * Download a model. Progress events are emitted via model://download-progress.
 */
export async function chatModelDownload(modelId: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>('chat_model_download', { modelId });
}

/**
 * Cancel an in-progress model download.
 */
export async function chatModelCancelDownload(modelId: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>('chat_model_cancel_download', { modelId });
}

/**
 * Delete a downloaded model from disk.
 */
export async function chatModelDelete(modelId: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>('chat_model_delete', { modelId });
}

/**
 * Load a downloaded model into memory for inference.
 */
export async function chatModelLoad(modelId: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>('chat_model_load', { modelId });
}

/**
 * Unload the currently loaded model, freeing resources.
 */
export async function chatModelUnload(): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>('chat_model_unload');
}

/** Return total system RAM in GiB (rounded down). Returns 0 outside Tauri. */
export function getSystemRamGb(): Promise<number> {
  if (!isTauri()) return Promise.resolve(0);
  return invoke<number>('get_system_ram_gb');
}

/**
 * Whether a local Ollama server is reachable (pings its API with a short
 * timeout). Used to gray out the Ollama provider mode when unavailable.
 * Returns false outside Tauri.
 */
export function ollamaAvailable(): Promise<boolean> {
  if (!isTauri()) return Promise.resolve(false);
  return invoke<boolean>('ollama_available');
}

/**
 * Ensure a model is downloaded, loaded, and the inference engine is ready.
 * Handles full lifecycle: download → load → engine swap.
 * Emits model://status and model://download-progress events during the process.
 */
/** Returns true if the engine was (re-)installed and sessions were dropped. */
export async function ensureModelReady(modelId: string): Promise<boolean> {
  if (!isTauri()) return false;
  return invoke<boolean>('ensure_model_ready', { modelId });
}

// ============================================================================
// ACP Commands — temporarily disabled
// ============================================================================
//
// The ACP transport and Tauri command bridge were removed in #1117 ahead of
// the PTY-based agent rewrite (ADR-032). The wrappers below stay so callers
// in the chat store / agent store don't need to be edited mid-transition,
// but they no longer invoke any Tauri command. The follow-up PTY-UI issue
// replaces these with real PTY-spawn commands.

export async function acpListAgents(): Promise<AcpAgentInfo[]> {
  return [];
}

export async function acpStartSession(_agentId: string): Promise<string> {
  return `pty-session-pending-${Date.now()}`;
}

export async function acpSendMessage(_sessionId: string, _message: string): Promise<void> {
  return;
}

export async function acpEndSession(_sessionId: string): Promise<void> {
  return;
}

export async function acpRefreshAgents(): Promise<AcpAgentInfo[]> {
  return [];
}

// ============================================================================
// PTY Agent Session Commands (Issue #1120)
// ============================================================================

export interface PtyLaunchInput {
  agentType: string;
  prompt?: string | null;
  cols: number;
  rows: number;
  /**
   * ID of the `ai-chat` node this PTY session is a view onto (provider mode 2d,
   * per ADR-034). When set, capture backfills this node at session end instead
   * of minting a new one. Omit to launch a session not bound to a node (capture
   * is then skipped).
   */
  nodeId?: string | null;
}

export interface PtyLaunchResult {
  sessionId: string;
  createdAt: number;
}

export interface PtySessionInfo {
  sessionId: string;
  agentType: string;
  startedAt: number;
}

export interface PtyListSessionsResult {
  sessions: PtySessionInfo[];
  count: number;
}

export interface PtyTerminateResult {
  sessionId: string;
  wasRunning: boolean;
}

export async function ptyLaunchSession(input: PtyLaunchInput): Promise<PtyLaunchResult> {
  return invoke<PtyLaunchResult>('launch_session', { input });
}

export async function ptyWriteInput(sessionId: string, data: number[]): Promise<number> {
  return invoke<number>('write_input', { sessionId, data });
}

export async function ptyResizeTerminal(
  sessionId: string,
  cols: number,
  rows: number
): Promise<void> {
  return invoke<void>('resize_terminal', { sessionId, cols, rows });
}

export async function ptyTerminateSession(sessionId: string): Promise<PtyTerminateResult> {
  return invoke<PtyTerminateResult>('terminate_session', { sessionId });
}

export async function ptyListSessions(): Promise<PtyListSessionsResult> {
  return invoke<PtyListSessionsResult>('list_sessions');
}

// ============================================================================
// Session Capture Settings Commands (Issue #1125)
// ============================================================================

export type CaptureContentLevel = 'metadata_only' | 'summary' | 'full';

export interface CaptureSettings {
  enabled: boolean;
  sync: boolean;
  content: CaptureContentLevel;
}

export async function getCaptureSettings(): Promise<CaptureSettings> {
  if (!isTauri()) {
    return { enabled: false, sync: false, content: 'metadata_only' };
  }
  return invoke<CaptureSettings>('get_capture_settings');
}

export async function updateCaptureSettings(
  settings: Partial<CaptureSettings>
): Promise<CaptureSettings> {
  if (!isTauri()) {
    return {
      enabled: false,
      sync: false,
      content: 'metadata_only',
      ...settings,
    } as CaptureSettings;
  }
  return invoke<CaptureSettings>('update_capture_settings', {
    enabled: settings.enabled ?? null,
    sync: settings.sync ?? null,
    content: settings.content ?? null,
  });
}

// ============================================================================
// PTY Agent Availability Commands (Issue #1124)
// ============================================================================

export interface AgentAvailabilityInfo {
  agentType: string;
  binary: string;
  binaryFound: boolean;
  authFound: boolean;
  binaryPath: string | null;
  installHint: string | null;
}

export interface CheckAvailabilityResult {
  agents: AgentAvailabilityInfo[];
}

export async function ptyCheckAgentAvailability(): Promise<CheckAvailabilityResult> {
  return invoke<CheckAvailabilityResult>('check_agent_availability');
}

