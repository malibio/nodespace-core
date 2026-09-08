/**
 * Tab title computation — pure derivation from tab content and node data.
 *
 * Tab titles are never pushed/stored as independent state: a title is
 * always a pure function of the tab's content and the current node data, computed on
 * read. This avoids a whole class of Svelte state_unsafe_mutation bugs where a viewer
 * component pushes a computed title into the tab store as a side effect of rendering.
 */

import { LOADING_TAB_TITLE, type Tab } from '$lib/stores/navigation.svelte';
import type { Node } from '$lib/types/node';
import { pluginRegistry } from '$lib/plugins/plugin-registry';
import { formatTabTitle } from '$lib/utils/text-formatting';

/**
 * Compute the display title for a tab.
 *
 * Type-specific title logic (e.g. date nodes formatting "Today"/"Tomorrow" from their id)
 * lives in each node type's plugin (see PluginDefinition.getTitle), not here — this stays
 * generic and delegates to pluginRegistry.getNodeTitle.
 *
 * @param tab - The tab to compute a title for
 * @param getNode - Looks up the current node for a nodeId (e.g. sharedNodeStore.getNode)
 */
export function computeTabTitle(tab: Tab, getNode: (nodeId: string) => Node | undefined): string {
  if (tab.type !== 'node' || !tab.content) {
    return tab.title;
  }

  const node = getNode(tab.content.nodeId);
  // Not yet hydrated: keep whatever placeholder the tab opened with (e.g.
  // "Loading...") until the node actually arrives in the store.
  if (!node) return tab.title;

  const title = pluginRegistry.getNodeTitle(node);
  if (title) return formatTabTitle(title);

  // Hydrated, but the node resolves to no title of its own (e.g. a brand-new
  // ai-chat with no content yet). A tab that opened with the generic
  // "Loading..." placeholder must NOT keep showing it forever — a genuinely
  // titleless node never makes getNodeTitle return truthy, so nothing would
  // ever clear it — hence formatTabTitle's own 'Untitled' default here.
  //
  // But a tab opened with some OTHER caller-supplied title (not the generic
  // placeholder) keeps it: a related-node row, say, falls back to the
  // target's raw id as a stable, distinguishing label when it has neither a
  // title nor content — collapsing every such tab to the same generic
  // "Untitled" would be the identical-placeholder-rows problem this whole
  // fallback exists to avoid, just moved from the rail to the tab bar.
  return tab.title === LOADING_TAB_TITLE ? formatTabTitle('') : tab.title;
}
