/**
 * Comprehensive tests for FocusManager service
 * Target: 95%+ coverage
 *
 * This file covers all code paths not covered by focus-manager-cursor-positioning.test.ts
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { focusManager, getFocusManager } from '$lib/services/focus-manager.svelte';

describe('FocusManager - Comprehensive Coverage', () => {
  beforeEach(() => {
    // Clear state before each test
    focusManager.clearEditing();
  });

  describe('Pane ID tracking', () => {
    it('should track editing pane ID when focusing a node', () => {
      focusManager.focusNode('test-node', 'pane-1');

      expect(focusManager.editingNodeId).toBe('test-node');
      expect(focusManager.editingPaneId).toBe('pane-1');
    });

    it('should update pane ID when focusing in different pane', () => {
      focusManager.focusNode('test-node', 'pane-1');
      expect(focusManager.editingPaneId).toBe('pane-1');

      focusManager.focusNode('test-node', 'pane-2');
      expect(focusManager.editingPaneId).toBe('pane-2');
    });

    it('should track pane ID for focusNodeAtPosition', () => {
      focusManager.focusNodeAtPosition('test-node', 10, 'custom-pane');
      expect(focusManager.editingPaneId).toBe('custom-pane');
    });

    it('should track pane ID for focusNodeAtLine', () => {
      focusManager.focusNodeAtLine('test-node', 'sidebar-pane', 2);
      expect(focusManager.editingPaneId).toBe('sidebar-pane');
    });

    it('should track pane ID for focusNodeFromArrowNav', () => {
      focusManager.focusNodeFromArrowNav('test-node', 'down', 100, 'main-pane');
      expect(focusManager.editingPaneId).toBe('main-pane');
    });
  });

  describe('Node type conversion', () => {
    it('focusNodeFromTypeConversion should set node-type-conversion cursor position', () => {
      focusManager.focusNodeFromTypeConversion('converted-node', 25, 'default');

      expect(focusManager.editingNodeId).toBe('converted-node');
      expect(focusManager.editingPaneId).toBe('default');
      expect(focusManager.cursorPosition).toEqual({
        type: 'node-type-conversion',
        position: 25
      });
    });

    it('focusNodeFromTypeConversion should clear previous cursor state', () => {
      // Set arrow navigation first
      focusManager.focusNodeFromArrowNav('node-1', 'up', 50, 'default');
      expect(focusManager.cursorPosition?.type).toBe('arrow-navigation');

      // Convert node type - should replace arrow nav cursor
      focusManager.focusNodeFromTypeConversion('node-2', 30, 'default');

      expect(focusManager.cursorPosition).toEqual({
        type: 'node-type-conversion',
        position: 30
      });
    });
  });

  describe('Inherited type node', () => {
    it('focusNodeFromInheritedType should set inherited-type cursor position', () => {
      focusManager.focusNodeFromInheritedType('inherited-node', 15, 'default');

      expect(focusManager.editingNodeId).toBe('inherited-node');
      expect(focusManager.editingPaneId).toBe('default');
      expect(focusManager.cursorPosition).toEqual({
        type: 'inherited-type',
        position: 15
      });
    });

    it('focusNodeFromInheritedType should replace previous cursor state', () => {
      focusManager.focusNodeFromArrowNav('node-1', 'down', 100, 'default');
      focusManager.focusNodeFromTypeConversion('node-2', 30, 'default');

      focusManager.focusNodeFromInheritedType('inherited', 20, 'pane-1');

      expect(focusManager.cursorPosition).toEqual({
        type: 'inherited-type',
        position: 20
      });
    });

    it('focusNodeFromInheritedType should track pane ID', () => {
      focusManager.focusNodeFromInheritedType('node', 10, 'custom-pane');
      expect(focusManager.editingPaneId).toBe('custom-pane');
    });
  });

  describe('isNodeEditing', () => {
    it('should return true when the node is being edited', () => {
      focusManager.focusNode('test-node', 'default');
      expect(focusManager.isNodeEditing('test-node')).toBe(true);
    });

    it('should return false when a different node is being edited', () => {
      focusManager.focusNode('node-1', 'default');
      expect(focusManager.isNodeEditing('node-2')).toBe(false);
    });

    it('should return false when no node is being edited', () => {
      focusManager.clearEditing();
      expect(focusManager.isNodeEditing('any-node')).toBe(false);
    });

    it('should update when editing state changes', () => {
      focusManager.focusNode('node-1', 'default');
      expect(focusManager.isNodeEditing('node-1')).toBe(true);

      focusManager.focusNode('node-2', 'default');
      expect(focusManager.isNodeEditing('node-1')).toBe(false);
      expect(focusManager.isNodeEditing('node-2')).toBe(true);
    });
  });

  describe('getCurrentState', () => {
    it('should include cursor position in state', () => {
      focusManager.focusNodeFromArrowNav('node', 'down', 75, 'default');

      const state = focusManager.getCurrentState();

      expect(state.nodeId).toBe('node');
      expect(state.cursorPosition).toEqual({
        type: 'arrow-navigation',
        direction: 'down',
        pixelOffset: 75
      });
    });

    it('should return null cursorPosition when not set', () => {
      focusManager.clearEditing();

      const state = focusManager.getCurrentState();

      expect(state.cursorPosition).toBeNull();
    });
  });

  describe('getFocusManager (legacy export)', () => {
    it('should return the same focusManager instance', () => {
      const manager = getFocusManager();
      expect(manager).toBe(focusManager);
    });

    it('should maintain state across both references', () => {
      const manager = getFocusManager();
      manager.focusNode('test-node', 'default');

      expect(focusManager.editingNodeId).toBe('test-node');
      expect(manager.editingNodeId).toBe('test-node');
    });
  });

  describe('Edge cases and state transitions', () => {
    it('should handle rapid state changes without conflicts', () => {
      focusManager.focusNode('node-1', 'default');
      focusManager.focusNodeAtPosition('node-2', 10, 'default');
      focusManager.focusNodeFromArrowNav('node-3', 'up', 50, 'default');
      focusManager.focusNodeFromTypeConversion('node-4', 20, 'default');
      focusManager.focusNodeFromInheritedType('node-5', 15, 'default');

      expect(focusManager.editingNodeId).toBe('node-5');
      expect(focusManager.cursorPosition).toEqual({
        type: 'inherited-type',
        position: 15
      });
    });

    it('should handle clearing state multiple times', () => {
      focusManager.focusNode('node', 'default');
      focusManager.clearEditing();
      focusManager.clearEditing();

      expect(focusManager.editingNodeId).toBeNull();
      expect(focusManager.cursorPosition).toBeNull();
    });

    it('should handle clearing cursor position when no cursor is set', () => {
      focusManager.clearCursorPosition();
      expect(focusManager.cursorPosition).toBeNull();
    });

    it('should maintain pane ID through cursor position changes', () => {
      focusManager.focusNode('node', 'pane-1');
      expect(focusManager.editingPaneId).toBe('pane-1');

      focusManager.focusNodeAtPosition('node', 20, 'pane-2');
      expect(focusManager.editingPaneId).toBe('pane-2');

      focusManager.clearCursorPosition();
      // Pane ID should remain after cursor clear (editing still active)
      expect(focusManager.editingPaneId).toBe('pane-2');
    });
  });

  describe('Multiple pane scenarios', () => {
    it('should switch editing between panes correctly', () => {
      focusManager.focusNode('node-1', 'left-pane');
      expect(focusManager.editingPaneId).toBe('left-pane');

      focusManager.focusNode('node-2', 'right-pane');
      expect(focusManager.editingPaneId).toBe('right-pane');
      expect(focusManager.editingNodeId).toBe('node-2');
    });

    it('should track pane through type conversion', () => {
      focusManager.focusNode('node', 'pane-1');
      focusManager.focusNodeFromTypeConversion('node', 10, 'pane-2');
      expect(focusManager.editingPaneId).toBe('pane-2');
    });

    it('should track pane through inherited type', () => {
      focusManager.focusNodeFromInheritedType('node', 5, 'special-pane');
      expect(focusManager.editingPaneId).toBe('special-pane');
    });
  });
});
