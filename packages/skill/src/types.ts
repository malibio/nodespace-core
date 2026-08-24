export type AgentName = 'claude-code' | 'codex' | 'gemini' | 'opencode';

export interface AgentConfig {
  name: AgentName;
  detectionDir: string;
  installDir: string;
  shims: string[];
  /**
   * Frontmatter to prepend to `SKILL.md` when installing for this agent.
   *
   * The Agent Skills standard discovers a skill by its YAML `name` +
   * `description`, and the checked-in SKILL.md body carries none so that the
   * file stays a plain body with no harness assumptions baked in. The installer
   * writes `frontmatter + body`.
   *
   * Every target supplies this today — a skill folder without frontmatter is
   * not a valid skill under the standard. It stays optional only so a future
   * target that genuinely needs a different block, or none, can say so.
   */
  skillFrontmatter?: string;
}

export interface InstallResult {
  agent: AgentName;
  installed: string[];
}

export interface UninstallResult {
  agent: AgentName;
  removed: string[];
}
