<script lang="ts">
    import { settingsStore } from '$lib/stores/settings.svelte';
</script>

<div class="max-w-[600px]">
    <h2 class="text-foreground mb-6 text-xl font-semibold">Database</h2>

    <div class="mb-6">
        <span class="text-muted-foreground mb-2 block text-sm font-medium">Active Database Path</span>
        <div class="text-foreground bg-muted rounded-[var(--radius)] break-all px-3 py-2 font-mono text-sm">
            {settingsStore.appSettings?.activeDatabasePath ?? 'Loading...'}
        </div>
    </div>
</div>
