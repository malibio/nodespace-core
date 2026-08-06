import { describe, it, expect } from 'vitest';
import {
  buildRelationshipsView,
  groupDisplayLabel,
  groupEdgeColumns,
  groupLayout,
  humanizeName,
  type RawNodeRelationships,
  type RawRelationshipGroup
} from '$lib/services/relationship-grouping';

function makeGroup(overrides: Partial<RawRelationshipGroup> = {}): RawRelationshipGroup {
  return {
    relationshipName: 'assigned_to',
    direction: 'out',
    targetType: 'person',
    reverseName: null,
    sourceType: 'task',
    cardinality: 'many',
    required: null,
    edgeFields: null,
    description: null,
    related: [],
    count: 0,
    ...overrides
  };
}

describe('relationship-grouping: humanizeName', () => {
  it('title-cases snake and kebab identifiers', () => {
    expect(humanizeName('assigned_to')).toBe('Assigned To');
    expect(humanizeName('billed-to')).toBe('Billed To');
    expect(humanizeName('tasks')).toBe('Tasks');
  });

  it('collapses repeated separators and trims', () => {
    expect(humanizeName('__blocked__by__')).toBe('Blocked By');
  });
});

describe('relationship-grouping: groupDisplayLabel', () => {
  it('uses the humanized relationship name for outbound groups', () => {
    expect(groupDisplayLabel(makeGroup({ direction: 'out', relationshipName: 'assigned_to' }))).toBe(
      'Assigned To'
    );
  });

  it('uses reverseName for inbound groups when present', () => {
    const group = makeGroup({
      direction: 'in',
      relationshipName: 'assigned_to',
      reverseName: 'tasks',
      sourceType: 'task'
    });
    expect(groupDisplayLabel(group)).toBe('Tasks');
  });

  it('falls back to "{SourceType} ({Relationship Name})" when reverseName is absent', () => {
    const group = makeGroup({
      direction: 'in',
      relationshipName: 'assigned_to',
      reverseName: null,
      sourceType: 'task'
    });
    expect(groupDisplayLabel(group)).toBe('Task (Assigned To)');
  });
});

describe('relationship-grouping: groupLayout', () => {
  it('is a table when the schema declares edge fields', () => {
    const group = makeGroup({
      edgeFields: [{ name: 'role', type: 'string' }],
      related: [{ id: 'p1', nodeType: 'person', title: 'Sarah', contentPreview: '', edgeProperties: {} }]
    });
    expect(groupLayout(group)).toBe('table');
  });

  it('is a table when an edge carries properties even without declared fields', () => {
    const group = makeGroup({
      edgeFields: null,
      related: [
        { id: 'p1', nodeType: 'person', title: 'Sarah', contentPreview: '', edgeProperties: { role: 'lead' } }
      ]
    });
    expect(groupLayout(group)).toBe('table');
  });

  it('is chips for a bare relationship with no edge data', () => {
    const group = makeGroup({
      edgeFields: null,
      related: [{ id: 'p1', nodeType: 'person', title: 'Sarah', contentPreview: '', edgeProperties: {} }]
    });
    expect(groupLayout(group)).toBe('chips');
  });
});

describe('relationship-grouping: groupEdgeColumns', () => {
  it('lists declared edge fields first, then undeclared edge keys', () => {
    const group = makeGroup({
      edgeFields: [
        { name: 'role', type: 'string' },
        { name: 'assigned_at', type: 'date' }
      ],
      related: [
        {
          id: 'p1',
          nodeType: 'person',
          title: 'Sarah',
          contentPreview: '',
          edgeProperties: { role: 'lead', priority: 'high' }
        }
      ]
    });
    expect(groupEdgeColumns(group)).toEqual(['role', 'assigned_at', 'priority']);
  });

  it('returns no columns for a chips group', () => {
    const group = makeGroup({ edgeFields: null, related: [] });
    expect(groupEdgeColumns(group)).toEqual([]);
  });
});

describe('relationship-grouping: buildRelationshipsView', () => {
  it('keeps outbound and inbound as separate groups and classifies each', () => {
    const raw: RawNodeRelationships = {
      nodeId: 'task-1',
      nodeType: 'task',
      groups: [
        makeGroup({
          relationshipName: 'assigned_to',
          direction: 'out',
          targetType: 'person',
          edgeFields: [{ name: 'role', type: 'string' }],
          count: 1,
          related: [
            {
              id: 'p1',
              nodeType: 'person',
              title: 'Sarah Chen',
              contentPreview: '',
              edgeProperties: { role: 'lead' }
            }
          ]
        }),
        makeGroup({
          relationshipName: 'belongs_to',
          direction: 'out',
          targetType: 'project',
          edgeFields: null,
          count: 1,
          related: [
            { id: 'pr1', nodeType: 'project', title: 'Apollo', contentPreview: '', edgeProperties: {} }
          ]
        })
      ]
    };

    const view = buildRelationshipsView(raw);
    expect(view.isEmpty).toBe(false);
    expect(view.groups).toHaveLength(2);

    const assigned = view.groups.find((g) => g.key === 'out:assigned_to:person');
    expect(assigned?.layout).toBe('table');
    expect(assigned?.edgeColumns).toEqual(['role']);
    expect(assigned?.rows[0]).toEqual({
      id: 'p1',
      nodeType: 'person',
      label: 'Sarah Chen',
      edgeValues: { role: 'lead' }
    });

    const belongs = view.groups.find((g) => g.key === 'out:belongs_to:project');
    expect(belongs?.layout).toBe('chips');
    expect(belongs?.edgeColumns).toEqual([]);
  });

  it('drops groups with no related nodes and reports isEmpty', () => {
    const raw: RawNodeRelationships = {
      nodeId: 'task-1',
      nodeType: 'task',
      groups: [makeGroup({ related: [], count: 0 })]
    };
    const view = buildRelationshipsView(raw);
    expect(view.groups).toHaveLength(0);
    expect(view.isEmpty).toBe(true);
  });

  it('labels an inbound group by reverseName and falls back to the row id when a target has no title', () => {
    const raw: RawNodeRelationships = {
      nodeId: 'person-1',
      nodeType: 'person',
      groups: [
        makeGroup({
          relationshipName: 'assigned_to',
          direction: 'in',
          reverseName: 'tasks',
          sourceType: 'task',
          targetType: 'task',
          count: 1,
          related: [{ id: 't-42', nodeType: 'task', title: null, contentPreview: '', edgeProperties: {} }]
        })
      ]
    };
    const view = buildRelationshipsView(raw);
    expect(view.groups[0].label).toBe('Tasks');
    expect(view.groups[0].rows[0].label).toBe('t-42');
  });
});
