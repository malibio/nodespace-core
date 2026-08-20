/**
 * AI Chats Store
 *
 * Manages the state for the "AI Chats" section in the navigation sidebar:
 * the list of existing ai-chat nodes (most-recently-modified first) and the
 * "+ New chat" create action.
 *
 * Mirrors `collections.svelte.ts`'s data-store shape (a `load*` method called
 * from the sidebar's `onMount`, reactive `$state`), simplified — unlike
 * collections there is no hide-empty filter to work around, so a newly
 * created chat is simply prepended to the loaded list rather than needing an
 * optimistic-insert/reconcile dance.
 */

import { backendAdapter } from '$lib/services/backend-adapter';
import { createSchemaInstance } from '$lib/services/schema-authoring';
import { createLogger } from '$lib/utils/logger';
import { onDaemonReconnect } from '$lib/services/daemon-status';
import type { Node } from '$lib/types';

const log = createLogger('AiChatsStore');

/**
 * Cap on how many ai-chat nodes are fetched for the sidebar. The backend
 * query has no order-by (see `NodeQuery`), so this bounds the fetch — the
 * result is sorted client-side by `modifiedAt` — rather than being the
 * on-screen row count; the list itself scrolls past that, matching how
 * Collections / Schema Types handle overflow.
 */
const LOAD_LIMIT = 50;

export interface AiChatListItem {
  id: string;
  /** Raw node content. Empty until background titling (core#1698) fills it in. */
  content: string;
  modifiedAt: string;
}

interface AiChatsState {
  chats: AiChatListItem[];
  loading: boolean;
  error: string | null;
}

const initialState: AiChatsState = {
  chats: [],
  loading: false,
  error: null,
};

function toListItem(node: Node): AiChatListItem {
  return { id: node.id, content: node.content, modifiedAt: node.modifiedAt };
}

class AiChatsStore {
  state = $state<AiChatsState>({ ...initialState });

  /** True while a "+ New chat" create is in flight — disables the button. */
  createBusy = $state(false);
  /** Message from the most recent failed create, cleared on the next attempt. */
  createError = $state('');

  /** Load ai-chat nodes from the backend, most-recently-modified first. */
  async loadAiChats(): Promise<void> {
    this.state = { ...this.state, loading: true, error: null };

    try {
      const nodes = await backendAdapter.queryNodes({ nodeType: 'ai-chat', limit: LOAD_LIMIT });
      const chats = [...nodes]
        .sort((a, b) => new Date(b.modifiedAt).getTime() - new Date(a.modifiedAt).getTime())
        .map(toListItem);
      log.debug('Loaded AI chats', { count: chats.length });
      this.state = { ...this.state, chats, loading: false };
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to load AI chats';
      log.error('Failed to load AI chats', { error: message });
      this.state = { ...this.state, loading: false, error: message };
    }
  }

  /**
   * Create a new ai-chat node immediately — no name prompt, since a chat's
   * title comes from its content later rather than being required up front.
   * Prepends the new chat to the local list so it appears without a reload.
   *
   * Returns the created node, or null on failure (`createError` is set for
   * the caller to surface; the caller re-enables its own button once this
   * resolves).
   */
  async createChat(): Promise<Node | null> {
    if (this.createBusy) return null;
    this.createBusy = true;
    this.createError = '';

    try {
      const created = await createSchemaInstance('ai-chat');
      this.state = { ...this.state, chats: [toListItem(created), ...this.state.chats] };
      return created;
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to create chat';
      log.error('Failed to create AI chat', { error: message });
      this.createError = message;
      return null;
    } finally {
      this.createBusy = false;
    }
  }

  /** Reset to initial state (test use only — no production caller needs this,
   * same as `collectionsData.reset()`; `loadAiChats` overwrites `chats`
   * wholesale so no explicit reset is needed on database switch). */
  reset(): void {
    this.state = { ...initialState };
    this.createBusy = false;
    this.createError = '';
  }
}

export const aiChatsData = new AiChatsStore();

/** Load ai-chat nodes from the backend and update the store. */
export const loadAiChats = (): Promise<void> => aiChatsData.loadAiChats();

// Registered once at module load (this file is a singleton — ES modules only
// evaluate once), not per component mount. Retries loadAiChats whenever the
// daemon becomes reachable, so a load that failed while the daemon was still
// starting up recovers automatically without a manual reload.
onDaemonReconnect(() => aiChatsData.loadAiChats());
