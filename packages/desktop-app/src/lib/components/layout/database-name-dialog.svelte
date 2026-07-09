<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';

  interface Props {
    open: boolean;
    title: string;
    description?: string;
    label: string;
    confirmLabel: string;
    placeholder?: string;
    initialValue?: string;
    onConfirm: (_name: string) => void;
  }

  let {
    open = $bindable(false),
    title,
    description,
    label,
    confirmLabel,
    placeholder = '',
    initialValue = '',
    onConfirm
  }: Props = $props();

  let name = $state('');
  let inputRef = $state<HTMLInputElement | null>(null);

  // Seed the field from initialValue each time the dialog opens (rename reuses
  // the same instance for different databases).
  $effect(() => {
    if (open) {
      name = initialValue;
      // Focus after the dialog mounts its content.
      setTimeout(() => inputRef?.select(), 0);
    }
  });

  const trimmed = $derived(name.trim());
  const canConfirm = $derived(trimmed.length > 0);

  function confirm() {
    if (!canConfirm) return;
    onConfirm(trimmed);
    open = false;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      confirm();
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title>{title}</Dialog.Title>
      {#if description}
        <Dialog.Description>{description}</Dialog.Description>
      {/if}
    </Dialog.Header>

    <div class="grid gap-2">
      <label class="text-muted-foreground text-sm font-medium" for="database-name-input">
        {label}
      </label>
      <Input
        id="database-name-input"
        bind:ref={inputRef}
        bind:value={name}
        {placeholder}
        onkeydown={handleKeydown}
      />
    </div>

    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
      <Button variant="default" disabled={!canConfirm} onclick={confirm}>{confirmLabel}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
