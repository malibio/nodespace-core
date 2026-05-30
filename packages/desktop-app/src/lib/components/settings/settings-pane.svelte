<script lang="ts">
    import { onMount, onDestroy } from 'svelte';
    import { loadSettings } from '$lib/stores/settings';
    import SettingsSidebar from './settings-sidebar.svelte';
    import DatabaseSettings from './sections/database-settings.svelte';
    import DisplaySettings from './sections/display-settings.svelte';
    import ImportSettings from './sections/import-settings.svelte';
    import DiagnosticsSettings from './sections/diagnostics-settings.svelte';
    import ModelManager from './model-manager.svelte';
    import IntegrationsSettings from './sections/integrations-settings.svelte';

    let activeCategory = $state('database');
    let unlistenNavigate: (() => void) | null = null;

    onMount(async () => {
        loadSettings();

        if (
            typeof window !== 'undefined' &&
            (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
        ) {
            const { listen } = await import('@tauri-apps/api/event');
            unlistenNavigate = await listen<string>('settings-navigate-to', (event) => {
                activeCategory = event.payload;
            });
        }
    });

    onDestroy(() => {
        unlistenNavigate?.();
    });
</script>

<div class="settings-container">
    <SettingsSidebar {activeCategory} onCategoryChange={(cat) => activeCategory = cat} />
    <div class="settings-content">
        {#if activeCategory === 'database'}
            <DatabaseSettings />
        {:else if activeCategory === 'display'}
            <DisplaySettings />
        {:else if activeCategory === 'import'}
            <ImportSettings />
        {:else if activeCategory === 'ai-models'}
            <ModelManager />
        {:else if activeCategory === 'integrations'}
            <IntegrationsSettings />
        {:else if activeCategory === 'about'}
            <DiagnosticsSettings />
        {/if}
    </div>
</div>

<style>
    .settings-container {
        display: flex;
        height: 100%;
        background: hsl(var(--background));
    }

    .settings-content {
        flex: 1;
        padding: 2rem;
        overflow-y: auto;
    }
</style>
