//! Agent subsystem: local inference, ACP transport, tool execution.
//!
//! This crate contains the business logic for the agent layer, decoupled
//! from Tauri. The desktop-app crate provides thin Tauri command bindings
//! that delegate to types defined here.

// Shared types, traits, and interface contracts for agent subsystems
pub mod agent_types;
pub use agent_types::*;

// Local agent subsystem: model management, inference, tool execution
pub mod local_agent;

// Shared agent guidance rules: single source of truth for tool strategy,
// schema creation, and node reference guidance (issue #1089). Consumed by
// `prompt_assembler` (local Ollama agent) and by ADR-032 context-file
// writers in `acp`.
pub mod agent_guidance;

// Prompt assembly: hardcoded base + graph-stored overrides
pub mod prompt_assembler;

// Skill seeding templates: default skill nodes inserted on first run.
// Skill discovery itself is LLM-orchestrated via the `search_skills` tool
// (issue #1130) — there is no longer a pre-turn intent pipeline.
pub mod skill_pipeline;

// Property access helpers for namespaced node properties
pub mod props;

// ACP (Agent Communication Protocol) subsystem
pub mod acp;

// PTY-based agent session engine (ADR-032)
pub mod pty;
