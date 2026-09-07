<!--
  EdgePropertiesModal — edit ONE relationship edge's properties.

  A typed relationship can declare `edge_fields` on its schema, and every
  relationship declares a different set, so the form here is built from the
  group's own `edgeFields` rather than from any fixed column set. Giving it a
  dialog of its own (instead of swapping table cells inline) keeps the row's
  read-only values stable, leaves room for a long field list, and scales to
  relationships that carry substantial edge data — the collection↔person RBAC
  fields being the case in view.

  Editing is scoped to this one edge: the panel behind it stays in exactly the
  state it was in. Persistence is NOT owned here — the caller supplies the
  current values plus an `onSave` that routes through the relationship service,
  so this component stays free of any Tauri/adapter dependency.
-->
<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import LoaderIcon from '@lucide/svelte/icons/loader-circle';
  import type { RawEdgeField } from '$lib/services/relationship-grouping';
  import {
    coerceNumber,
    edgeInputKind,
    edgeInputType,
    edgeInputValue,
    formatEdgeFieldLabel,
    toInputString
  } from '$lib/services/edge-field-input';

  interface Props {
    /** Heading context: the relationship's label and the row being edited. */
    relationshipLabel: string;
    rowLabel: string;
    /** The declared edge fields to render inputs for. */
    fields: RawEdgeField[];
    /** Current value per field name, layered draft-over-stored by the caller. */
    valueFor: (_fieldName: string) => unknown;
    /** Record a single field edit into the caller's draft for this row. */
    onChange: (_fieldName: string, _value: unknown) => void;
    /** Persist the draft. The caller closes this modal on success. */
    onSave: () => void;
    /** Discard the draft and close. */
    onCancel: () => void;
    busy?: boolean;
  }

  let {
    relationshipLabel,
    rowLabel,
    fields,
    valueFor,
    onChange,
    onSave,
    onCancel,
    busy = false
  }: Props = $props();
</script>

<!--
  The caller owns this dialog's lifetime: it mounts this component only while a
  row is being edited, so `open` is always true here and every dismissal path
  routes back through `onCancel` to unmount it. That keeps one source of truth
  for "which row is open" instead of a second flag that could disagree with it.
-->
<Dialog.Root
  open={true}
  onOpenChange={(next) => {
    // Dismissing by ✕/Esc/overlay must discard the draft exactly as Cancel
    // does, so a closed editor never leaves a half-edited row behind.
    if (!next) onCancel();
  }}
>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title>Edit {relationshipLabel}</Dialog.Title>
      <Dialog.Description>
        Properties of the relationship to <span class="font-medium">{rowLabel}</span>.
      </Dialog.Description>
    </Dialog.Header>

    <div class="grid max-h-[60vh] gap-3 overflow-y-auto py-1">
      {#each fields as field (field.name)}
        {@const kind = edgeInputKind(field)}
        {@const value = valueFor(field.name)}
        <div class="grid gap-1.5">
          <span class="text-muted-foreground text-xs capitalize">
            {formatEdgeFieldLabel(field.name)}
            {#if field.required}
              <span class="text-destructive">*</span>
            {/if}
          </span>
          {#if kind === 'boolean'}
            <Checkbox
              checked={Boolean(value)}
              aria-label={formatEdgeFieldLabel(field.name)}
              onCheckedChange={(v) => onChange(field.name, v === true)}
            />
          {:else if kind === 'number'}
            <Input
              type="number"
              class="h-8"
              aria-label={formatEdgeFieldLabel(field.name)}
              value={toInputString(value)}
              oninput={(e) => onChange(field.name, coerceNumber(e.currentTarget.value))}
            />
          {:else}
            <Input
              type={edgeInputType(kind)}
              class="h-8"
              aria-label={formatEdgeFieldLabel(field.name)}
              value={edgeInputValue(kind, value)}
              oninput={(e) => onChange(field.name, e.currentTarget.value)}
            />
          {/if}
          {#if field.description}
            <span class="text-muted-foreground text-xs">{field.description}</span>
          {/if}
        </div>
      {/each}
    </div>

    <Dialog.Footer>
      <Button variant="ghost" disabled={busy} onclick={onCancel}>Cancel</Button>
      <Button disabled={busy} onclick={onSave}>
        {#if busy}
          <LoaderIcon class="mr-1.5 size-4 animate-spin" />
        {/if}
        Save
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
