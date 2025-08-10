import { expect, afterEach, vi, beforeAll, afterAll } from 'vitest';
import { cleanup } from '@testing-library/svelte';
import '@testing-library/jest-dom';

// Setup MSW (Mock Service Worker) for API mocking
import { server } from './tests/mocks/server';

// Cleanup after each test
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

// Setup MSW
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

// Global test utilities
global.ResizeObserver = vi.fn().mockImplementation(() => ({
  observe: vi.fn(),
  unobserve: vi.fn(),
  disconnect: vi.fn(),
}));

// Mock IntersectionObserver
global.IntersectionObserver = vi.fn().mockImplementation(() => ({
  observe: vi.fn(),
  unobserve: vi.fn(),
  disconnect: vi.fn(),
}));

// Mock window.matchMedia
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: vi.fn().mockImplementation(query => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(), // deprecated
    removeListener: vi.fn(), // deprecated
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
});

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: vi.fn().mockReturnValue({
    listen: vi.fn(),
    emit: vi.fn(),
    show: vi.fn(),
    hide: vi.fn(),
    close: vi.fn(),
    minimize: vi.fn(),
    maximize: vi.fn(),
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
  emit: vi.fn(),
}));

// Add custom matchers
expect.extend({
  toBeInTheViewport(received) {
    const rect = received.getBoundingClientRect();
    const isInViewport = rect.top >= 0 && 
                        rect.left >= 0 && 
                        rect.bottom <= window.innerHeight && 
                        rect.right <= window.innerWidth;
    
    return {
      message: () => isInViewport
        ? `Expected element to not be in viewport`
        : `Expected element to be in viewport`,
      pass: isInViewport,
    };
  },
});