import { render, type RenderResult } from '@testing-library/svelte';
import { vi, type MockedFunction } from 'vitest';
import { tick } from 'svelte';
import userEvent from '@testing-library/user-event';
import type { ComponentProps, SvelteComponent } from 'svelte';

// Enhanced render function with common setup
export function renderWithContext<T extends SvelteComponent>(
  Component: new (...args: any[]) => T,
  props: ComponentProps<T> = {} as ComponentProps<T>,
  options: {
    context?: Map<string, any>;
    container?: HTMLElement;
  } = {}
): RenderResult<T> & { user: ReturnType<typeof userEvent.setup> } {
  const user = userEvent.setup();
  
  const result = render(Component, {
    props,
    context: options.context || new Map(),
    container: options.container,
  });

  return {
    ...result,
    user,
  };
}

// Wait for Svelte reactivity and DOM updates
export async function waitForSvelteUpdate(): Promise<void> {
  await tick();
  await new Promise(resolve => setTimeout(resolve, 100));
}

// Wait for element to appear with custom timeout
export function waitForElement(
  selector: string,
  container: HTMLElement = document.body,
  timeout: number = 5000
): Promise<Element> {
  return new Promise((resolve, reject) => {
    const startTime = Date.now();
    
    function check() {
      const element = container.querySelector(selector);
      if (element) {
        resolve(element);
        return;
      }
      
      if (Date.now() - startTime > timeout) {
        reject(new Error(`Element with selector "${selector}" not found within ${timeout}ms`));
        return;
      }
      
      setTimeout(check, 100);
    }
    
    check();
  });
}

// Simulate realistic user typing
export async function simulateTyping(
  element: HTMLInputElement | HTMLTextAreaElement,
  text: string,
  { delay = 50, clear = false }: { delay?: number; clear?: boolean } = {}
): Promise<void> {
  if (clear) {
    element.value = '';
  }
  
  element.focus();
  
  for (const char of text) {
    element.value += char;
    element.dispatchEvent(new Event('input', { bubbles: true }));
    await new Promise(resolve => setTimeout(resolve, delay));
  }
}

// Wait for async operations to complete
export async function waitForLoadingToFinish(container: HTMLElement): Promise<void> {
  // Wait for any loading indicators to disappear
  const loadingSelectors = [
    '[data-testid="loading"]',
    '.loading',
    '[aria-label*="loading" i]',
    '[aria-label*="Loading" i]'
  ];
  
  for (const selector of loadingSelectors) {
    const loadingElement = container.querySelector(selector);
    if (loadingElement) {
      // Wait for the loading element to be removed
      while (container.querySelector(selector)) {
        await new Promise(resolve => setTimeout(resolve, 100));
      }
    }
  }
  
  // Additional wait for any pending async operations
  await new Promise(resolve => setTimeout(resolve, 100));
}

// Mock Tauri API functions
export function mockTauriAPI() {
  const mockInvoke = vi.fn();
  const mockListen = vi.fn();
  const mockEmit = vi.fn();

  vi.mock('@tauri-apps/api/core', () => ({
    invoke: mockInvoke,
  }));

  vi.mock('@tauri-apps/api/event', () => ({
    listen: mockListen,
    emit: mockEmit,
  }));

  return {
    invoke: mockInvoke,
    listen: mockListen,
    emit: mockEmit,
    reset: () => {
      mockInvoke.mockReset();
      mockListen.mockReset();
      mockEmit.mockReset();
    },
  };
}

// Create test context with mock data and APIs
export function createTestContext() {
  const mockAPI = mockTauriAPI();
  
  return {
    mockAPI,
    setupNodeAPI: (nodes: any[]) => {
      mockAPI.invoke.mockImplementation(async (command: string, args?: any) => {
        switch (command) {
          case 'create_node':
            const newNode = { 
              id: `node-${Date.now()}`, 
              created_at: new Date().toISOString(),
              updated_at: new Date().toISOString(),
              metadata: {},
              ...args 
            };
            nodes.push(newNode);
            return newNode;
          case 'get_node':
            return nodes.find(n => n.id === args.id) || null;
          case 'update_node':
            const nodeIndex = nodes.findIndex(n => n.id === args.id);
            if (nodeIndex >= 0) {
              nodes[nodeIndex] = { ...nodes[nodeIndex], ...args, updated_at: new Date().toISOString() };
              return nodes[nodeIndex];
            }
            throw new Error('Node not found');
          case 'delete_node':
            const deleteIndex = nodes.findIndex(n => n.id === args.id);
            if (deleteIndex >= 0) {
              nodes.splice(deleteIndex, 1);
              return true;
            }
            return false;
          case 'search_nodes':
            return nodes.filter(n =>
              n.content.toLowerCase().includes(args.query.toLowerCase())
            );
          default:
            throw new Error(`Unknown command: ${command}`);
        }
      });
    },
  };
}

// Performance testing utilities
export function measureRenderTime<T extends SvelteComponent>(
  Component: new (...args: any[]) => T,
  props: ComponentProps<T> = {} as ComponentProps<T>
): { result: RenderResult<T>; renderTime: number } {
  const startTime = performance.now();
  const result = render(Component, { props });
  const renderTime = performance.now() - startTime;
  
  return { result, renderTime };
}

export async function measureAsyncOperation<T>(
  operation: () => Promise<T>
): Promise<{ result: T; duration: number }> {
  const startTime = performance.now();
  const result = await operation();
  const duration = performance.now() - startTime;
  
  return { result, duration };
}

// Accessibility testing helpers
export function checkAccessibility(container: HTMLElement): {
  issues: string[];
  hasIssues: boolean;
} {
  const issues: string[] = [];
  
  // Check for missing alt text on images
  const images = container.querySelectorAll('img:not([alt])');
  if (images.length > 0) {
    issues.push(`${images.length} images missing alt text`);
  }
  
  // Check for interactive elements without proper labels
  const interactiveElements = container.querySelectorAll(
    'button:not([aria-label]):not([aria-labelledby]), input:not([aria-label]):not([aria-labelledby]):not([id])'
  );
  if (interactiveElements.length > 0) {
    issues.push(`${interactiveElements.length} interactive elements missing labels`);
  }
  
  // Check for proper heading hierarchy
  const headings = Array.from(container.querySelectorAll('h1, h2, h3, h4, h5, h6'));
  if (headings.length > 0) {
    const levels = headings.map(h => parseInt(h.tagName[1]));
    for (let i = 1; i < levels.length; i++) {
      if (levels[i] > levels[i - 1] + 1) {
        issues.push('Heading hierarchy skip detected');
        break;
      }
    }
  }
  
  return {
    issues,
    hasIssues: issues.length > 0,
  };
}

// Error boundary testing
export function expectNoConsoleErrors(): void {
  const originalError = console.error;
  const mockError = vi.fn();
  console.error = mockError;
  
  return () => {
    console.error = originalError;
    expect(mockError).not.toHaveBeenCalled();
  };
}

// Local storage mocking
export function mockLocalStorage() {
  const storage = new Map<string, string>();
  
  const mockStorage = {
    getItem: vi.fn((key: string) => storage.get(key) || null),
    setItem: vi.fn((key: string, value: string) => storage.set(key, value)),
    removeItem: vi.fn((key: string) => storage.delete(key)),
    clear: vi.fn(() => storage.clear()),
    key: vi.fn((index: number) => Array.from(storage.keys())[index] || null),
    get length() { return storage.size; },
  };
  
  Object.defineProperty(window, 'localStorage', {
    value: mockStorage,
    writable: true,
  });
  
  return {
    storage: mockStorage,
    data: storage,
    reset: () => {
      storage.clear();
      mockStorage.getItem.mockClear();
      mockStorage.setItem.mockClear();
      mockStorage.removeItem.mockClear();
      mockStorage.clear.mockClear();
    },
  };
}