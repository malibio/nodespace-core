/**
 * App Initialization - Runs when the app mounts
 *
 * Initializes critical backend services that must be ready before
 * any components try to use Tauri commands.
 */

let initialized = false;

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
      typeof (window as any).__TAURI__ !== 'undefined' &&
      typeof (window as any).__TAURI__.core?.invoke === 'function'
    ) {
      console.log('[App Init] Tauri API ready after', attempts * delayMs, 'ms');
      return;
    }

    // Fallback for older Tauri versions that use __TAURI__.invoke
    if (
      typeof window !== 'undefined' &&
      typeof (window as any).__TAURI__?.invoke === 'function'
    ) {
      console.log('[App Init] Tauri API ready (legacy) after', attempts * delayMs, 'ms');
      return;
    }

    await new Promise((resolve) => setTimeout(resolve, delayMs));
    attempts++;
  }

  // More detailed error message for debugging
  const isWindow = typeof window !== 'undefined';
  const hasTauri = isWindow && typeof (window as any).__TAURI__ !== 'undefined';
  const hasInvokeCore = hasTauri && typeof (window as any).__TAURI__.core?.invoke === 'function';
  const hasInvokeLegacy = hasTauri && typeof (window as any).__TAURI__.invoke === 'function';

  console.error('[App Init] Tauri API check results:', {
    isWindow,
    hasTauri,
    hasInvokeCore,
    hasInvokeLegacy,
    tauriKeys: hasTauri ? Object.keys((window as any).__TAURI__) : 'N/A'
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

  try {
    // Wait for Tauri API to be available
    await waitForTauriReady();

    // Initialize database
    // This ensures SchemaService and other services are available to Tauri commands
    try {
      // Tauri 2.x uses window.__TAURI__.core.invoke
      const invoke = (window as any).__TAURI__.core?.invoke || (window as any).__TAURI__.invoke;
      await invoke('initialize_database');
      console.log('[App Init] Database initialized');
    } catch (error: any) {
      // Check if already initialized (this is expected on subsequent calls)
      const errorMsg = error?.toString?.() || String(error);
      if (errorMsg.includes('already initialized')) {
        console.log('[App Init] Database already initialized');
      } else {
        console.warn('[App Init] Database initialization warning:', error);
      }
    }
  } catch (error: any) {
    console.error('[App Init] Critical initialization error:', error);
    throw error;
  }
}
