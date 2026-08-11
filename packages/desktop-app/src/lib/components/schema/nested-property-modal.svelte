<!--
  NestedPropertyModal — edit a single nested (object/array) schema property.

  Opens a dialog for one `field` and renders the recursive NestedFieldEditor over
  the `value` the caller supplies. On every edit it hands the whole rebuilt value
  back through `onPersist`.

  Persistence is deliberately NOT owned here: the property forms that open this
  modal each store values under a different namespace — flat `properties[field]`
  for the generic schema form, `properties.task[field]` for the task form,
  `properties[nodeType][field]` for the schema property form — so each caller
  supplies both the current value and the write. That keeps one modal (and one
  editor) shared by every form.

  Mirrors RelationshipViewerModal's dialog + `open = $bindable()` idiom.
-->
<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import NestedFieldEditor from './nested-field-editor.svelte';
  import type { SchemaField } from '$lib/types/schema-node';

  interface Props {
    open: boolean;
    field: SchemaField;
    value: unknown;
    onPersist: (_value: unknown) => void;
  }

  let { open = $bindable(false), field, value, onPersist }: Props = $props();

  function formatFieldLabel(fieldName: string): string {
    return fieldName
      .replace(/[_-]/g, ' ')
      .split(' ')
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
      .join(' ');
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
      <!-- Remount per field so the editor's per-child expand/collapse state
           doesn't bleed across fields in the single reused modal instance. -->
      {#key field.name}
        <NestedFieldEditor {field} {value} onChange={onPersist} />
      {/key}
    </div>

    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (open = false)}>Close</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
