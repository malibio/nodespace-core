/**
 * Node type predicates — core vs custom schema type classification.
 *
 * `project` is a built-in core node type (backend core#134), so it must be
 * treated as a core type rather than a custom, UUID-keyed schema type.
 */

import { describe, it, expect } from 'vitest';
import { CORE_NODE_TYPES, isCustomSchemaType } from '$lib/design/components/node-type-predicates';

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
