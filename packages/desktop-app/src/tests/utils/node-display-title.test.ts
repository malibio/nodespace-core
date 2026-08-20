/**
 * Unit tests for resolveTitleOrContent — the shared title-vs-content rule.
 *
 * Consolidation for issue #2012: computeHeaderDisplayValue (design/node-type-predicates.ts)
 * and PluginRegistry.resolveDisplayTitle/getNodeTitle both delegate here, so these tests
 * pin the core rule once rather than being re-verified per call site.
 */

import { describe, it, expect } from 'vitest';
import { resolveTitleOrContent } from '$lib/utils/node-display-title';

describe('resolveTitleOrContent', () => {
  describe('non-template types (content is the source of truth)', () => {
    it('returns content, ignoring a stale title', () => {
      // e.g. a node whose title was computed while it was still `text` (content "/"),
      // then converted to `task` with new content — title was never refreshed.
      expect(resolveTitleOrContent({ title: '/', content: 'Another Task' }, false)).toBe(
        'Another Task'
      );
    });

    it('returns empty string, not a stale title, when content is empty', () => {
      expect(resolveTitleOrContent({ title: '/', content: '' }, false)).toBe('');
    });

    it('does not strip markdown syntax from content', () => {
      // Stripping is a presentation concern for specific callers (e.g. header markdown
      // syntax, formatTabTitle, stripMarkdown) — this function returns the raw value.
      expect(resolveTitleOrContent({ content: '## Heading' }, false)).toBe('## Heading');
    });
  });

  describe('title_template-driven schemas (title is property-computed)', () => {
    it('returns the computed title, ignoring content', () => {
      expect(resolveTitleOrContent({ title: 'Acme Corp — Invoice', content: 'raw' }, true)).toBe(
        'Acme Corp — Invoice'
      );
    });

    it('returns empty rather than falling back to content when title has not resolved', () => {
      expect(resolveTitleOrContent({ title: '', content: 'raw content' }, true)).toBe('');
    });

    it('treats a title with no word characters as unresolved', () => {
      // e.g. "{first_name} {last_name}" with both fields empty resolves to " ".
      expect(resolveTitleOrContent({ title: ' ', content: 'raw' }, true)).toBe('');
      expect(resolveTitleOrContent({ title: ' — ', content: 'raw' }, true)).toBe('');
    });
  });

  it('handles a missing node', () => {
    expect(resolveTitleOrContent(undefined, false)).toBe('');
    expect(resolveTitleOrContent(undefined, true)).toBe('');
    expect(resolveTitleOrContent(null, false)).toBe('');
    expect(resolveTitleOrContent(null, true)).toBe('');
  });
});
