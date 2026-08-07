/**
 * Node type predicates — core vs custom schema type classification.
 *
 * `project` is a built-in core node type (backend core#134), so it must be
 * treated as a core type rather than a custom, UUID-keyed schema type.
 */

import { describe, it, expect } from 'vitest';
import {
  CORE_NODE_TYPES,
  computeHeaderDisplayValue,
  isCustomSchemaType
} from '$lib/design/components/node-type-predicates';

describe('node-type-predicates', () => {
  it('classifies project as a core built-in, like task', () => {
    expect(CORE_NODE_TYPES.has('project')).toBe(true);
    expect(CORE_NODE_TYPES.has('task')).toBe(true);
    expect(isCustomSchemaType('project')).toBe(false);
    expect(isCustomSchemaType('task')).toBe(false);
  });

  it('still treats a UUID-keyed schema type as custom', () => {
    expect(isCustomSchemaType('7b1c2d3e-4f56-7890-abcd-ef1234567890')).toBe(true);
  });
});

describe('computeHeaderDisplayValue', () => {
  describe('non-template types (title is content-derived)', () => {
    it('shows current content, not a stale cached title', () => {
      // The traced repro: `title` was computed for the transient `text`-typed node whose
      // content was a lone `/`, and never refreshed after the conversion to `task`.
      // The unfocused header must still show the content the user actually typed.
      expect(computeHeaderDisplayValue({ title: '/', content: 'Another Task' }, false)).toBe(
        'Another Task'
      );
    });

    it('does not fall back to a stale title when content is empty', () => {
      // An emptied header reads as empty, rather than resurrecting the previous title.
      expect(computeHeaderDisplayValue({ title: '/', content: '' }, false)).toBe('');
    });

    it('strips markdown header syntax from content', () => {
      expect(computeHeaderDisplayValue({ content: '## My Heading' }, false)).toBe('My Heading');
    });
  });

  describe('titleTemplate-driven schemas (title is property-computed)', () => {
    it('shows the computed title, ignoring raw content', () => {
      expect(
        computeHeaderDisplayValue({ title: 'Acme Corp — Invoice', content: 'raw' }, true)
      ).toBe('Acme Corp — Invoice');
    });

    it('renders empty rather than leaking content when the title has not resolved', () => {
      // The read-only header shows the titleTemplate placeholder in this state,
      // matching node-row.svelte's inline rendering.
      expect(computeHeaderDisplayValue({ title: '', content: 'raw content' }, true)).toBe('');
    });
  });

  it('handles a missing node', () => {
    expect(computeHeaderDisplayValue(undefined, false)).toBe('');
    expect(computeHeaderDisplayValue(undefined, true)).toBe('');
    // currentViewedNode is null before a node resolves
    expect(computeHeaderDisplayValue(null, false)).toBe('');
    expect(computeHeaderDisplayValue(null, true)).toBe('');
  });
});
