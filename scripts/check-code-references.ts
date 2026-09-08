#!/usr/bin/env bun
// Prevents new drift against CLAUDE.md's rule against citing GitHub issue
// numbers or nodespace-docs/ paths in code comments ("describe the
// behavior/constraint directly, and reference decisions by ADR").
//
// This does NOT enforce zero. A prior audit found 336 issue-number
// references and 23 doc-path references across the whole repo; 13 of the
// doc-path references (all outside packages/agent/nlp-engine) were fixed in
// that same pass, and this check's own scan of SCAN_ROOTS below — which
// deliberately excludes packages/agent and packages/nlp-engine, outside its
// scope — now counts 282 issue-number references and 0 doc-path references.
// Retroactively triaging all 282 remaining issue-number references is a
// separate, much larger undertaking — each one needs individual judgment (is
// this constraint-bearing, provenance-only, or a doc-pointer?) — not
// attempted here. What this DOES do: ratchet the count down-only, so the
// backlog can shrink over time but can never silently grow again without a
// deliberate, reviewed bump to the baseline below.
//
// Lower BASELINES whenever a change pays down part of the backlog. Never
// raise a baseline to accommodate a new reference — inline the constraint or
// cite an ADR instead, per the rule this check exists to hold the line on.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";

const REPO = join(dirname(new URL(import.meta.url).pathname), "..");

// packages/agent and packages/nlp-engine are outside this check's scope —
// see the file-level comment.
const SCAN_ROOTS = [
  "scripts",
  "packages/desktop-app",
  "packages/core",
  "packages/daemon",
  "packages/cli",
  "packages/nodespace-types",
  "packages/proto",
];
const EXTENSIONS = new Set([".rs", ".ts", ".svelte", ".js"]);
const EXCLUDE_DIR_NAMES = new Set(["node_modules", "target", ".git", "dist", "build"]);

// This checker's own source and test necessarily describe the patterns they
// scan for in comments/messages/fixtures, which would otherwise self-match.
const EXCLUDE_FILE_NAMES = new Set(["check-code-references.ts", "check-code-references.test.ts"]);

const ISSUE_NUMBER_PATTERNS: RegExp[] = [/core#\d+/, /\(#\d+\)/, /\b[Ii]ssue #\d+\b/];
const DOC_PATH_PATTERN = /nodespace-docs\//;

// Ratchet baselines. See the file-level comment: lower on paydown, never raise.
export const BASELINES = {
  issueNumberReferences: 282,
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
