/**
 * Unit tests for assertExpectation() — the function that decides whether an
 * agent-matrix scenario passed.
 *
 * Runs via `bun run test:scripts` (and so under `bun run test:all`). This is a
 * deliberate exception to the project-wide "never use `bun test`" rule: that
 * rule exists so DOM tests cannot bypass the Happy-DOM Vitest config, and this
 * file touches no DOM. It cannot run under Vitest anyway — it imports
 * `bun:test`, and scripts/ is outside every Vitest project glob.
 *
 * It needs no model and no daemon, which is what makes the scoring logic
 * cheap to protect even though the eval itself is manual.
 */

import { describe, expect, test } from "bun:test";
import { assertExpectation, type Expectation } from "./agent-matrix.ts";

describe("assertExpectation", () => {
  describe("noTools", () => {
    const expect_: Expectation = { kind: "noTools" };

    test("passes with no action tools", () => {
      expect(assertExpectation(expect_, []).passed).toBe(true);
    });

    test("tolerates search_skills", () => {
      expect(assertExpectation(expect_, ["search_skills"]).passed).toBe(true);
    });

    test("fails with any action tool present", () => {
      const result = assertExpectation(expect_, ["create_node"]);
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("create_node");
    });

    test("fails with an action tool even alongside search_skills", () => {
      const result = assertExpectation(expect_, [
        "search_skills",
        "create_node",
      ]);
      expect(result.passed).toBe(false);
    });
  });

  describe("toolOnce", () => {
    const expect_: Expectation = { kind: "toolOnce", tool: "create_node" };

    test("passes with exactly one call", () => {
      expect(assertExpectation(expect_, ["create_node"]).passed).toBe(true);
    });

    test("passes with exactly one call interleaved with search_skills", () => {
      expect(
        assertExpectation(expect_, ["search_skills", "create_node"]).passed,
      ).toBe(true);
    });

    test("fails with zero calls", () => {
      const result = assertExpectation(expect_, []);
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("got 0");
    });

    test("fails with multiple calls", () => {
      const result = assertExpectation(expect_, ["create_node", "create_node"]);
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("got 2");
    });
  });

  describe("toolOnce minProperties", () => {
    const expect_: Expectation = {
      kind: "toolOnce",
      tool: "create_node",
      minProperties: 1,
    };

    test("fails when the call persisted no property values", () => {
      // The observed production case: create_node called with only `content`
      // and `node_type`, persisting a bare shell that later scenarios then key
      // on and cannot resolve.
      const result = assertExpectation(expect_, ["create_node"], [
        { name: "create_node", isError: false, fieldCount: 0 },
      ]);
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("persisted 0 property value(s)");
    });

    test("passes when the call persisted at least the minimum", () => {
      expect(
        assertExpectation(expect_, ["create_node"], [
          { name: "create_node", isError: false, fieldCount: 2 },
        ]).passed,
      ).toBe(true);
    });

    test("treats an absent field count as unknown, not as zero", () => {
      // A results file recorded before create_node reported its property count
      // must not read as a fresh failure.
      expect(
        assertExpectation(expect_, ["create_node"], [
          { name: "create_node", isError: false },
        ]).passed,
      ).toBe(true);
    });

    test("does not apply the minimum to an unrelated tool's calls", () => {
      expect(
        assertExpectation(expect_, ["create_node"], [
          { name: "create_node", isError: false, fieldCount: 3 },
          { name: "search_nodes", isError: false, fieldCount: 0 },
        ]).passed,
      ).toBe(true);
    });

    test("still enforces the count when no minimum is set", () => {
      // Without minProperties the assertion is unchanged: a zero-property call
      // passes, which is the behaviour every other scenario relies on.
      expect(
        assertExpectation({ kind: "toolOnce", tool: "create_node" }, [
          "create_node",
        ], [{ name: "create_node", isError: false, fieldCount: 0 }]).passed,
      ).toBe(true);
    });
  });

  describe("toolSequence", () => {
    const expect_: Expectation = {
      kind: "toolSequence",
      tools: ["search_nodes", "update_node"],
    };

    test("passes when the sequence appears in order", () => {
      expect(
        assertExpectation(expect_, ["search_nodes", "update_node"]).passed,
      ).toBe(true);
    });

    test("passes with other tools interleaved", () => {
      const result = assertExpectation(expect_, [
        "search_skills",
        "search_nodes",
        "search_skills",
        "update_node",
      ]);
      expect(result.passed).toBe(true);
    });

    test("fails when out of order", () => {
      const result = assertExpectation(expect_, [
        "update_node",
        "search_nodes",
      ]);
      expect(result.passed).toBe(false);
    });

    test("fails when a required tool is missing", () => {
      const result = assertExpectation(expect_, ["search_nodes"]);
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("search_nodes,update_node");
    });

    test("fails on an empty tool list", () => {
      expect(assertExpectation(expect_, []).passed).toBe(false);
    });
  });

  describe("noRetry", () => {
    const expect_: Expectation = { kind: "noRetry", tool: "search_nodes" };

    test("passes with no back-to-back repeats", () => {
      expect(assertExpectation(expect_, ["search_nodes"]).passed).toBe(true);
    });

    test("fails on a repeated call", () => {
      const result = assertExpectation(expect_, [
        "search_nodes",
        "search_nodes",
      ]);
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("retry loop");
    });

    test("tolerates non-adjacent repeats", () => {
      const result = assertExpectation(expect_, [
        "search_nodes",
        "create_node",
        "search_nodes",
      ]);
      expect(result.passed).toBe(true);
    });

    test("search_skills is filtered out before the run-length check, so it does not break up a repeat", () => {
      // search_skills is a routing tool (ROUTING_TOOLS), stripped by actionTools()
      // before noRetry evaluates adjacency — so this is still two search_nodes in a row.
      const result = assertExpectation(expect_, [
        "search_nodes",
        "search_skills",
        "search_nodes",
      ]);
      expect(result.passed).toBe(false);
    });
  });

  describe("noExtraTypes", () => {
    const expect_: Expectation = { kind: "noExtraTypes" };

    test("passes with exactly one create_schema", () => {
      expect(assertExpectation(expect_, ["create_schema"]).passed).toBe(true);
    });

    test("fails with zero create_schema calls", () => {
      const result = assertExpectation(expect_, []);
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("got 0");
    });

    test("fails with multiple create_schema calls", () => {
      const result = assertExpectation(expect_, [
        "create_schema",
        "create_schema",
      ]);
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("got 2");
    });

    test("tolerates search_skills alongside a single create_schema", () => {
      const result = assertExpectation(expect_, [
        "search_skills",
        "create_schema",
      ]);
      expect(result.passed).toBe(true);
    });
  });

  /**
   * A create_schema call can be counted once and still leave the user with
   * nothing usable. Both cases below scored as PASSES before the outcome of the
   * call was carried alongside its name.
   */
  describe("create_schema outcome", () => {
    const noExtra: Expectation = { kind: "noExtraTypes" };
    const once: Expectation = { kind: "toolOnce", tool: "create_schema" };

    // The observed scenario-3 false pass: a title_template referencing fields
    // that were never defined, rejected by title-template validation.
    test("fails when the single create_schema call was rejected", () => {
      const result = assertExpectation(
        noExtra,
        ["create_schema"],
        [{ name: "create_schema", isError: true }],
      );
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("REJECTED");
    });

    // Distinct from rejection: this call SUCCEEDS. A create_schema carrying
    // neither fields nor description persists a type with no properties.
    test("fails when create_schema succeeded with zero fields", () => {
      const result = assertExpectation(
        noExtra,
        ["create_schema"],
        [{ name: "create_schema", isError: false, fieldCount: 0 }],
      );
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("NO fields");
    });

    test("passes when create_schema persisted fields", () => {
      const result = assertExpectation(
        noExtra,
        ["create_schema"],
        [{ name: "create_schema", isError: false, fieldCount: 4 }],
      );
      expect(result.passed).toBe(true);
    });

    // Scenarios 8a/8b reach create_schema through toolOnce, so the same hole
    // has to be closed on that branch and not just on noExtraTypes.
    test("toolOnce fails on a rejected create_schema", () => {
      const result = assertExpectation(
        once,
        ["create_schema"],
        [{ name: "create_schema", isError: true }],
      );
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("REJECTED");
    });

    test("toolOnce fails on a fieldless create_schema", () => {
      const result = assertExpectation(
        once,
        ["create_schema"],
        [{ name: "create_schema", isError: false, fieldCount: 0 }],
      );
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("NO fields");
    });

    test("toolOnce still reports a wrong call count before outcome", () => {
      const result = assertExpectation(once, [], []);
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("got 0");
    });

    // A baseline recorded before fieldCount existed must not read as a fresh
    // failure: absence is unknown, not zero.
    test("passes when the outcome was never captured", () => {
      expect(assertExpectation(noExtra, ["create_schema"], []).passed).toBe(
        true,
      );
      expect(
        assertExpectation(
          noExtra,
          ["create_schema"],
          [{ name: "create_schema", isError: false }],
        ).passed,
      ).toBe(true);
    });

    // Only create_schema outcomes are judged here; an unrelated failing tool is
    // some other assertion's business.
    test("ignores non-create_schema calls", () => {
      const result = assertExpectation(
        once,
        ["create_schema"],
        [
          { name: "search_nodes", isError: true },
          { name: "create_schema", isError: false, fieldCount: 2 },
        ],
      );
      expect(result.passed).toBe(true);
    });
  });
});
