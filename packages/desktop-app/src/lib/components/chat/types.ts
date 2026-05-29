/**
 * View-model types for the chat UI components.
 *
 * `DisplayMessage` is the UI-facing message shape rendered by `ChatMessage` and
 * owned by `AiChatNodeViewer`. It is deliberately distinct from the two other
 * message shapes in play (see [[project_frontend_type_layering]]):
 *   - `ChatMessage` (`$lib/types/agent-types`) — the protocol/wire shape.
 *   - the ADR-028 persisted JSON shape on `ai-chat` node properties.
 * These three do NOT converge; converters bridge them at the viewer boundary.
 */

import type { ChatMessage, ToolExecutionRecord } from '$lib/types/agent-types';

/** UI display message with tool executions and streaming state. */
export interface DisplayMessage {
  readonly id: string;
  readonly role: ChatMessage['role'];
  content: string;
  readonly toolExecutions: ToolExecutionRecord[];
  readonly timestamp: number;
}
