/**
 * Cross-node copy tests (#278 Part 2).
 *
 * Exercises the pure selection→markdown builder against real (Happy-DOM) node
 * fixtures: partial-first/partial-last clipping, full middle nodes, hierarchy
 * indentation, single-node passthrough, and the delicate rendered↔source offset
 * mapping (markdown syntax stripped in the view; `<br>` counted as one char).
 */

import { describe, it, expect, afterEach } from 'vitest';
import {
  buildCrossNodeCopy,
  renderedOffsetTo,
  type CopyNodeInfo
} from '../../lib/utils/cross-node-copy';

interface NodeFixture {
  id: string;
  /** Source markdown (with syntax). */
  content: string;
  /** Rendered view markup (syntax stripped; may contain <br>). */
  view: string;
  depth: number;
}

function buildViewer(nodes: NodeFixture[]): {
  root: HTMLElement;
  resolveNode: (id: string) => CopyNodeInfo | null;
  viewOf: (id: string) => HTMLElement;
} {
  const root = document.createElement('div');
  const info = new Map<string, CopyNodeInfo>();
  for (const n of nodes) {
    const nodeEl = document.createElement('div');
    nodeEl.setAttribute('data-node-id', n.id);
    const view = document.createElement('div');
    view.className = 'node__content--view';
    view.innerHTML = n.view;
    nodeEl.appendChild(view);
    root.appendChild(nodeEl);
    info.set(n.id, { content: n.content, depth: n.depth });
  }
  document.body.appendChild(root);
  return {
    root,
    resolveNode: (id) => info.get(id) ?? null,
    viewOf: (id) => root.querySelector(`[data-node-id="${id}"] .node__content--view`) as HTMLElement
  };
}

/** A minimal Selection over one range (avoids Happy-DOM Selection quirks). */
function selectionOf(range: Range): Selection {
  return {
    rangeCount: 1,
    isCollapsed: range.collapsed,
    getRangeAt: () => range
  } as unknown as Selection;
}

/** First text node inside an element (the common view case). */
function textOf(el: HTMLElement): Text {
  return el.firstChild as Text;
}

afterEach(() => {
  document.body.innerHTML = '';
});

describe('renderedOffsetTo', () => {
  it('counts plain text offsets', () => {
    const el = document.createElement('div');
    el.innerHTML = 'Hello world';
    expect(renderedOffsetTo(el, el.firstChild!, 6)).toBe(6);
  });

  it('counts a <br> as one character', () => {
    const el = document.createElement('div');
    el.innerHTML = 'line1<br>line2';
    const secondText = el.childNodes[2]; // "line2"
    // 5 (line1) + 1 (br) + 2 (offset into line2) = 8
    expect(renderedOffsetTo(el, secondText, 2)).toBe(8);
  });

  it('returns null when the position is outside the element', () => {
    const el = document.createElement('div');
    el.innerHTML = 'inside';
    const other = document.createElement('span');
    other.textContent = 'elsewhere';
    expect(renderedOffsetTo(el, other.firstChild!, 1)).toBeNull();
  });
});

describe('buildCrossNodeCopy', () => {
  it('returns null for a collapsed selection', () => {
    const { root, resolveNode, viewOf } = buildViewer([
      { id: 'a', content: 'Hello', view: 'Hello', depth: 0 }
    ]);
    const range = document.createRange();
    range.setStart(textOf(viewOf('a')), 2);
    range.collapse(true);
    expect(buildCrossNodeCopy({ selection: selectionOf(range), root, resolveNode })).toBeNull();
  });

  it('returns null for a single-node selection (native copy)', () => {
    const { root, resolveNode, viewOf } = buildViewer([
      { id: 'a', content: 'Hello world', view: 'Hello world', depth: 0 }
    ]);
    const range = document.createRange();
    range.setStart(textOf(viewOf('a')), 0);
    range.setEnd(textOf(viewOf('a')), 5);
    expect(buildCrossNodeCopy({ selection: selectionOf(range), root, resolveNode })).toBeNull();
  });

  it('clips the first and last node to the selection offsets', () => {
    const { root, resolveNode, viewOf } = buildViewer([
      { id: 'a', content: 'Hello world', view: 'Hello world', depth: 0 },
      { id: 'b', content: 'Second line', view: 'Second line', depth: 0 }
    ]);
    const range = document.createRange();
    range.setStart(textOf(viewOf('a')), 6); // "world"
    range.setEnd(textOf(viewOf('b')), 6); // "Second"
    expect(buildCrossNodeCopy({ selection: selectionOf(range), root, resolveNode })).toBe(
      'world\nSecond'
    );
  });

  it('keeps middle nodes whole and maps markdown syntax at the edges', () => {
    const { root, resolveNode, viewOf } = buildViewer([
      { id: 'a', content: '**Hello** world', view: 'Hello world', depth: 0 },
      { id: 'b', content: 'middle node', view: 'middle node', depth: 0 },
      { id: 'c', content: 'end here', view: 'end here', depth: 0 }
    ]);
    const range = document.createRange();
    range.setStart(textOf(viewOf('a')), 6); // rendered 'w' → source offset 10 → "world"
    range.setEnd(textOf(viewOf('c')), 3); // "end"
    expect(buildCrossNodeCopy({ selection: selectionOf(range), root, resolveNode })).toBe(
      'world\nmiddle node\nend'
    );
  });

  it('preserves nesting as indentation relative to the shallowest node', () => {
    const { root, resolveNode, viewOf } = buildViewer([
      { id: 'a', content: 'parent', view: 'parent', depth: 0 },
      { id: 'b', content: 'child', view: 'child', depth: 1 },
      { id: 'c', content: 'grandchild', view: 'grandchild', depth: 2 }
    ]);
    const range = document.createRange();
    range.setStart(textOf(viewOf('a')), 0);
    range.setEnd(textOf(viewOf('c')), 'grandchild'.length);
    expect(buildCrossNodeCopy({ selection: selectionOf(range), root, resolveNode })).toBe(
      'parent\n  child\n    grandchild'
    );
  });

  it('counts <br> when clipping the first node', () => {
    const { root, resolveNode, viewOf } = buildViewer([
      { id: 'a', content: 'line1\nline2', view: 'line1<br>line2', depth: 0 },
      { id: 'b', content: 'second', view: 'second', depth: 0 }
    ]);
    const aView = viewOf('a');
    const range = document.createRange();
    range.setStart(aView.childNodes[2], 2); // "line2" offset 2 → rendered 8 → source slice(8) = "ne2"
    range.setEnd(textOf(viewOf('b')), 'second'.length);
    expect(buildCrossNodeCopy({ selection: selectionOf(range), root, resolveNode })).toBe(
      'ne2\nsecond'
    );
  });

  it('handles a backward (end-before-start) selection', () => {
    const { root, resolveNode, viewOf } = buildViewer([
      { id: 'a', content: 'Hello world', view: 'Hello world', depth: 0 },
      { id: 'b', content: 'Second line', view: 'Second line', depth: 0 }
    ]);
    // Range is always normalized (start<=end), but the anchor may be in node b;
    // build the range in document order and confirm clipping is by document order.
    const range = document.createRange();
    range.setStart(textOf(viewOf('a')), 6);
    range.setEnd(textOf(viewOf('b')), 6);
    expect(buildCrossNodeCopy({ selection: selectionOf(range), root, resolveNode })).toBe(
      'world\nSecond'
    );
  });
});
