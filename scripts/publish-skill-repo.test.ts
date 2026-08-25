// Covers the offline, deterministic parts of scripts/publish-skill-repo.ts:
// version normalization and file rendering. pushSkillUpdate (the
// GitHub-talking function) is intentionally not exercised here -- this suite
// runs as part of `bun run test:scripts` / `test:all` (the pre-push gate),
// which must stay fast and deterministic, not depend on network or a real
// SKILL_REPO_TOKEN.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import {
  normalizeVersion,
  readSkillSource,
  renderPublishFiles,
  sharedShimPaths,
  SKILL_REPO,
} from "./publish-skill-repo";

const REPO_ROOT = join(dirname(new URL(import.meta.url).pathname), "..");

describe("normalizeVersion", () => {
  test("strips a leading v", () => {
    expect(normalizeVersion("v0.2.2")).toBe("0.2.2");
  });

  test("leaves a bare version untouched", () => {
    expect(normalizeVersion("0.2.2")).toBe("0.2.2");
  });
});

describe("SKILL_REPO", () => {
  test("targets the public generated-only skill repo", () => {
    expect(SKILL_REPO).toBe("NodeSpaceAI/nodespace-skill");
  });
});

describe("readSkillSource", () => {
  test("reads the live packages/skill/SKILL.md and references/cli.md off disk", () => {
    const { body, referenceCli } = readSkillSource();
    // Read independently (not via the function under test) so this actually
    // catches the function reading a stale/wrong path, not just echoing it.
    const expectedBody = readFileSync(
      join(REPO_ROOT, "packages", "skill", "SKILL.md"),
      "utf8",
    );
    const expectedReferenceCli = readFileSync(
      join(REPO_ROOT, "packages", "skill", "references", "cli.md"),
      "utf8",
    );
    expect(body).toBe(expectedBody);
    expect(referenceCli).toBe(expectedReferenceCli);
  });

  // The checked-in SKILL.md body carries no frontmatter (renderPublishFiles
  // is the one place that adds it) -- a body that already starts with `---`
  // would double up frontmatter blocks in the published file.
  test("the body has no baked-in frontmatter", () => {
    const { body } = readSkillSource();
    expect(body.startsWith("---")).toBe(false);
  });
});

describe("sharedShimPaths", () => {
  // Guards the drift class packages/skill/src/tests/installer.test.ts's
  // "publishes every directory the agents install from" test guards
  // elsewhere: this derives the published file set from AGENTS instead of a
  // hardcoded list, so a shared file added to (or removed from) every
  // agent's shims here gets picked up automatically -- and this test fails
  // loudly if that derivation ever stops matching what today's AGENTS
  // actually declares as shared.
  test("is exactly SKILL.md and references/cli.md today", () => {
    expect(sharedShimPaths().sort()).toEqual(["SKILL.md", "references/cli.md"]);
  });

  test("excludes every harness-specific shim", () => {
    const shared = new Set(sharedShimPaths());
    expect(shared.has("shims/claude-code/nodespace-hook.ts")).toBe(false);
    expect(shared.has("shims/codex/nodespace-plugin.ts")).toBe(false);
    expect(shared.has("shims/gemini/nodespace-handler.ts")).toBe(false);
    expect(shared.has("shims/opencode/nodespace-plugin.ts")).toBe(false);
  });
});

describe("renderPublishFiles", () => {
  test("publishes exactly SKILL.md and references/cli.md under skills/nodespace/", () => {
    const files = renderPublishFiles("v0.2.2");
    expect(files.map((f) => f.relPath).sort()).toEqual([
      "skills/nodespace/SKILL.md",
      "skills/nodespace/references/cli.md",
    ]);
  });

  test("SKILL.md is spec-compliant frontmatter + the unmodified body", () => {
    const files = renderPublishFiles("v0.2.2");
    const skillMd = files.find((f) => f.relPath === "skills/nodespace/SKILL.md")!;
    const { body } = readSkillSource();

    expect(skillMd.content.startsWith("---\nname: nodespace\n")).toBe(true);
    expect(skillMd.content).toContain(body);
    // name must match the directory it publishes into.
    expect(skillMd.relPath.split("/")[1]).toBe("nodespace");
  });

  test("stamps a compatibility field with the released app version, within the spec's 500-char limit", () => {
    const files = renderPublishFiles("v0.2.2");
    const skillMd = files.find((f) => f.relPath === "skills/nodespace/SKILL.md")!;
    const m = /^compatibility:\s*(.+)$/m.exec(skillMd.content);
    expect(m).toBeTruthy();
    expect(m![1]).toContain("v0.2.2");
    expect(m![1].length).toBeLessThanOrEqual(500);
  });

  test("normalizes a leading v the same way for the compatibility field", () => {
    const withV = renderPublishFiles("v0.2.2").find(
      (f) => f.relPath === "skills/nodespace/SKILL.md",
    )!;
    const withoutV = renderPublishFiles("0.2.2").find(
      (f) => f.relPath === "skills/nodespace/SKILL.md",
    )!;
    expect(withV.content).toBe(withoutV.content);
  });

  test("references/cli.md is copied through verbatim", () => {
    const files = renderPublishFiles("v0.2.2");
    const referenceCli = files.find((f) => f.relPath === "skills/nodespace/references/cli.md")!;
    const { referenceCli: expected } = readSkillSource();
    expect(referenceCli.content).toBe(expected);
  });
});
