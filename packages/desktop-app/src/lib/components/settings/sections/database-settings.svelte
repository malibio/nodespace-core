<script lang="ts">
    import { settingsStore, loadSettings } from '$lib/stores/settings.svelte';
    import { invoke } from '@tauri-apps/api/core';
    import { createLogger } from '$lib/utils/logger';
    import { Button } from '$lib/components/ui/button';

    const log = createLogger('DatabaseSettings');

    let restartPending = $state(false);
</script>

<div class="max-w-[600px]">
    <h2 class="text-foreground mb-6 text-xl font-semibold">Database</h2>

    <div class="mb-6">
        <span class="text-muted-foreground mb-2 block text-sm font-medium">Active Database Path</span>
        <div class="text-foreground bg-muted rounded-[var(--radius)] break-all px-3 py-2 font-mono text-sm">
            {settingsStore.appSettings?.activeDatabasePath ?? 'Loading...'}
        </div>
    </div>

    {#if restartPending}
        <div class="border-amber-500/30 bg-amber-500/10 text-amber-700 rounded-[var(--radius)] mb-4 border px-3 py-2 text-sm">
            Restart required for the new database path to take effect.
        </div>
    {/if}

    <div class="mt-4 flex gap-3">
        <Button variant="secondary" onclick={async () => {
            try {
                const result = await invoke<{ newPath: string; success: boolean; restartRequired: boolean }>('select_new_database');
                if (result.success) {
                    await loadSettings();
                    if (result.restartRequired) {
                        restartPending = true;
                    }
                }
            } catch (err) {
                if (err !== 'No folder selected') {
                    log.error('Database selection failed:', err);
                }
            }
        }}>
            Change Location...
        </Button>

        <Button variant="outline" onclick={async () => {
            try {
                await invoke<string>('reset_database_to_default');
                await loadSettings();
                restartPending = true;
            } catch (err) {
                log.error('Failed to reset database:', err);
            }
        }}>
            Reset to Default
        </Button>
    </div>
</div>
