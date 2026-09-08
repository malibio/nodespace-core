/**
 * Tests for FocusManager cursor positioning refactor
 *
 * Verifies unified cursor position state and API methods
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { focusManager } from '$lib/services/focus-manager.svelte';

describe('FocusManager - Cursor Positioning', () => {
  beforeEach(() => {
    // Clear state before each test
    focusManager.clearEditing();
  });

  describe('Unified API', () => {
    it('focusNode should set default cursor position', () => {
      focusManager.focusNode('test-node-1', 'default');

      expect(focusManager.editingNodeId).toBe('test-node-1');
      expect(focusManager.cursorPosition).toEqual({ type: 'default', skipSyntax: true });
    });

    it('focusNodeAtPosition should set absolute cursor position', () => {
      focusManager.focusNodeAtPosition('test-node-2', 42, 'default');

      expect(focusManager.editingNodeId).toBe('test-node-2');
      expect(focusManager.cursorPosition).toEqual({ type: 'absolute', position: 42 });
    });

    it('focusNodeAtLine should set line-column cursor position', () => {
      focusManager.focusNodeAtLine('test-node-3', 'default', 2, false);

      expect(focusManager.editingNodeId).toBe('test-node-3');
      expect(focusManager.cursorPosition).toEqual({
        type: 'line-column',
        line: 2,
        skipSyntax: false
      });
    });

    it('focusNodeAtLine should default to line 0 with skipSyntax true', () => {
      focusManager.focusNodeAtLine('test-node-4', 'default');

      expect(focusManager.editingNodeId).toBe('test-node-4');
      expect(focusManager.cursorPosition).toEqual({
        type: 'line-column',
        line: 0,
        skipSyntax: true
      });
    });

    it('focusNodeFromArrowNav should set arrow navigation cursor position', () => {
      focusManager.focusNodeFromArrowNav('test-node-5', 'down', 150, 'default');

      expect(focusManager.editingNodeId).toBe('test-node-5');
      expect(focusManager.cursorPosition).toEqual({
        type: 'arrow-navigation',
        direction: 'down',
        pixelOffset: 150
      });
    });

    it('focusNodeFromTypeConversion should set node-type-conversion cursor position', () => {
      focusManager.focusNodeFromTypeConversion('test-node-6', 25, 'default');

      expect(focusManager.editingNodeId).toBe('test-node-6');
      expect(focusManager.cursorPosition).toEqual({
        type: 'node-type-conversion',
        position: 25
      });
    });

    it('focusNodeFromInheritedType should set inherited-type cursor position', () => {
      focusManager.focusNodeFromInheritedType('test-node-7', 10, 'default');

      expect(focusManager.editingNodeId).toBe('test-node-7');
      expect(focusManager.cursorPosition).toEqual({
        type: 'inherited-type',
        position: 10
      });
    });
  });

  describe('clearCursorPosition', () => {
    it('should clear unified cursor position', () => {
      focusManager.focusNodeAtPosition('test-node', 50, 'default');

      expect(focusManager.cursorPosition).toEqual({ type: 'absolute', position: 50 });

      focusManager.clearCursorPosition();

      expect(focusManager.cursorPosition).toBeNull();
    });
  });

  describe('clearEditing', () => {
    it('should clear all state including cursor position', () => {
      focusManager.focusNodeFromArrowNav('test-node', 'down', 100, 'default');

      expect(focusManager.editingNodeId).toBe('test-node');
      expect(focusManager.cursorPosition).not.toBeNull();

      focusManager.clearEditing();

      expect(focusManager.editingNodeId).toBeNull();
      expect(focusManager.cursorPosition).toBeNull();
    });
  });

  describe('getCurrentState', () => {
    it('should return current focus state with cursor position', () => {
      focusManager.focusNodeAtPosition('state-test', 30, 'default');

      const state = focusManager.getCurrentState();

      expect(state.nodeId).toBe('state-test');
      expect(state.cursorPosition).toEqual({ type: 'absolute', position: 30 });
    });
  });

  describe('State transitions', () => {
    it('should properly transition between different cursor position types', () => {
      // Start with default
      focusManager.focusNode('node-1', 'default');
      expect(focusManager.cursorPosition).toEqual({ type: 'default', skipSyntax: true });

      // Change to absolute
      focusManager.focusNodeAtPosition('node-1', 10, 'default');
      expect(focusManager.cursorPosition).toEqual({ type: 'absolute', position: 10 });

      // Change to arrow navigation
      focusManager.focusNodeFromArrowNav('node-1', 'up', 50, 'default');
      expect(focusManager.cursorPosition).toEqual({
        type: 'arrow-navigation',
        direction: 'up',
        pixelOffset: 50
      });

      // Change to line-column
      focusManager.focusNodeAtLine('node-1', 'default', 3, true);
      expect(focusManager.cursorPosition).toEqual({
        type: 'line-column',
        line: 3,
        skipSyntax: true
      });
    });

    it('should clear cursor position when focusing different node', () => {
      focusManager.focusNodeAtPosition('node-1', 20, 'default');
      expect(focusManager.cursorPosition?.type).toBe('absolute');

      // Focus different node with default position
      focusManager.focusNode('node-2', 'default');

      expect(focusManager.editingNodeId).toBe('node-2');
      expect(focusManager.cursorPosition).toEqual({ type: 'default', skipSyntax: true });
    });
  });

  describe('Reactivity and state consistency', () => {
    it('should update editingNodeId when focusing', () => {
      focusManager.focusNodeAtPosition('test', 15, 'default');
      expect(focusManager.editingNodeId).toBe('test');
      expect(focusManager.cursorPosition).toEqual({ type: 'absolute', position: 15 });

      focusManager.focusNodeFromArrowNav('test', 'down', 75, 'default');
      expect(focusManager.cursorPosition).toEqual({
        type: 'arrow-navigation',
        direction: 'down',
        pixelOffset: 75
      });

      focusManager.focusNode('test', 'default');
      expect(focusManager.cursorPosition?.type).toBe('default');
    });
  });
});
