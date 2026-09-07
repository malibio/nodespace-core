<!--
  RelationshipViewerModal — view and edit a node's typed relationships.

  Displays relationships grouped by name, keeping BOTH directions as separate
  groups (outbound declared on this node's schema + inbound resolved via the
  relationship cache). Groups that carry edge attributes render as a small table
  of target + edge values; bare relationships (no edge data) render as compact
  chips.

  ## The panel's size tracks the node's DATA, not its schema

  Only groups that actually HAVE edges get a section. Every outbound relationship
  with no edges yet collapses into a single "Add relationship" chooser, and empty
  inbound groups are dropped outright (see `partitionGroups`). Without this, a
  type declaring six relationships renders six empty sections before any edge
  exists — at its most overwhelming exactly when it has least to say.

  ## Outbound is editable; inbound is read-only

  An inbound group is not a second relationship — it is the SAME physical edge
  seen from the other end, declared on and owned by the OTHER node's schema.
  Editing it from here would write to a node the panel isn't showing, and an
  "Add" would have to invert the picker's meaning (choose a source, not a
  target) and would simply fail whenever the other type doesn't declare the
  forward relationship. So inbound rows are read-only and instead NAVIGATE to
  the node that owns the edge, where it can be edited properly.

  ## Controls are per row, never a panel-wide mode

  Outbound rows carry their own remove control, and — only where the schema
  declares `edge_fields` to edit — an edit control opening `EdgePropertiesModal`
  for that one edge. Edge values stay visible as read-only cells throughout;
  editing one row leaves every other row exactly as it was. All mutations route
  through the dual-mode relationship service, so this works in both the Tauri
  desktop app and `dev:browser`.
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
  import SlidersIcon from '@lucide/svelte/icons/sliders-horizontal';
  import * as Popover from '$lib/components/ui/popover';
  import { createLogger } from '$lib/utils/logger';
  import { getNavigationService } from '$lib/services/navigation-service';
  import EdgePropertiesModal from './edge-properties-modal.svelte';
  import {
    loadNodeRelationshipsView,
    addEdge,
    removeEdge,
    updateEdgeProperties,
    searchTargets,
    fetchTargetSchemaFields,
    fetchNodesProperties
  } from '$lib/services/relationship-viewer-service';
  import {
    findGroupByKey,
    findRowByKey,
    groupSupportsEdgeEditing,
    partitionGroups,
    type NodeRelationshipsView,
    type RelationshipGroupView,
    type RelationshipRowView
  } from '$lib/services/relationship-grouping';
  import {
    coerceNumber,
    edgeInputKind,
    edgeInputType,
    edgeInputValue,
    formatEdgeFieldLabel,
    toInputString
  } from '$lib/services/edge-field-input';
  import {
    LABEL_COLUMN,
    applyViewSettings,
    cellValue,
    defaultColumnTokens,
    defaultViewSettings,
    parseColumnToken,
    resolveColumnCandidates,
    resolveDisplayedColumns,
    type ColumnCandidate,
    type RelationshipViewRow,
    type RelationshipViewSettings,
    type SortDirection
  } from '$lib/services/relationship-view-settings';
  import { RelationshipViewSettingsService } from '$lib/services/relationship-view-settings-service';
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
  let busy = $state(false);
  let mutationError = $state<string | null>(null);

  // Per-row edge-attribute drafts, keyed by `${group.key}::${rowId}`. Only the
  // fields the user has touched are stored; the merge on save layers them over
  // the row's stored edge values.
  let edgeDrafts = $state<Record<string, Record<string, unknown>>>({});

  // --- View settings (issue #1918 slice d) -----------------------------------
  // Per-group presentation prefs (columns/sort/filter), keyed by group.key and
  // loaded from / persisted to localStorage per (nodeType, relationshipName,
  // direction). The stored view-model is never mutated — displayed columns/rows
  // are derived from these settings.
  let viewSettings = $state<Record<string, RelationshipViewSettings>>({});
  // Which group's settings popover is open, keyed by group.key.
  let settingsOpen = $state<Record<string, boolean>>({});
  // Target node field names per target type, lazily fetched when a settings
  // popover opens, to offer target-schema-field columns.
  let targetFieldNames = $state<Record<string, string[]>>({});
  // Related-node property bags keyed by node id, lazily fetched when a group has
  // a `field:` column selected (values for target-schema-field columns).
  let targetProps = $state<Record<string, Record<string, unknown>>>({});
  // Non-reactive guard tracking which node ids have been requested, so the
  // property-fetch effect never double-fetches or loops.
  let requestedPropIds = new Set<string>();

  // The one row whose edge properties are open in EdgePropertiesModal, if any,
  // held as a (group key, row id) pair rather than as the objects themselves.
  // Scoped to a single edge on purpose: opening it must not change the state of
  // any other row, nor of the panel behind it.
  //
  // Keys, not objects: every mutation reloads `view` into a new object graph, so
  // a captured group/row would leave the editor rendering — and saving — values
  // from before the reload. Resolving per render also hides the editor once a
  // reload no longer contains the edge. Note the limit: that is a reaction to a
  // reload, not a check at save time, so a save racing an out-of-band delete
  // still reaches the daemon and relies on its error being surfaced.
  let editingKey = $state<{ groupKey: string; rowId: string } | null>(null);
  // Whether the "Add relationship" type chooser is open.
  let addChooserOpen = $state(false);

  // Target picker (one group open at a time), keyed for the same reason.
  let addGroupKey = $state<string | null>(null);
  let addQuery = $state('');
  let addResults = $state<Node[]>([]);
  let addSearching = $state(false);
  // A picked target awaiting edge-attribute entry (only for groups with declared
  // edge fields); null means the picker is still in search mode.
  let addStaged = $state<{ id: string; label: string } | null>(null);
  let addEdgeDraft = $state<Record<string, unknown>>({});
  let searchTimer: ReturnType<typeof setTimeout> | undefined;

  const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

  // Sections come from groups that actually have edges; every empty outbound
  // relationship is folded into the single Add chooser instead of standing open
  // as an empty section. When BOTH are empty there is nothing to offer at all,
  // and the "no typed relationships" placeholder takes over.
  const partitioned = $derived(partitionGroups(view?.groups ?? []));
  const populatedGroups = $derived(partitioned.populated);
  const addableGroups = $derived(partitioned.addable);

  // Both resolve against the CURRENT view every render, so an open editor or
  // picker always reflects the latest reload — and resolves to null once the
  // group or edge it refers to is gone, which unmounts what was open.
  const editing = $derived(
    findRowByKey(view?.groups ?? [], editingKey?.groupKey ?? null, editingKey?.rowId ?? null)
  );
  const addGroup = $derived(findGroupByKey(view?.groups ?? [], addGroupKey));

  // Resolving to null hides the editor but does NOT by itself forget which row
  // was open, and neither key is unique enough to leave lying around: a group
  // key is `direction:name:targetType` and a row id is the TARGET NODE's id, not
  // the edge's. So re-adding the same target to the same relationship would
  // rebuild a row that matches the dangling key and pop the editor open unbidden
  // over the new edge, pre-filled with the draft from the old one. Drop the key
  // and its draft the moment resolution misses, so a closed editor stays closed.
  $effect(() => {
    if (editingKey && !editing) {
      const stale = draftKey(editingKey.groupKey, editingKey.rowId);
      editingKey = null;
      clearDraftKey(stale);
    }
  });

  // Same reasoning for the target picker: a group key outliving its group would
  // reopen the picker if a schema change brought that relationship back.
  $effect(() => {
    if (addGroupKey && !addGroup) closeAdd();
  });

  $effect(() => {
    if (!open) {
      loadedKey = null;
      resetTransient();
      return;
    }
    if (!nodeId || loadedKey === nodeId) return;
    loadedKey = nodeId;
    void load(nodeId);
  });

  function resetTransient() {
    edgeDrafts = {};
    editingKey = null;
    addChooserOpen = false;
    addGroupKey = null;
    addQuery = '';
    addResults = [];
    addSearching = false;
    addStaged = null;
    addEdgeDraft = {};
    mutationError = null;
    settingsOpen = {};
    targetProps = {};
    requestedPropIds = new Set();
  }

  // --- View settings: load / persist ----------------------------------------

  // Load each table group's persisted settings when the view (re)loads. Settings
  // are persisted on every change, so re-reading storage after a mutation reload
  // restores exactly what the user configured.
  $effect(() => {
    if (!view) return;
    const nodeType = view.nodeType;
    const next: Record<string, RelationshipViewSettings> = {};
    for (const group of view.groups) {
      if (group.layout !== 'table') continue;
      next[group.key] = RelationshipViewSettingsService.get(
        nodeType,
        group.relationshipName,
        group.direction,
        group.targetType
      );
    }
    viewSettings = next;
  });

  // Fetch target-node property bags for rows in groups that show a `field:`
  // column. Guarded by requestedPropIds so it fetches each id once and never
  // loops (it does not read the reactive targetProps map).
  $effect(() => {
    if (!view) return;
    const needed = new Set<string>();
    for (const group of view.groups) {
      if (group.layout !== 'table') continue;
      const tokens = (viewSettings[group.key] ?? defaultViewSettings()).columns ?? [];
      if (!tokens.some((token) => parseColumnToken(token).source === 'field')) continue;
      for (const row of group.rows) needed.add(row.id);
    }
    const missing = [...needed].filter((id) => !requestedPropIds.has(id));
    if (missing.length === 0) return;
    for (const id of missing) requestedPropIds.add(id);
    void loadTargetProps(missing);
  });

  async function loadTargetProps(ids: string[]) {
    try {
      const fetched = await fetchNodesProperties(ids);
      targetProps = { ...targetProps, ...fetched };
    } catch (error) {
      log.error('Failed to load target properties', error);
    }
  }

  function settingsFor(group: RelationshipGroupView): RelationshipViewSettings {
    return viewSettings[group.key] ?? defaultViewSettings();
  }

  function candidatesFor(group: RelationshipGroupView): ColumnCandidate[] {
    return resolveColumnCandidates({
      edgeColumns: group.edgeColumns,
      targetFieldNames: group.targetType ? targetFieldNames[group.targetType] : null
    });
  }

  function displayedColumnsFor(group: RelationshipGroupView): ColumnCandidate[] {
    return resolveDisplayedColumns(settingsFor(group), candidatesFor(group));
  }

  function displayedRowsFor(group: RelationshipGroupView): RelationshipViewRow[] {
    const rows: RelationshipViewRow[] = group.rows.map((row) => ({
      id: row.id,
      nodeType: row.nodeType,
      label: row.label,
      edgeValues: row.edgeValues,
      targetProperties: targetProps[row.id]
    }));
    return applyViewSettings(rows, settingsFor(group));
  }

  function isColumnShown(
    group: RelationshipGroupView,
    token: string,
    candidates: ColumnCandidate[]
  ): boolean {
    if (token === LABEL_COLUMN) return true;
    const tokens = settingsFor(group).columns ?? defaultColumnTokens(candidates);
    return tokens.includes(token);
  }

  function saveSettings(group: RelationshipGroupView, next: RelationshipViewSettings) {
    viewSettings = { ...viewSettings, [group.key]: next };
    if (view) {
      RelationshipViewSettingsService.set(
        view.nodeType,
        group.relationshipName,
        group.direction,
        group.targetType,
        next
      );
    }
  }

  function onSettingsOpenChange(group: RelationshipGroupView, open: boolean) {
    settingsOpen = { ...settingsOpen, [group.key]: open };
    if (open) void ensureTargetSchema(group.targetType);
  }

  async function ensureTargetSchema(targetType: string | null) {
    if (!targetType || targetType in targetFieldNames) return;
    try {
      const names = await fetchTargetSchemaFields(targetType);
      targetFieldNames = { ...targetFieldNames, [targetType]: names };
    } catch (error) {
      log.error('Failed to load target schema fields', error);
      targetFieldNames = { ...targetFieldNames, [targetType]: [] };
    }
  }

  function toggleColumn(group: RelationshipGroupView, token: string, checked: boolean) {
    const current = settingsFor(group);
    const base = current.columns ?? defaultColumnTokens(candidatesFor(group));
    const columns = checked
      ? base.includes(token)
        ? base
        : [...base, token]
      : base.filter((t) => t !== token);
    // Hiding a column that drives the sort/filter would leave those controls
    // pointing at a column the user can no longer see (and a select value with
    // no matching option). Clear them when their column is hidden.
    const sort = !checked && current.sort?.column === token ? null : current.sort;
    const filter = !checked && current.filter?.column === token ? null : current.filter;
    saveSettings(group, { ...current, columns, sort, filter });
  }

  function setSortColumn(group: RelationshipGroupView, token: string) {
    const current = settingsFor(group);
    const sort =
      token === '' ? null : { column: token, direction: current.sort?.direction ?? 'asc' };
    saveSettings(group, { ...current, sort });
  }

  function setSortDirection(group: RelationshipGroupView, direction: SortDirection) {
    const current = settingsFor(group);
    if (!current.sort) return;
    saveSettings(group, { ...current, sort: { ...current.sort, direction } });
  }

  function setFilterColumn(group: RelationshipGroupView, token: string) {
    const current = settingsFor(group);
    const filter =
      token === '' ? null : { column: token, value: current.filter?.value ?? '' };
    saveSettings(group, { ...current, filter });
  }

  function setFilterValue(group: RelationshipGroupView, value: string) {
    const current = settingsFor(group);
    const filter = { column: current.filter?.column ?? LABEL_COLUMN, value };
    saveSettings(group, { ...current, filter });
  }

  function resetGroupSettings(group: RelationshipGroupView) {
    saveSettings(group, defaultViewSettings());
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

  /** The `edgeDrafts` key for one row, from the same parts `editingKey` holds. */
  function draftKey(groupKey: string, rowId: string): string {
    return `${groupKey}::${rowId}`;
  }

  function rowKey(group: RelationshipGroupView, row: RelationshipRowView): string {
    return draftKey(group.key, row.id);
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

  /** Drop a single row's draft without disturbing any other row's unsaved edits. */
  function clearRowDraft(group: RelationshipGroupView, row: RelationshipRowView) {
    clearDraftKey(rowKey(group, row));
  }

  /**
   * Drop a draft by its composite key, for the case where the row object is no
   * longer reachable — its edge has gone from the view — but its draft is still
   * held under the key that row had.
   */
  function clearDraftKey(key: string) {
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
    if (ok) {
      clearRowDraft(group, row);
      editingKey = null;
    }
    // On failure the editor stays open with the draft intact so the surfaced
    // error can be acted on without retyping.
  }

  /** Open the edge-property editor for one row. */
  function openEdit(group: RelationshipGroupView, row: RelationshipRowView) {
    mutationError = null;
    editingKey = { groupKey: group.key, rowId: row.id };
  }

  /** Close the editor, discarding only THIS row's unsaved draft. */
  function cancelEdit() {
    // Read the resolved pair, not the key: the draft is keyed by group+row, and
    // if the edge has already vanished there is no draft left to clear.
    if (editing) clearRowDraft(editing.group, editing.row);
    editingKey = null;
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

  // --- Inbound navigation ---------------------------------------------------

  /**
   * Follow an inbound row to the node that OWNS the edge. That node's own panel
   * declares the relationship outbound, which is the only place the edge can
   * actually be edited — so this is the read-only side's route to editing.
   * The modal closes so it doesn't sit over the destination.
   */
  function navigateToOwner(row: RelationshipRowView) {
    open = false;
    void getNavigationService().navigateToNode(row.id);
  }

  // --- Target picker --------------------------------------------------------

  /** Choose a relationship type from the Add chooser, then pick its target. */
  function chooseAddType(group: RelationshipGroupView) {
    addChooserOpen = false;
    openAdd(group);
  }

  function openAdd(group: RelationshipGroupView) {
    addGroupKey = group.key;
    addQuery = '';
    addResults = [];
    addSearching = false;
    addStaged = null;
    addEdgeDraft = {};
    mutationError = null;
  }

  function closeAdd() {
    addGroupKey = null;
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
    const requestedKey = group.key;
    const q = addQuery.trim();
    if (!q) {
      addResults = [];
      return;
    }
    addSearching = true;
    try {
      const results = await searchTargets(group.targetType, q);
      // Discard a response for a picker that has since moved on.
      if (addGroupKey !== requestedKey) return;
      // Re-read the group AFTER the await: a mutation during the search would
      // have reloaded the view, and filtering on the pre-search rows could
      // offer a target that is already linked.
      const current = addGroup;
      if (!current) return;
      const existing = new Set(current.rows.map((r) => r.id));
      addResults = results.filter((n) => !existing.has(n.id));
    } catch (error) {
      log.error('Target search failed', error);
      // Only blank the results if this response still belongs to the open
      // picker — a superseded request must not clear what replaced it.
      if (addGroupKey === requestedKey) addResults = [];
    } finally {
      if (addGroupKey === requestedKey) addSearching = false;
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

  // --- Read-only formatting -------------------------------------------------

  function formatValue(value: unknown): string {
    if (value === null || value === undefined || value === '') return '—';
    if (typeof value === 'string') return value;
    if (typeof value === 'number' || typeof value === 'boolean') return String(value);
    return JSON.stringify(value);
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
      {:else if phase === 'loaded' && view && populatedGroups.length === 0 && addableGroups.length === 0}
        <div class="text-muted-foreground py-6 text-center text-sm">
          This node has no typed relationships.
        </div>
      {:else if phase === 'loaded' && view}
        <div class="grid gap-5 py-1">
          {#each populatedGroups as group (group.key)}
            {@const inbound = group.direction === 'in'}
            <section
              class="grid gap-2 {inbound ? 'border-muted-foreground/25 border-l-2 border-dashed pl-3' : ''}"
            >
              <header class="flex items-center gap-2">
                {#if inbound}
                  <ArrowLeftIcon class="text-muted-foreground size-4 shrink-0" />
                {:else}
                  <ArrowRightIcon class="text-muted-foreground size-4 shrink-0" />
                {/if}
                <span class="text-sm font-medium">{group.label}</span>
                {#if group.targetType}
                  <span class="text-muted-foreground text-xs">· {group.targetType}</span>
                {/if}
                {#if group.required}
                  <span class="text-muted-foreground text-xs">· required</span>
                {/if}
                {#if inbound}
                  <!-- The arrow alone is too subtle to carry "you cannot edit
                       this here"; say it, and say where it can be edited. -->
                  <span
                    class="border-border text-muted-foreground rounded-full border px-1.5 py-0.5 text-[0.6875rem] leading-none"
                    title="This edge is declared on the other node's schema — open that node to edit it."
                  >
                    incoming · read-only
                  </span>
                {/if}
                <span class="text-muted-foreground ml-auto text-xs">
                  {group.count}
                  {group.count === 1 ? 'item' : 'items'}
                </span>
                {#if group.layout === 'table'}
                  {@const candidates = candidatesFor(group)}
                  {@const settings = settingsFor(group)}
                  {@const sortableColumns = displayedColumnsFor(group)}
                  <Popover.Root
                    open={!!settingsOpen[group.key]}
                    onOpenChange={(o) => onSettingsOpenChange(group, o)}
                  >
                    <Popover.Trigger
                      class="text-muted-foreground hover:text-foreground hover:bg-muted focus-visible:ring-ring inline-flex size-7 shrink-0 items-center justify-center rounded-md focus-visible:outline-none focus-visible:ring-1"
                      aria-label="View settings"
                    >
                      <SlidersIcon class="size-4" />
                    </Popover.Trigger>
                    <Popover.Content class="w-72" align="end">
                      <div class="grid gap-3 text-sm">
                        <div class="grid gap-1.5">
                          <span class="text-xs font-medium">Columns</span>
                          {#each candidates as candidate (candidate.token)}
                            <label class="flex items-center gap-2">
                              <Checkbox
                                checked={isColumnShown(group, candidate.token, candidates)}
                                disabled={candidate.pinned}
                                onCheckedChange={(v) =>
                                  toggleColumn(group, candidate.token, v === true)}
                              />
                              <span>{candidate.label}</span>
                            </label>
                          {/each}
                        </div>

                        <div class="grid gap-1.5">
                          <span class="text-xs font-medium">Sort</span>
                          <div class="flex items-center gap-2">
                            <select
                              class="border-input bg-background h-8 flex-1 rounded-md border px-2 text-sm"
                              value={settings.sort?.column ?? ''}
                              onchange={(e) => setSortColumn(group, e.currentTarget.value)}
                            >
                              <option value="">None</option>
                              {#each sortableColumns as col (col.token)}
                                <option value={col.token}>{col.label}</option>
                              {/each}
                            </select>
                            <select
                              class="border-input bg-background h-8 rounded-md border px-2 text-sm disabled:opacity-50"
                              value={settings.sort?.direction ?? 'asc'}
                              disabled={!settings.sort}
                              onchange={(e) =>
                                setSortDirection(group, e.currentTarget.value === 'desc' ? 'desc' : 'asc')}
                            >
                              <option value="asc">Asc</option>
                              <option value="desc">Desc</option>
                            </select>
                          </div>
                        </div>

                        <div class="grid gap-1.5">
                          <span class="text-xs font-medium">Filter</span>
                          <div class="flex items-center gap-2">
                            <select
                              class="border-input bg-background h-8 flex-1 rounded-md border px-2 text-sm"
                              value={settings.filter?.column ?? ''}
                              onchange={(e) => setFilterColumn(group, e.currentTarget.value)}
                            >
                              <option value="">No filter</option>
                              {#each sortableColumns as col (col.token)}
                                <option value={col.token}>{col.label}</option>
                              {/each}
                            </select>
                            <Input
                              type="text"
                              class="h-8 flex-1"
                              placeholder="Value"
                              value={settings.filter?.value ?? ''}
                              disabled={!settings.filter}
                              oninput={(e) => setFilterValue(group, e.currentTarget.value)}
                            />
                          </div>
                        </div>

                        <div>
                          <Button variant="ghost" size="sm" onclick={() => resetGroupSettings(group)}>
                            Reset
                          </Button>
                        </div>
                      </div>
                    </Popover.Content>
                  </Popover.Root>
                {/if}
              </header>

              {#if group.layout === 'table'}
                {@const cols = displayedColumnsFor(group)}
                {@const displayRows = displayedRowsFor(group)}
                {@const canEditEdges = groupSupportsEdgeEditing(group)}
                {@const hasControls = !inbound}
                <div class="overflow-x-auto rounded-md border">
                  <table class="w-full border-collapse text-sm">
                    <thead>
                      <tr class="border-b">
                        {#each cols as col (col.token)}
                          <th class="text-muted-foreground px-3 py-2 text-left font-medium">
                            {col.label}
                          </th>
                        {/each}
                        {#if hasControls}
                          <th class="px-3 py-2"></th>
                        {/if}
                      </tr>
                    </thead>
                    <tbody>
                      {#each displayRows as row (row.id)}
                        <tr class="border-b last:border-b-0">
                          {#each cols as col (col.token)}
                            <td class="px-3 py-2 align-top">
                              {#if col.token === LABEL_COLUMN}
                                {#if inbound}
                                  <!-- The owning node is where this edge can be
                                       edited, so make the row the way there. -->
                                  <button
                                    type="button"
                                    class="hover:text-primary text-left hover:underline"
                                    aria-label="Open {row.label} to edit this relationship"
                                    onclick={() => navigateToOwner(row)}
                                  >
                                    <span class="font-medium">{row.label}</span>
                                    <span class="text-muted-foreground block text-xs">
                                      {row.nodeType}
                                    </span>
                                  </button>
                                {:else}
                                  <div class="font-medium">{row.label}</div>
                                  <div class="text-muted-foreground text-xs">{row.nodeType}</div>
                                {/if}
                              {:else}
                                <!-- Every value column reads read-only; edge
                                     properties are changed in the row's editor. -->
                                {formatValue(
                                  col.source === 'edge'
                                    ? row.edgeValues[col.key]
                                    : cellValue(row, col.token)
                                )}
                              {/if}
                            </td>
                          {/each}
                          {#if hasControls}
                            <td class="px-3 py-2 align-top">
                              <div class="flex items-center justify-end gap-1">
                                {#if canEditEdges}
                                  <Button
                                    variant="ghost"
                                    size="icon"
                                    class="text-muted-foreground hover:text-foreground size-8"
                                    disabled={busy}
                                    aria-label="Edit relationship properties"
                                    onclick={() => openEdit(group, row)}
                                  >
                                    <PencilIcon class="size-4" />
                                  </Button>
                                {/if}
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
                      {#if displayRows.length === 0}
                        <tr>
                          <td
                            class="text-muted-foreground px-3 py-3 text-sm"
                            colspan={cols.length + (hasControls ? 1 : 0)}
                          >
                            No relationships match the current filter.
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
                    >
                      {#if inbound}
                        <button
                          type="button"
                          class="hover:text-primary inline-flex items-center gap-1.5 hover:underline"
                          aria-label="Open {row.label} to edit this relationship"
                          onclick={() => navigateToOwner(row)}
                        >
                          <span class="font-medium">{row.label}</span>
                          <span class="text-muted-foreground text-xs">{row.nodeType}</span>
                        </button>
                      {:else}
                        <span class="font-medium">{row.label}</span>
                        <span class="text-muted-foreground text-xs">{row.nodeType}</span>
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

              {#if !inbound}
                <div class="mt-1">
                  {#if addGroup?.key === group.key}
                    {@render targetPicker(group)}
                  {:else}
                    <!-- Deliberately lighter than the panel's one "Add
                         relationship" control: this only extends a group that
                         is already on screen, so it should read as part of that
                         section rather than compete with the primary action. -->
                    <Button
                      variant="ghost"
                      size="sm"
                      class="text-muted-foreground hover:text-foreground h-7 px-2"
                      disabled={busy}
                      onclick={() => openAdd(group)}
                    >
                      <PlusIcon class="mr-1.5 size-3.5" /> Add
                    </Button>
                  {/if}
                </div>
              {/if}
            </section>
          {/each}

          <!-- Every outbound relationship with no edges yet lives behind this
               ONE control instead of getting a standing empty section. -->
          {#if addableGroups.length > 0}
            <div>
              <!-- The picker renders HERE only while its group is still empty.
                   Once its first edge lands, the group moves to `populated` and
                   its section renders the picker instead — but `confirmAdd`
                   closes the picker on success, so that hand-off is never seen
                   mid-interaction. It matters on the FAILURE path: the picker
                   stays open, exactly where the user left it. -->
              {#if addGroup && addableGroups.some((g) => g.key === addGroup.key)}
                {@render targetPicker(addGroup)}
              {:else}
                <Popover.Root bind:open={addChooserOpen}>
                  <Popover.Trigger
                    class="border-input bg-background hover:bg-accent hover:text-accent-foreground focus-visible:ring-ring inline-flex h-8 items-center rounded-md border px-3 text-sm font-medium focus-visible:outline-none focus-visible:ring-1 disabled:pointer-events-none disabled:opacity-50"
                    disabled={busy}
                  >
                    <PlusIcon class="mr-1.5 size-4" /> Add relationship
                  </Popover.Trigger>
                  <Popover.Content class="w-64 p-1" align="start">
                    <div class="grid">
                      {#each addableGroups as group (group.key)}
                        <button
                          type="button"
                          class="hover:bg-muted flex w-full flex-col items-start rounded-sm px-2 py-1.5 text-left text-sm"
                          onclick={() => chooseAddType(group)}
                        >
                          <span class="font-medium">{group.label}</span>
                          {#if group.targetType}
                            <span class="text-muted-foreground text-xs">{group.targetType}</span>
                          {/if}
                        </button>
                      {/each}
                    </div>
                  </Popover.Content>
                </Popover.Root>
              {/if}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </Dialog.Content>
</Dialog.Root>

<!--
  The target picker, shared by a populated group's inline Add and the chooser
  for groups with no edges yet — one implementation, so both paths stage edge
  attributes and accept a pasted ID identically.
-->
{#snippet targetPicker(group: RelationshipGroupView)}
  <div class="bg-muted/30 grid gap-2 rounded-md border p-3">
    <div class="text-muted-foreground text-xs">
      Add <span class="text-foreground font-medium">{group.label}</span>
      {#if group.targetType}· {group.targetType}{/if}
    </div>
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
              {formatEdgeFieldLabel(field.name)}
            </span>
            {#if kind === 'boolean'}
              <Checkbox
                checked={Boolean(addEdgeDraft[field.name])}
                aria-label={formatEdgeFieldLabel(field.name)}
                onCheckedChange={(v) =>
                  (addEdgeDraft = { ...addEdgeDraft, [field.name]: v === true })}
              />
            {:else if kind === 'number'}
              <Input
                type="number"
                class="h-8"
                aria-label={formatEdgeFieldLabel(field.name)}
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
                aria-label={formatEdgeFieldLabel(field.name)}
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
        <Button size="sm" disabled={busy} onclick={() => confirmAdd(group, addStaged!.id, addEdgeDraft)}>
          Add
        </Button>
        <Button variant="ghost" size="sm" disabled={busy} onclick={closeAdd}>Cancel</Button>
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
              onclick={() => pickTarget(group, addQuery.trim(), addQuery.trim())}
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
        <Button variant="ghost" size="sm" disabled={busy} onclick={closeAdd}>Cancel</Button>
      </div>
    {/if}
  </div>
{/snippet}

<!--
  One edge's properties, in their own dialog. Each relationship declares its own
  `edge_fields`, so the form is built per relationship rather than from a fixed
  column set — and the panel behind it stays exactly as it was.
-->
{#if editing}
  <EdgePropertiesModal
    relationshipLabel={editing.group.label}
    rowLabel={editing.row.label}
    fields={editing.group.edgeFields}
    valueFor={(name) => currentEdgeValue(editing.group, editing.row, name)}
    onChange={(name, value) => setEdgeDraft(editing.group, editing.row, name, value)}
    onSave={() => void saveRow(editing.group, editing.row)}
    onCancel={cancelEdit}
    {busy}
  />
{/if}
