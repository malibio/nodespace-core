/**
 * Unit tests for assertExpectation() in scripts/aichat-matrix.ts.
 *
 * scripts/ isn't part of any Vitest project's file glob, so this uses
 * Bun's native test runner directly:
 *
 *   bun test scripts/aichat-matrix.test.ts
 *
 * Do NOT use `bun run test` for this file — that command is scoped to
 * packages/desktop-app's Happy-DOM Vitest suite and won't discover it.
 */

import { describe, expect, test } from "bun:test";
import { assertExpectation, type Expectation } from "./aichat-matrix";

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
      const result = assertExpectation(expect_, ["search_skills", "create_node"]);
      expect(result.passed).toBe(false);
    });
  });

  describe("toolOnce", () => {
    const expect_: Expectation = { kind: "toolOnce", tool: "create_node" };

    test("passes with exactly one call", () => {
      expect(assertExpectation(expect_, ["create_node"]).passed).toBe(true);
    });

    test("passes with exactly one call interleaved with search_skills", () => {
      expect(assertExpectation(expect_, ["search_skills", "create_node"]).passed).toBe(true);
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

  describe("toolSequence", () => {
    const expect_: Expectation = { kind: "toolSequence", tools: ["search_nodes", "update_node"] };

    test("passes when the sequence appears in order", () => {
      expect(assertExpectation(expect_, ["search_nodes", "update_node"]).passed).toBe(true);
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
      const result = assertExpectation(expect_, ["update_node", "search_nodes"]);
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
      const result = assertExpectation(expect_, ["search_nodes", "search_nodes"]);
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("retry loop");
    });

    test("tolerates non-adjacent repeats", () => {
      const result = assertExpectation(expect_, ["search_nodes", "create_node", "search_nodes"]);
      expect(result.passed).toBe(true);
    });

    test("search_skills is filtered out before the run-length check, so it does not break up a repeat", () => {
      // search_skills is a routing tool (ROUTING_TOOLS), stripped by actionTools()
      // before noRetry evaluates adjacency — so this is still two search_nodes in a row.
      const result = assertExpectation(expect_, ["search_nodes", "search_skills", "search_nodes"]);
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
      const result = assertExpectation(expect_, ["create_schema", "create_schema"]);
      expect(result.passed).toBe(false);
      expect(result.failure).toContain("got 2");
    });

    test("tolerates search_skills alongside a single create_schema", () => {
      const result = assertExpectation(expect_, ["search_skills", "create_schema"]);
      expect(result.passed).toBe(true);
    });
  });
});
