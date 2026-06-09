<!--
  TableRow - Reactive row component for TableView

  Subscribes to per-node changes via sharedNodeStore.subscribe() and uses a
  local _updateTrigger counter (same pattern as ReactiveNodeService) to force
  Svelte to re-derive cellValues when the node is updated in another pane.

  Background: Svelte 5 $state(Map) does not track Map.get() calls automatically,
  so $derived(sharedNodeStore.getNode(id)) alone is not sufficient for reactivity.
-->

<script lang="ts">
  import type { SchemaField } from '$lib/types/schema-node';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
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

  // Manual reactivity trigger — same pattern as ReactiveNodeService._updateTrigger
  let _updateTrigger = $state(0);

  // Subscribe to this specific node's changes and increment trigger on each update
  $effect(() => {
    const unsubscribe = sharedNodeStore.subscribe(id, () => {
      _updateTrigger++;
    });
    return unsubscribe;
  });

  // Convert snake_case field name to camelCase for wire format lookups.
  // Schema field names are snake_case (e.g. due_date) but the API serializes
  // typed node fields as camelCase (e.g. dueDate) via serde rename_all.
  function toCamelCase(name: string): string {
    return name.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
  }

  // Derive the node and cell values — void _updateTrigger establishes the reactive dependency
  const cellValues = $derived.by(() => {
    void _updateTrigger;
    const node = sharedNodeStore.getNode(id);
    const map = new Map<string, string>();
    if (!node) return map;

    const nodeRecord = node as unknown as Record<string, unknown>;

    for (const col of columns) {
      const fieldSchema = fieldSchemaMap.get(col.field);
      // Resolution order:
      // 1. For 'content' column: prefer node.title (computed by title_template) over raw content
      // 2. camelCase top-level (typed core fields like task.dueDate serialized from Rust)
      // 3. snake_case top-level (fallback)
      // 4. node.properties[field] (user-defined fields on custom schema nodes)
      const camelKey = toCamelCase(col.field);
      const props = node.properties as Record<string, unknown> | undefined;
      const rawValue =
        (col.field === 'content' && node.title ? node.title : undefined) ??
        nodeRecord[camelKey] ??
        nodeRecord[col.field] ??
        props?.[col.field];

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

  // For title_template nodes, prefer the computed title over raw content
  const nodeContent = $derived.by(() => {
    void _updateTrigger;
    const node = sharedNodeStore.getNode(id);
    return node?.title ?? node?.content ?? '';
  });

  // Reactive existence check — void _updateTrigger ensures the guard re-evaluates
  // when the node is deleted from the store (same pattern as cellValues/nodeContent).
  const nodeExists = $derived.by(() => {
    void _updateTrigger;
    return !!sharedNodeStore.getNode(id);
  });
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
