import { invoke } from '@tauri-apps/api/core';
import { createLogger } from '$lib/utils/logger';
import { proSync } from '$lib/stores/pro-sync.svelte';

const log = createLogger('RecoveredItems');

/**
 * One superseded local edit, as written by the Pro daemon to the per-user
 * local-only log `~/.nodespace/recovered-items-<user>.jsonl`.
 * Field names mirror the on-disk snake_case keys returned by the
 * `pro_list_recovered_items` tauri command.
 */
export interface RecoveredItem {
  node_id: string;
  superseded_content: string;
  superseded_modified_at: string;
  winning_content: string;
  winning_modified_at: string;
  recovered_at: string;
}

/**
 * Pro-only store surfacing conflict "losers" the daemon preserved so the user
 * can review and restore them. Completely inert in the community build: `load()`
 * early-returns when not Pro, and the underlying tauri commands return empty when
 * there is no Pro daemon — so `items` stays `[]` and no badge/snackbar ever shows.
 */
class RecoveredItemsStore {
  items = $state<RecoveredItem[]>([]);
  loaded = $state(false);

  private nodeIds = $derived(new Set(this.items.map((i) => i.node_id)));

  /** True when `nodeId` has a preserved superseded edit (drives the inline badge). */
  hasFor(nodeId: string): boolean {
    return this.nodeIds.has(nodeId);
  }

  /** The most recent recovered entry for `nodeId`, if any. */
  itemFor(nodeId: string): RecoveredItem | undefined {
    // Last match wins — the log is append-ordered, newest last.
    let found: RecoveredItem | undefined;
    for (const i of this.items) {
      if (i.node_id === nodeId) found = i;
    }
    return found;
  }

  /** Load the recovery log. No-op (clears to empty) outside Pro mode. */
  async load(): Promise<void> {
    if (!proSync.isPro) {
      this.items = [];
      this.loaded = true;
      return;
    }
    try {
      const items = await invoke<RecoveredItem[]>('pro_list_recovered_items');
      this.items = items ?? [];
    } catch (e) {
      log.warn('Failed to load recovered items', { error: e });
      this.items = [];
    } finally {
      this.loaded = true;
    }
  }

  /** Remove every entry for `nodeId` locally and from the on-disk log. */
  async dismiss(nodeId: string): Promise<void> {
    this.items = this.items.filter((i) => i.node_id !== nodeId);
    try {
      await invoke('pro_dismiss_recovered_item', { nodeId });
    } catch (e) {
      log.warn('Failed to dismiss recovered item', { error: e, nodeId });
    }
  }

  /** Clear all recovered items locally and on disk. */
  async clear(): Promise<void> {
    this.items = [];
    try {
      await invoke('pro_clear_recovered_items');
    } catch (e) {
      log.warn('Failed to clear recovered items', { error: e });
    }
  }
}

export const recoveredItems = new RecoveredItemsStore();
