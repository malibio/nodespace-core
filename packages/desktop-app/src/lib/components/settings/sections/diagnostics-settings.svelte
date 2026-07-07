<script lang="ts">
    import { settingsStore } from '$lib/stores/settings.svelte';
</script>

<div class="max-w-[600px]">
    <h2 class="text-foreground mb-6 text-xl font-semibold">About NodeSpace</h2>

    <div class="flex flex-col gap-4">
        <div class="flex flex-col gap-1">
            <span class="text-muted-foreground text-xs font-medium uppercase tracking-widest">Version</span>
            <span class="text-foreground text-sm">Development Build</span>
        </div>

        <div class="flex flex-col gap-1">
            <span class="text-muted-foreground text-xs font-medium uppercase tracking-widest">Database Path</span>
            <span class="text-foreground break-all font-mono text-sm">{settingsStore.appSettings?.activeDatabasePath ?? 'Unknown'}</span>
        </div>
    </div>
</div>
