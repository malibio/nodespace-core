//! First-launch skill installer.
//!
//! Runs the bundled `packages/skill` installer (`dist/install.js`, executed
//! directly via `bun` — never `npx`/`npm`, see module docs on
//! `resolve_installer_path`) to copy SKILL.md and agent shims into detected
//! agents' directories. Persists completion state to `~/.nodespace/setup.json`
//! so subsequent launches are no-ops once installation succeeds.
//!
//! Also verifies that the `nodespace` CLI is resolvable on $PATH and emits a
//! warning if not (the skill is useless until the CLI is installed).
//!
//! # Failure surfacing
//!
//! A genuine install failure is logged at `WARN` and pushed to the frontend
//! (`skill:install-failed`) exactly once — the first time it happens.
//! Subsequent launches keep retrying (the environment may since have been
//! fixed — e.g. `bun` got installed), but log at `DEBUG` and skip the event
//! so a persistent failure never becomes per-launch log/UI spam. A later
//! success clears the persisted failure flag.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Emitter, Manager};

const SETUP_FILE: &str = ".nodespace/setup.json";

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SetupState {
    pub skill_installed: bool,
    /// Set when the most recent install attempt failed and cleared on
    /// success. Used solely to decide whether a failure is "new" (surface
    /// it) or a repeat of an already-known failure (log quietly, no event).
    pub skill_install_failed: bool,
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

pub async fn reset_skill_state() -> Result<()> {
    write_setup_state(&SetupState {
        skill_installed: false,
        skill_install_failed: false,
    })
    .await
}

async fn write_setup_state(state: &SetupState) -> Result<()> {
    let path = setup_path()?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("Failed to create ~/.nodespace dir")?;
    }
    let json = serde_json::to_string_pretty(state).context("Failed to serialize setup state")?;
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
pub async fn install_skill(force: bool, app: &AppHandle) -> SkillSetupResult {
    // Check idempotency guard unless forced.
    let mut previously_failed = false;
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
            Ok(state) => previously_failed = state.skill_install_failed,
            Err(e) => {
                tracing::warn!("Could not read setup state: {:#}", e);
            }
        }
    }

    let cli_on_path = check_cli_on_path();

    let installer_path = match resolve_installer_path(app) {
        Ok(path) => path,
        Err(e) => {
            return finish_failed(app, previously_failed, cli_on_path, e).await;
        }
    };

    // Run the installer in a blocking thread so we don't hold the async
    // runtime during the child process execution.
    let result = tokio::task::spawn_blocking(move || run_skill_installer(&installer_path)).await;

    match result {
        Err(join_err) => {
            finish_failed(
                app,
                previously_failed,
                cli_on_path,
                format!("Installer task panicked: {join_err}"),
            )
            .await
        }
        Ok(Err(exec_err)) => finish_failed(app, previously_failed, cli_on_path, exec_err).await,
        Ok(Ok(agents)) => {
            // Persist the setup flag so we don't re-run on the next launch,
            // and clear any previously-recorded failure.
            let state = SetupState {
                skill_installed: true,
                skill_install_failed: false,
            };
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

/// Persist the failed state and produce the result, logging + emitting a
/// frontend event only when this failure is new (see module docs).
async fn finish_failed(
    app: &AppHandle,
    previously_failed: bool,
    cli_on_path: bool,
    error: String,
) -> SkillSetupResult {
    let state = SetupState {
        skill_installed: false,
        skill_install_failed: true,
    };
    if let Err(e) = write_setup_state(&state).await {
        tracing::warn!("Failed to persist setup state: {:#}", e);
    }

    if previously_failed {
        // Already known and already surfaced once — keep retrying quietly.
        tracing::debug!("Skill install failed again: {}", error);
    } else {
        tracing::warn!("Skill install failed: {}", error);
        if let Err(e) = app.emit(
            "skill:install-failed",
            serde_json::json!({ "error": error }),
        ) {
            tracing::warn!("Failed to emit skill:install-failed: {:#}", e);
        }
    }

    SkillSetupResult {
        success: false,
        agents_installed: vec![],
        cli_on_path,
        cli_warning: cli_warning(cli_on_path),
        error: Some(error),
    }
}

/// Resolve the path to the built skill installer (`dist/install.js`).
///
/// The installer ships bundled inside the app (declared as a Tauri
/// `resources` entry — see `tauri.conf.json` and `scripts/build-skill.ts`)
/// rather than being published to npm and resolved at runtime via `npx`:
/// this is a local-first desktop app and startup should not depend on
/// registry availability. Falls back to the source-checkout path when
/// running outside the Tauri resource pipeline (e.g. `cargo test`/`cargo run`
/// against a monorepo checkout where `build:skill` has been run directly).
fn resolve_installer_path<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    use tauri::path::BaseDirectory;

    if let Ok(path) = app
        .path()
        .resolve("resources/skill/dist/install.js", BaseDirectory::Resource)
    {
        if path.exists() {
            return Ok(path);
        }
    }

    let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("skill")
        .join("dist")
        .join("install.js");
    if fallback.exists() {
        return Ok(fallback);
    }

    Err(format!(
        "Skill installer not found (checked bundled resource and {}). \
         Run `bun run build:skill` from the workspace root.",
        fallback.display()
    ))
}

/// Run the installer directly via `bun` — never `npx`/`npm` (this repo is
/// Bun-only) — and collect installed agent names from stdout. Returns an
/// error string on non-zero exit.
fn run_skill_installer(installer_path: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("bun")
        .arg(installer_path)
        .arg("install")
        .output()
        .map_err(|e| format!("Failed to launch bun: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    tracing::debug!("skill installer stdout: {}", stdout);
    if !stderr.is_empty() {
        tracing::debug!("skill installer stderr: {}", stderr);
    }

    if !output.status.success() {
        let detail = if stderr.is_empty() {
            stdout.to_string()
        } else {
            stderr.to_string()
        };
        return Err(format!(
            "Skill installer exited with status {}: {}",
            output.status,
            detail.trim()
        ));
    }

    // Parse agent names from success lines like "✓ claude-code: installed 2 file(s)".
    // Filter on the original line first so file-path sub-lines ("  → /path/...")
    // and diagnostic text ("Checked:", "To install manually...") are excluded.
    let agents = stdout
        .lines()
        .filter(|line| line.trim_start().starts_with('✓'))
        .filter_map(|line| {
            let line = line.trim_start_matches(['✓', ' ', '\t']);
            let agent = line.split(':').next()?.trim();
            if !agent.is_empty() {
                Some(agent.to_string())
            } else {
                None
            }
        })
        .collect();

    Ok(agents)
}

pub(crate) fn cli_warning(cli_on_path: bool) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `resolve_installer_path` must never fall through to `npx`/`npm` (the
    /// bug this issue fixes) — it either finds the bundled Tauri resource or
    /// the source-checkout fallback. `mock_app()` has no bundled resources
    /// registered, so this exercises the fallback branch and asserts it
    /// lands on the real `packages/skill/dist/install.js` built by
    /// `bun run build:skill` (staged before this test runs — see
    /// scripts/test-gate.ts and CLAUDE.md's Rust test instructions).
    #[test]
    fn resolve_installer_path_falls_back_to_source_checkout_dist() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();

        let resolved = resolve_installer_path(&handle)
            .expect("dist/install.js must exist — run `bun run build:skill` first");

        assert!(
            resolved.ends_with("skill/dist/install.js"),
            "expected the source-checkout dist/install.js, got {}",
            resolved.display()
        );
        assert!(resolved.exists());
    }

    /// Guards against a future edit re-introducing `npx`/`npm` — the exact
    /// invocation this issue fixes (exit 127, "command not found").
    #[test]
    fn run_skill_installer_never_shells_out_to_npx_or_npm() {
        let src = include_str!("skill_setup.rs");
        // Restricted to the invocation site itself, not this guard or the
        // doc comments that explain the history of the bug being fixed.
        let invocation_lines: Vec<&str> =
            src.lines().filter(|l| l.contains("Command::new")).collect();
        for line in invocation_lines {
            assert!(
                !line.contains("\"npx\"") && !line.contains("\"npm\""),
                "found a Command::new invoking npx/npm: {line}"
            );
        }
    }
}
