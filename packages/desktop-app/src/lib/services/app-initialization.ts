/**
 * App Initialization - Runs when the app mounts
 *
 * Initializes critical backend services that must be ready before
 * any components try to use Tauri commands.
 *
 * Also handles graceful shutdown to flush pending data persistence
 * before the app closes (Issue: nodes not persisting on other machines).
 */

import { createLogger } from '$lib/utils/logger';
import { sharedNodeStore } from './shared-node-store.svelte';

const log = createLogger('AppInit');

// Tauri API types
interface TauriCore {
  invoke: (command: string, ...args: unknown[]) => Promise<unknown>;
}

interface TauriAPI {
  core?: TauriCore;
  invoke?: (command: string, ...args: unknown[]) => Promise<unknown>;
}

interface WindowWithTauri extends Window {
  __TAURI__?: TauriAPI;
}

declare const window: WindowWithTauri;

let initialized = false;

/**
 * Check if running in Tauri desktop environment
 */
function isTauriEnvironment(): boolean {
  return (
    typeof window !== 'undefined' &&
    ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
  );
}

/**
 * Wait for Tauri API to be available
 *
 * Tauri injects the API asynchronously after the webview loads.
 * We need to wait for window.__TAURI__.core.invoke to be available.
 */
async function waitForTauriReady(): Promise<void> {
  const maxAttempts = 200; // 10 seconds with 50ms delays
  let attempts = 0;
  const delayMs = 50;

  while (attempts < maxAttempts) {
    // Tauri 2.x uses __TAURI__.core.invoke
    if (
      typeof window !== 'undefined' &&
      typeof window.__TAURI__ !== 'undefined' &&
      typeof window.__TAURI__.core?.invoke === 'function'
    ) {
      log.debug(`Tauri API ready after ${attempts * delayMs}ms`);
      return;
    }

    // Fallback for older Tauri versions that use __TAURI__.invoke
    if (
      typeof window !== 'undefined' &&
      typeof window.__TAURI__?.invoke === 'function'
    ) {
      log.debug(`Tauri API ready (legacy) after ${attempts * delayMs}ms`);
      return;
    }

    await new Promise((resolve) => setTimeout(resolve, delayMs));
    attempts++;
  }

  // More detailed error message for debugging
  const isWindow = typeof window !== 'undefined';
  const hasTauri = isWindow && typeof window.__TAURI__ !== 'undefined';
  const hasInvokeCore = hasTauri && typeof window.__TAURI__?.core?.invoke === 'function';
  const hasInvokeLegacy = hasTauri && typeof window.__TAURI__?.invoke === 'function';

  log.error('Tauri API check results:', {
    isWindow,
    hasTauri,
    hasInvokeCore,
    hasInvokeLegacy,
    tauriKeys: hasTauri && window.__TAURI__ ? Object.keys(window.__TAURI__) : 'N/A'
  });

  throw new Error(
    `Tauri API did not become available after ${maxAttempts * delayMs}ms. ` +
    `isWindow=${isWindow}, hasTauri=${hasTauri}, hasInvokeCore=${hasInvokeCore}, hasInvokeLegacy=${hasInvokeLegacy}`
  );
}

/**
 * Initialize app services asynchronously on app mount
 *
 * This runs in the first onMount hook before schema plugins or sync listeners initialize.
 * It ensures the database and all Tauri services are initialized before
 * any components try to call Tauri commands.
 */
export async function initializeApp(): Promise<void> {
  // Only initialize once
  if (initialized) {
    return;
  }
  initialized = true;

  // Skip Tauri initialization in browser mode (using HTTP dev-proxy)
  if (!isTauriEnvironment()) {
    log.debug('Running in browser mode, skipping Tauri initialization');
    // Still register shutdown handlers for browser mode
    // This ensures pending writes are flushed in dev mode too
    registerShutdownHandlers();
    return;
  }

  try {
    // Wait for Tauri API to be available
    await waitForTauriReady();

    // Database and all services are initialized by nodespaced at Tauri setup time.
    // No explicit initialize_database call needed here.
    log.debug('Tauri ready — services initialized at startup');

    // Register shutdown handlers to flush pending data on close
    registerShutdownHandlers();
  } catch (error: unknown) {
    log.error('Critical initialization error:', error);
    throw error;
  }
}

// Track whether shutdown handlers have been registered
let shutdownHandlersRegistered = false;

/**
 * Register shutdown handlers to flush pending data before app closes.
 *
 * CRITICAL: This prevents data loss when the app is closed before
 * debounced persistence operations complete (500ms debounce window).
 *
 * This is called during app initialization to register:
 * 1. Browser beforeunload event (for web/dev mode)
 * 2. Tauri window close event (for desktop mode)
 */
export function registerShutdownHandlers(): void {
  if (shutdownHandlersRegistered) {
    return;
  }
  shutdownHandlersRegistered = true;

  // Browser beforeunload handler (works in all modes)
  if (typeof window !== 'undefined') {
    window.addEventListener('beforeunload', async (event) => {
      log.debug('Window closing - flushing pending operations...');

      // Check if we have pending writes
      if (sharedNodeStore.hasPendingWrites()) {
        log.info('Flushing pending node writes before close');

        // Note: beforeunload is sync, but we start the flush anyway.
        // For Tauri apps, the close event handler below handles async flushing.
        // For browser mode, this provides best-effort flushing.
        sharedNodeStore.flushAllPending().catch((err) => {
          log.error('Error flushing pending writes:', err);
        });

        // In browser mode, show confirmation dialog to give flush time to complete
        if (!isTauriEnvironment()) {
          event.preventDefault();
          // Modern browsers require returnValue to be set
          event.returnValue = 'You have unsaved changes. Are you sure you want to leave?';
          return event.returnValue;
        }
      }
    });

    log.debug('Registered beforeunload shutdown handler');
  }

  // Tauri window close handler (for desktop app)
  if (isTauriEnvironment()) {
    registerTauriCloseHandler();
  }
}

/**
 * Register Tauri-specific window close handler
 *
 * Uses Tauri 2.x event API to intercept window close and flush
 * pending operations before allowing the window to actually close.
 */
async function registerTauriCloseHandler(): Promise<void> {
  try {
    // Dynamic import to avoid issues in non-Tauri environments
    const { getCurrentWindow } = await import('@tauri-apps/api/window');

    const currentWindow = getCurrentWindow();

    // Listen for close request
    await currentWindow.onCloseRequested(async (event) => {
      log.debug('Tauri window close requested - flushing pending operations...');

      // Check if we have pending writes
      if (sharedNodeStore.hasPendingWrites()) {
        log.info('Flushing pending node writes before Tauri window close');

        // Prevent the window from closing until flush completes
        event.preventDefault();

        try {
          // Flush all pending writes
          await sharedNodeStore.flushAllPending();
          log.info('Pending writes flushed successfully');
        } catch (err) {
          log.error('Error flushing pending writes:', err);
        }

        // Now allow the window to close
        await currentWindow.destroy();
      }
    });

    log.debug('Registered Tauri close request handler');
  } catch (err) {
    log.warn('Failed to register Tauri close handler:', err);
  }
}
