/**
 * End-state expectations: what the graph must look like when a turn ends.
 *
 * The scoring half of the outcome-based grading model (see ./graph.ts for the
 * capture half and for why trajectory scoring was replaced).
 *
 * DESIGN: CLAUSES OVER A DIFF, NOT OVER A SNAPSHOT
 *
 * Every clause scores against the DIFF between the pre-turn and post-turn
 * snapshots, never the post-turn snapshot alone. That is what keeps a
 * scenario's verdict about its own turn: a node created three scenarios ago
 * must not satisfy this turn's `createdNode`, and a spurious type left behind
 * by an earlier turn must not fail this turn's `createdSchemas` count.
 *
 * WHY `noUnexpectedNodes` IS OPT-IN RATHER THAN UNIVERSAL
 *
 * It is the clause that makes severity fall out for free, but it is only
 * meaningful where the fixture can say what the expected additions ARE. On a
 * read scenario the expectation is "nothing was written", which
 * `expectNoWrites` states directly and more legibly. Making the clause
 * unconditional would also red-line every turn in which the daemon
 * legitimately materializes a container node, which is a property of the
 * system rather than of the model's behavior.
 */

import type { GraphDiff, SnapshotNode } from "./graph.ts";
import type { Verdict } from "./types.ts";

/**
 * A node the turn was supposed to create.
 *
 * `type` is matched exactly. `contentMatches` is a case-insensitive substring
 * or regex over the node's content, present because a create_node's identity
 * lives in its text rather than in a property — a scenario asking to record
 * "offline sync" has no other way to say WHICH node it meant.
 *
 * `properties` asserts persisted field VALUES. `minProperties` asserts only
 * that some number of them reached storage, for scenarios that deliberately do
 * not pin which field the model chose (see scenario 4, where either the state
 * or the estimate is an acceptable record of "the particulars").
 */
export interface NodeExpectation {
  type?: string;
  contentMatches?: string;
  /**
   * Required property values. A value of `true` asserts only that the key is
   * present with a non-null, non-empty value — the right assertion when the
   * prompt supplies a value whose exact serialization is the model's choice
   * (a date the model may render several legal ways).
   */
  properties?: Record<string, unknown>;
  minProperties?: number;
  /**
   * SOME property holds this value, whichever key it landed under.
   *
   * The winnability-preserving half of `properties`: a scenario whose type was
   * built by an earlier turn cannot know which key the model chose to store a
   * value under, so naming one would measure the fixture's guess rather than
   * the model's behavior. Asserting the VALUE reached storage is the part that
   * is actually about the model — and it is strictly stronger than
   * `minProperties`, which a write of any unrelated value satisfies.
   */
  hasPropertyValue?: unknown;
}

/**
 * An edge the turn was supposed to record.
 *
 * ALWAYS name the `relation`. It is optional in the type only because an
 * unnamed one is meaningful in the abstract, but leaving it off is unsafe in
 * practice, and a fixture invariant rejects it.
 *
 * The reason is the runner's asymmetric edge walk: a node that did not exist
 * before the turn had no edges walked in the "before" snapshot, so EVERY edge
 * on a newly-created node lands in `addedEdges` — whether the turn recorded it
 * or the daemon materialized it at creation time. An expectation that pins no
 * relation would therefore pass on any turn that merely created a node.
 * Naming the relation is what keeps the assertion about the link the scenario
 * asked for.
 */
export interface EdgeExpectation {
  relation?: string;
}

/**
 * What the graph must look like after the turn.
 *
 * Every field is optional; a scenario states only the clauses it cares about.
 * An expectation with no clauses at all is rejected by the fixture's own tests
 * rather than silently passing everything.
 */
export interface EndState {
  /** A node matching this must have been created by this turn. */
  createdNode?: NodeExpectation;
  /**
   * A node that already existed must have been modified to match this.
   *
   * Distinct from `createdNode` because an update that creates a NEW node
   * instead of modifying the existing one is a real failure — it leaves the
   * user with a duplicate — and a clause that accepted either would score that
   * outcome green.
   */
  updatedNode?: NodeExpectation;
  /**
   * Exactly this many schemas (node types) must have been created.
   *
   * A COUNT rather than a name, and that is a winnability constraint rather
   * than imprecision: the model chooses the type's identifier, so asserting
   * `createdSchema: "spec"` would red-line a model that named the same concept
   * `feature_writeup` — measuring the fixture's vocabulary guess instead of the
   * model's behavior, the exact trap this fixture already documents for prompt
   * wording.
   *
   * The count is what carries the discrimination that matters: exactly one
   * means the requested type exists AND no second type was invented alongside
   * it, so a proactive related-type creation fails on the same clause. That is
   * the severity distinction the old exactly-one-CALL rule was reaching for,
   * now stated over what the user is actually left with — a model that called
   * create_schema twice for the SAME type ends with one type and passes.
   */
  createdSchemas?: number;
  /** An edge matching this must have been created. */
  createdEdge?: EdgeExpectation;
  /**
   * The turn must not have written anything at all.
   *
   * The read-side assertion, and the one that gives severity for free on
   * queries: a model that answers a question by creating a node to hold the
   * answer fails, while one that searches twice and answers correctly passes.
   */
  expectNoWrites?: boolean;
  /**
   * No nodes beyond those `createdNode`/`updatedNode` account for.
   *
   * This is the clause that discriminates two `create_schema` calls (a spurious
   * type in the user's graph) from two `search_nodes` calls (nothing
   * persisted) without a severity table.
   */
  noUnexpectedNodes?: boolean;
}

/** True when a property value counts as actually present. */
function isPresent(v: unknown): boolean {
  if (v === undefined || v === null) return false;
  if (typeof v === "string") return v.trim() !== "";
  if (Array.isArray(v)) return v.length > 0;
  return true;
}

/**
 * Compare a persisted value against an expected one.
 *
 * Strings compare case-insensitively on trimmed text, and numbers compare
 * across the string/number boundary, because the model chooses the
 * serialization: a day count may persist as `8` or `"8"`, and a status as
 * `"Signed off"` or `"signed_off"`. Pinning one spelling would red-line a
 * correct write for a formatting choice the product does not care about —
 * exactly the class of false failure this whole change exists to remove.
 */
export function valueMatches(actual: unknown, expected: unknown): boolean {
  if (expected === true) return isPresent(actual);
  if (typeof expected === "number") {
    // Only a number or a numeric string counts. Bare `Number(actual)` would
    // also accept a single-element array — `Number([8]) === 8` — which is not
    // a serialization the product wants to silently treat as the scalar the
    // prompt asked for. The string/number tolerance below is deliberate
    // (the model chooses the serialization); array coercion is not.
    const n =
      typeof actual === "number"
        ? actual
        : typeof actual === "string"
          ? Number(actual)
          : NaN;
    return !Number.isNaN(n) && n === expected;
  }
  if (typeof expected === "string") {
    if (typeof actual !== "string" && typeof actual !== "number") return false;
    const norm = (s: string) => s.trim().toLowerCase().replace(/[\s_-]+/g, "");
    return norm(String(actual)) === norm(expected);
  }
  return JSON.stringify(actual) === JSON.stringify(expected);
}

/** Count property entries that actually hold a value. */
export function populatedCount(props: Record<string, unknown>): number {
  return Object.values(props).filter(isPresent).length;
}

function contentMatches(node: SnapshotNode, pattern: string): boolean {
  return node.content.toLowerCase().includes(pattern.toLowerCase());
}

/** Does this node satisfy the expectation? */
export function nodeSatisfies(
  node: SnapshotNode,
  want: NodeExpectation,
): boolean {
  if (want.type !== undefined && node.node_type !== want.type) return false;
  if (want.contentMatches !== undefined && !contentMatches(node, want.contentMatches))
    return false;
  if (want.properties !== undefined) {
    for (const [k, v] of Object.entries(want.properties)) {
      if (!valueMatches(node.properties[k], v)) return false;
    }
  }
  if (
    want.minProperties !== undefined &&
    populatedCount(node.properties) < want.minProperties
  ) {
    return false;
  }
  if (want.hasPropertyValue !== undefined) {
    const found = Object.values(node.properties).some((v) =>
      valueMatches(v, want.hasPropertyValue),
    );
    if (!found) return false;
  }
  return true;
}

/** Render an expectation for a failure message. */
function describe(want: NodeExpectation): string {
  const parts: string[] = [];
  if (want.type) parts.push(`type=${want.type}`);
  if (want.contentMatches) parts.push(`content~"${want.contentMatches}"`);
  if (want.properties) {
    for (const [k, v] of Object.entries(want.properties)) {
      parts.push(v === true ? `${k}=<any value>` : `${k}=${JSON.stringify(v)}`);
    }
  }
  if (want.minProperties !== undefined)
    parts.push(`>=${want.minProperties} property value(s)`);
  if (want.hasPropertyValue !== undefined)
    parts.push(`some property = ${JSON.stringify(want.hasPropertyValue)}`);
  return parts.join(", ") || "(any node)";
}

/** Render observed nodes compactly, so a failure carries its evidence. */
function renderNodes(nodes: SnapshotNode[]): string {
  if (nodes.length === 0) return "(none)";
  return nodes
    .map((n) => {
      const props = Object.entries(n.properties)
        .filter(([, v]) => isPresent(v))
        .map(([k, v]) => `${k}=${JSON.stringify(v)}`)
        .join(" ");
      const head = `${n.node_type}:"${n.content.slice(0, 60)}"`;
      return props ? `${head} {${props}}` : head;
    })
    .join("; ");
}

/**
 * Score a turn against its end-state expectation.
 *
 * Pure over a `GraphDiff`, so it is unit-testable without a daemon — the same
 * property the trajectory assertions have, and the reason both can run in
 * `bun run test:all`.
 */
export function assertEndState(want: EndState, diff: GraphDiff): Verdict {
  // An uncapturable snapshot is an environment fault. Scoring it either way
  // files a dead daemon as a model verdict — the same confusion `sendFailed`
  // and `emptyGeneration` already exist to prevent, one layer down.
  if (diff.captureError) {
    return {
      passed: false,
      failure:
        `graph end-state could not be captured, so the turn was not scored on ` +
        `outcome: ${diff.captureError}`,
    };
  }

  const accountedFor = new Set<string>();

  if (want.createdNode) {
    const match = diff.addedNodes.find((n) => nodeSatisfies(n, want.createdNode!));
    if (!match) {
      return {
        passed: false,
        failure:
          `no node matching [${describe(want.createdNode)}] was created — ` +
          `nodes created this turn: ${renderNodes(diff.addedNodes)}`,
      };
    }
    accountedFor.add(match.id);
  }

  if (want.updatedNode) {
    const match = diff.changedNodes.find(({ after }) =>
      nodeSatisfies(after, want.updatedNode!),
    );
    if (!match) {
      // A turn that created a new node instead of updating the existing one is
      // the specific failure worth naming: it looks like success to the user
      // until they find two nodes where they expected one.
      const decoy = diff.addedNodes.find((n) =>
        nodeSatisfies(n, want.updatedNode!),
      );
      if (decoy) {
        return {
          passed: false,
          failure:
            `expected an EXISTING node to be updated to [${describe(want.updatedNode)}], ` +
            `but the turn CREATED a new node matching it instead — the original is ` +
            `unchanged and the user now has a duplicate`,
        };
      }
      return {
        passed: false,
        failure:
          `no existing node was updated to match [${describe(want.updatedNode)}] — ` +
          `nodes modified this turn: ${renderNodes(diff.changedNodes.map((c) => c.after))}`,
      };
    }
    accountedFor.add(match.after.id);
  }

  if (want.createdSchemas !== undefined) {
    const got = diff.addedSchemas.length;
    if (got !== want.createdSchemas) {
      const listed = diff.addedSchemas.join(", ") || "(none)";
      return {
        passed: false,
        failure:
          got < want.createdSchemas
            ? `expected ${want.createdSchemas} type(s) to be created, but ${got} ` +
              `were — the user has nowhere to record what they asked for ` +
              `(created: ${listed})`
            : `expected ${want.createdSchemas} type(s) to be created, but ${got} ` +
              `were — the extra type(s) are now in the user's graph and need ` +
              `real cleanup (created: ${listed})`,
      };
    }
  }

  if (want.createdEdge) {
    const rel = want.createdEdge.relation;
    const match = diff.addedEdges.find(
      (e) => rel === undefined || e.relation === rel,
    );
    if (!match) {
      return {
        passed: false,
        failure:
          `no ${rel ? `'${rel}' ` : ""}edge was recorded — edges created this turn: ` +
          (diff.addedEdges.map((e) => `${e.relation}`).join(", ") || "(none)"),
      };
    }
  }

  if (want.expectNoWrites) {
    const wrote =
      diff.addedNodes.length > 0 ||
      diff.addedSchemas.length > 0 ||
      diff.addedEdges.length > 0 ||
      diff.changedNodes.length > 0;
    if (wrote) {
      const bits: string[] = [];
      if (diff.addedNodes.length)
        bits.push(`created ${renderNodes(diff.addedNodes)}`);
      if (diff.changedNodes.length)
        bits.push(
          `modified ${renderNodes(diff.changedNodes.map((c) => c.after))}`,
        );
      if (diff.addedSchemas.length)
        bits.push(`created type(s) ${diff.addedSchemas.join(", ")}`);
      if (diff.addedEdges.length)
        bits.push(`recorded ${diff.addedEdges.length} edge(s)`);
      return {
        passed: false,
        failure: `expected the turn to write nothing, but it ${bits.join("; ")}`,
      };
    }
  }

  if (want.noUnexpectedNodes) {
    const extra = diff.addedNodes.filter((n) => !accountedFor.has(n.id));
    if (extra.length > 0) {
      return {
        passed: false,
        failure:
          `${extra.length} unexpected node(s) were created and are now in the ` +
          `user's graph: ${renderNodes(extra)}`,
      };
    }
  }

  return { passed: true };
}
