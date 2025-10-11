/**
 * Backend Adapter Pattern for Tauri IPC vs HTTP Dev Server
 *
 * This module provides a unified interface for backend communication that works
 * in both Tauri desktop mode (IPC) and web development mode (HTTP).
 *
 * # Architecture
 *
 * - **TauriAdapter**: Uses Tauri's `invoke()` for IPC communication
 * - **HttpAdapter**: Uses `fetch()` to communicate with HTTP dev server on port 3001
 * - **Auto-detection**: Runtime detection based on `window.__TAURI__` existence
 *
 * # Usage
 *
 * ```typescript
 * import { getBackendAdapter } from '$lib/services/backend-adapter';
 *
 * const adapter = getBackendAdapter();
 * const node = await adapter.getNode('node-123');
 * ```
 *
 * # Extension for Future Phases
 *
 * To add new operations in Phase 2/3:
 * 1. Add method to `BackendAdapter` interface
 * 2. Implement in both `TauriAdapter` and `HttpAdapter`
 * 3. No changes needed to detection or factory function
 */

import { invoke } from '@tauri-apps/api/core';
import type { Node, NodeUpdate } from '$lib/types';
import { toError, DatabaseInitializationError, NodeOperationError } from '$lib/types/errors';

// Phase 3: Embedding-related types
export interface SearchTopicsParams {
  query: string;
  threshold?: number;
  limit?: number;
  exact?: boolean;
}

export interface BatchEmbeddingResult {
  successCount: number;
  failedEmbeddings: Array<{
    topicId: string;
    error: string;
  }>;
}

export interface CreateContainerNodeInput {
  content: string;
  nodeType: string;
  properties?: Record<string, unknown>;
  mentionedBy?: string;
}

/**
 * Backend adapter interface - Phase 1, 2, and 3 operations
 *
 * Phase 1: Basic node CRUD
 * Phase 2: Query operations (to be added)
 * Phase 3: Embeddings and mentions
 */
export interface BackendAdapter {
  // === Phase 1: Basic Node CRUD ===

  /**
   * Initialize database with optional custom path
   * @param dbPath - Optional custom database path (for testing)
   * @returns Path to the initialized database
   */
  initializeDatabase(dbPath?: string): Promise<string>;

  /**
   * Create a new node
   * @param node - Node data without timestamps
   * @returns ID of the created node
   */
  createNode(node: Omit<Node, 'createdAt' | 'modifiedAt'>): Promise<string>;

  /**
   * Get a node by ID
   * @param id - Node ID
   * @returns Node data if found, null otherwise
   */
  getNode(id: string): Promise<Node | null>;

  /**
   * Update an existing node
   * @param id - Node ID
   * @param update - Fields to update (partial)
   */
  updateNode(id: string, update: NodeUpdate): Promise<void>;

  /**
   * Delete a node by ID
   * @param id - Node ID
   */
  deleteNode(id: string): Promise<void>;

  /**
   * Get child nodes of a parent node
   * @param parentId - Parent node ID
   * @returns Array of child nodes
   */
  getChildren(parentId: string): Promise<Node[]>;

  // === Phase 3: Embedding Operations ===

  /**
   * Generate embedding for a topic node
   * @param topicId - ID of the topic node to embed
   */
  generateTopicEmbedding(topicId: string): Promise<void>;

  /**
   * Search topics by semantic similarity
   * @param params - Search parameters (query, threshold, limit, exact)
   * @returns Array of matching topic nodes
   */
  searchTopics(params: SearchTopicsParams): Promise<Node[]>;

  /**
   * Update topic embedding immediately
   * @param topicId - ID of the topic to update
   */
  updateTopicEmbedding(topicId: string): Promise<void>;

  /**
   * Batch generate embeddings for multiple topics
   * @param topicIds - Array of topic IDs to embed
   * @returns Result with success count and failures
   */
  batchGenerateEmbeddings(topicIds: string[]): Promise<BatchEmbeddingResult>;

  /**
   * Get count of stale topics needing re-embedding
   * @returns Number of stale topics
   */
  getStaleTopicCount(): Promise<number>;

  /**
   * Smart trigger: Topic closed/unfocused
   * @param topicId - ID of the topic that was closed
   */
  onTopicClosed(topicId: string): Promise<void>;

  /**
   * Smart trigger: Idle timeout (30s of no activity)
   * @param topicId - ID of the topic to check
   * @returns True if re-embedding was triggered
   */
  onTopicIdle(topicId: string): Promise<boolean>;

  /**
   * Manually sync all stale topics
   * @returns Number of topics re-embedded
   */
  syncEmbeddings(): Promise<number>;

  // === Phase 3: Node Mention Operations ===

  /**
   * Create a container node (root node with no parent)
   * @param input - Container node data
   * @returns ID of the created container node
   */
  createContainerNode(input: CreateContainerNodeInput): Promise<string>;

  /**
   * Create a mention relationship between two nodes
   * @param mentioningNodeId - ID of the node that contains the mention
   * @param mentionedNodeId - ID of the node being mentioned
   */
  createNodeMention(mentioningNodeId: string, mentionedNodeId: string): Promise<void>;
}

/**
 * Tauri IPC Adapter - Uses invoke() for desktop app communication
 */
export class TauriAdapter implements BackendAdapter {
  async initializeDatabase(dbPath?: string): Promise<string> {
    try {
      if (dbPath) {
        // Note: Current Tauri command doesn't support custom path parameter
        // This would need a backend update to support custom paths
        console.warn('[TauriAdapter] Custom database path not supported via IPC yet');
      }
      return await invoke<string>('initialize_database');
    } catch (error) {
      const err = toError(error);
      throw new DatabaseInitializationError(err.message, err.stack);
    }
  }

  async createNode(node: Omit<Node, 'createdAt' | 'modifiedAt'>): Promise<string> {
    try {
      return await invoke<string>('create_node', { node });
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, node.id, 'create');
    }
  }

  async getNode(id: string): Promise<Node | null> {
    try {
      return await invoke<Node | null>('get_node', { id });
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, id, 'get');
    }
  }

  async updateNode(id: string, update: NodeUpdate): Promise<void> {
    try {
      await invoke<void>('update_node', { id, update });
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, id, 'update');
    }
  }

  async deleteNode(id: string): Promise<void> {
    try {
      await invoke<void>('delete_node', { id });
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, id, 'delete');
    }
  }

  async getChildren(parentId: string): Promise<Node[]> {
    try {
      return await invoke<Node[]>('get_children', { parentId });
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, parentId, 'getChildren');
    }
  }

  // === Phase 3: Embedding Operations ===

  async generateTopicEmbedding(topicId: string): Promise<void> {
    try {
      await invoke<void>('generate_topic_embedding', { topicId });
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, topicId, 'generateTopicEmbedding');
    }
  }

  async searchTopics(params: SearchTopicsParams): Promise<Node[]> {
    try {
      return await invoke<Node[]>('search_topics', { params });
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, params.query, 'searchTopics');
    }
  }

  async updateTopicEmbedding(topicId: string): Promise<void> {
    try {
      await invoke<void>('update_topic_embedding', { topicId });
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, topicId, 'updateTopicEmbedding');
    }
  }

  async batchGenerateEmbeddings(topicIds: string[]): Promise<BatchEmbeddingResult> {
    try {
      return await invoke<BatchEmbeddingResult>('batch_generate_embeddings', { topicIds });
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, 'batch', 'batchGenerateEmbeddings');
    }
  }

  async getStaleTopicCount(): Promise<number> {
    try {
      return await invoke<number>('get_stale_topic_count');
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, 'stale-count', 'getStaleTopicCount');
    }
  }

  async onTopicClosed(topicId: string): Promise<void> {
    try {
      await invoke<void>('on_topic_closed', { topicId });
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, topicId, 'onTopicClosed');
    }
  }

  async onTopicIdle(topicId: string): Promise<boolean> {
    try {
      return await invoke<boolean>('on_topic_idle', { topicId });
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, topicId, 'onTopicIdle');
    }
  }

  async syncEmbeddings(): Promise<number> {
    try {
      return await invoke<number>('sync_embeddings');
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, 'sync', 'syncEmbeddings');
    }
  }

  // === Phase 3: Node Mention Operations ===

  async createContainerNode(input: CreateContainerNodeInput): Promise<string> {
    try {
      return await invoke<string>('create_container_node', { input });
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, input.content, 'createContainerNode');
    }
  }

  async createNodeMention(mentioningNodeId: string, mentionedNodeId: string): Promise<void> {
    try {
      await invoke<void>('create_node_mention', { mentioningNodeId, mentionedNodeId });
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(
        err.message,
        `${mentioningNodeId} -> ${mentionedNodeId}`,
        'createNodeMention'
      );
    }
  }
}

/**
 * HTTP Dev Server Adapter - Uses fetch() for web mode communication
 */
export class HttpAdapter implements BackendAdapter {
  private readonly baseUrl: string;

  constructor(baseUrl: string = 'http://localhost:3001') {
    this.baseUrl = baseUrl;
  }

  private async handleResponse<T>(response: globalThis.Response): Promise<T> {
    if (!response.ok) {
      // Try to parse error as HttpError format
      try {
        const errorData = await response.json();
        throw new Error(errorData.message || `HTTP ${response.status}: ${response.statusText}`);
      } catch {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }
    }

    // Handle 204 No Content responses
    if (response.status === 204 || response.headers.get('content-length') === '0') {
      return undefined as T;
    }

    return await response.json();
  }

  async initializeDatabase(dbPath?: string): Promise<string> {
    try {
      const url = new URL(`${this.baseUrl}/api/database/init`);
      if (dbPath) {
        url.searchParams.set('db_path', dbPath);
      }

      const response = await globalThis.fetch(url.toString(), {
        method: 'POST'
      });

      const data = await this.handleResponse<{ dbPath: string }>(response);
      return data.dbPath;
    } catch (error) {
      const err = toError(error);
      throw new DatabaseInitializationError(err.message, err.stack);
    }
  }

  async createNode(node: Omit<Node, 'createdAt' | 'modifiedAt'>): Promise<string> {
    try {
      const response = await globalThis.fetch(`${this.baseUrl}/api/nodes`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify(node)
      });

      return await this.handleResponse<string>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, node.id, 'create');
    }
  }

  async getNode(id: string): Promise<Node | null> {
    try {
      const response = await globalThis.fetch(
        `${this.baseUrl}/api/nodes/${encodeURIComponent(id)}`
      );
      return await this.handleResponse<Node | null>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, id, 'get');
    }
  }

  async updateNode(id: string, update: NodeUpdate): Promise<void> {
    try {
      const response = await globalThis.fetch(
        `${this.baseUrl}/api/nodes/${encodeURIComponent(id)}`,
        {
          method: 'PATCH',
          headers: {
            'Content-Type': 'application/json'
          },
          body: JSON.stringify(update)
        }
      );

      await this.handleResponse<void>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, id, 'update');
    }
  }

  async deleteNode(id: string): Promise<void> {
    try {
      const response = await globalThis.fetch(
        `${this.baseUrl}/api/nodes/${encodeURIComponent(id)}`,
        {
          method: 'DELETE'
        }
      );

      await this.handleResponse<void>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, id, 'delete');
    }
  }

  async getChildren(parentId: string): Promise<Node[]> {
    try {
      const response = await globalThis.fetch(
        `${this.baseUrl}/api/nodes/${encodeURIComponent(parentId)}/children`
      );
      return await this.handleResponse<Node[]>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, parentId, 'getChildren');
    }
  }

  // === Phase 3: Embedding Operations ===

  async generateTopicEmbedding(topicId: string): Promise<void> {
    try {
      const response = await globalThis.fetch(`${this.baseUrl}/api/embeddings/generate`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({ topicId })
      });

      await this.handleResponse<void>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, topicId, 'generateTopicEmbedding');
    }
  }

  async searchTopics(params: SearchTopicsParams): Promise<Node[]> {
    try {
      const response = await globalThis.fetch(`${this.baseUrl}/api/embeddings/search`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify(params)
      });

      return await this.handleResponse<Node[]>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, params.query, 'searchTopics');
    }
  }

  async updateTopicEmbedding(topicId: string): Promise<void> {
    try {
      const response = await globalThis.fetch(
        `${this.baseUrl}/api/embeddings/${encodeURIComponent(topicId)}`,
        {
          method: 'PATCH'
        }
      );

      await this.handleResponse<void>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, topicId, 'updateTopicEmbedding');
    }
  }

  async batchGenerateEmbeddings(topicIds: string[]): Promise<BatchEmbeddingResult> {
    try {
      const response = await globalThis.fetch(`${this.baseUrl}/api/embeddings/batch`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({ topicIds })
      });

      return await this.handleResponse<BatchEmbeddingResult>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, 'batch', 'batchGenerateEmbeddings');
    }
  }

  async getStaleTopicCount(): Promise<number> {
    try {
      const response = await globalThis.fetch(`${this.baseUrl}/api/embeddings/stale-count`);
      return await this.handleResponse<number>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, 'stale-count', 'getStaleTopicCount');
    }
  }

  async onTopicClosed(topicId: string): Promise<void> {
    try {
      const response = await globalThis.fetch(`${this.baseUrl}/api/embeddings/on-topic-closed`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({ topicId })
      });

      await this.handleResponse<void>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, topicId, 'onTopicClosed');
    }
  }

  async onTopicIdle(topicId: string): Promise<boolean> {
    try {
      const response = await globalThis.fetch(`${this.baseUrl}/api/embeddings/on-topic-idle`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({ topicId })
      });

      return await this.handleResponse<boolean>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, topicId, 'onTopicIdle');
    }
  }

  async syncEmbeddings(): Promise<number> {
    try {
      const response = await globalThis.fetch(`${this.baseUrl}/api/embeddings/sync`, {
        method: 'POST'
      });

      return await this.handleResponse<number>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, 'sync', 'syncEmbeddings');
    }
  }

  // === Phase 3: Node Mention Operations ===

  async createContainerNode(input: CreateContainerNodeInput): Promise<string> {
    try {
      const response = await globalThis.fetch(`${this.baseUrl}/api/nodes/container`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify(input)
      });

      return await this.handleResponse<string>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(err.message, input.content, 'createContainerNode');
    }
  }

  async createNodeMention(mentioningNodeId: string, mentionedNodeId: string): Promise<void> {
    try {
      const response = await globalThis.fetch(`${this.baseUrl}/api/nodes/mention`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({ mentioningNodeId, mentionedNodeId })
      });

      await this.handleResponse<void>(response);
    } catch (error) {
      const err = toError(error);
      throw new NodeOperationError(
        err.message,
        `${mentioningNodeId} -> ${mentionedNodeId}`,
        'createNodeMention'
      );
    }
  }
}

/**
 * Detect if running in Tauri environment
 * In Tauri v2, we check for __TAURI__ global object
 */
function isTauriEnvironment(): boolean {
  return (
    typeof window !== 'undefined' &&
    (window as unknown as { __TAURI__?: unknown }).__TAURI__ !== undefined
  );
}

/**
 * Get the appropriate backend adapter based on runtime environment
 *
 * - Tauri desktop app: Returns TauriAdapter (uses IPC)
 * - Web dev mode: Returns HttpAdapter (uses HTTP to port 3001)
 *
 * @returns BackendAdapter instance appropriate for current environment
 */
export function getBackendAdapter(): BackendAdapter {
  if (isTauriEnvironment()) {
    console.log('[BackendAdapter] Using Tauri IPC adapter');
    return new TauriAdapter();
  } else {
    console.log('[BackendAdapter] Using HTTP dev server adapter (port 3001)');
    return new HttpAdapter();
  }
}

/**
 * Singleton instance for convenient access
 * Auto-detects environment and returns appropriate adapter
 */
export const backendAdapter = getBackendAdapter();
