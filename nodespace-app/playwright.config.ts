import { defineConfig, devices } from '@playwright/test';

/**
 * @see https://playwright.dev/docs/test-configuration
 */
export default defineConfig({
  testDir: './tests/e2e',
  
  /* Run tests in files in parallel */
  fullyParallel: true,
  
  /* Fail the build on CI if you accidentally left test.only in the source code. */
  forbidOnly: !!process.env.CI,
  
  /* Retry on CI only */
  retries: process.env.CI ? 2 : 0,
  
  /* Opt out of parallel tests on CI. */
  workers: process.env.CI ? 1 : undefined,
  
  /* Reporter to use. See https://playwright.dev/docs/test-reporters */
  reporter: [
    ['html', { outputFolder: 'test-results/playwright-report' }],
    ['json', { outputFile: 'test-results/playwright-results.json' }],
    ['line']
  ],
  
  /* Shared settings for all the projects below. See https://playwright.dev/docs/api/class-testoptions. */
  use: {
    /* Base URL to use in actions like `await page.goto('/')`. */
    baseURL: 'http://localhost:1420',
    
    /* Collect trace when retrying the failed test. See https://playwright.dev/docs/trace-viewer */
    trace: 'on-first-retry',
    
    /* Take screenshot on failure */
    screenshot: 'only-on-failure',
    
    /* Record video on failure */
    video: 'retain-on-failure',
  },

  /* Configure projects for major browsers and Tauri app */
  projects: [
    {
      name: 'tauri-app',
      use: {
        ...devices['Desktop Chrome'],
        // Tauri app runs on localhost:1420 by default in dev mode
        baseURL: 'http://localhost:1420',
      },
      testDir: './tests/e2e/tauri',
    },
    
    {
      name: 'web-chrome',
      use: { ...devices['Desktop Chrome'] },
      testDir: './tests/e2e/web',
    },

    {
      name: 'web-firefox',
      use: { ...devices['Desktop Firefox'] },
      testDir: './tests/e2e/web',
    },

    {
      name: 'web-safari',
      use: { ...devices['Desktop Safari'] },
      testDir: './tests/e2e/web',
    },
  ],

  /* Run your local dev server before starting the tests */
  webServer: process.env.CI ? undefined : {
    command: 'bun run tauri:dev',
    url: 'http://localhost:1420',
    timeout: 120 * 1000, // 2 minutes for Tauri app to start
    reuseExistingServer: !process.env.CI,
  },
  
  /* Global setup and teardown */
  globalSetup: './tests/e2e/global-setup.ts',
  globalTeardown: './tests/e2e/global-teardown.ts',
  
  /* Test timeout */
  timeout: 30 * 1000, // 30 seconds
  
  /* Expect timeout */
  expect: {
    timeout: 10 * 1000, // 10 seconds
  },
});