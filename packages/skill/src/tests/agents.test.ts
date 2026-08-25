import { describe, it, expect } from 'vitest';
import { SHARED_SKILL_FRONTMATTER, buildSkillFrontmatter } from '../agents.js';

// buildSkillFrontmatter() is the escape hatch scripts/publish-skill-repo.ts uses
// to stamp a `compatibility` line onto the frontmatter it pushes to
// NodeSpaceAI/nodespace-skill, without touching the installer path (AGENTS still
// installs the plain SHARED_SKILL_FRONTMATTER, unchanged).
describe('buildSkillFrontmatter', () => {
  it('returns SHARED_SKILL_FRONTMATTER unchanged when called with no options', () => {
    expect(buildSkillFrontmatter()).toBe(SHARED_SKILL_FRONTMATTER);
  });

  it('returns SHARED_SKILL_FRONTMATTER unchanged when compatibility is omitted', () => {
    expect(buildSkillFrontmatter({})).toBe(SHARED_SKILL_FRONTMATTER);
  });

  it('inserts a compatibility field right before the closing "---"', () => {
    const result = buildSkillFrontmatter({ compatibility: 'Targets NodeSpace app v0.2.2' });
    expect(result).toContain('\ncompatibility: Targets NodeSpace app v0.2.2\n---\n');
    // Every other field from the shared block must still be present, unchanged.
    expect(result).toContain('name: nodespace');
    expect(result).toContain('allowed-tools: Bash(nodespace:*)');
    // Exactly one "---\n" pair still closes the block (opening + closing).
    expect(result.match(/^---$/gm)).toHaveLength(2);
  });

  it('rejects a compatibility string over the spec\'s 500-character limit', () => {
    const tooLong = 'x'.repeat(501);
    expect(() => buildSkillFrontmatter({ compatibility: tooLong })).toThrow(/500-character limit/);
  });

  it('accepts a compatibility string at exactly the 500-character limit', () => {
    const atLimit = 'x'.repeat(500);
    expect(() => buildSkillFrontmatter({ compatibility: atLimit })).not.toThrow();
  });
});
