/**
 * Integration tests for keyboard commands with ContentEditableController
 *
 * Tests the full workflow:
 * 1. Commands registered with registry
 * 2. KeyboardEvent dispatched
 * 3. Registry executes appropriate command
 * 4. Command interacts with controller
 * 5. Controller emits appropriate events
 */

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { KeyboardCommandRegistry } from '$lib/services/keyboardCommandRegistry';
import { CreateNodeCommand } from '$lib/commands/keyboard/create-node.command';
import { IndentNodeCommand } from '$lib/commands/keyboard/indent-node.command';
import { OutdentNodeCommand } from '$lib/commands/keyboard/outdent-node.command';
import { MergeNodesCommand } from '$lib/commands/keyboard/merge-nodes.command';
import type { ContentEditableController } from '$lib/design/components/contentEditableController';

describe('Keyboard Command Integration', () => {
  let registry: KeyboardCommandRegistry;
  let mockController: Partial<ContentEditableController>;
  let mockEvents: {
    createNewNode: ReturnType<typeof vi.fn>;
    indentNode: ReturnType<typeof vi.fn>;
    outdentNode: ReturnType<typeof vi.fn>;
    combineWithPrevious: ReturnType<typeof vi.fn>;
    deleteNode: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    registry = KeyboardCommandRegistry.getInstance();
    registry.clearAll();

    // Create mock events
    mockEvents = {
      createNewNode: vi.fn(),
      indentNode: vi.fn(),
      outdentNode: vi.fn(),
      combineWithPrevious: vi.fn(),
      deleteNode: vi.fn()
    };

    // Create mock controller
    mockController = {
      events: mockEvents as any,
      element: {
        textContent: 'test content',
        innerHTML: 'test content'
      } as any,
      getCurrentColumn: vi.fn(() => 5)
    } as any;

    // Register commands (simulating what ContentEditableController does)
    registry.register({ key: 'Enter' }, new CreateNodeCommand());
    registry.register({ key: 'Tab' }, new IndentNodeCommand());
    registry.register({ key: 'Tab', shift: true }, new OutdentNodeCommand());
    registry.register({ key: 'Backspace' }, new MergeNodesCommand('up'));
  });

  afterEach(() => {
    registry.clearAll();
  });

  describe('Enter Key Integration', () => {
    it('should execute CreateNodeCommand and emit createNewNode event', async () => {
      const event = new KeyboardEvent('keydown', { key: 'Enter' });
      const context = {
        nodeId: 'test-node',
        nodeType: 'text',
        content: 'test content',
        cursorPosition: 5,
        allowMultiline: false,
        metadata: {}
      };

      const handled = await registry.execute(event, mockController as any, context);

      expect(handled).toBe(true);
      expect(mockEvents.createNewNode).toHaveBeenCalled();
    });

    it('should not execute CreateNodeCommand for Shift+Enter', async () => {
      const event = new KeyboardEvent('keydown', { key: 'Enter', shiftKey: true });
      const context = {
        nodeId: 'test-node',
        nodeType: 'text',
        content: 'test content',
        cursorPosition: 5,
        allowMultiline: false,
        metadata: {}
      };

      const handled = await registry.execute(event, mockController as any, context);

      expect(handled).toBe(false);
      expect(mockEvents.createNewNode).not.toHaveBeenCalled();
    });
  });

  describe('Tab Key Integration', () => {
    it('should execute IndentNodeCommand and emit indentNode event', async () => {
      const event = new KeyboardEvent('keydown', { key: 'Tab' });
      const context = {
        nodeId: 'test-node',
        nodeType: 'text',
        content: 'test content',
        cursorPosition: 5,
        allowMultiline: false,
        metadata: {}
      };

      const handled = await registry.execute(event, mockController as any, context);

      expect(handled).toBe(true);
      expect(mockEvents.indentNode).toHaveBeenCalledWith({
        nodeId: 'test-node'
      });
    });

    it('should execute OutdentNodeCommand for Shift+Tab and emit outdentNode event', async () => {
      const event = new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true });
      const context = {
        nodeId: 'test-node',
        nodeType: 'text',
        content: 'test content',
        cursorPosition: 5,
        allowMultiline: false,
        metadata: {}
      };

      const handled = await registry.execute(event, mockController as any, context);

      expect(handled).toBe(true);
      expect(mockEvents.outdentNode).toHaveBeenCalledWith({
        nodeId: 'test-node'
      });
    });
  });

  describe('Backspace Key Integration', () => {
    it('should execute MergeNodesCommand for non-empty node at start', async () => {
      // Mock controller at start position
      (mockController.getCurrentColumn as any).mockReturnValue(0);
      (mockController.element as any).textContent = 'test content';

      const event = new KeyboardEvent('keydown', { key: 'Backspace' });
      const context = {
        nodeId: 'test-node',
        nodeType: 'text',
        content: 'test content',
        cursorPosition: 0,
        allowMultiline: false,
        metadata: {}
      };

      const handled = await registry.execute(event, mockController as any, context);

      expect(handled).toBe(true);
      expect(mockEvents.combineWithPrevious).toHaveBeenCalledWith({
        nodeId: 'test-node',
        currentContent: 'test content'
      });
    });

    it('should execute MergeNodesCommand and delete empty node', async () => {
      // Mock controller at start position with empty content
      (mockController.getCurrentColumn as any).mockReturnValue(0);
      (mockController.element as any).textContent = '   '; // Whitespace only

      const event = new KeyboardEvent('keydown', { key: 'Backspace' });
      const context = {
        nodeId: 'test-node',
        nodeType: 'text',
        content: '',
        cursorPosition: 0,
        allowMultiline: false,
        metadata: {}
      };

      const handled = await registry.execute(event, mockController as any, context);

      expect(handled).toBe(true);
      expect(mockEvents.deleteNode).toHaveBeenCalledWith({
        nodeId: 'test-node'
      });
      expect(mockEvents.combineWithPrevious).not.toHaveBeenCalled();
    });

    it('should not execute MergeNodesCommand when not at start', async () => {
      // Mock controller NOT at start position
      (mockController.getCurrentColumn as any).mockReturnValue(5);

      const event = new KeyboardEvent('keydown', { key: 'Backspace' });
      const context = {
        nodeId: 'test-node',
        nodeType: 'text',
        content: 'test content',
        cursorPosition: 5,
        allowMultiline: false,
        metadata: {}
      };

      const handled = await registry.execute(event, mockController as any, context);

      expect(handled).toBe(false);
      expect(mockEvents.combineWithPrevious).not.toHaveBeenCalled();
      expect(mockEvents.deleteNode).not.toHaveBeenCalled();
    });
  });

  describe('Command Priority and Fallback', () => {
    it('should return false for unregistered keys', async () => {
      const event = new KeyboardEvent('keydown', { key: 'a' });
      const context = {
        nodeId: 'test-node',
        nodeType: 'text',
        content: 'test content',
        cursorPosition: 5,
        allowMultiline: false,
        metadata: {}
      };

      const handled = await registry.execute(event, mockController as any, context);

      expect(handled).toBe(false);
    });

    it('should handle multiple commands registered correctly', async () => {
      // Verify all commands are registered
      const commands = registry.getCommands();

      expect(commands.size).toBe(4);
      expect(commands.has('Enter')).toBe(true);
      expect(commands.has('Tab')).toBe(true);
      expect(commands.has('Shift+Tab')).toBe(true);
      expect(commands.has('Backspace')).toBe(true);
    });
  });

  describe('Context Building', () => {
    it('should properly use context data in command execution', async () => {
      const event = new KeyboardEvent('keydown', { key: 'Enter' });

      // Specific context with custom node type
      const context = {
        nodeId: 'custom-node-id',
        nodeType: 'task',
        content: 'custom content',
        cursorPosition: 10,
        allowMultiline: false,
        metadata: {}
      };

      const handled = await registry.execute(event, mockController as any, context);

      expect(handled).toBe(true);
      expect(mockEvents.createNewNode).toHaveBeenCalledWith(
        expect.objectContaining({
          afterNodeId: 'custom-node-id',
          nodeType: 'task'
        })
      );
    });
  });
});
