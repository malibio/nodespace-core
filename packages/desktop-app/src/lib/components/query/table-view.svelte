<!--
  TableView - Pure table rendering component for QueryNodeViewer

  Derives columns from schema field definitions (not by enumerating result node keys).
  Always includes 'content' (title) as the first column — rendered as a clickable link.
  Additional columns: one per schema field definition, in schema order, using field.label.
  Clicking the content/title cell calls onRowClick(node.id).
  Results are paginated at 25 rows per page.
-->

<script lang="ts">
  import type { SchemaField, SchemaNode } from '$lib/types/schema-node';
  import TableRow from '$lib/components/query/table-row.svelte';
  import { Table, TableHeader, TableBody, TableHead, TableRow as UiTableRow } from '$lib/components/ui/table';
  import { Button } from '$lib/components/ui/button';

  let {
    nodeIds,
    schema,
    fieldSchemaMap,
    onRowClick
  }: {
    nodeIds: string[];
    schema: SchemaNode | null;
    fieldSchemaMap: Map<string, SchemaField>;
    onRowClick: (_nodeId: string) => void;
  } = $props();

  const PAGE_SIZE = 25;
  let currentPage = $state(0);

  // Reset to page 0 when nodeIds change
  $effect(() => {
    nodeIds;
    currentPage = 0;
  });

  // Derive columns from schema fields — capitalize name and replace underscores with spaces
  const columns = $derived.by(() => {
    const cols: Array<{ field: string; label: string }> = [
      { field: 'content', label: '' }
    ];

    if (schema?.fields) {
      for (const field of schema.fields) {
        const label = field.description
          ? field.description
          : field.name
              .replace(/_/g, ' ')
              .replace(/([a-z])([A-Z])/g, '$1 $2')
              .replace(/^\w/, (c) => c.toUpperCase());
        cols.push({ field: field.name, label });
      }
    }

    return cols;
  });

  const totalPages = $derived(Math.ceil(nodeIds.length / PAGE_SIZE));

  const pageIds = $derived(
    nodeIds.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE)
  );

</script>

<Table>
  <TableHeader>
    <UiTableRow>
      {#each columns as col (col.field)}
        <TableHead>{col.label}</TableHead>
      {/each}
    </UiTableRow>
  </TableHeader>
  <TableBody>
    {#each pageIds as id (id)}
      <TableRow {id} {columns} {fieldSchemaMap} {onRowClick} />
    {/each}
  </TableBody>
</Table>

{#if totalPages > 1}
  <div class="border-border flex items-center justify-center gap-3 border-t p-4">
    <Button
      variant="outline"
      size="sm"
      onclick={() => currentPage--}
      disabled={currentPage === 0}
    >
      ‹
    </Button>
    <span class="text-muted-foreground text-sm">{currentPage + 1} / {totalPages}</span>
    <Button
      variant="outline"
      size="sm"
      onclick={() => currentPage++}
      disabled={currentPage >= totalPages - 1}
    >
      ›
    </Button>
  </div>
{/if}
