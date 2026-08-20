/**
 * Shared mocked export surface for `@tauri-apps/api/core`.
 *
 * `vi.mock('@tauri-apps/api/core', factory)` replaces the ENTIRE real module.
 * Any binding the factory omits becomes `undefined` for every importer
 * reachable from that test file's import graph — which fails at *import
 * time* the moment anything in the graph imports the missing binding (see
 * core#2170, and core#2165 for the incident that motivated this file).
 *
 * Declaring the full mocked surface here, once, means a new binding added to
 * the real `@tauri-apps/api/core` module only needs to be added to this
 * helper — not audited across every test file that mocks the module.
 *
 * Usage:
 * ```ts
 * import { mockTauriCore } from '../helpers/mock-tauri-core';
 *
 * const mockInvoke = vi.fn();
 * vi.mock('@tauri-apps/api/core', () =>
 *   mockTauriCore({ invoke: (...args: unknown[]) => mockInvoke(...args) })
 * );
 * ```
 *
 * `vi.mock` factories are hoisted above the rest of the file, so this helper
 * cannot close over per-test state passed in from outside — callers pass
 * their own `vi.fn()`-backed overrides instead. Referencing a same-file
 * `const` prefixed with `mock` (e.g. `mockInvoke` above) still works inside
 * the factory: Vitest's hoisting transform special-cases `mock`-prefixed
 * identifiers and hoists their declarations alongside the `vi.mock` call.
 */
import { vi } from 'vitest';

export function mockTauriCore(overrides: Record<string, unknown> = {}) {
  return {
    invoke: vi.fn(),
    isTauri: vi.fn(() => false),
    ...overrides
  };
}
