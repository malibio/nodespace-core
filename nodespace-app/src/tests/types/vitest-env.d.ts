/**
 * Vitest environment type definitions
 * Provides proper typing for test globals and utilities
 */

import type { TestingLibraryMatchers } from '@testing-library/jest-dom/matchers';

declare global {
  namespace Vi {
    interface JestAssertion<T = any> extends jest.Matchers<void, T>, TestingLibraryMatchers<T, void> {}
  }
  
  /**
   * Ensure the global object is available in test environment
   * In Node.js/Vitest, this should be equivalent to globalThis
   */
  var global: typeof globalThis;
}

/**
 * Node.js process global for test environment
 */
declare const process: NodeJS.Process;

export {};