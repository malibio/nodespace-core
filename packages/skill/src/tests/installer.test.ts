import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mkdirSync, rmSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join, basename } from 'node:path';
import { tmpdir } from 'node:os';

const TMP = join(tmpdir(), `nodespace-skill-test-${process.pid}`);
const FAKE_PKG_ROOT = join(TMP, 'pkg');

vi.mock('node:os', async (importOriginal) => {
  const actual = await importOriginal<typeof import('node:os')>();
  return { ...actual, homedir: () => TMP };
});

// Isolate from the ambient env: claude-code detection honors $CLAUDE_CONFIG_DIR,
// so an inherited value (e.g. a `claude-ns` profile) would point the default
// AGENTS at a real dir and break the mocked-home assumptions below. The dedicated
// CLAUDE_CONFIG_DIR describe sets it explicitly where it needs to.
delete process.env.CLAUDE_CONFIG_DIR;

const { install, uninstall, isNodespaceBinaryOnPath } = await import('../installer.js');
const { AGENTS } = await import('../agents.js');

const SKILL_MD_CONTENT = '# NodeSpace Skill\nTest content';
const SHIM_CONTENT = '// shim content';

function seedPkgRoot(root: string, agent: typeof AGENTS[number]): void {
  for (const shim of agent.shims) {
    const dir = join(root, shim.includes('/') ? shim.split('/').slice(0, -1).join('/') : '');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(root, shim), shim.endsWith('.md') ? SKILL_MD_CONTENT : SHIM_CONTENT, 'utf8');
  }
}

beforeEach(() => {
  mkdirSync(TMP, { recursive: true });
  mkdirSync(FAKE_PKG_ROOT, { recursive: true });
  writeFileSync(join(FAKE_PKG_ROOT, 'SKILL.md'), SKILL_MD_CONTENT, 'utf8');
});

afterEach(() => {
  rmSync(TMP, { recursive: true, force: true });
});

describe('AGENTS config', () => {
  it('defines four agents', () => {
    expect(AGENTS).toHaveLength(4);
    const names = AGENTS.map(a => a.name);
    expect(names).toContain('claude-code');
    expect(names).toContain('codex');
    expect(names).toContain('gemini');
    expect(names).toContain('opencode');
  });

  it('each agent has detectionDir, installDir, SKILL.md shim, and at least one agent shim', () => {
    for (const agent of AGENTS) {
      expect(agent.detectionDir).toBeTruthy();
      expect(agent.installDir).toBeTruthy();
      expect(agent.shims).toContain('SKILL.md');
      expect(agent.shims.length).toBeGreaterThan(1);
    }
  });

  it('install paths are under the expected agent dir', () => {
    const expectedDirs: Record<string, string> = {
      'claude-code': '.claude',
      codex: '.codex',
      gemini: '.gemini',
      opencode: '.opencode',
    };
    for (const agent of AGENTS) {
      expect(agent.installDir).toContain(expectedDirs[agent.name]);
    }
  });
});

describe('install', () => {
  it('returns empty array when no agents are detected', () => {
    const results = install(undefined, FAKE_PKG_ROOT);
    expect(results).toEqual([]);
  });

  it('installs SKILL.md when agent dir exists (only SKILL.md seeded)', () => {
    const agentName = 'claude-code';
    const config = AGENTS.find(a => a.name === agentName)!;
    mkdirSync(config.detectionDir, { recursive: true });

    const results = install([agentName], FAKE_PKG_ROOT);
    expect(results).toHaveLength(1);
    expect(results[0].agent).toBe(agentName);
    expect(results[0].installed).toHaveLength(1);
    expect(existsSync(join(config.installDir, 'SKILL.md'))).toBe(true);
    const expected = config.skillFrontmatter
      ? config.skillFrontmatter + '\n' + SKILL_MD_CONTENT
      : SKILL_MD_CONTENT;
    expect(readFileSync(join(config.installDir, 'SKILL.md'), 'utf8')).toBe(expected);
  });

  it('prepends Claude Code frontmatter to SKILL.md but copies verbatim for other agents', () => {
    const cc = AGENTS.find(a => a.name === 'claude-code')!;
    expect(cc.skillFrontmatter).toBeTruthy();
    mkdirSync(cc.detectionDir, { recursive: true });
    install(['claude-code'], FAKE_PKG_ROOT);
    const ccContent = readFileSync(join(cc.installDir, 'SKILL.md'), 'utf8');
    expect(ccContent.startsWith('---\nname: nodespace')).toBe(true);
    expect(ccContent).toContain('allowed-tools: Bash(nodespace:*)');
    expect(ccContent).toContain(SKILL_MD_CONTENT);

    const codex = AGENTS.find(a => a.name === 'codex')!;
    expect(codex.skillFrontmatter).toBeUndefined();
    mkdirSync(codex.detectionDir, { recursive: true });
    install(['codex'], FAKE_PKG_ROOT);
    expect(readFileSync(join(codex.installDir, 'SKILL.md'), 'utf8')).toBe(SKILL_MD_CONTENT);
  });

  it('installs all shims (SKILL.md + agent shim) when all source files exist', () => {
    const agentName = 'claude-code';
    const config = AGENTS.find(a => a.name === agentName)!;
    mkdirSync(config.detectionDir, { recursive: true });
    seedPkgRoot(FAKE_PKG_ROOT, config);

    const results = install([agentName], FAKE_PKG_ROOT);
    expect(results[0].installed).toHaveLength(config.shims.length);
    for (const shim of config.shims) {
      expect(existsSync(join(config.installDir, basename(shim)))).toBe(true);
    }
  });

  it('creates install directory if it does not exist', () => {
    const agentName = 'codex';
    const config = AGENTS.find(a => a.name === agentName)!;
    mkdirSync(config.detectionDir, { recursive: true });

    install([agentName], FAKE_PKG_ROOT);
    expect(existsSync(config.installDir)).toBe(true);
  });

  it('does NOT create install directory when no source files exist', () => {
    const agentName = 'gemini';
    const config = AGENTS.find(a => a.name === agentName)!;
    mkdirSync(config.detectionDir, { recursive: true });

    rmSync(join(FAKE_PKG_ROOT, 'SKILL.md'));

    const results = install([agentName], FAKE_PKG_ROOT);
    expect(results[0].installed).toHaveLength(0);
    expect(existsSync(config.installDir)).toBe(false);
  });

  it('detects multiple agents when their dirs exist', () => {
    const agentNames = ['claude-code', 'gemini'] as const;
    for (const name of agentNames) {
      const config = AGENTS.find(a => a.name === name)!;
      mkdirSync(config.detectionDir, { recursive: true });
    }

    const results = install(undefined, FAKE_PKG_ROOT);
    expect(results).toHaveLength(2);
    expect(results.map(r => r.agent).sort()).toEqual(['claude-code', 'gemini'].sort());
  });
});

describe('uninstall', () => {
  it('returns empty array when no agents are installed', () => {
    const results = uninstall();
    expect(results).toEqual([]);
  });

  it('removes SKILL.md and cleans up empty install dir', () => {
    const agentName = 'claude-code';
    const config = AGENTS.find(a => a.name === agentName)!;
    mkdirSync(config.installDir, { recursive: true });
    writeFileSync(join(config.installDir, 'SKILL.md'), SKILL_MD_CONTENT, 'utf8');

    const results = uninstall([agentName]);
    expect(results).toHaveLength(1);
    expect(results[0].removed).toHaveLength(1);
    expect(existsSync(join(config.installDir, 'SKILL.md'))).toBe(false);
    expect(existsSync(config.installDir)).toBe(false);
  });

  it('does not remove install dir when other files remain', () => {
    const agentName = 'opencode';
    const config = AGENTS.find(a => a.name === agentName)!;
    mkdirSync(config.installDir, { recursive: true });
    writeFileSync(join(config.installDir, 'SKILL.md'), SKILL_MD_CONTENT, 'utf8');
    writeFileSync(join(config.installDir, 'other-file.md'), 'other content', 'utf8');

    uninstall([agentName]);
    expect(existsSync(config.installDir)).toBe(true);
    expect(existsSync(join(config.installDir, 'SKILL.md'))).toBe(false);
    expect(existsSync(join(config.installDir, 'other-file.md'))).toBe(true);
  });

  it('uninstalls from all detected agents when no target specified', () => {
    for (const agent of AGENTS) {
      mkdirSync(agent.installDir, { recursive: true });
      for (const shim of agent.shims) {
        writeFileSync(join(agent.installDir, basename(shim)), SKILL_MD_CONTENT, 'utf8');
      }
    }

    const results = uninstall();
    expect(results).toHaveLength(AGENTS.length);
    for (const result of results) {
      expect(result.removed.length).toBeGreaterThan(0);
    }
  });
});

describe('isNodespaceBinaryOnPath', () => {
  it('returns true when execFileSync exits 0', async () => {
    vi.resetModules();
    vi.doMock('node:child_process', () => ({
      execFileSync: () => Buffer.from('nodespace 0.1.0\n'),
    }));
    const { isNodespaceBinaryOnPath: check } = await import('../installer.js');
    expect(check()).toBe(true);
    vi.doUnmock('node:child_process');
    vi.resetModules();
  });

  it('returns false when execFileSync throws (binary not found)', async () => {
    // Patch child_process on the installer module's live reference by re-importing
    // after hoisting the mock.  vi.doMock + resetModules lets us control this.
    vi.resetModules();
    vi.doMock('node:child_process', () => ({
      execFileSync: () => { throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' }); },
    }));
    const { isNodespaceBinaryOnPath: check } = await import('../installer.js');
    expect(check()).toBe(false);
    vi.doUnmock('node:child_process');
    vi.resetModules();
  });
});

describe('install PATH warning', () => {
  it('writes a warning to stderr when nodespace is not on PATH', async () => {
    vi.resetModules();
    vi.doMock('node:child_process', () => ({
      execFileSync: () => { throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' }); },
    }));
    const { install: freshInstall } = await import('../installer.js');

    const stderrSpy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    freshInstall([], FAKE_PKG_ROOT);
    expect(stderrSpy).toHaveBeenCalledWith(expect.stringContaining('nodespace` is not on $PATH'));

    stderrSpy.mockRestore();
    vi.doUnmock('node:child_process');
    vi.resetModules();
  });

  it('does not write a warning when nodespace is on PATH', async () => {
    vi.resetModules();
    vi.doMock('node:child_process', () => ({
      execFileSync: () => Buffer.from('nodespace 0.1.0\n'),
    }));
    const { install: freshInstall } = await import('../installer.js');

    const stderrSpy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    freshInstall([], FAKE_PKG_ROOT);
    expect(stderrSpy).not.toHaveBeenCalled();

    stderrSpy.mockRestore();
    vi.doUnmock('node:child_process');
    vi.resetModules();
  });
});

describe('CLAUDE_CONFIG_DIR', () => {
  afterEach(() => {
    delete process.env.CLAUDE_CONFIG_DIR;
    vi.resetModules();
  });

  it('claude-code detects + installs into $CLAUDE_CONFIG_DIR when set', async () => {
    const custom = join(TMP, 'custom-claude-profile');
    process.env.CLAUDE_CONFIG_DIR = custom;
    vi.resetModules();
    const { AGENTS: A } = await import('../agents.js');
    const cc = A.find(a => a.name === 'claude-code')!;
    expect(cc.detectionDir).toBe(custom);
    expect(cc.installDir).toBe(join(custom, 'skills', 'nodespace'));
  });

  it('claude-code falls back to ~/.claude when $CLAUDE_CONFIG_DIR is unset', async () => {
    delete process.env.CLAUDE_CONFIG_DIR;
    vi.resetModules();
    const { AGENTS: A } = await import('../agents.js');
    const cc = A.find(a => a.name === 'claude-code')!;
    expect(cc.installDir).toBe(join(TMP, '.claude', 'skills', 'nodespace'));
  });
});
