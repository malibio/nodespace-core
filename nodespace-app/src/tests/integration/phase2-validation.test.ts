/**
 * Phase 2 Implementation Validation Test
 * Issue #69 - Universal Node References Phase 2.2: Rich Decorations
 * 
 * This test validates the complete component-based decoration system:
 * 1. ContentProcessor renders nodespace references with component placeholders
 * 2. ComponentHydrationSystem can mount Svelte components from placeholders
 * 3. Plugin architecture is ready for future node types
 */

import { describe, it, expect, beforeEach } from 'bun:test';
import { JSDOM } from 'jsdom';

// Set up DOM environment for testing
const dom = new JSDOM('<!DOCTYPE html><html><body></body></html>', {
  url: 'http://localhost',
  pretendToBeVisual: true,
  resources: 'usable'
});

// Make DOM globally available
global.document = dom.window.document;
global.window = dom.window as any;
global.HTMLElement = dom.window.HTMLElement;
global.Element = dom.window.Element;
global.Node = dom.window.Node;

import { contentProcessor } from '../../lib/services/contentProcessor';
import { componentHydrationSystem } from '../../lib/services/ComponentHydrationSystem';
import { NodeDecoratorFactory } from '../../lib/services/BaseNodeDecoration';
import type { DecorationContext } from '../../lib/services/BaseNodeDecoration';
import { getNodeReferenceComponent, NODE_REFERENCE_COMPONENTS } from '../../lib/components/references';

// Mock NodeReferenceService for testing
const mockNodeReferenceService = {
  resolveNodespaceURI: (uri: string) => {
    // Mock resolved nodes based on URI patterns
    if (uri.includes('/task/')) {
      return {
        type: 'task',
        content: 'Mock task content',
        metadata: { status: 'pending', priority: 'medium' }
      };
    }
    if (uri.includes('/user/')) {
      return {
        type: 'user',
        content: 'Mock user content',
        metadata: { role: 'developer' }
      };
    }
    if (uri.includes('/date/')) {
      return {
        type: 'date',
        content: '2024-03-15',
        metadata: {}
      };
    }
    return {
      type: 'text',
      content: 'Mock content',
      metadata: {}
    };
  },
  parseNodespaceURI: (uri: string) => ({
    isValid: true,
    title: 'Mock Reference',
    displayText: 'Mock Display'
  })
};

describe('Phase 2 Component-Based Decoration System', () => {
  beforeEach(() => {
    // Clean up any existing components
    componentHydrationSystem.cleanup();
  });

  describe('Component Registry', () => {
    it('should have core node reference components registered', () => {
      expect(NODE_REFERENCE_COMPONENTS['BaseNodeReference']).toBeDefined();
      expect(NODE_REFERENCE_COMPONENTS['TaskNodeReference']).toBeDefined();
      expect(NODE_REFERENCE_COMPONENTS['DateNodeReference']).toBeDefined();
      expect(NODE_REFERENCE_COMPONENTS['UserNodeReference']).toBeDefined();
    });

    it('should resolve components by node type', () => {
      const taskComponent = getNodeReferenceComponent('task');
      const userComponent = getNodeReferenceComponent('user');
      const dateComponent = getNodeReferenceComponent('date');
      const unknownComponent = getNodeReferenceComponent('unknown');

      expect(taskComponent).toBeDefined();
      expect(userComponent).toBeDefined();
      expect(dateComponent).toBeDefined();
      expect(unknownComponent).toBeDefined(); // Should fallback to BaseNodeReference
    });
  });

  describe('NodeDecoratorFactory', () => {
    it('should create component decorations for different node types', () => {
      const factory = new NodeDecoratorFactory(mockNodeReferenceService as any);

      const taskContext: DecorationContext = {
        nodeId: 'task-123',
        nodeType: 'task',
        title: 'Test Task',
        content: 'Task content',
        uri: 'nodespace://task/123/test-task',
        metadata: { status: 'pending' },
        targetElement: null as any,
        displayContext: 'inline'
      };

      const decoration = factory.decorateReference(taskContext);

      expect(decoration).toBeDefined();
      expect(decoration.component).toBeDefined();
      expect(decoration.props).toBeDefined();
      expect(decoration.props.nodeId).toBe('task-123');
      expect(decoration.props.nodeType).toBe('task');
      expect(decoration.metadata).toBeDefined();
    });

    it('should handle different node types appropriately', () => {
      const factory = new NodeDecoratorFactory(mockNodeReferenceService as any);

      const testCases = [
        { nodeType: 'task', expectedProps: ['completed', 'priority'] },
        { nodeType: 'user', expectedProps: ['isOnline', 'displayName'] },
        { nodeType: 'date', expectedProps: ['date', 'isToday', 'isPast'] },
        { nodeType: 'document', expectedProps: ['nodeType'] }
      ];

      for (const { nodeType, expectedProps } of testCases) {
        const context: DecorationContext = {
          nodeId: `${nodeType}-123`,
          nodeType,
          title: `Test ${nodeType}`,
          content: `${nodeType} content`,
          uri: `nodespace://${nodeType}/123/test`,
          metadata: {},
          targetElement: null as any,
          displayContext: 'inline'
        };

        const decoration = factory.decorateReference(context);
        
        expect(decoration.component).toBeDefined();
        expect(decoration.props.nodeType).toBe(nodeType);
        
        // Check for expected type-specific props
        for (const prop of expectedProps) {
          expect(decoration.props).toHaveProperty(prop);
        }
      }
    });
  });

  describe('ContentProcessor Integration', () => {
    beforeEach(() => {
      // Set up ContentProcessor with mock service
      (contentProcessor as any).nodeReferenceService = mockNodeReferenceService;
      
      // Enable nodespace reference processing
      (contentProcessor as any).options = {
        enableNodespaceReferences: true,
        enableBacklinks: true
      };
    });

    it('should render component placeholders for nodespace references', async () => {
      const markdown = `# Test Document

This is a reference to nodespace://task/123/my-task and another to nodespace://user/456/john-doe.

End of test.`;

      const html = await contentProcessor.markdownToDisplayWithReferences(markdown);

      expect(html).toContain('ns-component-placeholder');
      expect(html).toContain('data-component=');
      expect(html).toContain('data-node-type=');
      expect(html).toContain('data-props=');
      expect(html).toContain('data-hydrate="pending"');
    });

    it('should include proper component data in placeholders', async () => {
      const markdown = `Reference: nodespace://task/123/test-task`;

      const html = await contentProcessor.markdownToDisplayWithReferences(markdown);

      // Should contain component placeholder with proper data attributes
      expect(html).toMatch(/data-component="[^"]+"/);
      expect(html).toMatch(/data-node-type="task"/);
      expect(html).toMatch(/data-props="[^"]+"/); // JSON with HTML entities
    });
  });

  describe('ComponentHydrationSystem', () => {
    let container: HTMLElement;

    beforeEach(() => {
      container = document.createElement('div');
      document.body.appendChild(container);
    });

    it('should hydrate component placeholders', async () => {
      // Create a mock placeholder
      container.innerHTML = `
        <span class="ns-component-placeholder" 
              data-component="TaskNodeReference"
              data-node-type="task"
              data-props='{"nodeId":"task-123","content":"Test Task","href":"#","nodeType":"task","completed":false,"priority":"medium"}'
              data-metadata='{"nodeType":"task"}'
              data-hydrate="pending">
          Test Task
        </span>
      `;

      const result = await componentHydrationSystem.hydrate({
        container
      });

      expect(result.hydrated).toBe(1);
      expect(result.failed).toBe(0);
      expect(result.errors).toHaveLength(0);

      // Check that placeholder was marked as hydrated
      const placeholder = container.querySelector('.ns-component-placeholder');
      expect(placeholder?.getAttribute('data-hydrate')).toBe('completed');
    });

    it('should handle hydration errors gracefully', async () => {
      // Create a placeholder with invalid data
      container.innerHTML = `
        <span class="ns-component-placeholder" 
              data-component="NonExistentComponent"
              data-node-type="unknown"
              data-props='invalid-json'
              data-hydrate="pending">
          Test Content
        </span>
      `;

      const result = await componentHydrationSystem.hydrate({
        container
      });

      expect(result.hydrated).toBe(0);
      expect(result.failed).toBe(1);
      expect(result.errors).toHaveLength(1);

      // Check that placeholder was marked as failed
      const placeholder = container.querySelector('.ns-component-placeholder');
      expect(placeholder?.getAttribute('data-hydrate')).toBe('failed');
    });

    it('should provide accurate statistics', async () => {
      // Create multiple placeholders
      container.innerHTML = `
        <span class="ns-component-placeholder" 
              data-component="TaskNodeReference"
              data-node-type="task"
              data-props='{"nodeId":"task-123","content":"Task 1","href":"#","nodeType":"task","completed":false,"priority":"medium"}'
              data-hydrate="pending">Task 1</span>
        <span class="ns-component-placeholder" 
              data-component="UserNodeReference"
              data-node-type="user"
              data-props='{"nodeId":"user-456","content":"User 1","href":"#","nodeType":"user","isOnline":true}'
              data-hydrate="pending">User 1</span>
      `;

      await componentHydrationSystem.hydrate({ container });
      const stats = componentHydrationSystem.getStats();

      expect(stats.totalMounted).toBe(2);
      expect(stats.componentTypes['TaskNodeReference']).toBe(1);
      expect(stats.componentTypes['UserNodeReference']).toBe(1);
    });

    it('should cleanup components properly', async () => {
      container.innerHTML = `
        <span class="ns-component-placeholder" 
              data-component="TaskNodeReference"
              data-node-type="task"
              data-props='{"nodeId":"task-123","content":"Test Task","href":"#","nodeType":"task","completed":false,"priority":"medium"}'
              data-hydrate="pending">Test Task</span>
      `;

      await componentHydrationSystem.hydrate({ container });
      
      let stats = componentHydrationSystem.getStats();
      expect(stats.totalMounted).toBe(1);

      componentHydrationSystem.cleanup(container);
      
      stats = componentHydrationSystem.getStats();
      expect(stats.totalMounted).toBe(0);
    });
  });

  describe('Plugin Architecture Readiness', () => {
    it('should support plugin component registration pattern', () => {
      // This test validates that the architecture supports plugin registration
      // In real usage, plugins would register during build time
      const originalComponentCount = Object.keys(NODE_REFERENCE_COMPONENTS).length;

      // Mock plugin component registration
      const MockPluginComponent = class MockPluginComponent {
        constructor() {}
      };

      NODE_REFERENCE_COMPONENTS['MockPluginComponent'] = MockPluginComponent;
      NODE_REFERENCE_COMPONENTS['mock'] = MockPluginComponent;

      const resolvedComponent = getNodeReferenceComponent('mock');
      expect(resolvedComponent).toBe(MockPluginComponent);

      const newComponentCount = Object.keys(NODE_REFERENCE_COMPONENTS).length;
      expect(newComponentCount).toBe(originalComponentCount + 2);

      // Cleanup
      delete NODE_REFERENCE_COMPONENTS['MockPluginComponent'];
      delete NODE_REFERENCE_COMPONENTS['mock'];
    });
  });

  describe('End-to-End Integration', () => {
    let container: HTMLElement;

    beforeEach(() => {
      container = document.createElement('div');
      document.body.appendChild(container);
      (contentProcessor as any).nodeReferenceService = mockNodeReferenceService;
    });

    it('should complete the full pipeline: markdown → components', async () => {
      const markdown = `# Integration Test

Task reference: nodespace://task/123/test-task
User reference: nodespace://user/456/test-user`;

      // Step 1: Process markdown and render to HTML with placeholders
      const html = await contentProcessor.markdownToDisplayWithReferences(markdown);
      expect(html).toContain('ns-component-placeholder');
      
      // Step 3: Insert into DOM
      container.innerHTML = html;

      // Step 4: Hydrate components
      const result = await componentHydrationSystem.hydrate({ container });
      
      // Validate the complete pipeline
      expect(result.hydrated).toBeGreaterThan(0);
      expect(result.failed).toBe(0);
      expect(componentHydrationSystem.getStats().totalMounted).toBeGreaterThan(0);
    });
  });
});