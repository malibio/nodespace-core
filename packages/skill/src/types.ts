export type AgentName = 'claude-code' | 'codex' | 'gemini' | 'opencode';

export interface AgentConfig {
  name: AgentName;
  detectionDir: string;
  installDir: string;
  shims: string[];
  /**
   * Frontmatter to prepend to `SKILL.md` when installing for this agent. Claude
   * Code discovers a skill by the YAML `name` + `description` frontmatter; the
   * shared SKILL.md body carries none (it is agent-agnostic), so agents that
   * need it supply it here and the installer writes `frontmatter + body`.
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
