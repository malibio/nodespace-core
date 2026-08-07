import { describe, it, expect } from 'vitest';
import { resolveEdgeEndpoints } from '$lib/services/relationship-grouping';

/**
 * Orientation is the crux of correct relationship mutations: an edge is always
 * stored source→target, but a group displayed on a node can face either way.
 * These tests pin the mapping the create/delete/update calls depend on.
 */
describe('relationship orientation: resolveEdgeEndpoints', () => {
  const nodeId = 'node-A';
  const otherId = 'node-B';

  it("stores an OUTbound edge as source=this node, target=the other node", () => {
    expect(resolveEdgeEndpoints(nodeId, 'out', otherId)).toEqual({
      sourceId: 'node-A',
      targetId: 'node-B'
    });
  });

  it("stores an INbound edge as source=the other node, target=this node", () => {
    expect(resolveEdgeEndpoints(nodeId, 'in', otherId)).toEqual({
      sourceId: 'node-B',
      targetId: 'node-A'
    });
  });

  it('is direction-symmetric: out and in swap source/target for the same pair', () => {
    const out = resolveEdgeEndpoints(nodeId, 'out', otherId);
    const inn = resolveEdgeEndpoints(nodeId, 'in', otherId);
    expect(out.sourceId).toBe(inn.targetId);
    expect(out.targetId).toBe(inn.sourceId);
  });
});
