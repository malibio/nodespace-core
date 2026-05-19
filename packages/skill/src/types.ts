export type AgentName = 'claude-code' | 'codex' | 'gemini' | 'opencode';

export interface AgentConfig {
  name: AgentName;
  detectionDir: string;
  installDir: string;
  shims: string[];
}

export interface InstallResult {
  agent: AgentName;
  installed: string[];
}

export interface UninstallResult {
  agent: AgentName;
  removed: string[];
}
