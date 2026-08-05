/**
 * Vitest environment type definitions
 * Provides proper typing for test globals and utilities
 */

/// <reference types="vitest/globals" />
/// <reference types="@testing-library/jest-dom" />
/// <reference types="node" />

import type { TestingLibraryMatchers } from '@testing-library/jest-dom/matchers';

declare global {
  namespace Vi {
    interface JestAssertion<T = unknown> extends TestingLibraryMatchers<T, void> {
      // Vitest assertions interface - add specific method signatures as needed
      toBe: (expected: T) => void;
      toEqual: (expected: T) => void;
      toContain: (expected: unknown) => void;
    }
  }

  /**
   * Ensure the global object is available in test environment
   * In Node.js/Vitest, this should be equivalent to globalThis
   */
  var global: typeof globalThis;

  /**
   * Node.js process object available in test environment
   */
  var process: {
    memoryUsage?: () => {
      heapUsed: number;
      heapTotal: number;
      external: number;
      arrayBuffers: number;
    };
    [key: string]: unknown;
  };
}

/**
 * Values published by `global-setup.ts` via `provide()` and read in tests with
 * `inject()`. Declaring them here is what makes both ends type-safe — without
 * it the key is inferred as `never` and neither call compiles.
 */
declare module 'vitest' {
  interface ProvidedContext {
    /**
     * True when this run is collecting coverage. Tests asserting on elapsed
     * time use it to select a budget matching how the code is executing:
     * V8 instrumentation adds substantial per-call overhead, so a bound
     * calibrated on an uninstrumented run is not valid for an instrumented one.
     */
    coverageEnabled: boolean;
  }
}

export {};
