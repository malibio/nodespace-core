<!--
  RelationshipViewerModal — view AND edit a node's schema-declared typed
  relationships (issue #1918).

  Displays relationships grouped by name, keeping BOTH directions as separate
  groups (outbound declared on this node's schema + inbound resolved via the
  relationship cache). Groups that carry edge attributes render as a small table
  of target + edge values; bare relationships (no edge data) render as compact
  chips.

  An "Edit" toggle turns the viewer editable: each row gains a remove control,
  edge-attribute groups gain per-row editable inputs with a Save action, and each
  group gains an Add control with a type-ahead target picker (searching nodes of
  the declared target type by title, or accepting a pasted UUID). All mutations
  route through the dual-mode relationship service, so this works in both the
  Tauri desktop app and `dev:browser`.
-->
<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import LoaderIcon from '@lucide/svelte/icons/loader-circle';
  import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
  import ArrowRightIcon from '@lucide/svelte/icons/arrow-right';
  import ArrowLeftIcon from '@lucide/svelte/icons/arrow-left';
  import PencilIcon from '@lucide/svelte/icons/pencil';
  import XIcon from '@lucide/svelte/icons/x';
  import PlusIcon from '@lucide/svelte/icons/plus';
  import CheckIcon from '@lucide/svelte/icons/check';
  import { createLogger } from '$lib/utils/logger';
  import {
    loadNodeRelationshipsView,
    addEdge,
    removeEdge,
    updateEdgeProperties,
    searchTargets
  } from '$lib/services/relationship-viewer-service';
  import type {
    NodeRelationshipsView,
    RelationshipGroupView,
    RelationshipRowView,
    RawEdgeField
  } from '$lib/services/relationship-grouping';
  import type { Node } from '$lib/types';

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

  // --- Edit state -----------------------------------------------------------
  let editMode = $state(false);
  let busy = $state(false);
  let mutationError = $state<string | null>(null);

  // Per-row edge-attribute drafts, keyed by `${group.key}::${rowId}`. Only the
  // fields the user has touched are stored; the merge on save layers them over
  // the row's stored edge values.
  let edgeDrafts = $state<Record<string, Record<string, unknown>>>({});

  // Target picker (one group open at a time).
  let addGroup = $state<RelationshipGroupView | null>(null);
  let addQuery = $state('');
  let addResults = $state<Node[]>([]);
  let addSearching = $state(false);
  // A picked target awaiting edge-attribute entry (only for groups with declared
  // edge fields); null means the picker is still in search mode.
  let addStaged = $state<{ id: string; label: string } | null>(null);
  let addEdgeDraft = $state<Record<string, unknown>>({});
  let searchTimer: ReturnType<typeof setTimeout> | undefined;

  const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

  // Read mode shows only populated groups (the clean, read-only view). Edit mode
  // shows every group — including a declared-but-empty outbound relationship — so
  // its Add control can create the first edge. When this ends up empty (nothing
  // populated in read mode, or genuinely no groups at all) the "no typed
  // relationships" placeholder takes over.
  const visibleGroups = $derived(
    view ? (editMode ? view.groups : view.groups.filter((g) => g.rows.length > 0)) : []
  );

  $effect(() => {
    if (!open) {
      loadedKey = null;
      editMode = false;
      resetTransient();
      return;
    }
    if (!nodeId || loadedKey === nodeId) return;
    loadedKey = nodeId;
    editMode = false;
    void load(nodeId);
  });

  function resetTransient() {
    edgeDrafts = {};
    addGroup = null;
    addQuery = '';
    addResults = [];
    addSearching = false;
    addStaged = null;
    addEdgeDraft = {};
    mutationError = null;
  }

  async function load(id: string) {
    phase = 'loading';
    view = null;
    errorMessage = null;
    resetTransient();
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

  /** Re-fetch after a mutation WITHOUT blanking the view or leaving edit mode. */
  async function reloadAfterMutation() {
    try {
      const result = await loadNodeRelationshipsView(nodeId);
      if (loadedKey !== nodeId) return;
      view = result;
    } catch (error) {
      log.error('Failed to reload relationships', error);
      mutationError = error instanceof Error ? error.message : String(error);
    }
  }

  /**
   * Run a relationship mutation, then reload so both directions reflect the
   * change. Returns `true` on success. Transient cleanup is left to the caller so
   * a completed action only clears its OWN state (e.g. the saved row's draft, or
   * the open picker) and never wipes an unrelated row's unsaved edit. On failure
   * nothing is cleared so the user can retry, and the error is surfaced.
   */
  async function runMutation(fn: () => Promise<void>): Promise<boolean> {
    busy = true;
    mutationError = null;
    try {
      await fn();
      await reloadAfterMutation();
      return true;
    } catch (error) {
      log.error('Relationship mutation failed', error);
      mutationError = error instanceof Error ? error.message : String(error);
      return false;
    } finally {
      busy = false;
    }
  }

  // --- Edge-attribute editing ----------------------------------------------

  function rowKey(group: RelationshipGroupView, row: RelationshipRowView): string {
    return `${group.key}::${row.id}`;
  }

  function currentEdgeValue(
    group: RelationshipGroupView,
    row: RelationshipRowView,
    fieldName: string
  ): unknown {
    const draft = edgeDrafts[rowKey(group, row)];
    if (draft && fieldName in draft) return draft[fieldName];
    return row.edgeValues[fieldName];
  }

  function setEdgeDraft(
    group: RelationshipGroupView,
    row: RelationshipRowView,
    fieldName: string,
    value: unknown
  ) {
    const key = rowKey(group, row);
    edgeDrafts = { ...edgeDrafts, [key]: { ...(edgeDrafts[key] ?? {}), [fieldName]: value } };
  }

  function rowHasDraft(group: RelationshipGroupView, row: RelationshipRowView): boolean {
    const draft = edgeDrafts[rowKey(group, row)];
    return !!draft && Object.keys(draft).length > 0;
  }

  /** Drop a single row's draft without disturbing any other row's unsaved edits. */
  function clearRowDraft(group: RelationshipGroupView, row: RelationshipRowView) {
    const key = rowKey(group, row);
    if (!(key in edgeDrafts)) return;
    const next = { ...edgeDrafts };
    delete next[key];
    edgeDrafts = next;
  }

  async function saveRow(group: RelationshipGroupView, row: RelationshipRowView) {
    const draft = edgeDrafts[rowKey(group, row)] ?? {};
    // update replaces edge properties wholesale — send the full merged bag.
    const properties = { ...row.edgeValues, ...draft };
    const ok = await runMutation(() => updateEdgeProperties(nodeId, group, row.id, properties));
    // Only this row's draft is now stale (its values are persisted); leave any
    // other row's in-progress edit untouched.
    if (ok) clearRowDraft(group, row);
  }

  // --- Removal --------------------------------------------------------------

  async function removeRow(group: RelationshipGroupView, row: RelationshipRowView) {
    if (group.required && group.rows.length === 1) {
      const confirmed = window.confirm(
        `Remove the last "${group.label}" relationship? This relationship is required.`
      );
      if (!confirmed) return;
    }
    const ok = await runMutation(() => removeEdge(nodeId, group, row.id));
    // Drop any draft that belonged to the now-removed row so it can't linger.
    if (ok) clearRowDraft(group, row);
  }

  // --- Target picker --------------------------------------------------------

  function openAdd(group: RelationshipGroupView) {
    addGroup = group;
    addQuery = '';
    addResults = [];
    addSearching = false;
    addStaged = null;
    addEdgeDraft = {};
    mutationError = null;
  }

  function closeAdd() {
    addGroup = null;
    addQuery = '';
    addResults = [];
    addSearching = false;
    addStaged = null;
    addEdgeDraft = {};
  }

  function onAddQueryInput(value: string) {
    addQuery = value;
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => void runSearch(), 200);
  }

  async function runSearch() {
    const group = addGroup;
    if (!group) return;
    const q = addQuery.trim();
    if (!q) {
      addResults = [];
      return;
    }
    addSearching = true;
    try {
      const results = await searchTargets(group.targetType, q);
      if (addGroup?.key !== group.key) return;
      const existing = new Set(group.rows.map((r) => r.id));
      addResults = results.filter((n) => !existing.has(n.id));
    } catch (error) {
      log.error('Target search failed', error);
      addResults = [];
    } finally {
      addSearching = false;
    }
  }

  function nodeLabel(node: Node): string {
    return node.title?.trim() || node.content?.trim() || node.id;
  }

  /** Pick a target: create immediately when the group has no edge fields, else
   *  stage it and prompt for the declared edge-attribute values. */
  function pickTarget(group: RelationshipGroupView, targetId: string, label: string) {
    if (group.edgeFields.length === 0) {
      void confirmAdd(group, targetId, {});
      return;
    }
    addStaged = { id: targetId, label };
    addEdgeDraft = {};
  }

  async function confirmAdd(
    group: RelationshipGroupView,
    targetId: string,
    edgeData: Record<string, unknown>
  ) {
    const hasEdgeData = Object.keys(edgeData).length > 0;
    const ok = await runMutation(() =>
      addEdge(nodeId, group, targetId, hasEdgeData ? edgeData : undefined)
    );
    // Close only the picker for the completed add; other rows' drafts are kept.
    if (ok) closeAdd();
  }

  // --- Edge-field input helpers --------------------------------------------

  type EdgeInputKind = 'number' | 'boolean' | 'date' | 'datetime' | 'text';

  function edgeInputKind(field: RawEdgeField): EdgeInputKind {
    switch (field.type) {
      case 'number':
      case 'integer':
      case 'float':
        return 'number';
      case 'boolean':
      case 'bool':
        return 'boolean';
      case 'date':
        return 'date';
      case 'datetime':
        // A whole-day `date` input would silently drop the time component.
        return 'datetime';
      default:
        // enum has no declared option set on the edge-field definition, so it
        // falls back to a free-text input alongside string/text/unknown types.
        return 'text';
    }
  }

  /** Native input `type` for a text-like edge-field kind. */
  function edgeInputType(kind: EdgeInputKind): 'date' | 'datetime-local' | 'text' {
    if (kind === 'date') return 'date';
    if (kind === 'datetime') return 'datetime-local';
    return 'text';
  }

  function coerceNumber(raw: string): number | null {
    if (raw.trim() === '') return null;
    const n = Number(raw);
    return Number.isNaN(n) ? null : n;
  }

  function toInputString(value: unknown): string {
    if (value === null || value === undefined) return '';
    if (typeof value === 'string' || typeof value === 'number') return String(value);
    return String(value);
  }

  /**
   * Format a stored value for a `datetime-local` input (`YYYY-MM-DDTHH:mm`),
   * preserving the time a plain `date` input would drop. Accepts an ISO string
   * (with or without a trailing `Z`/offset) or anything `Date` can parse; returns
   * `''` for an unparseable/empty value. Values the input already yields (naive
   * local `YYYY-MM-DDTHH:mm`) pass straight back through, so a save→reload round
   * trip does not drift.
   */
  function toDateTimeLocalString(value: unknown): string {
    const raw = toInputString(value).trim();
    if (raw === '') return '';
    const isoish = raw.match(/^(\d{4}-\d{2}-\d{2})[T ](\d{2}:\d{2})/);
    if (isoish) return `${isoish[1]}T${isoish[2]}`;
    const parsed = new Date(raw);
    if (Number.isNaN(parsed.getTime())) return '';
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${parsed.getFullYear()}-${pad(parsed.getMonth() + 1)}-${pad(parsed.getDate())}T${pad(parsed.getHours())}:${pad(parsed.getMinutes())}`;
  }

  /** Value string for a text-like edge input, formatted for its `type`. */
  function edgeInputValue(kind: EdgeInputKind, value: unknown): string {
    return kind === 'datetime' ? toDateTimeLocalString(value) : toInputString(value);
  }

  // --- Read-only formatting (unchanged) ------------------------------------

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
      <div class="flex items-start justify-between gap-4">
        <div class="grid gap-1">
          <Dialog.Title>Relationships</Dialog.Title>
          <Dialog.Description>
            Typed relationships connecting this node to others, in both directions.
          </Dialog.Description>
        </div>
        {#if phase === 'loaded' && view}
          <Button
            variant={editMode ? 'secondary' : 'outline'}
            size="sm"
            class="shrink-0"
            onclick={() => (editMode = !editMode)}
          >
            {#if editMode}
              <CheckIcon class="mr-1.5 size-4" /> Done
            {:else}
              <PencilIcon class="mr-1.5 size-4" /> Edit
            {/if}
          </Button>
        {/if}
      </div>
    </Dialog.Header>

    {#if mutationError}
      <div
        class="border-destructive/30 bg-destructive/10 text-destructive flex items-start gap-2 rounded-md border p-3 text-sm"
      >
        <CircleAlertIcon class="mt-0.5 size-4 shrink-0" />
        <span>{mutationError}</span>
      </div>
    {/if}

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
      {:else if phase === 'loaded' && view && visibleGroups.length === 0}
        <div class="text-muted-foreground py-6 text-center text-sm">
          This node has no typed relationships.
        </div>
      {:else if phase === 'loaded' && view}
        <div class="grid gap-5 py-1">
          {#each visibleGroups as group (group.key)}
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
                {#if group.required}
                  <span class="text-muted-foreground text-xs">· required</span>
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
                        {#if editMode}
                          <th class="px-3 py-2"></th>
                        {/if}
                      </tr>
                    </thead>
                    <tbody>
                      {#each group.rows as row (row.id)}
                        <tr class="border-b last:border-b-0">
                          <td class="px-3 py-2 align-top">
                            <div class="font-medium">{row.label}</div>
                            <div class="text-muted-foreground text-xs">{row.nodeType}</div>
                          </td>
                          {#each group.edgeColumns as column (column)}
                            {@const field = group.edgeFields.find((f) => f.name === column)}
                            <td class="px-3 py-2 align-top">
                              {#if editMode && field}
                                {@const kind = edgeInputKind(field)}
                                {#if kind === 'boolean'}
                                  <Checkbox
                                    checked={Boolean(currentEdgeValue(group, row, column))}
                                    onCheckedChange={(v) =>
                                      setEdgeDraft(group, row, column, v === true)}
                                  />
                                {:else if kind === 'number'}
                                  <Input
                                    type="number"
                                    class="h-8"
                                    value={toInputString(currentEdgeValue(group, row, column))}
                                    oninput={(e) =>
                                      setEdgeDraft(
                                        group,
                                        row,
                                        column,
                                        coerceNumber(e.currentTarget.value)
                                      )}
                                  />
                                {:else}
                                  <Input
                                    type={edgeInputType(kind)}
                                    class="h-8"
                                    value={edgeInputValue(kind, currentEdgeValue(group, row, column))}
                                    oninput={(e) => setEdgeDraft(group, row, column, e.currentTarget.value)}
                                  />
                                {/if}
                              {:else if editMode}
                                <!-- Undeclared edge key: editable as free text. -->
                                <Input
                                  type="text"
                                  class="h-8"
                                  value={toInputString(currentEdgeValue(group, row, column))}
                                  oninput={(e) => setEdgeDraft(group, row, column, e.currentTarget.value)}
                                />
                              {:else}
                                {formatValue(row.edgeValues[column])}
                              {/if}
                            </td>
                          {/each}
                          {#if editMode}
                            <td class="px-3 py-2 align-top">
                              <div class="flex items-center justify-end gap-1">
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  class="h-8"
                                  disabled={busy || !rowHasDraft(group, row)}
                                  onclick={() => saveRow(group, row)}
                                >
                                  Save
                                </Button>
                                <Button
                                  variant="ghost"
                                  size="icon"
                                  class="text-muted-foreground hover:text-destructive size-8"
                                  disabled={busy}
                                  aria-label="Remove relationship"
                                  onclick={() => removeRow(group, row)}
                                >
                                  <XIcon class="size-4" />
                                </Button>
                              </div>
                            </td>
                          {/if}
                        </tr>
                      {/each}
                      {#if group.rows.length === 0}
                        <tr>
                          <td
                            class="text-muted-foreground px-3 py-3 text-sm"
                            colspan={group.edgeColumns.length + (editMode ? 2 : 1)}
                          >
                            No relationships yet — use Add below to create the first one.
                          </td>
                        </tr>
                      {/if}
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
                      {#if editMode}
                        <button
                          type="button"
                          class="text-muted-foreground hover:text-destructive -mr-1 ml-0.5 inline-flex disabled:opacity-50"
                          disabled={busy}
                          aria-label="Remove relationship"
                          onclick={() => removeRow(group, row)}
                        >
                          <XIcon class="size-3.5" />
                        </button>
                      {/if}
                    </span>
                  {/each}
                </div>
              {/if}

              {#if editMode}
                <div class="mt-1">
                  {#if addGroup?.key === group.key}
                    <div class="bg-muted/30 grid gap-2 rounded-md border p-3">
                      {#if addStaged}
                        <div class="flex items-center gap-2 text-sm">
                          <span class="font-medium">{addStaged.label}</span>
                          <button
                            type="button"
                            class="text-muted-foreground hover:text-foreground text-xs underline"
                            onclick={() => (addStaged = null)}
                          >
                            change
                          </button>
                        </div>
                        <div class="grid gap-2">
                          {#each group.edgeFields as field (field.name)}
                            {@const kind = edgeInputKind(field)}
                            <div class="grid gap-1">
                              <span class="text-muted-foreground text-xs capitalize">
                                {formatColumn(field.name)}
                              </span>
                              {#if kind === 'boolean'}
                                <Checkbox
                                  checked={Boolean(addEdgeDraft[field.name])}
                                  onCheckedChange={(v) =>
                                    (addEdgeDraft = { ...addEdgeDraft, [field.name]: v === true })}
                                />
                              {:else if kind === 'number'}
                                <Input
                                  type="number"
                                  class="h-8"
                                  value={toInputString(addEdgeDraft[field.name])}
                                  oninput={(e) =>
                                    (addEdgeDraft = {
                                      ...addEdgeDraft,
                                      [field.name]: coerceNumber(e.currentTarget.value)
                                    })}
                                />
                              {:else}
                                <Input
                                  type={edgeInputType(kind)}
                                  class="h-8"
                                  value={edgeInputValue(kind, addEdgeDraft[field.name])}
                                  oninput={(e) =>
                                    (addEdgeDraft = {
                                      ...addEdgeDraft,
                                      [field.name]: e.currentTarget.value
                                    })}
                                />
                              {/if}
                            </div>
                          {/each}
                        </div>
                        <div class="flex items-center gap-2">
                          <Button
                            size="sm"
                            disabled={busy}
                            onclick={() => confirmAdd(group, addStaged!.id, addEdgeDraft)}
                          >
                            Add
                          </Button>
                          <Button variant="ghost" size="sm" disabled={busy} onclick={closeAdd}>
                            Cancel
                          </Button>
                        </div>
                      {:else}
                        <Input
                          type="text"
                          placeholder={group.targetType
                            ? `Search ${group.targetType} by title, or paste an ID…`
                            : 'Search by title, or paste an ID…'}
                          value={addQuery}
                          oninput={(e) => onAddQueryInput(e.currentTarget.value)}
                        />
                        <div class="max-h-40 overflow-y-auto">
                          {#if addSearching}
                            <div class="text-muted-foreground flex items-center gap-2 px-1 py-2 text-sm">
                              <LoaderIcon class="size-4 animate-spin" />
                              <span>Searching…</span>
                            </div>
                          {:else}
                            {#each addResults as node (node.id)}
                              <button
                                type="button"
                                class="hover:bg-muted flex w-full flex-col items-start rounded-sm px-2 py-1.5 text-left text-sm disabled:opacity-50"
                                disabled={busy}
                                onclick={() => pickTarget(group, node.id, nodeLabel(node))}
                              >
                                <span class="font-medium">{nodeLabel(node)}</span>
                                <span class="text-muted-foreground text-xs">{node.nodeType}</span>
                              </button>
                            {/each}
                            {#if UUID_RE.test(addQuery.trim())}
                              <button
                                type="button"
                                class="hover:bg-muted flex w-full items-center gap-1.5 rounded-sm px-2 py-1.5 text-left text-sm disabled:opacity-50"
                                disabled={busy}
                                onclick={() =>
                                  pickTarget(group, addQuery.trim(), addQuery.trim())}
                              >
                                <PlusIcon class="size-3.5 shrink-0" />
                                <span>Use ID <code class="text-xs">{addQuery.trim()}</code></span>
                              </button>
                            {:else if addResults.length === 0 && addQuery.trim() !== ''}
                              <div class="text-muted-foreground px-2 py-1.5 text-sm">No matches.</div>
                            {/if}
                          {/if}
                        </div>
                        <div>
                          <Button variant="ghost" size="sm" disabled={busy} onclick={closeAdd}>
                            Cancel
                          </Button>
                        </div>
                      {/if}
                    </div>
                  {:else}
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={busy}
                      onclick={() => openAdd(group)}
                    >
                      <PlusIcon class="mr-1.5 size-4" /> Add
                    </Button>
                  {/if}
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
