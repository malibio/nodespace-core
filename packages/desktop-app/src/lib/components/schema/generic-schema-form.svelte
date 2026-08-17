<!--
  GenericSchemaForm - Schema-driven property form for custom node types

  Renders fields from a SchemaNode definition as appropriate inputs.
  Used by BaseNodeViewer when a node's nodeType is a UUID (custom schema type)
  that has no registered plugin schema form.

  Field type → control:
  - leaf fields (string/text, number, boolean, enum, date) → SchemaFieldLeaf
  - object/array → summary trigger opening the shared NestedPropertyModal

  Values are stored/read from node.properties[field.name] (flat, not namespaced),
  nested values included.

  Props:
  - nodeId: ID of the node to display properties for
  - schema: SchemaNode definition to render fields from
-->

<script lang="ts">
  import { Collapsible } from 'bits-ui';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
  import type { SchemaNode, SchemaField } from '$lib/types/schema-node';
  import type { Node } from '$lib/types';
  import { createLogger } from '$lib/utils/logger';
  import { labelForField } from '$lib/utils/schema-field-label';
  import RelationshipViewerModal from '$lib/components/relationships/relationship-viewer-modal.svelte';
  import SchemaFieldLeaf from './schema-field-leaf.svelte';
  import NestedFieldTrigger from './nested-field-trigger.svelte';
  import NestedPropertyModal from './nested-property-modal.svelte';
  import { isNestedField } from '$lib/utils/nested-property-ops';
  import { loadNodeRelationshipsView } from '$lib/services/relationship-viewer-service';
  import WaypointsIcon from '@lucide/svelte/icons/waypoints';

  const log = createLogger('GenericSchemaForm');

  // Read-only relationship viewer entry point (issue #1918, first slice). This is
  // the properties area for custom schema types — where typed relationships are
  // conceptual siblings of fields. Task/date nodes use their own plugin schema
  // forms and do not yet expose this trigger (follow-up).
  let showRelationships = $state(false);

  // Nested (object/array) property editor. One modal instance is reused; the
  // clicked field determines what it edits.
  let nestedModalField = $state<SchemaField | null>(null);
  let nestedModalOpen = $state(false);

  let { nodeId, schema, autoOpen = false }: { nodeId: string; schema: SchemaNode; autoOpen?: boolean } = $props();

  // Gate the Relationships trigger on whether this node's type actually has any
  // typed relationship — otherwise it opens only to say "no typed relationships".
  // The viewer's own load resolves both sides (outbound declared on this schema +
  // inbound declared by another schema targeting this type) into one group per
  // relationship, so we run it once per node and gate on whether any group exists.
  // Default hidden; fail-open on a query error so a transient failure never hides a
  // real feature.
  let hasRelationships = $state(false);
  let relCheckedFor = '';
  $effect(() => {
    const id = nodeId;
    if (relCheckedFor === id) return;
    relCheckedFor = id;
    hasRelationships = false;
    loadNodeRelationshipsView(id)
      .then((view) => {
        if (nodeId === id) hasRelationships = view.groups.length > 0;
      })
      .catch((err) => {
        log.error('Failed to check relationships for the trigger gate', err);
        if (nodeId === id) hasRelationships = true;
      });
  });

  // Initial value only (IIFE avoids Svelte's state_referenced_locally warning) — after
  // mount isOpen is fully user-controlled via bind:open below.
  let isOpen = $state((() => autoOpen)());
  let formEl = $state<HTMLElement | null>(null);
  let autoFocusDone = false;

  $effect(() => {
    if (autoOpen && isOpen && !autoFocusDone) {
      autoFocusDone = true;
      // Delay to allow Collapsible animation to complete before querying DOM
      setTimeout(() => {
        const first = formEl?.querySelector<HTMLElement>('input, select, textarea');
        first?.focus();
      }, 150);
    }
  });
  const node = $derived<Node | null>(nodeId ? (sharedNodeStore.getNode(nodeId) ?? null) : null);

  const fieldStats = $derived(() => {
    let filled = 0;
    for (const field of schema.fields) {
      const value = getFieldValue(field.name);
      if (value !== null && value !== undefined && value !== '') filled++;
    }
    return { filled, total: schema.fields.length };
  });

  function getFieldValue(fieldName: string): unknown {
    if (!node) return undefined;
    return node.properties?.[fieldName] ?? null;
  }

  function updateField(fieldName: string, value: unknown) {
    if (!node) return;
    sharedNodeStore.updateNode(
      nodeId,
      { properties: { ...node.properties, [fieldName]: value } },
      { type: 'viewer', viewerId: 'generic-schema-form' }
    );
  }

  function openNestedModal(field: SchemaField) {
    nestedModalField = field;
    nestedModalOpen = true;
  }
</script>

{#if node}
  <div class="schema-form-wrapper">
    {#if schema.fields.length > 0}
    <Collapsible.Root bind:open={isOpen}>
      <Collapsible.Trigger
        class="flex w-full items-center justify-between py-3 font-medium transition-all hover:opacity-80"
      >
        <span class="text-sm font-medium"></span>
        <div class="flex items-center gap-2">
          <span class="text-sm text-muted-foreground">
            {fieldStats().filled}/{fieldStats().total} fields
          </span>
          <svg
            class="h-4 w-4 text-muted-foreground transition-transform duration-200"
            class:rotate-180={isOpen}
            viewBox="0 0 16 16"
            fill="none"
          >
            <path
              d="M4 6l4 4 4-4"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </div>
      </Collapsible.Trigger>

      <Collapsible.Content class="pb-4">
        <div class="grid grid-cols-2 gap-4" bind:this={formEl}>
          {#each schema.fields as field (field.name)}
            {@const fieldId = `generic-${nodeId}-${field.name}`}
            <div class="space-y-2">
              <label for={fieldId} class="text-sm font-medium">
                {labelForField(field)}
              </label>

              {#if isNestedField(field)}
                <NestedFieldTrigger
                  {field}
                  {fieldId}
                  value={getFieldValue(field.name)}
                  onopen={() => openNestedModal(field)}
                />
              {:else}
                <SchemaFieldLeaf
                  {field}
                  {fieldId}
                  value={getFieldValue(field.name)}
                  onChange={(newValue) => updateField(field.name, newValue)}
                />
              {/if}
            </div>
          {/each}
        </div>
      </Collapsible.Content>
    </Collapsible.Root>
    {/if}

    <!-- Relationships entry point (read-only viewer, issue #1918). Gated on the
         type actually having typed relationships (outbound declared or inbound). -->
    {#if hasRelationships}
    <button
      type="button"
      class="flex w-full items-center gap-2 py-3 text-sm font-medium text-muted-foreground transition-all hover:opacity-80"
      onclick={() => (showRelationships = true)}
    >
      <WaypointsIcon class="h-4 w-4" />
      <span>Relationships</span>
    </button>
    {/if}
  </div>

  <RelationshipViewerModal bind:open={showRelationships} {nodeId} />

  {#if nestedModalField}
    {@const nestedField = nestedModalField}
    <NestedPropertyModal
      bind:open={nestedModalOpen}
      field={nestedField}
      value={getFieldValue(nestedField.name)}
      onPersist={(newValue) => updateField(nestedField.name, newValue)}
    />
  {/if}
{/if}

<style>
  .schema-form-wrapper {
    width: calc(100% + (var(--viewer-padding-horizontal) * 2));
    margin-left: calc(-1 * var(--viewer-padding-horizontal));
    padding: 0 var(--viewer-padding-horizontal);
    border-bottom: 1px solid hsl(var(--border));
  }
</style>
