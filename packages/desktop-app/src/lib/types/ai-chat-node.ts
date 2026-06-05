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
 * Handles both:
 * - Flat wire format: properties.messages, properties.status, etc. (from daemon)
 * - Already promoted: node.messages, node.status (already an AiChatNode)
 */
export function nodeToAiChatNode(node: Node): AiChatNode {
  // Already promoted (messages already at top level)
  const nodeAsAny = node as unknown as AiChatNode;
  if ('messages' in node && Array.isArray(nodeAsAny.messages)) {
    return nodeAsAny;
  }

  const props = node.properties as Record<string, unknown> | undefined;

  // Flat wire format (after flatten_properties_for_api)
  const status = (props?.['status'] as AiChatStatus) ?? 'active';
  const provider = props?.['provider'] as AiChatProvider | undefined;
  const model = props?.['model'] as string | undefined;
  const messages = Array.isArray(props?.['messages'])
    ? (props['messages'] as AiChatMessage[])
    : [];

  return {
    id: node.id,
    nodeType: 'ai-chat',
    content: node.content,
    version: node.version,
    lifecycleStatus: (node as unknown as { lifecycleStatus?: string }).lifecycleStatus,
    createdAt: node.createdAt,
    modifiedAt: node.modifiedAt,
    status,
    provider,
    model,
    messages,
  };
}
