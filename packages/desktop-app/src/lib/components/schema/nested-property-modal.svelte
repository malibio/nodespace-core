<!--
  NestedPropertyModal — edit a single nested (object/array) schema property.

  Opens a dialog for one `field` of a node, reads the current top-level value from
  the shared store (`node.properties[field.name]`), and renders the recursive
  NestedFieldEditor. On every edit it persists the whole rebuilt value back through
  `sharedNodeStore.updateNode` — the same persistence path GenericSchemaForm's
  `updateField` uses (full properties bag spread with the one changed top-level
  field). The editor stays pure; this component is the only place that touches the
  store.

  Mirrors RelationshipViewerModal's dialog + `open = $bindable()` idiom.
-->
<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import NestedFieldEditor from './nested-field-editor.svelte';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
  import type { SchemaField } from '$lib/types/schema-node';
  import type { Node } from '$lib/types';

  interface Props {
    open: boolean;
    nodeId: string;
    field: SchemaField;
  }

  let { open = $bindable(false), nodeId, field }: Props = $props();

  const node = $derived<Node | null>(nodeId ? (sharedNodeStore.getNode(nodeId) ?? null) : null);

  const value = $derived(node?.properties?.[field.name]);

  function formatFieldLabel(fieldName: string): string {
    return fieldName
      .replace(/[_-]/g, ' ')
      .split(' ')
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
      .join(' ');
  }

  function persist(newValue: unknown) {
    if (!node) return;
    sharedNodeStore.updateNode(
      nodeId,
      { properties: { ...node.properties, [field.name]: newValue } },
      { type: 'viewer', viewerId: 'nested-property-modal' }
    );
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-2xl">
    <Dialog.Header>
      <Dialog.Title>{field.description || formatFieldLabel(field.name)}</Dialog.Title>
      <Dialog.Description>
        {field.type === 'array' ? 'Edit the items in this list.' : 'Edit the fields of this object.'}
      </Dialog.Description>
    </Dialog.Header>

    <div class="max-h-[60vh] overflow-y-auto py-1">
      {#if node}
        <NestedFieldEditor {field} {value} onChange={persist} />
      {:else}
        <div class="text-muted-foreground py-6 text-center text-sm">Node not found.</div>
      {/if}
    </div>

    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (open = false)}>Close</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
