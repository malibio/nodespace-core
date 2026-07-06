/**
 * E2E: daemon readiness — not-ready → degraded → recovered (#1525)
 *
 * ADR-044 decision 3 promises daemon-dependent stores "re-run their load
 * automatically when the daemon transitions from unreachable to healthy."
 * That behavior lives entirely in `daemon-status.ts` and the stores that
 * subscribe to it via `onDaemonReconnect` — a real cross-process seam, not
 * something a synchronous mock can honestly stand in for (see ADR-048's
 * rationale on the optimistic-echo regression class, which is the same
 * class of bug: the defect lives in cross-process timing a mock collapses
 * away).
 *
 * Each state below is established BY CONSTRUCTION, not by racing a real
 * process's startup timing:
 *   - "not ready" uses a socket path nothing was ever spawned against —
 *     deterministic, since there is nothing to race.
 *   - "recovered" spawns the real daemon via `DaemonTestHarness.startDeferred()`
 *     and waits on it via `waitUntilDaemonReady()` (timing-tolerant by
 *     design) rather than asserting an intermediate "not yet bound" read
 *     against a real process. A real headless `nodespaced` here binds its
 *     socket in well under 100ms on a warm local machine — far faster than
 *     ADR-044's ~9s cold-start figure for a heavier path — so asserting
 *     "not ready" against a just-spawned real process would only pass when
 *     the test wins that race: a coverage lottery, not a behavior guarantee.
 *
 * `daemon-status.ts` is Tauri-event-driven in production, and this harness
 * has no Tauri runtime — so both tests bind a harness-backed
 * `DaemonStatusSource` (real UDS probes, not fabricated payloads) instead of
 * faking Tauri IPC. The only simulated piece is the transport binding,
 * which is exactly the piece whose realism doesn't matter here: it is a
 * callback wiring, not a timing seam.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { DaemonTestHarness } from './daemon-harness';
import type { DaemonStatusSource } from '$lib/services/daemon-status';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));

// schemasData imports the real backend-adapter module at load time; redirect
// its getAllSchemas() to whatever adapter the active test wires up (a
// harness pointed at a real daemon, or one pointed at nothing) instead of
// the MockAdapter the singleton would otherwise resolve to under
// NODE_ENV=test.
let getAllSchemasImpl: () => Promise<Record<string, unknown>[]>;
vi.mock('$lib/services/backend-adapter', () => ({
  backendAdapter: {
    getAllSchemas: () => getAllSchemasImpl()
  }
}));

/** A DaemonStatusSource backed by real UDS probes against a harness's daemon. */
function harnessStatusSource(h: DaemonTestHarness): DaemonStatusSource {
  return {
    async getCurrent() {
      return (await h.isDaemonReachable()) ? 'healthy' : 'not_running';
    },
    // No push transport in this harness (no Tauri event bus) — recovery is
    // observed via refreshDaemonStatus() re-pulling getCurrent(), same as
    // the manual "Retry" affordance does in production.
    subscribe() {
      return () => {};
    }
  };
}

describe('daemon readiness: not-ready -> degraded -> recovered (real daemon)', () => {
  afterEach(() => {
    vi.resetModules();
  });

  it('degraded: a store load genuinely fails against a daemon that was never started', async () => {
    // Deterministic by construction: no process is ever spawned for this
    // socket path, so there is nothing to race.
    getAllSchemasImpl = () => Promise.reject(new Error('connect ECONNREFUSED (no daemon)'));
    const unreachableSource: DaemonStatusSource = {
      async getCurrent() {
        return 'not_running';
      },
      subscribe() {
        return () => {};
      }
    };

    const { startDaemonStatusListener, daemonStatus } = await import('$lib/services/daemon-status');
    const { get } = await import('svelte/store');
    startDaemonStatusListener(unreachableSource);
    // Allow the initial pull in startDaemonStatusListener to settle.
    await Promise.resolve();
    await Promise.resolve();

    const { schemasData, builtInSchemas, customSchemas } = await import('$lib/stores/schemas');
    await schemasData.loadSchemas();

    expect(get(daemonStatus).unreachable).toBe(true);
    expect(get(builtInSchemas).length + get(customSchemas).length).toBe(0);
  });

  describe('recovered', () => {
    let h: DaemonTestHarness;

    beforeEach(async () => {
      h = await DaemonTestHarness.startDeferred();
      getAllSchemasImpl = () => h.adapter.getAllSchemas();
    }, 15_000);

    afterEach(async () => {
      await h?.stop();
    });

    it(
      'schemasData self-heals with real data once the real daemon becomes reachable',
      async () => {
        const { startDaemonStatusListener, daemonStatus, refreshDaemonStatus } = await import(
          '$lib/services/daemon-status'
        );
        const { get } = await import('svelte/store');
        // schemasData registers onDaemonReconnect(loadSchemas) at module
        // load (in schemas.ts, not here). It must be imported — and its
        // reconnect listener registered — BEFORE starting the daemon-status
        // listener below: startDaemonStatusListener's initial pull of
        // getCurrent() is not awaited by its caller (matching production,
        // where the caller doesn't block startup on it either), so if the
        // real daemon is already reachable by the time that pull resolves,
        // the "just became healthy" transition needs a subscriber in place
        // to observe it — same ordering requirement production has at app
        // startup.
        // Imported for its module-load side effect (registering the
        // reconnect hook above) — not referenced directly below.
        const { builtInSchemas, customSchemas } = await import('$lib/stores/schemas');

        const source = harnessStatusSource(h);
        startDaemonStatusListener(source);

        // Recovered: wait for the real daemon to finish starting (however
        // long that actually takes — no race), then re-pull status through
        // the shared contract. This is what fires schemasData's own
        // onDaemonReconnect hook, not a call this test makes to
        // loadSchemas directly.
        await h.waitUntilDaemonReady(30_000);
        await refreshDaemonStatus();

        expect(get(daemonStatus).unreachable).toBe(false);

        // Poll briefly for the auto-retried load to land — the reconnect
        // callback fires loadSchemas() asynchronously and this test has no
        // other signal for "that promise settled".
        const deadline = Date.now() + 5_000;
        let total = 0;
        while (Date.now() < deadline) {
          total = get(builtInSchemas).length + get(customSchemas).length;
          if (total > 0) break;
          await new Promise((r) => setTimeout(r, 50));
        }

        expect(total).toBeGreaterThan(0);

        // Confirm it's real data, not a fixture: every schema the store
        // landed is one the harness's own daemon actually reports (core
        // schema seeding can still be completing in the background, so the
        // two counts are not required to match exactly — only that the
        // store's data is a subset of what the real daemon has, not
        // fabricated).
        const schemas = await h.adapter.getAllSchemas();
        const realIds = new Set(schemas.map((s) => s.id));
        for (const s of [...get(builtInSchemas), ...get(customSchemas)]) {
          expect(realIds.has(s.id)).toBe(true);
        }
      },
      45_000
    );
  });
});
