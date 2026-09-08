#!/usr/bin/env bun
// Prevents new drift against CLAUDE.md's rule against citing GitHub issue
// numbers or nodespace-docs/ paths in code comments ("describe the
// behavior/constraint directly, and reference decisions by ADR").
//
// The full-repo retroactive triage this check used to merely ratchet against
// is now complete: every issue-number and doc-path reference across the
// whole repo (SCAN_ROOTS below, now including packages/agent and
// packages/nlp-engine, previously excluded as out of scope) was triaged —
// constraint-bearing comments had their constraint inlined before the
// reference was dropped, provenance-only citations were deleted outright,
// and doc-path references either had their essential fact inlined or were
// replaced with an ADR citation. Both baselines are now 0.
//
// Lower BASELINES whenever a change pays down part of the backlog (should
// stay at 0 now). Never raise a baseline to accommodate a new reference —
// inline the constraint or cite an ADR instead, per the rule this check
// exists to hold the line on.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";

const REPO = join(dirname(new URL(import.meta.url).pathname), "..");

// Every package is in scope. List them explicitly (rather than deriving from
// `packages/*`) so a newly added package fails closed — it stays unscanned
// until someone notices and adds it here, rather than silently entering the
// ratchet with whatever count it happens to start with.
const SCAN_ROOTS = [
  "scripts",
  "packages/desktop-app",
  "packages/core",
  "packages/daemon",
  "packages/cli",
  "packages/nodespace-types",
  "packages/proto",
  "packages/dev-tools",
  "packages/skill",
  "packages/agent",
  "packages/nlp-engine",
];
const EXTENSIONS = new Set([".rs", ".ts", ".svelte", ".js"]);
const EXCLUDE_DIR_NAMES = new Set(["node_modules", "target", ".git", "dist", "build"]);

// This checker's own source and test necessarily describe the patterns they
// scan for in comments/messages/fixtures, which would otherwise self-match.
//
// search_skills_latency.rs computes a real `nodespace-docs` sibling-directory
// path at runtime to write a benchmark report — functional filesystem logic,
// gracefully skipped when the directory is absent, not a stale documentation
// citation. That's a different thing from the rule this check enforces (a
// comment pointing a human at a doc for context), so it's excluded rather
// than rewritten to hide a real dependency.
const EXCLUDE_FILE_NAMES = new Set([
  "check-code-references.ts",
  "check-code-references.test.ts",
  "search_skills_latency.rs",
]);

const ISSUE_NUMBER_PATTERNS: RegExp[] = [
  /core#\d+/,
  /\(#\d+\)/,
  /\b[Ii]ssue #\d+\b/,
  /\bPR#\d+\b/,
  /\b(?:pre|post)-#\d+\b/,
  /\b(?:pre|post)-issue-\d+\b/,
];
const DOC_PATH_PATTERN = /nodespace-docs\//;

// Ratchet baselines. See the file-level comment: lower on paydown, never raise.
export const BASELINES = {
  issueNumberReferences: 0,
  docPathReferences: 0,
};

function walk(dir: string, out: string[]): void {
  let entries: string[];
  try {
    entries = readdirSync(dir);
  } catch {
    return;
  }
  for (const entry of entries) {
    if (EXCLUDE_DIR_NAMES.has(entry)) continue;
    const full = join(dir, entry);
    const info = statSync(full);
    if (info.isDirectory()) {
      walk(full, out);
    } else if (!EXCLUDE_FILE_NAMES.has(entry)) {
      const dot = entry.lastIndexOf(".");
      if (dot !== -1 && EXTENSIONS.has(entry.slice(dot))) {
        out.push(full);
      }
    }
  }
}

export interface ReferenceCounts {
  issueNumberReferences: number;
  docPathReferences: number;
  issueNumberFiles: string[];
  docPathFiles: string[];
}

/**
 * Scans SCAN_ROOTS for issue-number and nodespace-docs/ path references in
 * code comments/strings, line by line. A pure function of the filesystem —
 * no baseline comparison here, so it's independently testable against
 * injected fixtures.
 */
export function countReferences(roots: string[] = SCAN_ROOTS, repoRoot: string = REPO): ReferenceCounts {
  const files: string[] = [];
  for (const root of roots) {
    walk(join(repoRoot, root), files);
  }

  let issueNumberReferences = 0;
  let docPathReferences = 0;
  const issueNumberFiles = new Set<string>();
  const docPathFiles = new Set<string>();

  for (const file of files) {
    const content = readFileSync(file, "utf8");
    for (const line of content.split("\n")) {
      if (ISSUE_NUMBER_PATTERNS.some((re) => re.test(line))) {
        issueNumberReferences++;
        issueNumberFiles.add(file);
      }
      if (DOC_PATH_PATTERN.test(line)) {
        docPathReferences++;
        docPathFiles.add(file);
      }
    }
  }

  return {
    issueNumberReferences,
    docPathReferences,
    issueNumberFiles: [...issueNumberFiles],
    docPathFiles: [...docPathFiles],
  };
}

if (import.meta.main) {
  const counts = countReferences();
  let failed = false;

  if (counts.issueNumberReferences > BASELINES.issueNumberReferences) {
    console.error(
      `❌ ${counts.issueNumberReferences} issue-number references in code (core#NNNN, (#NNNN), Issue #NNNN), ` +
        `up from the ${BASELINES.issueNumberReferences}-reference baseline in scripts/check-code-references.ts. ` +
        "Describe the behavior/constraint directly and cite an ADR instead, per CLAUDE.md.",
    );
    failed = true;
  }

  if (counts.docPathReferences > BASELINES.docPathReferences) {
    console.error(
      `❌ ${counts.docPathReferences} nodespace-docs/ path references in code, up from the ` +
        `${BASELINES.docPathReferences}-reference baseline in scripts/check-code-references.ts. ` +
        "Inline the essential fact, or cite an ADR, instead of a path into a separate repo.",
    );
    failed = true;
  }

  if (failed) process.exit(1);

  console.log(
    `✅ Issue-number references: ${counts.issueNumberReferences} (baseline ${BASELINES.issueNumberReferences}). ` +
      `Doc-path references: ${counts.docPathReferences} (baseline ${BASELINES.docPathReferences}).`,
  );
}
