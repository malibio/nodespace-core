/**
 * Relationship viewer — pure grouping/normalization (issue #1918, read-only slice).
 *
 * Turns the `get_node_relationships` command payload (see
 * `rel_ops::NodeRelationshipsOutput` on the Rust side) into the view model the
 * modal renders. Kept free of any Tauri/DOM imports so it is unit-testable in
 * isolation (see `src/tests/unit/relationship-grouping.test.ts`).
 *
 * Responsibilities:
 * - Keep outbound and inbound as SEPARATE groups (never flatten different
 *   directions / target types together).
 * - Derive each group's display label — inbound uses `reverseName`, falling back
 *   to `"{SourceType} ({relationship_name})"` when it is absent, rather than
 *   hiding the group.
 * - Classify each group as a `table` (carries edge attributes) or `chips`
 *   (bare edge with no edge data) layout.
 * - Compute the ordered edge-attribute column set for the table layout.
 */

export type RelationshipDirection = 'out' | 'in';
export type RelationshipCardinality = 'one' | 'many';
export type RelationshipLayout = 'table' | 'chips';

/** An edge field definition as declared on the schema relationship. */
export interface RawEdgeField {
  name: string;
  type: string;
  indexed?: boolean;
  required?: boolean;
  default?: unknown;
  targetType?: string;
  description?: string;
}

/** A related node plus the connecting edge's stored properties. */
export interface RawRelatedNode {
  id: string;
  nodeType: string;
  title: string | null;
  contentPreview: string;
  edgeProperties: Record<string, unknown>;
}

/** One relationship group as returned by the command (per name + direction). */
export interface RawRelationshipGroup {
  relationshipName: string;
  direction: RelationshipDirection;
  targetType: string | null;
  reverseName: string | null;
  sourceType: string;
  cardinality: RelationshipCardinality | null;
  required: boolean | null;
  edgeFields: RawEdgeField[] | null;
  description: string | null;
  related: RawRelatedNode[];
  count: number;
}

/** The full command payload. */
export interface RawNodeRelationships {
  nodeId: string;
  nodeType: string;
  groups: RawRelationshipGroup[];
}

/** A single row (target + edge attribute values) in a group. */
export interface RelationshipRowView {
  id: string;
  nodeType: string;
  /** Display label: current title, else content preview, else the id. */
  label: string;
  /** Edge attribute values keyed by field name. */
  edgeValues: Record<string, unknown>;
}

/** A relationship group ready to render. */
export interface RelationshipGroupView {
  /** Stable, unique key across name + direction + target type. */
  key: string;
  /** Human-readable heading for the group. */
  label: string;
  direction: RelationshipDirection;
  targetType: string | null;
  cardinality: RelationshipCardinality | null;
  description: string | null;
  /** `table` when the group carries edge attributes, else `chips`. */
  layout: RelationshipLayout;
  /** Ordered edge-attribute column names (empty for the `chips` layout). */
  edgeColumns: string[];
  rows: RelationshipRowView[];
  count: number;
}

/** The view model consumed by the modal. */
export interface NodeRelationshipsView {
  nodeId: string;
  nodeType: string;
  groups: RelationshipGroupView[];
  /** True when the node has no typed relationships to show. */
  isEmpty: boolean;
}

/**
 * Turn a snake/kebab identifier into a Title Case label
 * (`assigned_to` → `Assigned To`).
 */
export function humanizeName(name: string): string {
  return name
    .replace(/[_-]+/g, ' ')
    .trim()
    .split(/\s+/)
    .filter((word) => word.length > 0)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ');
}

/**
 * Display label for a group.
 * - Outbound: the humanized relationship name.
 * - Inbound: the humanized `reverseName` when present, else a derived
 *   `"{SourceType} ({Relationship Name})"` fallback — never hide the group,
 *   since that would make the edge invisible from one side.
 */
export function groupDisplayLabel(group: RawRelationshipGroup): string {
  if (group.direction === 'out') {
    return humanizeName(group.relationshipName);
  }
  if (group.reverseName && group.reverseName.trim().length > 0) {
    return humanizeName(group.reverseName);
  }
  return `${humanizeName(group.sourceType)} (${humanizeName(group.relationshipName)})`;
}

/**
 * A group renders as a table when it carries edge attributes — either the
 * schema declares `edgeFields`, or at least one related edge actually stores
 * properties. Otherwise it is a bare relationship shown as compact chips (a
 * one-to-one relationship with no edge fields must not render as a one-row
 * table).
 */
export function groupLayout(group: RawRelationshipGroup): RelationshipLayout {
  const hasDeclaredFields = !!group.edgeFields && group.edgeFields.length > 0;
  const hasEdgeData = group.related.some(
    (node) => node.edgeProperties && Object.keys(node.edgeProperties).length > 0
  );
  return hasDeclaredFields || hasEdgeData ? 'table' : 'chips';
}

/**
 * Ordered edge-attribute column names for a group: declared `edgeFields` first
 * (in declared order), then any additional keys present on the stored edge
 * properties that were not declared (so ad-hoc edge data still surfaces).
 * Empty for a `chips` group.
 */
export function groupEdgeColumns(group: RawRelationshipGroup): string[] {
  if (groupLayout(group) !== 'table') {
    return [];
  }
  const columns: string[] = [];
  const seen = new Set<string>();
  for (const field of group.edgeFields ?? []) {
    if (!seen.has(field.name)) {
      seen.add(field.name);
      columns.push(field.name);
    }
  }
  for (const node of group.related) {
    for (const key of Object.keys(node.edgeProperties ?? {})) {
      if (!seen.has(key)) {
        seen.add(key);
        columns.push(key);
      }
    }
  }
  return columns;
}

function rowLabel(node: RawRelatedNode): string {
  if (node.title && node.title.trim().length > 0) {
    return node.title;
  }
  if (node.contentPreview && node.contentPreview.trim().length > 0) {
    return node.contentPreview;
  }
  return node.id;
}

function buildGroupView(group: RawRelationshipGroup): RelationshipGroupView {
  const layout = groupLayout(group);
  const edgeColumns = groupEdgeColumns(group);
  const rows: RelationshipRowView[] = group.related.map((node) => ({
    id: node.id,
    nodeType: node.nodeType,
    label: rowLabel(node),
    edgeValues: node.edgeProperties ?? {}
  }));
  return {
    key: `${group.direction}:${group.relationshipName}:${group.targetType ?? '*'}`,
    label: groupDisplayLabel(group),
    direction: group.direction,
    targetType: group.targetType,
    cardinality: group.cardinality,
    description: group.description,
    layout,
    edgeColumns,
    rows,
    count: group.count
  };
}

/**
 * Build the modal's view model from the command payload. Groups with no related
 * nodes are dropped (the aggregation already omits them, but this stays robust
 * to either behavior); `isEmpty` is true when nothing remains to show.
 */
export function buildRelationshipsView(raw: RawNodeRelationships): NodeRelationshipsView {
  const groups = raw.groups
    .filter((group) => group.related.length > 0)
    .map(buildGroupView);
  return {
    nodeId: raw.nodeId,
    nodeType: raw.nodeType,
    groups,
    isEmpty: groups.length === 0
  };
}
