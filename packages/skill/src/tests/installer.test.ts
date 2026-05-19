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

const { install, uninstall } = await import('../installer.js');
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
    expect(readFileSync(join(config.installDir, 'SKILL.md'), 'utf8')).toBe(SKILL_MD_CONTENT);
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
