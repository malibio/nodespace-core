<!--
  PersonSchemaForm - Property form for person nodes

  Provides direct editing of name and email fields stored in
  properties.person.{name,email}. Name is also synced to node content
  so it displays inline.
-->

<script lang="ts">
  import { Input } from '$lib/components/ui/input';
  import { backendAdapter } from '$lib/services/backend-adapter';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('PersonSchemaForm');

  let { nodeId }: { nodeId: string } = $props();

  const node = $derived(sharedNodeStore.getNode(nodeId));
  const personProps = $derived(
    (node?.properties?.['person'] as Record<string, unknown> | undefined) ?? {}
  );

  const name = $derived((personProps['name'] as string | undefined) ?? '');
  const email = $derived((personProps['email'] as string | undefined) ?? '');

  async function updateField(field: 'name' | 'email', value: string) {
    if (!node) return;
    try {
      const updatedProperties = {
        ...node.properties,
        person: { ...personProps, [field]: value }
      };
      // Sync name to node content so it renders inline
      const updatedContent = field === 'name' ? value : node.content;
      await backendAdapter.updateNode(nodeId, node.version, {
        content: updatedContent,
        properties: updatedProperties
      });
    } catch (err) {
      log.error('Failed to update person field', { field, err });
    }
  }

  function handleNameBlur(e: FocusEvent) {
    const value = (e.currentTarget as HTMLInputElement).value;
    if (value !== name) updateField('name', value);
  }

  function handleEmailBlur(e: FocusEvent) {
    const value = (e.currentTarget as HTMLInputElement).value;
    if (value !== email) updateField('email', value);
  }
</script>

<div class="person-schema-form">
  <div class="field">
    <label for="person-name">Name</label>
    <Input
      id="person-name"
      type="text"
      value={name}
      placeholder="Display name"
      onblur={handleNameBlur}
    />
  </div>
  <div class="field">
    <label for="person-email">Email</label>
    <Input
      id="person-email"
      type="email"
      value={email}
      placeholder="email@example.com"
      onblur={handleEmailBlur}
    />
  </div>
</div>

<style>
  .person-schema-form {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 0.5rem 0;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  label {
    font-size: 0.75rem;
    color: hsl(var(--muted-foreground));
    font-weight: 500;
  }
</style>
