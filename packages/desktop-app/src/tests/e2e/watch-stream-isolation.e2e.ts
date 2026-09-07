/**
 * E2E: a long-lived WatchNodes stream must not wedge unary RPCs.
 *
 * Regression test for the dev-proxy hang that made `bun run test:e2e` fail on
 * `main`: with the watch stream sharing a connection with the unary RPCs, the
 * 4th and 5th tests in `schema-seeding.e2e.ts` timed out after 30s each.
 *
 * HTTP/2 accounts flow control per connection as well as per stream, and the
 * connection window opens at 65,535 bytes. Against the real daemon, an open
 * server-streaming call on that connection stops the window being replenished:
 * once roughly 64 KiB of response data has arrived every later RPC on it hangs
 * forever, while the channel still reports READY and the daemon sits idle.
 *
 * `packages/dev-tools/src/grpc-client.ts` fixes this by giving the watch stream
 * its own connection via `grpc.use_local_subchannel_pool` (see
 * `WATCH_CHANNEL_OPTIONS` there).
 *
 * This test drives the REAL daemon through the dev-proxy, deliberately, rather
 * than a grpc-js stub server. A stub server does NOT reproduce the wedge —
 * verified by removing the fix and watching a stub-based version of this test
 * pass while `schema-seeding.e2e.ts` still failed — so only the real tonic/hyper
 * peer exercises the behavior this guards. Removing the fix fails this test.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { DaemonTestHarness } from './daemon-harness';

let h: DaemonTestHarness;

beforeAll(async () => {
  h = await DaemonTestHarness.start();
}, 15_000);

afterAll(async () => {
  await h?.stop();
});

/**
 * Enough `getAllSchemas` round trips to carry several times the 65,535-byte
 * connection window: the seeded core schemas serialize to ~21 KB per response,
 * so the window is exhausted after ~3 calls without the fix.
 */
const UNARY_CALLS = 25;

describe('dev-proxy: a parked WatchNodes stream does not wedge unary RPCs', () => {
  it(
    'unary calls keep succeeding past the HTTP/2 connection window while SSE is subscribed',
    async () => {
      // Subscribing to the proxy's SSE endpoint is what puts a real, long-lived
      // WatchNodes stream in flight — the browser-mode path this regressed on.
      const controller = new AbortController();
      const res = await fetch(h.sseUrl, {
        headers: { Accept: 'text/event-stream' },
        signal: controller.signal
      });
      expect(res.ok).toBe(true);

      // Drain the SSE body in the background so the subscription stays live for
      // the duration of the test rather than being closed by backpressure.
      const reader = res.body!.getReader();
      const drain = (async () => {
        try {
          for (;;) {
            const { done } = await reader.read();
            if (done) return;
          }
        } catch {
          // Aborted at teardown — expected.
        }
      })();

      let bytes = 0;
      for (let i = 0; i < UNARY_CALLS; i++) {
        // Pre-fix this hangs on the 4th iteration and the test times out.
        const schemas = await h.adapter.getAllSchemas();
        expect(schemas.length).toBeGreaterThan(0);
        bytes += JSON.stringify(schemas).length;
      }

      // Sanity-check the premise: this run really did move more data than the
      // window that used to wedge it, so a future regression cannot slip past
      // on payloads too small to reach the limit.
      expect(bytes).toBeGreaterThan(65_535);

      controller.abort();
      await drain;
    },
    30_000
  );
});
