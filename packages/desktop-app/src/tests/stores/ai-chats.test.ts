/**
 * AI Chats store — sidebar list load + "+ New chat" create.
 *
 * `loadAiChats` fetches ai-chat nodes and sorts them most-recently-modified
 * first (the backend query has no order-by). `createChat` mints a new chat
 * immediately (no name prompt) and prepends it to the list so it shows up
 * without a reload; `createBusy`/`createError` mirror how Collections
 * surfaces create-in-flight state.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));

import type { Node } from '$lib/types';

const mockQueryNodes = vi.fn<(query: unknown) => Promise<Node[]>>();
vi.mock('$lib/services/backend-adapter', () => ({
  backendAdapter: {
    queryNodes: (query: unknown) => mockQueryNodes(query)
  }
}));

const mockCreateSchemaInstance = vi.fn<(typeId: string) => Promise<Node>>();
vi.mock('$lib/services/schema-authoring', () => ({
  createSchemaInstance: (typeId: string) => mockCreateSchemaInstance(typeId)
}));

import { aiChatsData } from '$lib/stores/ai-chats.svelte';

function makeChat(id: string, content: string, modifiedAt: string): Node {
  return {
    id,
    nodeType: 'ai-chat',
    content,
    createdAt: modifiedAt,
    modifiedAt,
    version: 1,
    properties: {}
  } as Node;
}

describe('aiChatsData', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    aiChatsData.reset();
  });

  describe('loadAiChats', () => {
    it('queries ai-chat nodes with a bounded limit', async () => {
      mockQueryNodes.mockResolvedValue([]);

      await aiChatsData.loadAiChats();

      expect(mockQueryNodes).toHaveBeenCalledWith({ nodeType: 'ai-chat', limit: 50 });
    });

    it('sorts results most-recently-modified first, regardless of fetch order', async () => {
      mockQueryNodes.mockResolvedValue([
        makeChat('old', 'Oldest', '2026-01-01T00:00:00.000Z'),
        makeChat('newest', 'Newest', '2026-03-01T00:00:00.000Z'),
        makeChat('mid', 'Middle', '2026-02-01T00:00:00.000Z')
      ]);

      await aiChatsData.loadAiChats();

      expect(aiChatsData.state.chats.map((c) => c.id)).toEqual(['newest', 'mid', 'old']);
    });

    it('carries raw content through, including empty strings', async () => {
      mockQueryNodes.mockResolvedValue([makeChat('empty', '', '2026-01-01T00:00:00.000Z')]);

      await aiChatsData.loadAiChats();

      expect(aiChatsData.state.chats[0].content).toBe('');
    });

    it('sets loading state during the fetch and clears it after', async () => {
      let resolveQuery: (nodes: Node[]) => void = () => {};
      mockQueryNodes.mockReturnValue(
        new Promise<Node[]>((resolve) => {
          resolveQuery = resolve;
        })
      );

      const loadPromise = aiChatsData.loadAiChats();
      expect(aiChatsData.state.loading).toBe(true);

      resolveQuery([]);
      await loadPromise;

      expect(aiChatsData.state.loading).toBe(false);
    });

    it('surfaces a failure without throwing, and leaves the prior list untouched', async () => {
      mockQueryNodes.mockResolvedValue([makeChat('a', 'A', '2026-01-01T00:00:00.000Z')]);
      await aiChatsData.loadAiChats();

      mockQueryNodes.mockRejectedValue(new Error('daemon unavailable'));
      await aiChatsData.loadAiChats();

      expect(aiChatsData.state.error).toContain('daemon unavailable');
      expect(aiChatsData.state.chats.map((c) => c.id)).toEqual(['a']);
    });
  });

  describe('createChat', () => {
    it('creates via createSchemaInstance and prepends the result to the list', async () => {
      mockQueryNodes.mockResolvedValue([makeChat('existing', 'Existing', '2026-01-01T00:00:00.000Z')]);
      await aiChatsData.loadAiChats();

      const created = makeChat('new-chat', '', '2026-06-01T00:00:00.000Z');
      mockCreateSchemaInstance.mockResolvedValue(created);

      const result = await aiChatsData.createChat();

      expect(mockCreateSchemaInstance).toHaveBeenCalledWith('ai-chat');
      expect(result).toEqual(created);
      expect(aiChatsData.state.chats.map((c) => c.id)).toEqual(['new-chat', 'existing']);
    });

    it('sets createBusy while the create is in flight and clears it after', async () => {
      let resolveCreate: (node: Node) => void = () => {};
      mockCreateSchemaInstance.mockReturnValue(
        new Promise<Node>((resolve) => {
          resolveCreate = resolve;
        })
      );

      const createPromise = aiChatsData.createChat();
      expect(aiChatsData.createBusy).toBe(true);

      resolveCreate(makeChat('new-chat', '', '2026-06-01T00:00:00.000Z'));
      await createPromise;

      expect(aiChatsData.createBusy).toBe(false);
    });

    it('ignores a second call while a create is already in flight', async () => {
      mockCreateSchemaInstance.mockReturnValue(new Promise<Node>(() => {})); // never resolves

      const first = aiChatsData.createChat();
      const second = await aiChatsData.createChat();

      expect(second).toBeNull();
      expect(mockCreateSchemaInstance).toHaveBeenCalledOnce();
      void first; // left pending deliberately — nothing further to await
    });

    it('surfaces a failure via createError, returns null, and re-enables the button', async () => {
      mockCreateSchemaInstance.mockRejectedValue(new Error('daemon unavailable'));

      const result = await aiChatsData.createChat();

      expect(result).toBeNull();
      expect(aiChatsData.createError).toContain('daemon unavailable');
      expect(aiChatsData.createBusy).toBe(false);
      expect(aiChatsData.state.chats).toEqual([]);
    });

    it('clears a previous createError at the start of the next attempt', async () => {
      mockCreateSchemaInstance.mockRejectedValueOnce(new Error('boom'));
      await aiChatsData.createChat();
      expect(aiChatsData.createError).toContain('boom');

      mockCreateSchemaInstance.mockResolvedValueOnce(
        makeChat('new-chat', '', '2026-06-01T00:00:00.000Z')
      );
      const result = await aiChatsData.createChat();

      expect(result).not.toBeNull();
      expect(aiChatsData.createError).toBe('');
    });
  });

  describe('reset', () => {
    it('clears chats, createBusy, and createError back to initial state', async () => {
      mockQueryNodes.mockResolvedValue([makeChat('a', 'A', '2026-01-01T00:00:00.000Z')]);
      await aiChatsData.loadAiChats();
      mockCreateSchemaInstance.mockRejectedValue(new Error('boom'));
      await aiChatsData.createChat();

      aiChatsData.reset();

      expect(aiChatsData.state.chats).toEqual([]);
      expect(aiChatsData.state.error).toBeNull();
      expect(aiChatsData.createBusy).toBe(false);
      expect(aiChatsData.createError).toBe('');
    });
  });
});
