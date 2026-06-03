/// <reference types="vitest" />
import { defineConfig } from 'vitest/config';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

/**
 * Vitest config for e2e tests that spawn a real nodespaced + dev-proxy.
 *
 * Run with: bun run test:e2e
 *
 * Prerequisites:
 *   - nodespaced binary in packages/desktop-app/src-tauri/binaries/ (prebuilt sidecar)
 *   - Or set NODESPACED_BINARY=/path/to/nodespaced
 *
 * Optional env vars:
 *   E2E_DAEMON_TIMEOUT — ms to wait for daemon readiness (default 10000)
 *   E2E_VERBOSE        — set to any value to print daemon/proxy stdout
 */
export default defineConfig({
  resolve: {
    alias: {
      $lib: path.resolve(__dirname, 'src/lib')
    }
  },

  test: {
    include: ['src/tests/e2e/**/*.e2e.ts'],
    environment: 'node',
    globals: true,

    // E2e tests start real processes — allow longer timeouts
    testTimeout: 30_000,
    hookTimeout: 20_000,

    // Each test file gets its own daemon instance via beforeAll/afterAll,
    // so run files sequentially to avoid port conflicts
    pool: 'forks',
    poolOptions: {
      forks: {
        singleFork: true
      }
    },

    sequence: {
      concurrent: false,
      shuffle: false
    }
  }
});
