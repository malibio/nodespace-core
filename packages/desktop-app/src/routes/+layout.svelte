<script lang="ts">
  import { onMount } from 'svelte';
  import '../app.css';
  import '$lib/styles/noderef.css';
  import AppShell from '$lib/components/layout/app-shell.svelte';
  import DiagnosticPanel from '$lib/design/components/diagnostic-panel.svelte';
  import DeleteConfirmationModal from '$lib/components/delete-confirmation-modal.svelte';
  import { initializeSchemaPluginSystem } from '$lib/plugins/schema-plugin-loader';
  import { initializeTauriSyncListeners } from '$lib/services/tauri-sync-listener';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
  import { initializeApp } from '$lib/services/app-initialization';
  import { isTauri } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';

  let isInitialized = false;
  let initError: string | null = null;

  /** Wait for daemon-status: healthy or not_running (max 30s). */
  async function waitForDaemon(): Promise<void> {
    if (!isTauri()) return;
    await new Promise<void>(async (resolve) => {
      const unlisten = await listen<string>('daemon-status', (event) => {
        if (event.payload === 'healthy' || event.payload === 'not_running') {
          clearTimeout(timer);
          unlisten();
          resolve();
        }
      });
      const timer = setTimeout(() => {
        unlisten();
        resolve();
      }, 30_000);
    });
  }

  // Initialize database first, then schema plugins, then sync listeners
  onMount(async () => {
    try {
      // Step 1: Initialize database and Tauri services
      await initializeApp();

      // Step 2: Wait for the daemon to be ready before issuing any gRPC calls.
      // daemon_setup emits 'daemon-status: starting' immediately, then 'healthy'
      // or 'not_running' once ensure_daemon_running completes.
      await waitForDaemon();

      // Step 3: Initialize schema plugin auto-registration system
      // This must happen after database is ready
      try {
        const result = await initializeSchemaPluginSystem();

        if (!result.success) {
          console.warn(
            `[App Layout] Custom entities unavailable: ${result.error}. ` +
              `Core functionality will work normally.`
          );
          // Don't block app startup on plugin registration failure
          // Custom entities will be unavailable but app remains functional
        } else {
          console.log(
            `[App Layout] Custom entity system ready (${result.registeredCount} entities loaded)`
          );
        }
      } catch (error) {
        console.error('[App Layout] Schema plugin initialization failed:', error);
      }

      // Step 4: Initialize Tauri domain event listeners for real-time synchronization
      try {
        await initializeTauriSyncListeners();
      } catch (error) {
        console.warn('[App Layout] Tauri sync listeners failed to initialize:', error);
        // Don't block app startup if sync listeners fail
        // App will continue to work, just without real-time updates
      }

      // Mark as initialized - this will trigger the app to render
      isInitialized = true;
      console.log('[App Layout] Initialization complete, rendering app');
    } catch (error) {
      console.error('[App Layout] Critical initialization error:', error);
      // Store error for display on screen
      initError = error instanceof Error ? error.message : String(error);
    }
  });

  // Flush pending saves on window close to prevent data loss
  onMount(() => {
    const handleBeforeUnload = (event: BeforeUnloadEvent) => {
      // Check if there are pending writes
      if (sharedNodeStore.hasPendingWrites()) {
        // Try to flush pending operations
        // Note: beforeunload has limited async support, but we try our best
        event.preventDefault();
        // returnValue assignment is required for Chrome
        event.returnValue = '';

        // Flush pending operations synchronously if possible
        // Note: async operations may not complete in beforeunload, but we try
        sharedNodeStore.flushAllPending();
      }
    };

    window.addEventListener('beforeunload', handleBeforeUnload);

    return () => {
      window.removeEventListener('beforeunload', handleBeforeUnload);
    };
  });
</script>

{#if isInitialized}
  <AppShell>
    <slot />
  </AppShell>
{:else if initError}
  <div class="initialization-error">
    <h1>Initialization Failed</h1>
    <p class="error-message">{initError}</p>
    <p class="hint">Please report this issue with the error message above.</p>
    <p class="hint">Press Ctrl+Shift+D to open the diagnostic panel for more details.</p>
  </div>
{:else}
  <div class="initialization-screen">
    <p>Initializing NodeSpace...</p>
  </div>
{/if}

<!-- Diagnostic Panel - Press Ctrl+Shift+D to toggle -->
<DiagnosticPanel />

<!-- Delete confirmation modal - mounted globally so services can trigger it -->
<DeleteConfirmationModal />

<style>
  .initialization-error {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100vh;
    background: #1a1a1a;
    color: #fff;
    padding: 2rem;
    text-align: center;
  }

  .initialization-error h1 {
    color: #f85149;
    margin-bottom: 1rem;
  }

  .initialization-error .error-message {
    background: #2d1f1f;
    border: 1px solid #f85149;
    border-radius: 8px;
    padding: 1rem 2rem;
    font-family: monospace;
    font-size: 14px;
    max-width: 80%;
    word-break: break-word;
    margin-bottom: 1.5rem;
  }

  .initialization-error .hint {
    color: #888;
    font-size: 14px;
    margin: 0.5rem 0;
  }
</style>
