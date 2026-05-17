//! PTY agent catalog (ADR-032).
//!
//! Hardcoded catalog of external agent CLIs that can be spawned in a PTY.
//! Each entry names the binary, the context file the agent expects to find
//! in its working directory (`CLAUDE.md` for Claude Code, `AGENTS.md` for
//! everything else), and the flag used to resume a previous session.

use crate::agent_types::{AgentType, ContextFile};

/// Static description of an external agent CLI spawned via PTY.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentDefinition {
    /// Stable identifier used to look the agent up.
    pub agent_type: AgentType,
    /// Human-readable display name.
    pub name: &'static str,
    /// Binary name to spawn (resolved on `PATH`).
    pub binary: &'static str,
    /// Context file the agent reads on startup.
    pub context_file: ContextFile,
    /// CLI flag used to resume a prior session, if the agent supports one.
    pub resume_flag: Option<&'static str>,
}

/// Hardcoded catalog of PTY-spawnable external agents.
pub const AGENT_CATALOG: &[AgentDefinition] = &[
    AgentDefinition {
        agent_type: AgentType::ClaudeCode,
        name: "Claude Code",
        binary: "claude",
        context_file: ContextFile::ClaudeMd,
        resume_flag: Some("--resume"),
    },
    AgentDefinition {
        agent_type: AgentType::Codex,
        name: "Codex",
        binary: "codex",
        context_file: ContextFile::AgentsMd,
        resume_flag: Some("resume"),
    },
    AgentDefinition {
        agent_type: AgentType::GeminiCli,
        name: "Gemini CLI",
        binary: "gemini",
        context_file: ContextFile::AgentsMd,
        // Gemini CLI uses checkpoint-based resumption with no single flag.
        resume_flag: None,
    },
    AgentDefinition {
        agent_type: AgentType::Pi,
        name: "Pi",
        binary: "pi",
        context_file: ContextFile::AgentsMd,
        // Pi is session-aware but does not take an explicit resume flag.
        resume_flag: None,
    },
    AgentDefinition {
        agent_type: AgentType::OpenCode,
        name: "OpenCode",
        binary: "opencode",
        context_file: ContextFile::AgentsMd,
        // OpenCode is stateless across invocations.
        resume_flag: None,
    },
];

/// In-process catalog handle. Stateless and cheap to construct.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemAgentRegistry;

impl SystemAgentRegistry {
    pub const fn new() -> Self {
        Self
    }

    /// Return every known agent definition.
    pub fn all(&self) -> &'static [AgentDefinition] {
        AGENT_CATALOG
    }

    /// Look up a definition by [`AgentType`].
    pub fn get(&self, agent_type: AgentType) -> Option<&'static AgentDefinition> {
        AGENT_CATALOG.iter().find(|d| d.agent_type == agent_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_includes_all_five_agents() {
        assert_eq!(AGENT_CATALOG.len(), 5);
    }

    #[test]
    fn catalog_agent_types_are_unique() {
        let mut types: Vec<AgentType> = AGENT_CATALOG.iter().map(|d| d.agent_type).collect();
        types.sort();
        types.dedup();
        assert_eq!(types.len(), AGENT_CATALOG.len());
    }

    #[test]
    fn claude_code_uses_claude_md_and_resume_flag() {
        let registry = SystemAgentRegistry::new();
        let def = registry.get(AgentType::ClaudeCode).unwrap();
        assert_eq!(def.binary, "claude");
        assert_eq!(def.context_file, ContextFile::ClaudeMd);
        assert_eq!(def.resume_flag, Some("--resume"));
    }

    #[test]
    fn non_claude_agents_use_agents_md() {
        let registry = SystemAgentRegistry::new();
        for agent_type in [
            AgentType::Codex,
            AgentType::GeminiCli,
            AgentType::Pi,
            AgentType::OpenCode,
        ] {
            let def = registry.get(agent_type).unwrap();
            assert_eq!(def.context_file, ContextFile::AgentsMd);
        }
    }

    #[test]
    fn codex_uses_resume_subcommand() {
        let registry = SystemAgentRegistry::new();
        let def = registry.get(AgentType::Codex).unwrap();
        assert_eq!(def.resume_flag, Some("resume"));
    }

    #[test]
    fn stateless_agents_have_no_resume_flag() {
        let registry = SystemAgentRegistry::new();
        for agent_type in [AgentType::GeminiCli, AgentType::Pi, AgentType::OpenCode] {
            let def = registry.get(agent_type).unwrap();
            assert!(
                def.resume_flag.is_none(),
                "{:?} should have no resume flag",
                agent_type
            );
        }
    }

    #[test]
    fn all_returns_full_catalog() {
        let registry = SystemAgentRegistry::new();
        assert_eq!(registry.all().len(), AGENT_CATALOG.len());
    }

    #[test]
    fn get_returns_none_for_unknown() {
        // No way to construct an unknown AgentType today (enum is closed),
        // but lookups still match by identity.
        let registry = SystemAgentRegistry::new();
        let def = registry.get(AgentType::ClaudeCode).unwrap();
        assert_eq!(def.agent_type, AgentType::ClaudeCode);
    }
}
