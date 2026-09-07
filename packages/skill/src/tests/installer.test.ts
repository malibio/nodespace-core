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

const { install, uninstall, isNodespaceBinaryOnPath, claudeCodePluginManagedSkillExists } =
  await import('../installer.js');
const { AGENTS, SHARED_SKILL_FRONTMATTER } = await import('../agents.js');

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

  // A skill folder with no frontmatter is not a valid skill under the Agent
  // Skills standard — `name` + `description` are the entire discovery surface.
  // Three of the four targets used to install without it, so the skill could
  // never activate there.
  it('prepends frontmatter to SKILL.md for every target, not just Claude Code', () => {
    for (const config of AGENTS) {
      expect(config.skillFrontmatter, `${config.name} has no frontmatter`).toBeTruthy();
      mkdirSync(config.detectionDir, { recursive: true });
      install([config.name], FAKE_PKG_ROOT);
      const content = readFileSync(join(config.installDir, 'SKILL.md'), 'utf8');
      expect(content.startsWith('---\nname: nodespace')).toBe(true);
      expect(content).toContain('allowed-tools: Bash(nodespace:*)');
      expect(content).toContain(SKILL_MD_CONTENT);
    }
  });

  // Four separate places enumerate what the skill is made of: this package's
  // `files` array (npm), scripts/build-skill.ts (Tauri bundle), the per-agent
  // `shims` lists (install), and context_assembly.rs (PTY). A path present in
  // one and missing from another ships a body linking to a file that isn't
  // there — silently, because nothing errors.
  it('publishes every directory the agents install from', () => {
    const pkg = JSON.parse(
      readFileSync(join(import.meta.dirname, '../../package.json'), 'utf8')
    ) as { files: string[] };

    // The first segment of each shim path: a directory (`references`, `shims`)
    // or a bare file (`SKILL.md`). npm's `files` accepts either form verbatim,
    // so an exact match is the whole check.
    const topLevel = new Set(AGENTS.flatMap(a => a.shims).map(s => s.split('/')[0]));
    for (const entry of topLevel) {
      expect(
        pkg.files,
        `"${entry}" is installed but not in package.json "files"`
      ).toContain(entry);
    }
  });

  it('installs the same frontmatter block for every target', () => {
    const blocks = new Set(AGENTS.map(a => a.skillFrontmatter));
    expect(blocks.size).toBe(1);
    expect([...blocks][0]).toBe(SHARED_SKILL_FRONTMATTER);
  });

  // The six fields below are the entire set the standard defines. Claude Code
  // tolerates extra keys, but other distribution paths hard-error on any key
  // they don't recognize — so a harness-specific field added to the shared
  // block would silently break installs everywhere else.
  it('uses only spec-defined frontmatter fields', () => {
    const SPEC_FIELDS = [
      'name',
      'description',
      'license',
      'compatibility',
      'metadata',
      'allowed-tools',
    ];
    const body = SHARED_SKILL_FRONTMATTER.replace(/^---\n/, '').replace(/---\n?$/, '');
    const topLevelKeys = body
      .split('\n')
      .filter(line => /^[A-Za-z][A-Za-z0-9-]*:/.test(line))
      .map(line => line.slice(0, line.indexOf(':')));
    expect(topLevelKeys.length).toBeGreaterThan(0);
    for (const key of topLevelKeys) {
      expect(SPEC_FIELDS, `"${key}" is not a spec-defined frontmatter field`).toContain(key);
    }
  });

  // `name` must match the directory the skill installs into, and is capped at
  // 64 characters of lowercase alphanumerics and hyphens.
  it('uses a spec-valid name matching the install directory', () => {
    const name = /^name:\s*(\S+)/m.exec(SHARED_SKILL_FRONTMATTER)?.[1];
    expect(name).toBe('nodespace');
    expect(name!.length).toBeLessThanOrEqual(64);
    expect(name).toMatch(/^[a-z0-9][a-z0-9-]*$/);
    for (const config of AGENTS) {
      expect(basename(config.installDir)).toBe(name);
    }
  });

  // The description is capped at 1024 characters by the spec. It is also the
  // only text an agent sees before deciding whether to load the skill, so it
  // must carry the vocabulary of the work it should be reached for.
  it('has a description within the spec length limit that covers docs/specs vocabulary', () => {
    const description = /description:\s*>\n([\s\S]*?)\n(?=[a-z-]+:|---)/m.exec(
      SHARED_SKILL_FRONTMATTER
    )?.[1];
    expect(description).toBeTruthy();
    const flattened = description!.trim().replace(/\s+/g, ' ');
    expect(flattened.length).toBeLessThanOrEqual(1024);
    for (const term of ['spec', 'ADR', 'architecture', 'design', 'plan']) {
      expect(flattened.toLowerCase(), `description omits "${term}"`).toContain(term.toLowerCase());
    }
  });

  it('installs all shims (SKILL.md + agent shim) when all source files exist', () => {
    const agentName = 'claude-code';
    const config = AGENTS.find(a => a.name === agentName)!;
    mkdirSync(config.detectionDir, { recursive: true });
    seedPkgRoot(FAKE_PKG_ROOT, config);

    const results = install([agentName], FAKE_PKG_ROOT);
    expect(results[0].installed).toHaveLength(config.shims.length);
    for (const shim of config.shims) {
      const relative = shim.startsWith('references/') ? shim : basename(shim);
      expect(existsSync(join(config.installDir, relative))).toBe(true);
    }
  });

  // SKILL.md links to `references/cli.md` by relative path. Shim paths are
  // flattened to a basename on install, so a reference flattened the same way
  // would leave the body pointing at a file that isn't where it says — the
  // agent follows the link, finds nothing, and silently loses the CLI
  // reference.
  it('installs references into a references/ subdirectory, not flattened', () => {
    for (const config of AGENTS) {
      const refs = config.shims.filter(s => s.startsWith('references/'));
      expect(refs.length, `${config.name} installs no references`).toBeGreaterThan(0);

      mkdirSync(config.detectionDir, { recursive: true });
      seedPkgRoot(FAKE_PKG_ROOT, config);
      install([config.name], FAKE_PKG_ROOT);

      for (const ref of refs) {
        expect(existsSync(join(config.installDir, ref)), `${config.name}: ${ref}`).toBe(true);
        expect(existsSync(join(config.installDir, basename(ref)))).toBe(false);
      }

      const body = readFileSync(join(config.installDir, 'SKILL.md'), 'utf8');
      for (const ref of refs) {
        if (body.includes(ref)) {
          expect(existsSync(join(config.installDir, ref))).toBe(true);
        }
      }
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
      mkdirSync(agent.detectionDir, { recursive: true });
      seedPkgRoot(FAKE_PKG_ROOT, agent);
      install([agent.name], FAKE_PKG_ROOT);
    }

    const results = uninstall();
    expect(results).toHaveLength(AGENTS.length);
    for (const result of results) {
      expect(result.removed.length).toBeGreaterThan(0);
    }
  });

  // Seeding the install dir by hand lets uninstall's own path assumptions go
  // unchallenged — which is exactly how uninstall kept using basename() after
  // install learned to preserve references/. Driving the real install() means
  // the two sides are tested against each other, not against a fixture that
  // agrees with whichever one is wrong.
  it('removes everything install() created, leaving no directory behind', () => {
    for (const config of AGENTS) {
      mkdirSync(config.detectionDir, { recursive: true });
      seedPkgRoot(FAKE_PKG_ROOT, config);
      const installed = install([config.name], FAKE_PKG_ROOT)[0].installed;
      expect(installed.length).toBe(config.shims.length);

      const removed = uninstall([config.name])[0].removed;
      expect(removed.length, `${config.name}: not everything was removed`).toBe(installed.length);

      for (const path of installed) {
        expect(existsSync(path), `${config.name}: ${path} survived uninstall`).toBe(false);
      }
      expect(
        existsSync(config.installDir),
        `${config.name}: install dir survived a full uninstall`
      ).toBe(false);
    }
  });

  // A leftover directory holding references/cli.md but no SKILL.md is worse
  // than either a clean removal or no removal at all: it is a malformed skill
  // folder sitting in the harness's scan path, produced by the command whose
  // job was to clean up.
  it('never leaves a reference file behind without its SKILL.md', () => {
    for (const config of AGENTS) {
      mkdirSync(config.detectionDir, { recursive: true });
      seedPkgRoot(FAKE_PKG_ROOT, config);
      install([config.name], FAKE_PKG_ROOT);
      uninstall([config.name]);

      for (const shim of config.shims.filter(s => s.startsWith('references/'))) {
        expect(
          existsSync(join(config.installDir, shim)),
          `${config.name}: ${shim} survived uninstall`
        ).toBe(false);
      }
    }
  });

  // Uninstall must not reach outside what it installed. Pruning "any empty
  // directory" would delete a user's own folder, empty the parent, and take
  // the whole install directory with it — none of it reported in `removed`.
  // Removing more than we installed is a worse failure than leaving something
  // behind.
  it('never deletes directories it did not install', () => {
    const config = AGENTS.find(a => a.name === 'claude-code')!;
    mkdirSync(config.detectionDir, { recursive: true });
    seedPkgRoot(FAKE_PKG_ROOT, config);
    install([config.name], FAKE_PKG_ROOT);

    const userDir = join(config.installDir, 'user-scripts');
    mkdirSync(userDir, { recursive: true });

    uninstall([config.name]);

    expect(existsSync(userDir), 'uninstall deleted a user-created directory').toBe(true);
    expect(existsSync(config.installDir)).toBe(true);
    expect(existsSync(join(config.installDir, 'references'))).toBe(false);
    expect(existsSync(join(config.installDir, 'SKILL.md'))).toBe(false);
  });

  it('preserves user files inside a directory it does own', () => {
    const config = AGENTS.find(a => a.name === 'claude-code')!;
    mkdirSync(config.detectionDir, { recursive: true });
    seedPkgRoot(FAKE_PKG_ROOT, config);
    install([config.name], FAKE_PKG_ROOT);

    const userNote = join(config.installDir, 'references', 'my-notes.md');
    writeFileSync(userNote, 'user content', 'utf8');

    uninstall([config.name]);

    expect(existsSync(userNote), 'uninstall deleted a user file inside references/').toBe(true);
  });

  it('still removes the install dir when a shim was already deleted by hand', () => {
    const config = AGENTS.find(a => a.name === 'claude-code')!;
    mkdirSync(config.detectionDir, { recursive: true });
    seedPkgRoot(FAKE_PKG_ROOT, config);
    install([config.name], FAKE_PKG_ROOT);

    rmSync(join(config.installDir, 'SKILL.md'));
    uninstall([config.name]);

    expect(existsSync(config.installDir)).toBe(false);
  });
});

describe('claudeCodePluginManagedSkillExists', () => {
  const claudeConfigDir = join(TMP, '.claude');
  const registryPath = join(claudeConfigDir, 'plugins', 'installed_plugins.json');

  function writeRegistry(plugins: Record<string, unknown>): void {
    mkdirSync(join(claudeConfigDir, 'plugins'), { recursive: true });
    writeFileSync(registryPath, JSON.stringify({ version: 2, plugins }), 'utf8');
  }

  it('returns false when the registry file does not exist at all', () => {
    expect(claudeCodePluginManagedSkillExists(claudeConfigDir)).toBe(false);
  });

  it('returns true when a nodespace plugin key is registered', () => {
    writeRegistry({ 'nodespace@nodespace-skill': [{ scope: 'user' }] });
    expect(claudeCodePluginManagedSkillExists(claudeConfigDir)).toBe(true);
  });

  // The marketplace half of the key is whatever label the user's own Claude
  // Code registered it under locally -- matching a fixed marketplace name
  // would miss a real install under a different one.
  it('matches regardless of which marketplace the plugin was registered under', () => {
    writeRegistry({ 'nodespace@some-other-marketplace': [{ scope: 'user' }] });
    expect(claudeCodePluginManagedSkillExists(claudeConfigDir)).toBe(true);
  });

  // Mirrors the shape of a real local installed_plugins.json containing only
  // unrelated plugins (verified against an actual file during design).
  it('returns false when only unrelated plugins are registered', () => {
    writeRegistry({ 'rust-analyzer-lsp@claude-plugins-official': [{ scope: 'user' }] });
    expect(claudeCodePluginManagedSkillExists(claudeConfigDir)).toBe(false);
  });

  it('fails open (returns false) on a malformed registry file rather than throwing', () => {
    mkdirSync(join(claudeConfigDir, 'plugins'), { recursive: true });
    writeFileSync(registryPath, '{ not valid json', 'utf8');
    expect(claudeCodePluginManagedSkillExists(claudeConfigDir)).toBe(false);
  });
});

describe('install — Claude Code plugin-managed reconciliation', () => {
  const config = AGENTS.find(a => a.name === 'claude-code')!;
  const registryPath = join(config.detectionDir, 'plugins', 'installed_plugins.json');

  function markPluginManaged(): void {
    mkdirSync(join(config.detectionDir, 'plugins'), { recursive: true });
    writeFileSync(
      registryPath,
      JSON.stringify({ version: 2, plugins: { 'nodespace@nodespace-skill': [{ scope: 'user' }] } }),
      'utf8'
    );
  }

  it('skips writing files and reports skipReason plugin-managed', () => {
    mkdirSync(config.detectionDir, { recursive: true });
    markPluginManaged();

    const results = install(['claude-code'], FAKE_PKG_ROOT);
    expect(results).toHaveLength(1);
    expect(results[0].installed).toEqual([]);
    expect(results[0].skipReason).toBe('plugin-managed');
    expect(existsSync(config.installDir)).toBe(false);
  });

  // An app-installed copy from before this reconciliation existed must not
  // be left behind once the marketplace copy is authoritative -- otherwise
  // "no NEW copy" would still leave the old one in place, and Claude Code
  // would still see the skill twice.
  it('cleans up a pre-existing app-installed copy once the marketplace copy is detected', () => {
    mkdirSync(config.detectionDir, { recursive: true });
    seedPkgRoot(FAKE_PKG_ROOT, config);
    install(['claude-code'], FAKE_PKG_ROOT);
    expect(existsSync(join(config.installDir, 'SKILL.md'))).toBe(true);

    markPluginManaged();
    const results = install(['claude-code'], FAKE_PKG_ROOT);

    expect(results[0].installed).toEqual([]);
    expect(results[0].skipReason).toBe('plugin-managed');
    expect(existsSync(config.installDir)).toBe(false);
  });

  it('does not block other agents from installing normally', () => {
    const gemini = AGENTS.find(a => a.name === 'gemini')!;
    mkdirSync(config.detectionDir, { recursive: true });
    markPluginManaged();
    mkdirSync(gemini.detectionDir, { recursive: true });
    seedPkgRoot(FAKE_PKG_ROOT, gemini);

    const results = install(['claude-code', 'gemini'], FAKE_PKG_ROOT);
    const geminiResult = results.find(r => r.agent === 'gemini')!;
    expect(geminiResult.installed.length).toBe(gemini.shims.length);
    expect(geminiResult.skipReason).toBeUndefined();
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
