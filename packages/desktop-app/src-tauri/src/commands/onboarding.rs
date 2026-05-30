//! Onboarding wizard Tauri commands (Issue #1180).
//!
//! Handles first-launch setup: PATH configuration and Claude Code skill
//! installation. Completion state is persisted to `~/.nodespace/config.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Current onboarding status returned to the frontend on startup.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingStatus {
    pub completed: bool,
    pub path_configured: bool,
    pub skill_configured: bool,
    pub claude_code_detected: bool,
    pub path_already_configured: bool,
}

/// Shape of `~/.nodespace/config.json` on disk.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct NodespaceConfig {
    #[serde(default)]
    onboarding_completed: bool,
    #[serde(default)]
    integrations: IntegrationsConfig,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct IntegrationsConfig {
    path_configured: bool,
    skill_configured: bool,
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn nodespace_config_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    Ok(home.join(".nodespace").join("config.json"))
}

async fn read_config() -> Result<NodespaceConfig, String> {
    let path = nodespace_config_path()?;
    if !path.exists() {
        return Ok(NodespaceConfig::default());
    }
    let raw = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read config: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("Failed to parse config: {e}"))
}

async fn write_config(cfg: &NodespaceConfig) -> Result<(), String> {
    let path = nodespace_config_path()?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create ~/.nodespace dir: {e}"))?;
    }
    let serialized = serde_json::to_string_pretty(cfg)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    tokio::fs::write(&path, serialized)
        .await
        .map_err(|e| format!("Failed to write config: {e}"))
}

/// Return true if the PATH export line is already present in the given file.
async fn file_contains_nodespace_path(path: &PathBuf) -> bool {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => content.contains("$HOME/.nodespace/bin"),
        Err(_) => false,
    }
}

/// Append `export PATH` line to a shell file if the file exists and the line
/// is not already present.  Returns `true` if the file was modified.
async fn append_path_to_file(path: &PathBuf) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    if file_contains_nodespace_path(path).await {
        return Ok(false);
    }
    let line = "\n# NodeSpace CLI\nexport PATH=\"$HOME/.nodespace/bin:$PATH\"\n";
    let mut content = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    content.push_str(line);
    tokio::fs::write(path, content)
        .await
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    Ok(true)
}

// ── commands ─────────────────────────────────────────────────────────────────

/// Read persisted onboarding state and detect installed integrations.
#[tauri::command]
pub async fn check_onboarding_status() -> Result<OnboardingStatus, String> {
    let cfg = read_config().await?;

    let home = dirs::home_dir().ok_or("Could not determine home directory")?;

    let claude_code_detected = home.join(".claude").exists();

    // Check whether the PATH export is already in any shell config.
    let zshrc = home.join(".zshrc");
    let bash_profile = home.join(".bash_profile");
    let path_already_configured = file_contains_nodespace_path(&zshrc).await
        || file_contains_nodespace_path(&bash_profile).await;

    Ok(OnboardingStatus {
        completed: cfg.onboarding_completed,
        path_configured: cfg.integrations.path_configured,
        skill_configured: cfg.integrations.skill_configured,
        claude_code_detected,
        path_already_configured,
    })
}

/// Append the NodeSpace PATH export to `~/.zshrc` and/or `~/.bash_profile`
/// (whichever exist). Idempotent — will not add the line if already present.
#[tauri::command]
pub async fn configure_path() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;

    append_path_to_file(&home.join(".zshrc")).await?;
    append_path_to_file(&home.join(".bash_profile")).await?;

    Ok(())
}

const SKILL_MD_CONTENT: &str = r#"# NodeSpace Knowledge Graph

NodeSpace is running locally. Use the `nodespace` CLI or the skill shims to
interact with the knowledge graph.

## Available Tools

Use `nodespace_*` tools to interact with the knowledge graph:
- `nodespace_create_node` — Create a new node
- `nodespace_get_node` — Retrieve a node by ID
- `nodespace_update_node` — Update node content or properties
- `nodespace_search_semantic` — Search nodes by content
- `nodespace_get_children` — Get child nodes

## When to Use

- When the user asks to save, retrieve, or organize information
- When capturing notes, tasks, or decisions from a coding session
- When the user references their NodeSpace or knowledge graph
"#;

/// Write `SKILL.md` to `~/.claude/skills/nodespace/SKILL.md`.
/// Creates parent directories as needed. Safe to call repeatedly.
#[tauri::command]
pub async fn configure_skill() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let skill_dir = home.join(".claude").join("skills").join("nodespace");

    tokio::fs::create_dir_all(&skill_dir)
        .await
        .map_err(|e| format!("Failed to create skill directory: {e}"))?;

    tokio::fs::write(skill_dir.join("SKILL.md"), SKILL_MD_CONTENT)
        .await
        .map_err(|e| format!("Failed to write SKILL.md: {e}"))?;

    Ok(())
}

/// Persist the onboarding completion state to `~/.nodespace/config.json`.
#[tauri::command]
pub async fn complete_onboarding(
    path_configured: bool,
    skill_configured: bool,
) -> Result<(), String> {
    let cfg = NodespaceConfig {
        onboarding_completed: true,
        integrations: IntegrationsConfig {
            path_configured,
            skill_configured,
        },
    };
    write_config(&cfg).await
}
