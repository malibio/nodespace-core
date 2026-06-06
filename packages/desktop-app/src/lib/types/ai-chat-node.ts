import type { Node } from './node';

export type AiChatStatus = 'active' | 'processing' | 'archived';
export type AiChatProvider = 'native' | 'ollama' | 'openai' | 'pty';

export interface AiChatMessage {
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp?: string;
  /** Model chain-of-thought reasoning toward the answer, when captured. */
  reasoning?: string;
}

/**
 * AiChatNode - typed interface for ai-chat nodes.
 *
 * Flat structure matching the wire format (daemon flattens the 'ai-chat' namespace
 * via flatten_properties_for_api before sending over gRPC/Tauri).
 *
 * Always use nodeToAiChatNode() to convert a generic Node from the store.
 */
export interface AiChatNode {
  id: string;
  nodeType: 'ai-chat';
  content: string;
  version: number;
  lifecycleStatus?: string;
  createdAt: string;
  modifiedAt: string;

  status: AiChatStatus;
  provider?: AiChatProvider;
  model?: string;
  messages: AiChatMessage[];
}

export function isAiChatNode(node: Node | AiChatNode): node is AiChatNode {
  return node.nodeType === 'ai-chat';
}

/**
 * Convert a generic Node to AiChatNode.
 *
 * The backend (`node_to_typed_value` in `nodespace-types`) is the single typing
 * authority: for every transport (Tauri IPC and HTTP/SSE) it promotes ai-chat
 * fields to the TOP LEVEL of the node and flattens the `properties.ai-chat`
 * namespace away. See the `wire_contract` tests in `nodespace-types/src/convert.rs`.
 * This converter therefore trusts the flat contract and only fills defaults.
 */
export function nodeToAiChatNode(node: Node): AiChatNode {
  const chat = node as unknown as Partial<AiChatNode> & { lifecycleStatus?: string };
  return {
    id: node.id,
    nodeType: 'ai-chat',
    content: node.content,
    version: node.version,
    lifecycleStatus: chat.lifecycleStatus,
    createdAt: node.createdAt,
    modifiedAt: node.modifiedAt,
    status: chat.status ?? 'active',
    provider: chat.provider,
    model: chat.model,
    messages: Array.isArray(chat.messages) ? chat.messages : [],
  };
}
