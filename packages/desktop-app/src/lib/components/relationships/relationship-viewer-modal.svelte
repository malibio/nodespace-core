<!--
  RelationshipViewerModal — read-only viewer for a node's schema-declared typed
  relationships (issue #1918, first slice).

  Displays relationships grouped by name, keeping BOTH directions as separate
  groups (outbound declared on this node's schema + inbound resolved via the
  relationship cache). Groups that carry edge attributes render as a small table
  of target + edge values; bare relationships (no edge data) render as compact
  chips. Editing, target selection, cardinality enforcement and view settings are
  intentionally out of scope for this slice.
-->
<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import LoaderIcon from '@lucide/svelte/icons/loader-circle';
  import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
  import ArrowRightIcon from '@lucide/svelte/icons/arrow-right';
  import ArrowLeftIcon from '@lucide/svelte/icons/arrow-left';
  import { createLogger } from '$lib/utils/logger';
  import { loadNodeRelationshipsView } from '$lib/services/relationship-viewer-service';
  import type { NodeRelationshipsView } from '$lib/services/relationship-grouping';

  const log = createLogger('RelationshipViewerModal');

  interface Props {
    open: boolean;
    nodeId: string;
  }

  let { open = $bindable(false), nodeId }: Props = $props();

  type Phase = 'idle' | 'loading' | 'loaded' | 'error';

  let phase = $state<Phase>('idle');
  let view = $state<NodeRelationshipsView | null>(null);
  let errorMessage = $state<string | null>(null);
  // Tracks which node's data is currently loaded/loading, so re-renders don't
  // refetch and a stale response for a previous node is discarded.
  let loadedKey: string | null = null;

  $effect(() => {
    if (!open) {
      loadedKey = null;
      return;
    }
    if (!nodeId || loadedKey === nodeId) return;
    loadedKey = nodeId;
    void load(nodeId);
  });

  async function load(id: string) {
    phase = 'loading';
    view = null;
    errorMessage = null;
    try {
      const result = await loadNodeRelationshipsView(id);
      // Discard a stale response if the modal was reopened for another node.
      if (loadedKey !== id) return;
      view = result;
      phase = 'loaded';
    } catch (error) {
      if (loadedKey !== id) return;
      log.error('Failed to load relationships', error);
      errorMessage = error instanceof Error ? error.message : String(error);
      phase = 'error';
    }
  }

  function formatValue(value: unknown): string {
    if (value === null || value === undefined || value === '') return '—';
    if (typeof value === 'string') return value;
    if (typeof value === 'number' || typeof value === 'boolean') return String(value);
    return JSON.stringify(value);
  }

  function formatColumn(name: string): string {
    return name.replace(/[_-]+/g, ' ');
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-2xl">
    <Dialog.Header>
      <Dialog.Title>Relationships</Dialog.Title>
      <Dialog.Description>
        Typed relationships connecting this node to others, in both directions.
      </Dialog.Description>
    </Dialog.Header>

    <div class="max-h-[60vh] overflow-y-auto">
      {#if phase === 'loading'}
        <div class="text-muted-foreground flex items-center gap-2 py-6 text-sm">
          <LoaderIcon class="size-4 animate-spin" />
          <span>Loading relationships…</span>
        </div>
      {:else if phase === 'error'}
        <div
          class="border-destructive/30 bg-destructive/10 text-destructive flex items-start gap-2 rounded-md border p-3 text-sm"
        >
          <CircleAlertIcon class="mt-0.5 size-4 shrink-0" />
          <span>{errorMessage ?? 'Failed to load relationships.'}</span>
        </div>
      {:else if phase === 'loaded' && view && view.isEmpty}
        <div class="text-muted-foreground py-6 text-center text-sm">
          This node has no typed relationships.
        </div>
      {:else if phase === 'loaded' && view}
        <div class="grid gap-5 py-1">
          {#each view.groups as group (group.key)}
            <section class="grid gap-2">
              <header class="flex items-center gap-2">
                {#if group.direction === 'out'}
                  <ArrowRightIcon class="text-muted-foreground size-4 shrink-0" />
                {:else}
                  <ArrowLeftIcon class="text-muted-foreground size-4 shrink-0" />
                {/if}
                <span class="text-sm font-medium">{group.label}</span>
                {#if group.targetType}
                  <span class="text-muted-foreground text-xs">· {group.targetType}</span>
                {/if}
                <span class="text-muted-foreground ml-auto text-xs">
                  {group.count}
                  {group.count === 1 ? 'item' : 'items'}
                </span>
              </header>

              {#if group.layout === 'table'}
                <div class="overflow-x-auto rounded-md border">
                  <table class="w-full border-collapse text-sm">
                    <thead>
                      <tr class="border-b">
                        <th class="text-muted-foreground px-3 py-2 text-left font-medium">Target</th>
                        {#each group.edgeColumns as column (column)}
                          <th class="text-muted-foreground px-3 py-2 text-left font-medium capitalize">
                            {formatColumn(column)}
                          </th>
                        {/each}
                      </tr>
                    </thead>
                    <tbody>
                      {#each group.rows as row (row.id)}
                        <tr class="border-b last:border-b-0">
                          <td class="px-3 py-2">
                            <div class="font-medium">{row.label}</div>
                            <div class="text-muted-foreground text-xs">{row.nodeType}</div>
                          </td>
                          {#each group.edgeColumns as column (column)}
                            <td class="px-3 py-2">{formatValue(row.edgeValues[column])}</td>
                          {/each}
                        </tr>
                      {/each}
                    </tbody>
                  </table>
                </div>
              {:else}
                <div class="flex flex-wrap gap-2">
                  {#each group.rows as row (row.id)}
                    <span
                      class="border-border bg-muted/40 inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-sm"
                      title={row.nodeType}
                    >
                      <span class="font-medium">{row.label}</span>
                      <span class="text-muted-foreground text-xs">{row.nodeType}</span>
                    </span>
                  {/each}
                </div>
              {/if}
            </section>
          {/each}
        </div>
      {/if}
    </div>

    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (open = false)}>Close</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
