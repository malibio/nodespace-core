/**
 * Generic schema form field resolution.
 *
 * The generic form serves two property storage shapes: core types that namespace their
 * schema fields under `properties[nodeType]` (project, task) and user-defined schema types
 * that store them flat. Resolution must mirror the backend's nested-first, flat-fallback
 * order so one form renders both.
 */

import { describe, it, expect } from 'vitest';
import {
  resolveFieldValue,
  buildFieldWrite
} from '$lib/components/schema/schema-field-resolution';

describe('resolveFieldValue', () => {
  it('reads a namespaced field for a core type', () => {
    const node = {
      nodeType: 'project',
      properties: { project: { status: 'planning', priority: 'high' } }
    };

    expect(resolveFieldValue(node, 'status')).toBe('planning');
    expect(resolveFieldValue(node, 'priority')).toBe('high');
  });

  it('reads a flat field for a user-defined schema type', () => {
    const node = {
      nodeType: '7b1c2d3e-4f56-7890-abcd-ef1234567890',
      properties: { capacity: 250 }
    };

    expect(resolveFieldValue(node, 'capacity')).toBe(250);
  });

  it('prefers the namespaced value over a stale flat one', () => {
    // Matches the backend's precedence: a normalized namespaced value wins over a flat
    // value left behind by a pre-normalization write.
    const node = {
      nodeType: 'project',
      properties: { status: 'archived', project: { status: 'active' } }
    };

    expect(resolveFieldValue(node, 'status')).toBe('active');
  });

  it('falls back to flat when the namespace lacks the field', () => {
    // A namespaced type whose value has not been hoisted yet (e.g. just written flat by
    // this form) must still render.
    const node = {
      nodeType: 'project',
      properties: { project: { status: 'active' }, end_date: '2026-01-31' }
    };

    expect(resolveFieldValue(node, 'end_date')).toBe('2026-01-31');
  });

  it('returns null for an unset field in either shape', () => {
    expect(resolveFieldValue({ nodeType: 'project', properties: { project: {} } }, 'status')).toBe(
      null
    );
    expect(resolveFieldValue({ nodeType: 'project', properties: {} }, 'status')).toBe(null);
    expect(resolveFieldValue({ nodeType: 'project' }, 'status')).toBe(null);
  });

  it('ignores a non-object value sitting at the namespace key', () => {
    // A flat field that happens to share the type's name must not be mistaken for a namespace.
    const node = { nodeType: 'project', properties: { project: 'Some project name' } };

    expect(resolveFieldValue(node, 'status')).toBe(null);
  });
});

describe('buildFieldWrite', () => {
  it('writes into the namespace for a namespaced type', () => {
    // The critical case: a flat write here would be stored as a top-level sibling and then
    // silently discarded on read, because the backend skips hoisting when the namespace
    // already exists and returns only the namespace contents.
    const node = {
      nodeType: 'project',
      properties: { project: { status: 'planning', priority: 'high' } }
    };

    expect(buildFieldWrite(node, 'status', 'active')).toEqual({
      project: { status: 'active', priority: 'high' }
    });
  });

  it('preserves sibling namespaces and unrelated keys', () => {
    const node = {
      nodeType: 'project',
      properties: { project: { status: 'planning' }, task: { status: 'open' } }
    };

    expect(buildFieldWrite(node, 'status', 'active')).toEqual({
      project: { status: 'active' },
      task: { status: 'open' }
    });
  });

  it('writes flat for a user-defined schema type', () => {
    const node = {
      nodeType: '7b1c2d3e-4f56-7890-abcd-ef1234567890',
      properties: { capacity: 250 }
    };

    expect(buildFieldWrite(node, 'capacity', 400)).toEqual({ capacity: 400 });
  });

  it('writes flat when the node has no properties yet', () => {
    expect(buildFieldWrite({ nodeType: 'project' }, 'status', 'active')).toEqual({
      status: 'active'
    });
  });

  it('does not mutate the original properties', () => {
    const properties = { project: { status: 'planning' } };
    const node = { nodeType: 'project', properties };

    buildFieldWrite(node, 'status', 'active');

    expect(properties.project.status).toBe('planning');
  });

  it('round-trips with resolveFieldValue in both shapes', () => {
    const namespaced = { nodeType: 'project', properties: { project: { status: 'planning' } } };
    const flat = { nodeType: 'venue-uuid', properties: { capacity: 100 } };

    expect(
      resolveFieldValue(
        { ...namespaced, properties: buildFieldWrite(namespaced, 'status', 'active') },
        'status'
      )
    ).toBe('active');
    expect(
      resolveFieldValue({ ...flat, properties: buildFieldWrite(flat, 'capacity', 250) }, 'capacity')
    ).toBe(250);
  });
});
