// Covers the origin/main staleness warning (nodespace-core#2447): the
// pre-push gate (scripts/test-gate.ts, ADR-047) has no awareness of
// origin/main, so a green gate only proves the branch passes on its own
// base, not on the merge result. These tests inject fake fetch/countBehind
// implementations so they exercise the behind/not-behind/fetch-failed
// branches without touching the network or the real git repo.
import { describe, expect, test } from "bun:test";
import { checkBranchBehind, formatBehindWarning, formatSkippedNote, reportBranchBehind } from "./check-branch-behind";

describe("checkBranchBehind", () => {
  test("reports behind with the exact commit count when rev-list returns > 0", async () => {
    const result = await checkBranchBehind({
      fetch: async () => {},
      countBehind: async () => 3,
    });
    expect(result).toEqual({ status: "behind", count: 3 });
  });

  test("reports up-to-date with count 0 when rev-list returns 0", async () => {
    const result = await checkBranchBehind({
      fetch: async () => {},
      countBehind: async () => 0,
    });
    expect(result).toEqual({ status: "up-to-date", count: 0 });
  });

  test("degrades to skipped, never throws, when git fetch fails (no network)", async () => {
    const result = await checkBranchBehind({
      fetch: async () => {
        throw new Error("Could not resolve host: github.com");
      },
      countBehind: async () => {
        throw new Error("should never be called — fetch already failed");
      },
    });
    expect(result.status).toBe("skipped");
    expect(result.reason).toContain("Could not resolve host");
  });

  test("degrades to skipped, never throws, when rev-list fails after a successful fetch", async () => {
    const result = await checkBranchBehind({
      fetch: async () => {},
      countBehind: async () => {
        throw new Error("unknown revision: origin/main");
      },
    });
    expect(result.status).toBe("skipped");
    expect(result.reason).toContain("unknown revision");
  });
});

describe("formatBehindWarning", () => {
  test("names the exact count and the rebase command", () => {
    const msg = formatBehindWarning(2);
    expect(msg).toContain("2 commits behind origin/main");
    expect(msg).toContain("git rebase origin/main");
  });

  test('uses singular "commit" for count === 1', () => {
    expect(formatBehindWarning(1)).toContain("1 commit behind");
    expect(formatBehindWarning(1)).not.toContain("1 commits");
  });

  test("states plainly that tests ran against the base, not the merge result", () => {
    expect(formatBehindWarning(5)).toContain("not the merge result");
  });
});

describe("formatSkippedNote", () => {
  test("includes the given reason", () => {
    expect(formatSkippedNote("network unreachable")).toContain("network unreachable");
  });
});

describe("reportBranchBehind", () => {
  test("prints nothing on the common up-to-date path (no noise)", async () => {
    const warnings: unknown[][] = [];
    const original = console.warn;
    console.warn = (...args: unknown[]) => warnings.push(args);
    try {
      const result = await reportBranchBehind({ fetch: async () => {}, countBehind: async () => 0 });
      expect(result.status).toBe("up-to-date");
      expect(warnings).toEqual([]);
    } finally {
      console.warn = original;
    }
  });

  test("prints exactly one warning naming the count when behind", async () => {
    const warnings: unknown[][] = [];
    const original = console.warn;
    console.warn = (...args: unknown[]) => warnings.push(args);
    try {
      await reportBranchBehind({ fetch: async () => {}, countBehind: async () => 4 });
      expect(warnings.length).toBe(1);
      expect(String(warnings[0][0])).toContain("4 commits behind");
    } finally {
      console.warn = original;
    }
  });

  test("prints a note (not a blocked push) when the check is skipped", async () => {
    const warnings: unknown[][] = [];
    const original = console.warn;
    console.warn = (...args: unknown[]) => warnings.push(args);
    try {
      const result = await reportBranchBehind({
        fetch: async () => {
          throw new Error("offline");
        },
        countBehind: async () => 0,
      });
      expect(result.status).toBe("skipped");
      expect(warnings.length).toBe(1);
      expect(String(warnings[0][0])).toContain("offline");
    } finally {
      console.warn = original;
    }
  });
});
