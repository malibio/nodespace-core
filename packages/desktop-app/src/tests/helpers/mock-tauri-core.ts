/**
 * Shared mocked export surface for `@tauri-apps/api/core`.
 *
 * `vi.mock('@tauri-apps/api/core', factory)` replaces the ENTIRE real module.
 * A factory that hand-lists only the bindings a given test happens to need
 * leaves every OTHER binding `undefined` for any importer reachable from
 * that test file's import graph — which fails at *import time* the moment
 * anything in the graph imports the missing binding. This file exists
 * because `daemon-status.ts` importing `isTauri` went silently red for five
 * nightly runs when 34 of 35 mock factories only listed `invoke`.
 *
 * This helper closes the omission class entirely rather than just
 * centralizing it: it starts from `vi.importActual`'s real export object —
 * so it never goes stale as `@tauri-apps/api/core` gains bindings this file
 * doesn't know about — and only replaces `invoke`/`isTauri` with safe,
 * deterministic defaults (the real `invoke` reaches for
 * `window.__TAURI_INTERNALS__`, absent in every test tier, so calling it
 * unmocked throws; the real `isTauri` happens to already return `false` in
 * every test tier, but is pinned here as an explicit, controllable default
 * rather than relying on that incidentally).
 *
 * Measured: a `vi.mock` factory returning a Promise — which is
 * what an `async` helper like this produces — is supported by Vitest under
 * `bun run test` (Happy-DOM) and `bun run test:browser` (real Chromium via
 * Playwright); `bun run test:webkit` could not be exercised locally in this
 * environment (Playwright's WebKit launch itself times out here regardless
 * of this change — see the tier's own "NIGHTLY and NON-BLOCKING, CI-only"
 * note in vitest.webkit.config.ts).
 *
 * CAVEAT: only `invoke`/`isTauri` get a safe override here — every other
 * export is the REAL implementation from `vi.importActual`. A few of those
 * (`checkPermissions`, `requestPermissions`, `addPluginListener`,
 * `PluginListener.unregister()`, `Resource.close()`) call `invoke` through a
 * closure-private reference inside the real module, not through the exported
 * `invoke` binding this file overrides — so overriding `invoke` here does
 * NOT redirect them. Nothing in `src/lib` imports those today, so this is
 * currently inert, and if it ever does fire it fails loudly (a thrown
 * exception from the missing `window.__TAURI_INTERNALS__`), not silently
 * like the bug this file exists to close. If a test starts exercising one of
 * those, mock it directly via an override rather than assuming this helper
 * covers it.
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

export async function mockTauriCore(overrides: Record<string, unknown> = {}) {
  const actual = await vi.importActual<typeof import('@tauri-apps/api/core')>(
    '@tauri-apps/api/core'
  );
  return {
    ...actual,
    invoke: vi.fn(),
    isTauri: vi.fn(() => false),
    ...overrides
  };
}
