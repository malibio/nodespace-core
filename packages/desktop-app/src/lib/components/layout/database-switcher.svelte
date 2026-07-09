<script lang="ts">
  import { onMount } from 'svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import DatabaseNameDialog from './database-name-dialog.svelte';
  import { databaseStore, type DatabaseInfo } from '$lib/stores/database.svelte';
  import { createLogger } from '$lib/utils/logger';
  import Check from '@lucide/svelte/icons/check';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import HardDrive from '@lucide/svelte/icons/hard-drive';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Plus from '@lucide/svelte/icons/plus';
  import FolderOpen from '@lucide/svelte/icons/folder-open';

  const log = createLogger('DatabaseSwitcher');

  let newDialogOpen = $state(false);

  const databases = $derived(databaseStore.databases);
  const activeDatabase = $derived(databaseStore.activeDatabase);
  const activeName = $derived(activeDatabase?.name ?? 'Default Database');

  onMount(() => {
    databaseStore.load();
  });

  function selectDatabase(id: string) {
    databaseStore.switchTo(id);
  }

  // Defer opening a dialog until after the menu has closed so focus management
  // doesn't fight between the closing menu and the opening dialog.
  function openNewDialog() {
    setTimeout(() => (newDialogOpen = true), 0);
  }

  async function createDatabase(name: string) {
    const entry = await databaseStore.create(name);
    if (entry) {
      await databaseStore.switchTo(entry.id);
    }
  }

  async function openExisting() {
    try {
      const selected = await openDialog({
        directory: false,
        multiple: false,
        title: 'Open an existing NodeSpace database'
      });
      if (typeof selected !== 'string') return;
      const entry = await databaseStore.register(selected);
      if (entry) {
        await databaseStore.switchTo(entry.id);
      }
    } catch (err) {
      log.error('Failed to open existing database', err);
    }
  }
</script>

<div class="db-switcher">
  <DropdownMenu.Root>
    <DropdownMenu.Trigger
      class="db-trigger"
      aria-label="Switch database (current: {activeName})"
    >
      {#if activeDatabase?.status === 'missing'}
        <TriangleAlert class="db-glyph db-glyph-missing" />
      {:else}
        <HardDrive class="db-glyph" />
      {/if}
      <span class="db-name">{activeName}</span>
      <ChevronDown class="db-chevron" />
    </DropdownMenu.Trigger>

    <DropdownMenu.Content class="w-[220px]" align="start">
      {#each databases as db (db.id)}
        {@render databaseRow(db)}
      {/each}

      <DropdownMenu.Separator />

      <DropdownMenu.Item onSelect={openNewDialog}>
        <Plus />
        <span>New Database…</span>
      </DropdownMenu.Item>
      <DropdownMenu.Item onSelect={openExisting}>
        <FolderOpen />
        <span>Open Database…</span>
      </DropdownMenu.Item>
    </DropdownMenu.Content>
  </DropdownMenu.Root>
</div>

{#snippet databaseRow(db: DatabaseInfo)}
  <DropdownMenu.Item onSelect={() => selectDatabase(db.id)}>
    <span class="db-check" aria-hidden="true">
      {#if db.id === databaseStore.activeDatabaseId}
        <Check />
      {/if}
    </span>
    {#if db.status === 'missing'}
      <TriangleAlert class="db-glyph db-glyph-missing" />
    {:else}
      <HardDrive class="db-glyph" />
    {/if}
    <span class="db-row-name">{db.name}</span>
    {#if db.isDefault}
      <span class="db-default-badge">default</span>
    {/if}
  </DropdownMenu.Item>
{/snippet}

<DatabaseNameDialog
  bind:open={newDialogOpen}
  title="New Database"
  description="Create a new local database. It opens immediately once created."
  label="Name"
  confirmLabel="Create"
  placeholder="e.g. Work"
  onConfirm={createDatabase}
/>

<style>
  .db-switcher {
    margin: 0 -1rem 0.5rem;
    padding: 0 1rem;
  }

  :global(.db-trigger) {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    height: 40px;
    padding: 0 0.5rem;
    background: none;
    border: 1px solid hsl(var(--border));
    border-radius: var(--radius);
    color: hsl(var(--foreground));
    cursor: pointer;
    font-size: 0.875rem;
    font-weight: 500;
    text-align: left;
    transition:
      background-color 0.2s,
      border-color 0.2s;
  }

  :global(.db-trigger:hover) {
    background: hsl(var(--border));
  }

  :global(.db-trigger .db-glyph) {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    color: hsl(var(--muted-foreground));
  }

  :global(.db-trigger .db-glyph-missing),
  :global(.db-glyph-missing) {
    color: hsl(var(--destructive));
  }

  .db-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :global(.db-trigger .db-chevron) {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    color: hsl(var(--muted-foreground));
  }

  .db-check {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    flex-shrink: 0;
  }

  .db-row-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .db-default-badge {
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground));
    background: hsl(var(--muted));
    padding: 0.05rem 0.35rem;
    border-radius: 0.25rem;
    flex-shrink: 0;
  }
</style>
