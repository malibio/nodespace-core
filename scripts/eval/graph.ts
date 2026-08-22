/**
 * Graph end-state capture and comparison.
 *
 * WHY THIS EXISTS
 *
 * The matrix used to score TRAJECTORY — which tool fired, how many times, in
 * what order. That disagrees with the product in three measured ways:
 *
 *   - A correct result scored as a failure. A model that reached the right end
 *     state by a shorter path (update_node without a preceding resolve_query)
 *     lost a point for the path, not the result.
 *   - Self-correction scored as a failure. A rejected create_node, corrected by
 *     the model and persisted on the second call, red-lined on an exactly-once
 *     rule — punishing the recovery behavior we want.
 *   - Severity flattened. Two search_nodes calls (wasted latency, nothing
 *     persisted) scored identically to two create_schema calls (a spurious type
 *     in the user's graph) and to zero search_nodes on a query (a wrong answer).
 *
 * Asserting on the graph's END STATE fixes all three at once, and the third
 * falls out for free rather than needing a severity table: two search_nodes
 * calls change nothing and pass, two create_schema calls leave an unexpected
 * type behind and fail the same `noUnexpectedSchemas` clause that every other
 * write scenario already carries.
 *
 * This is tau-bench's design — the agent operates on a database through an API
 * and is graded by diffing final DB state against a goal state — and it is the
 * closest published analogue to what NodeSpace's agent actually does.
 *
 * WHAT IS AND IS NOT ASSERTED
 *
 * A snapshot is deliberately NOT a whole-database dump. It covers what the
 * scenarios act on — nodes (with their properties), schemas, and edges between
 * observed nodes — read back through the same `nodespace` CLI every eval turn
 * already goes through. Anything outside that (embeddings, collections,
 * lifecycle transitions) is invisible here by design: an assertion this module
 * cannot express belongs in a scenario's trajectory diagnostic, not in a
 * silently-passing end-state clause.
 */

import type { EvalEnv } from "./env.ts";

/** One node as the CLI reports it, narrowed to the fields assertions read. */
export interface SnapshotNode {
  id: string;
  node_type: string;
  content: string;
  properties: Record<string, unknown>;
}

/** One edge, as discovered by traversing a known node's relationships. */
export interface SnapshotEdge {
  from: string;
  relation: string;
  to: string;
}

/**
 * The graph as it stood at one instant, scoped to what the eval acts on.
 *
 * `schemas` is a list of schema ids (node types the user can record against)
 * rather than the full definitions: every scenario's end-state clause asks
 * whether a type EXISTS or whether an unexpected one appeared, and neither
 * needs the field list. `schemaCallsAreSound` remains the trajectory-side
 * assertion for "the type persisted with usable fields".
 */
export interface GraphSnapshot {
  nodes: SnapshotNode[];
  schemas: string[];
  edges: SnapshotEdge[];
  /**
   * The snapshot could not be captured (the CLI failed, the daemon went away).
   *
   * Recorded rather than thrown, and distinguished from an empty graph: an
   * empty snapshot would make "no unexpected nodes" pass and "node exists"
   * fail, silently converting an environment fault into a model verdict. Every
   * end-state assertion checks this first and declines to score.
   */
  captureError?: string;
}

/**
 * Relationship names traversed when discovering edges.
 *
 * The daemon exposes relationship reads per (node, relation) pair rather than
 * as a whole-graph edge dump, so discovery has to name the relations it walks.
 * These are the four the validator treats as universal — legal between any two
 * nodes regardless of their schemas (see node_service/relationship.rs's "are
 * universal" error). A custom relation defined on a user type is NOT walked:
 * no scenario asserts one today, and probing invented names would multiply CLI
 * round-trips per snapshot for edges nothing reads.
 */
const UNIVERSAL_RELATIONS = ["member_of", "has_child", "mentions", "has_role"];

/**
 * Node types excluded from a snapshot.
 *
 * Chat nodes are the eval's own scaffolding — the runner creates one per group
 * and every turn appends messages to it — so counting them as graph state would
 * fail `noUnexpectedNodes` on every scenario for the harness's own bookkeeping.
 * `date` nodes are auto-created by the daemon as containers rather than by any
 * model action, and would fail the same clause for the same reason.
 */
const SCAFFOLDING_TYPES = ["ai-chat", "date"];

/** Max nodes pulled per type. Far above any scenario's footprint. */
const QUERY_LIMIT = "200";

function runCli(env: EvalEnv, args: string[]): unknown {
  const r = Bun.spawnSync(
    [env.nsBin, "--socket", env.socket, "--json", ...args],
    { stdout: "pipe", stderr: "pipe" },
  );
  if (r.exitCode !== 0) {
    throw new Error(
      `nodespace ${args.join(" ")} failed (exit ${r.exitCode}): ${r.stderr
        .toString()
        .trim()}`,
    );
  }
  const out = r.stdout.toString().trim();
  return out ? JSON.parse(out) : null;
}

/** Narrow one CLI node object into a SnapshotNode, tolerating absent fields. */
export function toSnapshotNode(raw: unknown): SnapshotNode | null {
  if (typeof raw !== "object" || raw === null) return null;
  const o = raw as Record<string, unknown>;
  if (typeof o.id !== "string" || typeof o.node_type !== "string") return null;
  // `properties` inlines as nested JSON (output.rs::node_to_json), but degrades
  // to a raw string if the daemon's encoding ever breaks. Treat a non-object as
  // "no properties" rather than crashing the snapshot.
  const props =
    typeof o.properties === "object" &&
    o.properties !== null &&
    !Array.isArray(o.properties)
      ? (o.properties as Record<string, unknown>)
      : {};
  return {
    id: o.id,
    node_type: o.node_type,
    content: typeof o.content === "string" ? o.content : "",
    properties: props,
  };
}

/** Pull the `nodes` array out of a CLI node-list payload. */
export function readNodeList(payload: unknown): SnapshotNode[] {
  if (typeof payload !== "object" || payload === null) return [];
  const nodes = (payload as Record<string, unknown>).nodes;
  if (!Array.isArray(nodes)) return [];
  return nodes.map(toSnapshotNode).filter((n): n is SnapshotNode => n !== null);
}

/** Pull target ids out of a `relationship get` payload. */
export function readRelatedIds(payload: unknown): string[] {
  if (typeof payload !== "object" || payload === null) return [];
  const related = (payload as Record<string, unknown>).related_nodes;
  if (!Array.isArray(related)) return [];
  const ids: string[] = [];
  for (const r of related) {
    if (typeof r === "string") {
      ids.push(r);
      continue;
    }
    if (typeof r === "object" && r !== null) {
      const id = (r as Record<string, unknown>).id;
      if (typeof id === "string") ids.push(id);
    }
  }
  return ids;
}

/**
 * Capture the graph's current state.
 *
 * `types` is the set of node types to enumerate — the caller supplies it from
 * the types its scenarios can plausibly touch, because the daemon has no
 * "query every node" verb and probing every conceivable type would be a
 * round-trip per guess. Types created DURING the run (a model inventing a
 * custom type) are picked up via `schema list`, which is enumerated first and
 * folded into the query set, so a type nothing predicted still lands in the
 * snapshot — which is what makes `noUnexpectedNodes` able to see a node of a
 * type the fixture never anticipated.
 */
export function captureSnapshot(
  env: EvalEnv,
  types: string[],
  options: { edgesFor?: (n: SnapshotNode) => boolean } = {},
): GraphSnapshot {
  try {
    const schemaNodes = readNodeList(runCli(env, ["schema", "list"]));
    // A schema node's id IS the type identifier it defines (`schema get` takes
    // the same value), so the ids double as the list of queryable types.
    const schemas = schemaNodes.map((s) => s.id).sort();

    const queryTypes = [...new Set([...types, ...schemas])].filter(
      (t) => !SCAFFOLDING_TYPES.includes(t),
    );

    const nodes: SnapshotNode[] = [];
    const seen = new Set<string>();
    for (const t of queryTypes) {
      // A type with no instances is a normal outcome and the CLI reports it as
      // an empty list. A type that does not exist at all can error instead —
      // tolerated for the same reason, since the caller's predicted type set
      // intentionally names types a given run may never create.
      let payload: unknown;
      try {
        payload = runCli(env, [
          "node",
          "query",
          "--type",
          t,
          "--limit",
          QUERY_LIMIT,
        ]);
      } catch {
        continue;
      }
      for (const n of readNodeList(payload)) {
        if (seen.has(n.id)) continue;
        seen.add(n.id);
        nodes.push(n);
      }
    }

    const edges: SnapshotEdge[] = [];
    const walk = options.edgesFor ?? (() => true);
    for (const n of nodes) {
      if (!walk(n)) continue;
      for (const rel of UNIVERSAL_RELATIONS) {
        let payload: unknown;
        try {
          payload = runCli(env, ["relationship", "get", n.id, "--type", rel]);
        } catch {
          continue;
        }
        for (const target of readRelatedIds(payload)) {
          edges.push({ from: n.id, relation: rel, to: target });
        }
      }
    }

    return { nodes, schemas, edges };
  } catch (e) {
    return {
      nodes: [],
      schemas: [],
      edges: [],
      captureError: e instanceof Error ? e.message : String(e),
    };
  }
}

/**
 * What changed between two snapshots.
 *
 * The unit every end-state assertion scores against: a scenario asserts what
 * ITS OWN turn did, so a node that already existed before the turn must not
 * satisfy a "created" clause, and a spurious type left behind by an earlier
 * scenario must not fail a later one's "no unexpected types" clause.
 */
export interface GraphDiff {
  addedNodes: SnapshotNode[];
  addedSchemas: string[];
  addedEdges: SnapshotEdge[];
  /** Nodes present in both snapshots whose properties or content changed. */
  changedNodes: Array<{ before: SnapshotNode; after: SnapshotNode }>;
  /** Neither snapshot can be trusted — see `GraphSnapshot.captureError`. */
  captureError?: string;
}

function edgeKey(e: SnapshotEdge): string {
  return `${e.from}|${e.relation}|${e.to}`;
}

export function diffSnapshots(
  before: GraphSnapshot,
  after: GraphSnapshot,
): GraphDiff {
  const captureError = before.captureError ?? after.captureError;
  if (captureError) {
    return {
      addedNodes: [],
      addedSchemas: [],
      addedEdges: [],
      changedNodes: [],
      captureError,
    };
  }

  const beforeById = new Map(before.nodes.map((n) => [n.id, n]));
  const beforeSchemas = new Set(before.schemas);
  const beforeEdges = new Set(before.edges.map(edgeKey));

  const addedNodes = after.nodes.filter((n) => !beforeById.has(n.id));
  const addedSchemas = after.schemas.filter((s) => !beforeSchemas.has(s));
  const addedEdges = after.edges.filter((e) => !beforeEdges.has(edgeKey(e)));

  const changedNodes: GraphDiff["changedNodes"] = [];
  for (const a of after.nodes) {
    const b = beforeById.get(a.id);
    if (!b) continue;
    if (
      b.content !== a.content ||
      JSON.stringify(b.properties) !== JSON.stringify(a.properties)
    ) {
      changedNodes.push({ before: b, after: a });
    }
  }

  return { addedNodes, addedSchemas, addedEdges, changedNodes };
}
