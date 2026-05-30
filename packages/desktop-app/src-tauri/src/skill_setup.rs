//! First-launch skill installer (Issue #1199).
//!
//! Invokes `npx @nodespaceai/skill install` to copy SKILL.md and agent shims
//! into detected agents' directories. Persists completion state to
//! `~/.nodespace/setup.json` so subsequent launches are no-ops.
//!
//! Also verifies that the `nodespace` CLI is resolvable on $PATH and emits a
//! warning if not (the skill is useless until the CLI is installed).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

const SETUP_FILE: &str = ".nodespace/setup.json";

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SetupState {
    pub skill_installed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSetupResult {
    pub success: bool,
    /// Agents the skill was installed into (e.g. ["claude-code"]).
    pub agents_installed: Vec<String>,
    /// true if `nodespace` CLI was found on PATH.
    pub cli_on_path: bool,
    /// Human-readable warning shown in the UI when cli_on_path is false.
    pub cli_warning: Option<String>,
    pub error: Option<String>,
}

fn setup_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot resolve home directory")?;
    Ok(home.join(SETUP_FILE))
}

pub async fn read_setup_state() -> Result<SetupState> {
    let path = setup_path()?;
    if !path.exists() {
        return Ok(SetupState::default());
    }
    let raw = tokio::fs::read_to_string(&path)
        .await
        .context("Failed to read ~/.nodespace/setup.json")?;
    serde_json::from_str(&raw).context("Failed to parse ~/.nodespace/setup.json")
}

async fn write_setup_state(state: &SetupState) -> Result<()> {
    let path = setup_path()?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("Failed to create ~/.nodespace dir")?;
    }
    let json =
        serde_json::to_string_pretty(state).context("Failed to serialize setup state")?;
    tokio::fs::write(&path, json)
        .await
        .context("Failed to write ~/.nodespace/setup.json")
}

/// Check whether `nodespace --version` resolves on $PATH.
/// Runs synchronously — safe to call from a blocking context.
pub fn check_cli_on_path() -> bool {
    Command::new("nodespace")
        .args(["--version"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run the skill installer. If `force` is false, this is a no-op when
/// `~/.nodespace/setup.json` already marks skill_installed = true.
pub async fn install_skill(force: bool) -> SkillSetupResult {
    // Check idempotency guard unless forced.
    if !force {
        match read_setup_state().await {
            Ok(state) if state.skill_installed => {
                let cli_on_path = check_cli_on_path();
                return SkillSetupResult {
                    success: true,
                    agents_installed: vec![],
                    cli_on_path,
                    cli_warning: cli_warning(cli_on_path),
                    error: None,
                };
            }
            Err(e) => {
                tracing::warn!("Could not read setup state: {:#}", e);
            }
            _ => {}
        }
    }

    let cli_on_path = check_cli_on_path();

    // Run `npx @nodespaceai/skill install` in a blocking thread so we don't
    // hold the async runtime during the child process execution.
    let result = tokio::task::spawn_blocking(run_skill_installer).await;

    match result {
        Err(join_err) => SkillSetupResult {
            success: false,
            agents_installed: vec![],
            cli_on_path,
            cli_warning: cli_warning(cli_on_path),
            error: Some(format!("Installer task panicked: {join_err}")),
        },
        Ok(Err(exec_err)) => SkillSetupResult {
            success: false,
            agents_installed: vec![],
            cli_on_path,
            cli_warning: cli_warning(cli_on_path),
            error: Some(exec_err),
        },
        Ok(Ok(agents)) => {
            // Persist the setup flag so we don't re-run on the next launch.
            let state = SetupState { skill_installed: true };
            if let Err(e) = write_setup_state(&state).await {
                tracing::warn!("Failed to persist setup state: {:#}", e);
            }
            SkillSetupResult {
                success: true,
                agents_installed: agents,
                cli_on_path,
                cli_warning: cli_warning(cli_on_path),
                error: None,
            }
        }
    }
}

/// Spawn `npx @nodespaceai/skill install` and collect installed agent names
/// from stdout. Returns an error string on non-zero exit.
fn run_skill_installer() -> Result<Vec<String>, String> {
    let output = Command::new("npx")
        .args(["--yes", "@nodespaceai/skill", "install"])
        .output()
        .map_err(|e| format!("Failed to launch npx: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    tracing::debug!("skill installer stdout: {}", stdout);
    if !stderr.is_empty() {
        tracing::debug!("skill installer stderr: {}", stderr);
    }

    if !output.status.success() {
        let detail = if stderr.is_empty() { stdout.to_string() } else { stderr.to_string() };
        return Err(format!(
            "Skill installer exited with status {}: {}",
            output.status,
            detail.trim()
        ));
    }

    // Parse agent names from lines like "✓ claude-code: installed 2 file(s)"
    let agents = stdout
        .lines()
        .filter_map(|line| {
            let line = line.trim_start_matches(['✓', '⚠', ' ', '\t']);
            let agent = line.split(':').next()?;
            let agent = agent.trim();
            if !agent.is_empty() && !agent.starts_with("No supported") {
                Some(agent.to_string())
            } else {
                None
            }
        })
        .collect();

    Ok(agents)
}

fn cli_warning(cli_on_path: bool) -> Option<String> {
    if cli_on_path {
        return None;
    }
    Some(
        "The `nodespace` CLI was not found on $PATH. \
         Install it via the NodeSpace DMG or `cargo install nodespace-cli`, \
         then restart your terminal."
            .to_string(),
    )
}
