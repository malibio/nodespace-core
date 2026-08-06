<!--
  KanbanView - Board view for QueryNodeViewer

  Groups results into columns by the values of a selected enum field (the only
  eligible group-by kind — enum values give both the column set and their labels).
  Cards show the node title and read from sharedNodeStore, so a change made in
  another pane (or by a drag here) moves the card reactively. Dragging a card to
  another column — or choosing a column from the card's keyboard-accessible
  "Move to" control — writes that column's value onto the node. Nodes with no
  value land in an "Unassigned" column.
-->

<script lang="ts">
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
  import { queryPreferencesService } from '$lib/services/query-preferences-service';
  import { createLogger } from '$lib/utils/logger';
  import type { Node } from '$lib/types';
  import type { SchemaField, SchemaNode } from '$lib/types/schema-node';
  import {
    UNASSIGNED,
    eligibleGroupByFields,
    enumColumns,
    readGroupValue,
    resolveFieldWrite,
    groupByColumn,
    resolveActiveGroupBy
  } from '$lib/components/query/kanban-grouping';

  const log = createLogger('KanbanView');

  let {
    nodeIds,
    schema,
    nodeId,
    onRowClick
  }: {
    nodeIds: string[];
    schema: SchemaNode | null;
    /** Query node id this board belongs to — the key its group-by is persisted under. */
    nodeId: string;
    onRowClick: (_nodeId: string) => void;
  } = $props();

  // The user's explicit picker choice this session; null means "use the default
  // resolved from stored prefs / first eligible field".
  let picked = $state<string | null>(null);
  let draggingId = $state<string | null>(null);
  let dragOverColumn = $state<string | null>(null);

  const eligible = $derived(eligibleGroupByFields(schema));
  const storedGroupBy = $derived(
    queryPreferencesService.getPreferences(nodeId).viewConfigs.kanban?.groupBy
  );
  const activeGroupBy = $derived(picked ?? resolveActiveGroupBy(eligible, storedGroupBy));
  const activeField = $derived(eligible.find((f) => f.name === activeGroupBy) ?? null);

  // Columns from the enum's values, plus a trailing Unassigned bucket.
  const columns = $derived(enumColumns(activeField));
  const displayColumns = $derived([...columns, { value: UNASSIGNED, label: 'Unassigned' }]);

  // Bucket the (existing) result nodes by their group-by value. Reads each node
  // from the store, so a move — which rewrites the value — re-derives the board.
  const buckets = $derived.by(() => {
    if (!activeGroupBy) return new Map<string, string[]>();
    const items = nodeIds
      .map((id) => {
        const n = sharedNodeStore.getNode(id);
        return n ? { id, value: readGroupValue(n, activeGroupBy) } : null;
      })
      .filter((it): it is { id: string; value: string | null } => it !== null);
    return groupByColumn(items, columns.map((c) => c.value));
  });

  function fieldLabel(f: SchemaField): string {
    if (f.description) return f.description;
    return f.name
      .replace(/_/g, ' ')
      .replace(/([a-z])([A-Z])/g, '$1 $2')
      .replace(/^\w/, (c) => c.toUpperCase());
  }

  function titleOf(node: Node): string {
    return node.title || node.content || 'Untitled';
  }

  /** The column a node currently belongs to (for the move control's value). */
  function currentColumn(node: Node): string {
    const v = activeGroupBy ? readGroupValue(node, activeGroupBy) : null;
    return v !== null && columns.some((c) => c.value === v) ? v : UNASSIGNED;
  }

  function onPickGroupBy(name: string): void {
    picked = name;
    queryPreferencesService.saveViewConfig(nodeId, 'kanban', { groupBy: name });
  }

  /** Move a card into the column identified by `toColumn` (UNASSIGNED clears it). */
  function moveCard(id: string, toColumn: string): void {
    const node = sharedNodeStore.getNode(id);
    if (!node || !activeGroupBy) return;
    const from = readGroupValue(node, activeGroupBy);
    const target = toColumn === UNASSIGNED ? null : toColumn;
    if (from === target) return; // dropping into its own column is a no-op — no write
    // Optimistic store write: the card moves as soon as the value changes. If the
    // persisted write is later rejected, the store rolls the value back — returning
    // the card to its original column — and raises its global conflict notification.
    // This is the same fire-and-forget path generic-schema-form uses; error
    // surfacing is the store's responsibility, not a per-view banner.
    const changes = resolveFieldWrite(node, activeGroupBy, target ?? '');
    log.debug('KanbanView: moving card', { id, field: activeGroupBy, toColumn });
    sharedNodeStore.updateNode(id, changes, { type: 'viewer', viewerId: 'kanban-view' });
  }

  function onDragStart(e: DragEvent, id: string): void {
    draggingId = id;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', id);
    }
  }

  function onDragEnd(): void {
    draggingId = null;
    dragOverColumn = null;
  }

  function onDragOver(e: DragEvent, columnValue: string): void {
    e.preventDefault();
    dragOverColumn = columnValue;
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
  }

  function onDrop(e: DragEvent, columnValue: string): void {
    e.preventDefault();
    const id = draggingId ?? e.dataTransfer?.getData('text/plain') ?? null;
    draggingId = null;
    dragOverColumn = null;
    if (id) moveCard(id, columnValue);
  }
</script>

{#if eligible.length === 0}
  <div class="kanban-empty">
    <p>This type has no enum field to group by, so there's no board to show.</p>
    <p class="kanban-empty-hint">Add an enum field to the schema to use the Kanban view.</p>
  </div>
{:else}
  <div class="kanban">
    <div class="kanban-toolbar">
      <label class="groupby">
        <span>Group by</span>
        <select
          value={activeGroupBy ?? ''}
          onchange={(e) => onPickGroupBy(e.currentTarget.value)}
        >
          {#each eligible as f (f.name)}
            <option value={f.name}>{fieldLabel(f)}</option>
          {/each}
        </select>
      </label>
    </div>

    <div class="kanban-board">
      {#each displayColumns as col (col.value)}
        {@const ids = buckets.get(col.value) ?? []}
        <section
          class="kanban-column"
          class:drag-over={dragOverColumn === col.value}
          role="group"
          aria-label={`${col.label} column`}
          ondragover={(e) => onDragOver(e, col.value)}
          ondragleave={() => (dragOverColumn = null)}
          ondrop={(e) => onDrop(e, col.value)}
        >
          <header class="kanban-column-header">
            <span class="kanban-column-title">{col.label}</span>
            <span class="kanban-column-count">{ids.length}</span>
          </header>
          <div class="kanban-cards">
            {#each ids as id (id)}
              {@const node = sharedNodeStore.getNode(id)}
              {#if node}
                {@const title = titleOf(node)}
                <article
                  class="kanban-card"
                  class:dragging={draggingId === id}
                  draggable="true"
                  ondragstart={(e) => onDragStart(e, id)}
                  ondragend={onDragEnd}
                >
                  <button class="kanban-card-title" onclick={() => onRowClick(id)} title={`Open ${title}`}>
                    {title}
                  </button>
                  <select
                    class="kanban-card-move"
                    aria-label={`Move ${title} to another column`}
                    value={currentColumn(node)}
                    onchange={(e) => moveCard(id, e.currentTarget.value)}
                  >
                    {#each displayColumns as target (target.value)}
                      <option value={target.value}>{target.label}</option>
                    {/each}
                  </select>
                </article>
              {/if}
            {/each}
          </div>
        </section>
      {/each}
    </div>
  </div>
{/if}

<style>
  .kanban {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    height: 100%;
  }

  .kanban-toolbar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-shrink: 0;
  }

  .groupby {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
  }

  .groupby select {
    font-size: 0.8125rem;
    padding: 0.25rem 0.5rem;
    border: 1px solid hsl(var(--border));
    border-radius: 0.375rem;
    background: hsl(var(--background));
    color: hsl(var(--foreground));
  }

  .kanban-board {
    display: flex;
    gap: 0.75rem;
    align-items: flex-start;
    overflow-x: auto;
    padding-bottom: 0.5rem;
    flex: 1;
  }

  .kanban-column {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    min-width: 15rem;
    max-width: 18rem;
    flex: 0 0 auto;
    background: hsl(var(--muted) / 0.4);
    border: 1px solid hsl(var(--border));
    border-radius: 0.5rem;
    padding: 0.5rem;
    transition: background-color 0.15s ease, border-color 0.15s ease;
  }

  .kanban-column.drag-over {
    background: hsl(var(--primary) / 0.08);
    border-color: hsl(var(--primary));
  }

  .kanban-column-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.25rem 0.25rem 0.5rem;
    border-bottom: 1px solid hsl(var(--border));
  }

  .kanban-column-title {
    font-size: 0.8125rem;
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .kanban-column-count {
    font-size: 0.75rem;
    color: hsl(var(--muted-foreground));
    background: hsl(var(--muted));
    border-radius: 9999px;
    padding: 0.0625rem 0.5rem;
  }

  .kanban-cards {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    min-height: 2rem;
  }

  .kanban-card {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    background: hsl(var(--background));
    border: 1px solid hsl(var(--border));
    border-radius: 0.375rem;
    padding: 0.5rem;
    cursor: grab;
  }

  .kanban-card.dragging {
    opacity: 0.5;
  }

  .kanban-card-title {
    text-align: left;
    background: transparent;
    border: none;
    padding: 0;
    font-size: 0.8125rem;
    font-weight: 500;
    color: hsl(var(--foreground));
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .kanban-card-title:hover {
    color: hsl(var(--primary));
    text-decoration: underline;
  }

  .kanban-card-move {
    font-size: 0.75rem;
    padding: 0.125rem 0.25rem;
    border: 1px solid hsl(var(--border));
    border-radius: 0.25rem;
    background: hsl(var(--background));
    color: hsl(var(--muted-foreground));
  }

  .kanban-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 3rem;
    text-align: center;
    color: hsl(var(--muted-foreground));
    gap: 0.5rem;
  }

  .kanban-empty p {
    margin: 0;
  }

  .kanban-empty-hint {
    font-size: 0.8125rem;
  }
</style>
