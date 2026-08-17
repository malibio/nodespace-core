<!--
  SchemaPropertyForm - Schema-Driven Property UI

  Dynamically generates form fields based on schema definitions.
  Leaf fields (enum, date, number, boolean, string/text) render through the
  shared SchemaFieldLeaf; object/array fields render a summary trigger that opens
  the shared NestedPropertyModal. The one control this form still owns is the
  assignee combobox. Uses Collapsible UI pattern with field completion tracking.

  Values are stored/read under properties[nodeType][field.name] — including
  nested values, which persist through the same updateProperty (and therefore
  the same flat→nested migration) as every other field.

  Props:
  - nodeId: ID of the node to display properties for
  - nodeType: Type of the node (used to fetch schema)

  Features:
  - Automatic field type detection and rendering
  - Field completion tracking (filled fields / total fields)
  - Collapsible section with status summary in header
  - Reactive updates to node properties
  - Schema caching for performance
-->

<script lang="ts">
  import { onMount } from 'svelte';
  import { Collapsible } from 'bits-ui';
  import * as Popover from '$lib/components/ui/popover';
  import { backendAdapter } from '$lib/services/backend-adapter';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
  import {
    type SchemaNode,
    type SchemaField,
    type EnumValue,
    isSchemaNode
  } from '$lib/types/schema-node';
  import type { Node } from '$lib/types';
  import { parseDate, type DateValue } from '@internationalized/date';
  import { createLogger } from '$lib/utils/logger';
  import { labelForField } from '$lib/utils/schema-field-label';
  import { evaluateTitleTemplate } from '$lib/utils/title-template';
  import SchemaFieldLeaf from '$lib/components/schema/schema-field-leaf.svelte';
  import NestedFieldTrigger from '$lib/components/schema/nested-field-trigger.svelte';
  import NestedPropertyModal from '$lib/components/schema/nested-property-modal.svelte';
  import { isNestedField } from '$lib/utils/nested-property-ops';

  // Logger instance for SchemaPropertyForm component
  const log = createLogger('SchemaPropertyForm');

  // Props
  let {
    nodeId,
    nodeType
  }: {
    nodeId: string;
    nodeType: string;
  } = $props();

  // State
  let schema = $state<SchemaNode | null>(null);
  let isOpen = $state(false); // Collapsed by default
  let schemaError = $state<string | null>(null);

  // Reactive node data - direct read from the store's SvelteMap
  const node = $derived(nodeId ? (sharedNodeStore.getNode(nodeId) ?? null) : null);

  // Combobox state for text fields that could have autocomplete (like assignee)
  let comboboxOpen = $state<Record<string, boolean>>({});
  let comboboxSearch = $state<Record<string, string>>({});

  // Nested (object/array) property editor. One modal instance is reused; the
  // clicked field determines what it edits. It persists through updateProperty
  // below, so nested values land under `properties[nodeType].<field>` alongside
  // every other field of this type.
  let nestedModalField = $state<SchemaField | null>(null);
  let nestedModalOpen = $state(false);

  /**
   * Assignee options - currently empty placeholder
   *
   * TODO: Populate from UserService once implemented
   * - Will integrate with user management system (planned)
   * - Should provide autocomplete for user names/emails
   * - Consider caching user list for performance
   *
   * Related: User service integration (future enhancement)
   */
  const assigneeOptions: Array<{ value: string; label: string }> = [];

  // Load schema on mount. base-node-viewer wraps this form in {#key node.id-nodeType},
  // so a nodeType change remounts it — a discrete per-type load, not a reactive watch (ADR-049).
  onMount(() => {
    async function loadSchema() {
      if (!nodeType) return;

      schemaError = null;

      try {
        const schemaNode = await backendAdapter.getSchema(nodeType);
        if (isSchemaNode(schemaNode)) {
          schema = schemaNode;
        } else {
          schemaError = `Invalid schema node for type: ${nodeType}`;
          schema = null;
        }
      } catch (error) {
        log.error('Failed to load schema:', error);
        schemaError = error instanceof Error ? error.message : 'Failed to load schema';
        schema = null;
      }
    }

    loadSchema();
  });

  /**
   * Get property value with backward compatibility
   *
   * Supports multiple formats:
   * - Strongly-typed nodes: top-level type-specific fields (e.g., task.status)
   * - New nested: properties.task.status
   * - Old flat: properties.status
   */
  function getPropertyValue(fieldName: string): unknown {
    if (!node) return undefined;

    // For strongly-typed nodes (TaskNode, etc.), check top-level fields first
    // Type-specific fields like status, priority, dueDate are at the top level
    if (
      fieldName in node &&
      (node as unknown as Record<string, unknown>)[fieldName] !== undefined
    ) {
      return (node as unknown as Record<string, unknown>)[fieldName];
    }

    // Properties are namespaced under properties[nodeType][fieldName]
    const typeNamespace = node.properties?.[nodeType];
    if (typeNamespace && typeof typeNamespace === 'object' && fieldName in typeNamespace) {
      return (typeNamespace as Record<string, unknown>)[fieldName];
    }

    // Old flat shape, not yet migrated. `updateProperty` migrates it on the first write, so the
    // value must be READ from here too — otherwise an un-migrated field renders empty and that
    // first write persists the empty edit over the real value. Harmless for a leaf (the user
    // retypes what they can see is missing); destructive for a nested object, where the keys
    // that were never displayed would be replaced wholesale.
    return node.properties?.[fieldName];
  }

  // Get schema fields directly from typed field (no helper needed)
  const schemaFields = $derived(schema ? schema.fields : []);

  // Calculate field completion stats
  const fieldStats = $derived(() => {
    if (!schema || !node) {
      return { filled: 0, total: 0 };
    }

    // Count all fields (core, user, and system)
    const allFields = schemaFields;
    const total = allFields.length;

    // Count filled fields (non-null, non-undefined, non-empty)
    const filled = allFields.filter((field) => {
      // Type guard to ensure node is not null
      if (!node) return false;
      const value = getPropertyValue(field.name);
      if (value === null || value === undefined) return false;
      if (typeof value === 'string' && value.trim() === '') return false;
      return true;
    }).length;

    return { filled, total };
  });

  // Get display value for header (e.g., status badge, due date)
  const headerSummary = $derived(() => {
    if (!schema || !node) return null;

    // Find status field (enum type, common in task schemas)
    const statusField = schemaFields.find((f) => f.name === 'status' && f.type === 'enum');
    // Use current value or default value from schema
    const statusValue = statusField
      ? getPropertyValue(statusField.name) || statusField.default || null
      : null;
    // Ensure status is a string - handle arrays incorrectly stored
    let status: string | null = null;
    let statusLabel: string | null = null;
    if (statusValue) {
      if (Array.isArray(statusValue)) {
        status = statusValue.join(''); // Fix incorrectly stored array
      } else {
        status = String(statusValue);
      }
      // Look up label from enum values
      if (statusField && status) {
        const enumValues = getEnumValues(statusField);
        const enumValue = enumValues.find((ev) => ev.value === status);
        statusLabel = enumValue?.label || formatEnumLabel(status);
      }
    }

    // Find due date field
    const dueDateField = schemaFields.find((f) => f.name === 'dueDate' || f.name === 'due_date');
    const dueDate = dueDateField ? getPropertyValue(dueDateField.name) : null;

    return { status, statusLabel, dueDate };
  });

  // Update node property
  function updateProperty(fieldName: string, value: unknown) {
    if (!node || !schema) return;

    // AUTO-MIGRATION: If this is the first write and node is still in old
    // flat format, migrate all existing properties to new nested format. This prevents
    // mixed-format properties within the same node and ensures clean data migration.
    const typeNamespace = node.properties?.[nodeType];
    const isOldFormat = !typeNamespace || typeof typeNamespace !== 'object';

    let migratedNamespace: Record<string, unknown> = {};

    if (isOldFormat) {
      // Migrate all schema fields from old flat format to new nested format
      schemaFields.forEach((field) => {
        // Type guard: node is guaranteed non-null due to early return above
        if (!node) return;
        const oldValue = node.properties?.[field.name];
        if (oldValue !== undefined) {
          migratedNamespace[field.name] = oldValue;
        }
      });
    } else {
      // Already in new format, just copy existing namespace
      migratedNamespace = { ...(typeNamespace as Record<string, unknown>) };
    }

    // Apply the update
    migratedNamespace[fieldName] = value;

    // Build updated properties with nested namespace
    const updatedProperties: Record<string, unknown> = {
      ...node.properties,
      [nodeType]: migratedNamespace
    };

    // If we migrated from old format, remove the old flat properties
    if (isOldFormat) {
      schemaFields.forEach((field) => {
        // Type guard: node is guaranteed non-null due to early return above
        if (!node) return;
        delete updatedProperties[field.name];
      });
    }

    // Optimistic title computation: if the schema has a titleTemplate, compute the
    // new title from the updated field values immediately (before backend responds).
    // Whitespace normalization mirrors Rust's interpolate_title_template (trim + collapse).
    // The backend remains authoritative and will overwrite this value on response.
    const titleUpdate: Partial<Node> = schema.titleTemplate
      ? { title: evaluateTitleTemplate(schema.titleTemplate, migratedNamespace) || null }
      : {};

    // updatedProperties is the full intended bag (the old-format branch above
    // deliberately drops flat keys), so replace rather than deep-merge.
    sharedNodeStore.updateNode(
      nodeId,
      { properties: updatedProperties, ...titleUpdate },
      { type: 'viewer', viewerId: 'schema-property-form' },
      { replaceProperties: true }
    );
  }

  // Get enum values for a field (core + user values combined)
  function getEnumValues(field: SchemaField): EnumValue[] {
    const values: EnumValue[] = [];
    if (field.coreValues) values.push(...field.coreValues);
    if (field.userValues) values.push(...field.userValues);
    return values;
  }

  // Format date for display
  function formatDateDisplay(value: unknown): string {
    if (!value) return 'Pick a date';
    if (typeof value === 'string') {
      try {
        const date = parseDate(value);
        return date.toString();
      } catch {
        return value;
      }
    }
    if (typeof value === 'object' && 'toString' in value) {
      return (value as DateValue).toString();
    }
    return String(value);
  }

  // Format enum value for display (convert snake_case to Title Case)
  // Handles lowercase values like "in_progress" → "In Progress"
  function formatEnumLabel(value: string): string {
    return value
      .split('_')
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
      .join(' ');
  }

  // Get current enum value as string (with fallback to default)
  function getEnumValue(field: SchemaField): string {
    if (!node) return field.default ? String(field.default) : '';
    const value = getPropertyValue(field.name);
    // Use current value, or fall back to schema default, or empty string
    return value ? String(value) : field.default ? String(field.default) : '';
  }

  // Value handed to SchemaFieldLeaf. Enums resolve through getEnumValue so an
  // unset field still shows the schema default; every other type passes the
  // stored value through untouched (the leaf owns its own parsing/defaults).
  function leafValue(field: SchemaField): unknown {
    return field.type === 'enum' ? getEnumValue(field) : getPropertyValue(field.name);
  }

  function openNestedModal(field: SchemaField) {
    nestedModalField = field;
    nestedModalOpen = true;
  }
</script>

{#if schemaError}
  <div class="property-form-error">
    <span class="text-sm text-destructive">Error: {schemaError}</span>
  </div>
{:else if schema && node && fieldStats().total > 0}
  <!-- Wrapper with border-b (matches demo structure) -->
  <div class="border-b">
    <Collapsible.Root bind:open={isOpen}>
      <Collapsible.Trigger
        class="flex w-full items-center justify-between py-3 font-medium transition-all hover:opacity-80"
      >
        <div class="flex items-center gap-3">
          <!-- Status Badge (if available) -->
          {#if headerSummary()?.status}
            {@const statusLabel = headerSummary()!.statusLabel!}
            <span
              class="inline-flex items-center rounded-md border border-border bg-muted px-2.5 py-0.5 text-xs font-medium text-foreground"
            >
              {statusLabel}
            </span>
          {/if}

          <!-- Due Date (always show, with "None" if not set) -->
          <span class="text-sm text-muted-foreground">
            Due: {headerSummary()?.dueDate ? formatDateDisplay(headerSummary()?.dueDate) : 'None'}
          </span>
        </div>

        <!-- Field Completion + Chevron -->
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
        <!-- Property Grid (2 columns) -->
        <div class="grid grid-cols-2 gap-4">
          {#each schemaFields as field (field.name)}
            {@const fieldId = `property-${nodeId}-${field.name}`}
            <div class="space-y-2">
              <label for={fieldId} class="text-sm font-medium">
                {labelForField(field)}
              </label>

              {#if isNestedField(field)}
                <!-- Object/array field → summary trigger opening the shared nested editor -->
                <NestedFieldTrigger
                  {field}
                  {fieldId}
                  value={getPropertyValue(field.name)}
                  onopen={() => openNestedModal(field)}
                />
              {:else if field.name === 'assignee' && (field.type === 'text' || field.type === 'string')}
                <!-- Assignee Combobox — the one leaf this form renders itself -->
                {@const currentValue = (getPropertyValue(field.name) as string) || ''}
                {@const isAssigneeOpen = comboboxOpen[field.name] || false}
                {@const searchValue = comboboxSearch[field.name] || ''}
                <Popover.Root
                  open={isAssigneeOpen}
                  onOpenChange={(open) => {
                    comboboxOpen[field.name] = open;
                  }}
                >
                  <Popover.Trigger
                    class="flex h-10 w-full items-center justify-between rounded-md border border-input bg-background px-3 py-2 text-sm focus-visible:outline-none"
                  >
                    <span class={currentValue ? '' : 'text-muted-foreground'}>
                      {currentValue || 'Select assignee...'}
                    </span>
                    <svg class="h-4 w-4 opacity-50" viewBox="0 0 16 16" fill="none">
                      <path
                        d="M4 6l4 4 4-4"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      />
                    </svg>
                  </Popover.Trigger>
                  <Popover.Content class="w-[200px] p-0" align="start">
                    <div class="flex flex-col">
                      <input
                        type="text"
                        placeholder="Search assignee..."
                        value={searchValue}
                        oninput={(e) => {
                          comboboxSearch[field.name] = e.currentTarget.value;
                        }}
                        class="flex h-10 w-full border-b border-input bg-background px-3 py-2 text-sm focus-visible:outline-none"
                      />
                      <div class="max-h-[200px] overflow-y-auto">
                        {#if assigneeOptions.length === 0}
                          <div class="px-3 py-2 text-sm text-muted-foreground">
                            No assignees available
                          </div>
                        {:else}
                          {#each assigneeOptions.filter((a) => a.label
                              .toLowerCase()
                              .includes(searchValue.toLowerCase())) as assignee}
                            <button
                              type="button"
                              class="relative flex w-full cursor-pointer select-none items-center rounded-sm px-3 py-2 text-sm outline-none hover:bg-muted"
                              onclick={() => {
                                updateProperty(field.name, assignee.value);
                                comboboxOpen[field.name] = false;
                                comboboxSearch[field.name] = '';
                              }}
                            >
                              {assignee.label}
                              {#if currentValue === assignee.value}
                                <svg class="ml-auto h-4 w-4" viewBox="0 0 16 16" fill="none">
                                  <path
                                    d="M3 8l4 4 6-8"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                  />
                                </svg>
                              {/if}
                            </button>
                          {/each}
                        {/if}
                      </div>
                    </div>
                  </Popover.Content>
                </Popover.Root>
              {:else}
                <!-- Every other leaf type (enum, date, number, boolean, string/text) -->
                <SchemaFieldLeaf
                  {field}
                  {fieldId}
                  value={leafValue(field)}
                  onChange={(newValue) => {
                    // An empty emission from the enum control is not a user choosing "none" —
                    // this form writes with replaceProperties and recomputes the title from the
                    // new value, so persisting '' would blank both. Other fields may legitimately
                    // clear to ''.
                    if (field.type === 'enum' && !newValue) return;
                    updateProperty(field.name, newValue);
                  }}
                />
              {/if}
            </div>
          {/each}
        </div>
      </Collapsible.Content>
    </Collapsible.Root>
  </div>

  {#if nestedModalField}
    {@const nestedField = nestedModalField}
    <NestedPropertyModal
      bind:open={nestedModalOpen}
      field={nestedField}
      value={getPropertyValue(nestedField.name)}
      onPersist={(newValue) => updateProperty(nestedField.name, newValue)}
    />
  {/if}
{/if}

<style>
  .property-form-error {
    padding: 1rem;
    text-align: center;
  }
</style>
