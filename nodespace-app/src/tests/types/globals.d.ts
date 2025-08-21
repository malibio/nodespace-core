/**
 * Global type definitions for test environment
 * Fixes "global is not defined" errors in tests
 */

declare global {
  /**
   * Node.js global object (available in test environment)
   * This allows tests to use both `global` and `globalThis` interchangeably
   */
  var global: typeof globalThis;
  
  /**
   * Additional test-specific global extensions
   */
  namespace globalThis {
    interface Window extends GlobalEventHandlers {
      IntersectionObserver: typeof IntersectionObserver;
      MutationObserver: typeof MutationObserver;
      ResizeObserver: typeof ResizeObserver;
    }
  }
}

/**
 * Additional test environment type augmentations
 */
declare namespace NodeJS {
  interface Global extends GlobalThis {
    // Allow any properties to be set on global for testing
    [key: string]: any;
  }
}

export {};