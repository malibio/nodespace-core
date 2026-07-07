/**
 * Tab title computation — pure derivation from tab content and node data.
 *
 * Tab titles are never pushed/stored as independent state (see issue #1564): a title is
 * always a pure function of the tab's content and the current node data, computed on
 * read. This avoids a whole class of Svelte state_unsafe_mutation bugs where a viewer
 * component pushes a computed title into the tab store as a side effect of rendering.
 */

import type { Tab } from '$lib/stores/navigation';
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
  if (!node) return tab.title;

  const title = pluginRegistry.getNodeTitle(node);
  return title ? formatTabTitle(title) : tab.title;
}
