import { describe, it, expect } from 'vitest';
import {
  buildRelationshipsView,
  groupDisplayLabel,
  groupEdgeColumns,
  groupLayout,
  groupSupportsEdgeEditing,
  partitionGroups,
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
    // Mutation-driving fields carried onto the view model.
    expect(assigned?.relationshipName).toBe('assigned_to');
    expect(assigned?.required).toBe(false);
    expect(assigned?.edgeFields).toEqual([{ name: 'role', type: 'string' }]);
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

  it('retains a group with no related nodes but reports isEmpty', () => {
    const raw: RawNodeRelationships = {
      nodeId: 'task-1',
      nodeType: 'task',
      groups: [makeGroup({ related: [], count: 0 })]
    };
    const view = buildRelationshipsView(raw);
    // The empty declared group is kept so the editable modal can add the first
    // edge, but nothing is populated so the read-only view still counts as empty.
    expect(view.groups).toHaveLength(1);
    expect(view.groups[0].rows).toHaveLength(0);
    expect(view.isEmpty).toBe(true);
  });

  it('retains empty declared groups alongside populated ones and is not empty', () => {
    const raw: RawNodeRelationships = {
      nodeId: 'task-1',
      nodeType: 'task',
      groups: [
        makeGroup({
          relationshipName: 'assigned_to',
          direction: 'out',
          targetType: 'person',
          edgeFields: [{ name: 'role', type: 'string' }],
          count: 0,
          related: []
        }),
        makeGroup({
          relationshipName: 'belongs_to',
          direction: 'out',
          targetType: 'project',
          count: 1,
          related: [
            { id: 'pr1', nodeType: 'project', title: 'Apollo', contentPreview: '', edgeProperties: {} }
          ]
        })
      ]
    };
    const view = buildRelationshipsView(raw);
    expect(view.groups).toHaveLength(2);
    expect(view.isEmpty).toBe(false);
    const assigned = view.groups.find((g) => g.key === 'out:assigned_to:person');
    expect(assigned?.rows).toHaveLength(0);
    // Declared edge fields still classify the empty group as a table.
    expect(assigned?.layout).toBe('table');
  });

  it('carries relationshipName, required, and edgeFields onto the group view', () => {
    const raw: RawNodeRelationships = {
      nodeId: 'task-1',
      nodeType: 'task',
      groups: [
        makeGroup({
          relationshipName: 'assigned_to',
          direction: 'out',
          targetType: 'person',
          required: true,
          edgeFields: [
            { name: 'role', type: 'string' },
            { name: 'weight', type: 'number' }
          ],
          count: 1,
          related: [
            { id: 'p1', nodeType: 'person', title: 'Sarah', contentPreview: '', edgeProperties: { role: 'lead' } }
          ]
        })
      ]
    };
    const view = buildRelationshipsView(raw);
    const group = view.groups[0];
    expect(group.relationshipName).toBe('assigned_to');
    expect(group.required).toBe(true);
    expect(group.edgeFields).toEqual([
      { name: 'role', type: 'string' },
      { name: 'weight', type: 'number' }
    ]);
  });

  it('defaults required to false and edgeFields to [] when the group omits them', () => {
    const raw: RawNodeRelationships = {
      nodeId: 'task-1',
      nodeType: 'task',
      groups: [
        makeGroup({
          relationshipName: 'belongs_to',
          required: null,
          edgeFields: null,
          count: 1,
          related: [
            { id: 'pr1', nodeType: 'project', title: 'Apollo', contentPreview: '', edgeProperties: {} }
          ]
        })
      ]
    };
    const view = buildRelationshipsView(raw);
    expect(view.groups[0].required).toBe(false);
    expect(view.groups[0].edgeFields).toEqual([]);
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

describe('relationship-grouping: partitionGroups', () => {
  const relatedNode = (id: string) => ({
    id,
    nodeType: 'adr',
    title: id,
    contentPreview: '',
    edgeProperties: {}
  });

  function viewOf(groups: RawRelationshipGroup[]) {
    return buildRelationshipsView({ nodeId: 'adr-1', nodeType: 'adr', groups }).groups;
  }

  it('gives a section only to groups that have edges, in either direction', () => {
    const groups = viewOf([
      makeGroup({ relationshipName: 'supersedes', count: 1, related: [relatedNode('adr-2')] }),
      makeGroup({
        relationshipName: 'supersedes',
        direction: 'in',
        reverseName: 'superseded_by',
        count: 1,
        related: [relatedNode('adr-9')]
      })
    ]);
    const { populated, addable } = partitionGroups(groups);
    expect(populated).toHaveLength(2);
    expect(addable).toHaveLength(0);
  });

  it('folds empty OUTBOUND groups into the add chooser rather than sections', () => {
    const groups = viewOf([
      makeGroup({ relationshipName: 'supersedes' }),
      makeGroup({ relationshipName: 'depends_on' })
    ]);
    const { populated, addable } = partitionGroups(groups);
    expect(populated).toHaveLength(0);
    expect(addable.map((g) => g.relationshipName)).toEqual(['supersedes', 'depends_on']);
  });

  it('drops empty INBOUND groups entirely — they have no Add to justify a section', () => {
    const groups = viewOf([
      makeGroup({
        relationshipName: 'supersedes',
        direction: 'in',
        reverseName: 'superseded_by'
      })
    ]);
    const { populated, addable } = partitionGroups(groups);
    expect(populated).toHaveLength(0);
    expect(addable).toHaveLength(0);
  });

  it('renders the six-declared-relationships-no-edges case as zero sections and one chooser', () => {
    // The issue's benchmark: an `adr` with four outbound and two inbound
    // declared relationships and no edges must not produce six empty sections.
    const groups = viewOf([
      makeGroup({ relationshipName: 'supersedes' }),
      makeGroup({ relationshipName: 'depends_on' }),
      makeGroup({ relationshipName: 'implements' }),
      makeGroup({ relationshipName: 'authored_by' }),
      makeGroup({ relationshipName: 'supersedes', direction: 'in', reverseName: 'superseded_by' }),
      makeGroup({ relationshipName: 'relates_to', direction: 'in', reverseName: 'related_from' })
    ]);
    const { populated, addable } = partitionGroups(groups);
    expect(populated).toHaveLength(0);
    expect(addable).toHaveLength(4);
  });

  it('offers a populated outbound group its own section AND keeps other empty types addable', () => {
    const groups = viewOf([
      makeGroup({ relationshipName: 'supersedes', count: 1, related: [relatedNode('adr-2')] }),
      makeGroup({ relationshipName: 'depends_on' })
    ]);
    const { populated, addable } = partitionGroups(groups);
    expect(populated.map((g) => g.relationshipName)).toEqual(['supersedes']);
    expect(addable.map((g) => g.relationshipName)).toEqual(['depends_on']);
  });
});

describe('relationship-grouping: groupSupportsEdgeEditing', () => {
  function groupView(overrides: Partial<RawRelationshipGroup>) {
    return buildRelationshipsView({
      nodeId: 'n-1',
      nodeType: 'collection',
      groups: [makeGroup(overrides)]
    }).groups[0];
  }

  it('is true for an outbound group whose schema declares edge fields', () => {
    const group = groupView({
      edgeFields: [{ name: 'access', type: 'string' }]
    });
    expect(groupSupportsEdgeEditing(group)).toBe(true);
  });

  it('is false when the schema declares no edge fields — nothing to edit', () => {
    expect(groupSupportsEdgeEditing(groupView({ edgeFields: null }))).toBe(false);
    expect(groupSupportsEdgeEditing(groupView({ edgeFields: [] }))).toBe(false);
  });

  it('is false for an inbound group even when it declares edge fields', () => {
    // The edge belongs to the other node's schema; it is edited from there.
    const group = groupView({
      direction: 'in',
      reverseName: 'members',
      edgeFields: [{ name: 'access', type: 'string' }]
    });
    expect(groupSupportsEdgeEditing(group)).toBe(false);
  });

  it('is false for ad-hoc edge keys with no declared field to render an input from', () => {
    const group = groupView({
      edgeFields: null,
      count: 1,
      related: [
        {
          id: 'p-1',
          nodeType: 'person',
          title: 'Ada',
          contentPreview: '',
          edgeProperties: { note: 'ad-hoc' }
        }
      ]
    });
    // The group still renders as a table (it has edge data) but offers no editor.
    expect(group.layout).toBe('table');
    expect(groupSupportsEdgeEditing(group)).toBe(false);
  });
});
