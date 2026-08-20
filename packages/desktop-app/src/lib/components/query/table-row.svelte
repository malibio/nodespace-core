<!--
  TableRow - Reactive row component for TableView

  Reads node data directly from sharedNodeStore, which tracks reads/writes at
  per-node granularity (SvelteMap), so cellValues/nodeContent/nodeExists
  re-derive automatically when this row's node changes in another pane.
-->

<script lang="ts">
  import type { SchemaField } from '$lib/types/schema-node';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
  import { pluginRegistry } from '$lib/plugins/plugin-registry';
  import { TableRow as UiTableRow, TableCell } from '$lib/components/ui/table';

  let {
    id,
    columns,
    fieldSchemaMap,
    onRowClick
  }: {
    id: string;
    columns: Array<{ field: string; label: string }>;
    fieldSchemaMap: Map<string, SchemaField>;
    onRowClick: (_nodeId: string) => void;
  } = $props();

  // Convert snake_case field name to camelCase for wire format lookups.
  // Schema field names are snake_case (e.g. due_date) but the API serializes
  // typed node fields as camelCase (e.g. dueDate) via serde rename_all.
  function toCamelCase(name: string): string {
    return name.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
  }

  // Derive the node and cell values
  const cellValues = $derived.by(() => {
    const node = sharedNodeStore.getNode(id);
    const map = new Map<string, string>();
    if (!node) return map;

    const nodeRecord = node as unknown as Record<string, unknown>;

    for (const col of columns) {
      const fieldSchema = fieldSchemaMap.get(col.field);
      // Resolution order:
      // 1. For 'content' column: the node's current display value (pluginRegistry
      //    .resolveDisplayTitle — title only for title_template-driven schemas, content
      //    otherwise; see node-display-title.ts). node.title alone is stale for non-template
      //    types, since it's only refreshed by a backend round-trip while optimistic edits
      //    patch content directly.
      // 2. camelCase top-level (typed core fields like task.dueDate serialized from Rust)
      // 3. snake_case top-level (fallback)
      // 4. node.properties[field] (user-defined fields on custom schema nodes)
      const camelKey = toCamelCase(col.field);
      const props = node.properties as Record<string, unknown> | undefined;
      const rawValue =
        col.field === 'content'
          ? pluginRegistry.resolveDisplayTitle(node)
          : (nodeRecord[camelKey] ?? nodeRecord[col.field] ?? props?.[col.field]);

      if (rawValue === null || rawValue === undefined) {
        map.set(col.field, '');
        continue;
      }
      if (typeof rawValue === 'object') {
        map.set(col.field, JSON.stringify(rawValue));
        continue;
      }

      if (fieldSchema?.type === 'enum') {
        const strVal = String(rawValue);
        const allValues = [...(fieldSchema.coreValues ?? []), ...(fieldSchema.userValues ?? [])];
        const match = allValues.find((ev) => ev.value === strVal);
        if (match) {
          map.set(col.field, match.label);
          continue;
        }
      }

      if (fieldSchema?.type === 'date') {
        const strVal = String(rawValue);
        // Trim ISO datetime to date-only (2026-03-28T00:00:00Z → 2026-03-28)
        map.set(col.field, strVal.split('T')[0]);
        continue;
      }

      map.set(col.field, String(rawValue));
    }

    return map;
  });

  // Current display title (title_template-driven types) or content (everything else) —
  // see pluginRegistry.resolveDisplayTitle / node-display-title.ts.
  const nodeContent = $derived.by(() => {
    const node = sharedNodeStore.getNode(id);
    return (node && pluginRegistry.resolveDisplayTitle(node)) ?? '';
  });

  // Reactive existence check — re-evaluates when the node is deleted from the store
  const nodeExists = $derived(!!sharedNodeStore.getNode(id));
</script>

{#if nodeExists}
  <UiTableRow>
    {#each columns as col (col.field)}
      <TableCell>
        {#if col.field === 'content'}
          <button
            class="text-foreground hover:text-primary max-w-full overflow-hidden text-ellipsis whitespace-nowrap text-left text-sm font-medium hover:underline"
            onclick={() => onRowClick(id)}
            title="Open {nodeContent || 'node'}"
          >
            {cellValues.get(col.field) || 'Untitled'}
          </button>
        {:else}
          <span class="text-muted-foreground">{cellValues.get(col.field)}</span>
        {/if}
      </TableCell>
    {/each}
  </UiTableRow>
{/if}
