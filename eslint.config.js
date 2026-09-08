import js from '@eslint/js';
import ts from '@typescript-eslint/eslint-plugin';
import tsParser from '@typescript-eslint/parser';
import unicorn from 'eslint-plugin-unicorn';

export default [
  js.configs.recommended,
  {
    files: ['scripts/**/*.ts'],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: 'module'
      },
      globals: {
        // Bun/Node CLI globals — scripts/ runs under `bun run`, not a browser.
        console: 'readonly',
        process: 'readonly',
        Bun: 'readonly',
        __dirname: 'readonly',
        __filename: 'readonly',
        Buffer: 'readonly',
        fetch: 'readonly',
        URL: 'readonly',
        URLSearchParams: 'readonly',
        AbortController: 'readonly',
        AbortSignal: 'readonly',
        setTimeout: 'readonly',
        clearTimeout: 'readonly',
        setInterval: 'readonly',
        clearInterval: 'readonly',
        TextEncoder: 'readonly',
        TextDecoder: 'readonly',
        btoa: 'readonly',
        atob: 'readonly',
        performance: 'readonly',
        structuredClone: 'readonly',
        HeadersInit: 'readonly',
        RequestInit: 'readonly',
        // Bun's test runner (scripts/**/*.test.ts imports from "bun:test")
        describe: 'readonly',
        it: 'readonly',
        test: 'readonly',
        expect: 'readonly',
        beforeEach: 'readonly',
        afterEach: 'readonly',
        beforeAll: 'readonly',
        afterAll: 'readonly'
      }
    },
    plugins: {
      '@typescript-eslint': ts,
      unicorn
    },
    rules: {
      ...ts.configs.recommended.rules,
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_', varsIgnorePattern: '^_' }],
      '@typescript-eslint/no-explicit-any': 'warn',
      // CLI tooling — console output is the product, not a debugging leftover.
      'no-console': 'off',
      'unicorn/filename-case': ['error', {
        cases: {
          kebabCase: true
        },
        ignore: [
          '\\.test\\.ts$'
        ]
      }]
    }
  },
  {
    ignores: [
      'scripts/pkg-resources/',
      'scripts/results-e4b.json',
      'scripts/results-e4b.trace.jsonl',
      'node_modules/'
    ]
  }
];
