import { defineConfig } from 'vitest/config';
import { sveltekit } from '@sveltejs/kit/vite';

export default defineConfig({
  plugins: [sveltekit()],
  
  test: {
    // Test environment configuration
    environment: 'jsdom',
    
    // Test file patterns
    include: ['src/**/*.{test,spec}.{js,ts}'],
    exclude: [
      'src/**/*.e2e.{test,spec}.{js,ts}',
      'tests/**/*', // Exclude tests directory (contains Playwright tests)
      '**/node_modules/**',
      '**/dist/**',
      '**/cypress/**',
      '**/.{idea,git,cache,output,temp}/**',
      '**/{karma,rollup,webpack,vite,vitest,jest,ava,babel,nyc,cypress,tsup,build,eslint,prettier}.config.*'
    ],
    
    // Coverage configuration
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      reportsDirectory: './coverage',
      exclude: [
        'coverage/**',
        'dist/**',
        '**/node_modules/**',
        '**/src/app.html',
        '**/*.config.*',
        '**/*.d.ts',
        'tests/**',
        'src/lib/components/ui/**', // Generated UI components
      ],
      thresholds: {
        global: {
          branches: 80,
          functions: 85,
          lines: 90,
          statements: 90,
        },
      },
    },
    
    // Setup files
    setupFiles: ['./src/setupTests.ts'],
    
    // Global test configuration
    globals: true,
    
    // Test timeout
    testTimeout: 10000,
    
    // Watch options
    watch: {
      exclude: ['**/node_modules/**', '**/dist/**', '**/coverage/**']
    },
    
    // Reporter configuration
    reporter: ['verbose', 'json', 'html'],
    outputFile: {
      json: './test-results/results.json',
      html: './test-results/results.html'
    }
  },

  // Resolve configuration for tests
  resolve: {
    alias: {
      $lib: new URL('./src/lib', import.meta.url).pathname,
      $app: new URL('./src/app', import.meta.url).pathname,
    }
  }
});