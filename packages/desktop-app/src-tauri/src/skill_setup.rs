//! First-launch skill installer.
//!
//! Runs the bundled `packages/skill` installer to copy SKILL.md and agent
//! shims into detected agents' directories. Persists completion state to
//! `~/.nodespace/setup.json` so subsequent launches are no-ops once
//! installation succeeds.
//!
//! # Two ways to run the installer
//!
//! Preferred: the compiled standalone binary (`bun build --compile packages/
//! skill/src/install.ts`, staged as the `nodespace-skill-installer`
//! `externalBin` sidecar — see `resolve_compiled_installer_path`). No
//! external runtime needed at all; a packaged app's end user has no reason
//! to have `bun` or `node` installed, so this is what a real release build
//! actually uses.
//!
//! Fallback: the plain JS build (`dist/install.js`, executed via `bun` or
//! `node` — never `npx`/`npm`, see [`INSTALLER_RUNTIMES`]) when the compiled
//! binary isn't available — a dev/source checkout that hasn't run the
//! compile step, or a platform this hasn't been wired up for yet. See
//! `resolve_installer_path`.
//!
//! Also verifies that the `nodespace` CLI is resolvable on $PATH and emits a
//! warning if not (the skill is useless until the CLI is installed).
//!
//! # Failure surfacing
//!
//! A genuine install failure is logged at `WARN`; subsequent launches with
//! the same still-unresolved failure log at `DEBUG` only (retries keep
//! happening — the environment may since have been fixed, e.g. `bun` got
//! installed — but the log doesn't repeat the warning every launch). A
//! later success clears the persisted failure flag.
//!
//! [`SkillSetupResult::failure_is_new`] tells a caller whether *this*
//! failure is the first one seen (vs. a repeat of an already-known one).
//! Only the fire-and-forget startup call site (`lib.rs`) acts on it — it has
//! no other way to reach the user, so it pushes a one-time `skill:install-failed`
//! event to the frontend when `failure_is_new` is true. The onboarding/manual
//! retry commands (`configure_skill`, `install_skill`) don't need this: their
//! caller is a UI action already awaiting the command's return value, so
//! `error` on the returned `SkillSetupResult` is all they need — emitting the
//! event there too would show the same failure twice (once inline, once as a
//! toast).
//!
//! # Coexistence with the skill-triggered CLI/GUI install path
//!
//! `nodespace-website`'s `install.sh` (the `curl -fsSL https://nodespace.ai/
//! install.sh | sh` one-liner, also invoked non-interactively via `--no-gui`
//! from the skill's own failure-recovery guidance) installs the `nodespace`/
//! `nodespaced` **binaries** — it never touches SKILL.md or an agent's skill
//! directory, so it cannot race or clobber what this module installs. The
//! two installers share exactly one piece of state end to end:
//! `~/.nodespace/setup.json`'s `skill_installed` flag, read by
//! [`install_skill`]'s idempotency guard above. `install.sh` does not write
//! this file, so a machine that got its CLI via `install.sh` and later
//! launches the GUI for the first time still gets exactly one real skill
//! install here — not a second, silent one racing a manual edit the user
//! made to their installed SKILL.md.
//!
//! If a future change has `install.sh`'s skill-triggered entry point also
//! install skill files (today it does not), it must set `skill_installed:
//! true` in this same file/schema on success so this module's early-return
//! guard in [`install_skill`] correctly no-ops instead of re-running the
//! bundled installer over a skill the user already set up manually.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::ffi::{OsStr, OsString};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

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
    /// true when `error` is set AND this is the first launch to see this
    /// particular failure (vs. a persisted repeat of an already-known one).
    /// Always false when `success` is true. See module docs.
    pub failure_is_new: bool,
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

/// Check whether `nodespace --version` resolves on an augmented $PATH.
/// Runs synchronously (spawning up to two subprocesses: `path_helper` on
/// macOS, then `nodespace` itself) — safe to call from a blocking context,
/// not from an async task directly.
///
/// Deliberately does NOT rely on the raw PATH this process inherited. On
/// macOS, an app launched via LaunchServices (Finder/Dock/Spotlight -- how
/// virtually every non-developer user launches it) gets a minimal system
/// PATH that excludes Homebrew (`/opt/homebrew/bin`) and other
/// shell-configured locations, since those are only added by shell startup
/// files a GUI-launched process never sources. Without augmentation, this
/// check reports "not found" even when the CLI is genuinely correctly
/// installed and would resolve fine from a terminal (core#2350).
pub fn check_cli_on_path() -> bool {
    Command::new("nodespace")
        .args(["--version"])
        .env("PATH", augmented_cli_path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Builds the augmented PATH used by [`check_cli_on_path`]. Mirrors
/// `packages/agent/src/pty/detection.rs`'s `augmented_path()`, which solves
/// the identical problem for AI-agent-CLI detection (`claude`, `codex`,
/// etc.) -- `nodespace-app` has no dependency on `nodespace-agent`, so this
/// is a deliberate mirror rather than a shared function; keep the two in
/// sync if either changes.
fn augmented_cli_path() -> OsString {
    let shell_path = path_helper_output();
    let home = dirs::home_dir();
    let existing = std::env::var_os("PATH").unwrap_or_default();
    build_augmented_path(&existing, home.as_deref(), shell_path.as_deref())
}

/// Pure PATH-building logic, factored out of [`augmented_cli_path`] so it
/// can be unit tested without mutating the real process-wide `PATH` env var
/// (tests run many-per-process; mutating global env state is a known
/// footgun in this codebase).
fn build_augmented_path(
    existing_path: &OsStr,
    home_dir: Option<&Path>,
    shell_path: Option<&OsStr>,
) -> OsString {
    let mut extra: Vec<PathBuf> = Vec::new();

    if let Some(shell_path) = shell_path {
        extra.extend(std::env::split_paths(shell_path));
    }

    if let Some(home) = home_dir {
        extra.push(home.join(".npm-global").join("bin"));
        extra.push(home.join(".local").join("bin"));
        extra.push(home.join(".cargo").join("bin"));
        extra.push(home.join("go").join("bin"));
    }
    extra.push(PathBuf::from("/opt/homebrew/bin"));
    extra.push(PathBuf::from("/usr/local/bin"));

    extra.extend(std::env::split_paths(existing_path));

    std::env::join_paths(extra).unwrap_or_else(|_| existing_path.to_os_string())
}

/// Run `/usr/libexec/path_helper -s` and extract the PATH value from its
/// output. Output looks like: `PATH="/usr/local/bin:/usr/bin:/bin"; export PATH;`
#[cfg(target_os = "macos")]
fn path_helper_output() -> Option<OsString> {
    let output = std::process::Command::new("/usr/libexec/path_helper")
        .arg("-s")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let path_line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with("PATH="))?;
    let after_prefix = path_line.trim_start().strip_prefix("PATH=\"")?;
    let value = after_prefix
        .strip_suffix("\"; export PATH;")
        .or_else(|| after_prefix.strip_suffix('"'))?;
    Some(OsString::from(value))
}

#[cfg(not(target_os = "macos"))]
fn path_helper_output() -> Option<OsString> {
    None
}

/// Serializes the whole read-state → run-installer → write-state critical
/// section below across all three call sites (startup, the onboarding
/// `configure_skill` command, and the manual-retry `install_skill` command).
/// Without this, two concurrent calls (e.g. a user clicking "Reinstall" in
/// Settings while the startup call is still running) could interleave their
/// reads/writes of `~/.nodespace/setup.json` and silently drop one side's
/// result (a stale `skill_install_failed`/`skill_installed` value).
static INSTALL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Run the skill installer. If `force` is false, this is a no-op when
/// `~/.nodespace/setup.json` already marks skill_installed = true.
pub async fn install_skill(force: bool, app: &AppHandle) -> SkillSetupResult {
    let _guard = INSTALL_LOCK.lock().await;

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
                    failure_is_new: false,
                };
            }
            Ok(state) => previously_failed = state.skill_install_failed,
            Err(e) => {
                tracing::warn!("Could not read setup state: {:#}", e);
            }
        }
    }

    let cli_on_path = check_cli_on_path();

    let installer = match resolve_installer(app) {
        Ok(installer) => installer,
        Err(e) => {
            return finish_failed(previously_failed, cli_on_path, e).await;
        }
    };

    // Run the installer in a blocking thread so we don't hold the async
    // runtime during the child process execution.
    let result = tokio::task::spawn_blocking(move || run_skill_installer(&installer)).await;

    match result {
        Err(join_err) => {
            finish_failed(
                previously_failed,
                cli_on_path,
                format!("Installer task panicked: {join_err}"),
            )
            .await
        }
        Ok(Err(exec_err)) => finish_failed(previously_failed, cli_on_path, exec_err).await,
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
                failure_is_new: false,
            }
        }
    }
}

/// Persist the failed state and produce the result, logging at `WARN` only
/// when this failure is new (see module docs) — `failure_is_new` on the
/// returned result tells the caller whether to also push a one-time
/// frontend event; only the startup call site does.
async fn finish_failed(
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
        // Already known — keep retrying quietly, no repeat WARN/event.
        tracing::debug!("Skill install failed again: {}", error);
    } else {
        tracing::warn!("Skill install failed: {}", error);
    }

    SkillSetupResult {
        success: false,
        agents_installed: vec![],
        cli_on_path,
        cli_warning: cli_warning(cli_on_path),
        error: Some(error),
        failure_is_new: !previously_failed,
    }
}

/// How to invoke the skill installer — resolved once per [`install_skill`]
/// call by [`resolve_installer`], then dispatched on in
/// [`run_skill_installer`].
enum Installer {
    /// The compiled standalone binary (see module docs) — no external
    /// runtime needed. `resource_root` is where SKILL.md/shims/references
    /// actually live, passed as `--resource-root` (the binary has no
    /// source-relative sibling directory to infer it from the way
    /// `dist/install.js` does).
    Compiled {
        binary: PathBuf,
        resource_root: PathBuf,
    },
    /// The plain JS build, needing `bun` or `node` to execute — see
    /// [`INSTALLER_RUNTIMES`].
    Script { path: PathBuf },
}

/// Resolve which [`Installer`] to use: the compiled standalone binary if
/// it's available for this platform/build, the plain JS script otherwise.
fn resolve_installer<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<Installer, String> {
    if let Some((binary, resource_root)) = resolve_compiled_installer_path(app) {
        return Ok(Installer::Compiled {
            binary,
            resource_root,
        });
    }
    resolve_installer_path(app).map(|path| Installer::Script { path })
}

/// Locate the compiled standalone installer binary bundled beside the app's
/// own executable — Tauri's `externalBin` convention, the same mechanism
/// `daemon_setup` already uses for `nodespaced`/`nodespace` (see
/// `daemon_setup::sidecar_path_from_exe`'s doc comment for why sidecars land
/// beside the running executable rather than under a Resources tree) — plus
/// the resource root it needs (`SKILL.md`/`shims`/`references`, staged the
/// same place `dist/install.js` already was). Returns `None` when either
/// piece is missing, e.g. a platform this hasn't been wired up for yet, or a
/// dev/source checkout that hasn't run the compile step — the caller falls
/// through to [`Installer::Script`] in that case.
fn resolve_compiled_installer_path<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> Option<(PathBuf, PathBuf)> {
    use tauri::path::BaseDirectory;

    let exe = std::env::current_exe().ok()?;
    let binary_name = crate::daemon_setup::bundled_sidecar_name("nodespace-skill-installer");
    let binary = crate::daemon_setup::sidecar_path_from_exe(&exe, &binary_name)?;
    if !binary.exists() {
        return None;
    }

    let resource_root = app
        .path()
        .resolve("resources/skill", BaseDirectory::Resource)
        .ok()?;
    if !resource_root.exists() {
        return None;
    }

    Some((binary, resource_root))
}

/// Resolve the path to the built skill installer (`dist/install.js`) — the
/// [`Installer::Script`] fallback.
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

    // Keep the build-machine path out of the string that ends up in the UI
    // (SkillSetupResult.error / the skill:install-failed toast) — it's only
    // useful for whoever built the package, not whoever is running it.
    tracing::debug!(
        "Skill installer not found — checked bundled resource and {}",
        fallback.display()
    );
    Err("Skill installer is missing from this build (packaging issue).".to_string())
}

/// Runtimes capable of executing the installer's output, tried in this
/// order. `packages/skill`'s installer (`src/install.ts` and everything it
/// imports) has zero Bun-specific API usage — it's plain `node:fs`/`node:path`
/// TypeScript compiled to a standard ESM module via `tsc` — so it runs
/// identically under either. `bun` is tried first (this repo's own dev-time
/// convention, always present in CI/dev environments), `node` second: `bun`
/// is a requirement for *building* NodeSpace, never for *running* the shipped
/// app, so a packaged app's end user has no reason to have it — `node` is far
/// more likely to already be on the machine of someone who'd actually use
/// these AI-agent integrations (Claude Code, Codex, Gemini CLI, OpenCode are
/// themselves typically Node-ecosystem-adjacent). Only when neither is found
/// does the failure surface to the user.
const INSTALLER_RUNTIMES: [&str; 2] = ["bun", "node"];

/// Build the `<runtime> <installer_path> install` invocation — the single
/// place that decides how the installer is launched, so both production and
/// tests exercise the exact same command construction. Never `npx`/`npm` —
/// this repo is Bun-only, and `@nodespaceai/skill` isn't published anyway.
fn installer_command(installer_path: &Path, runtime: &str) -> Command {
    let mut cmd = Command::new(runtime);
    cmd.arg(installer_path).arg("install");
    cmd
}

/// Build the `<binary> install --resource-root <resource_root>` invocation
/// for the compiled standalone installer — the zero-runtime path. Mirrors
/// `installer_command`'s separation of command construction from execution
/// so both are independently testable.
fn compiled_installer_command(binary: &Path, resource_root: &Path) -> Command {
    let mut cmd = Command::new(binary);
    cmd.arg("install").arg("--resource-root").arg(resource_root);
    cmd
}

/// Run the installer and collect installed agent names from stdout. Returns
/// an error string on non-zero exit. Dispatches on [`Installer`]: the
/// compiled binary needs no runtime fallthrough at all, the script needs
/// [`run_installer_with_runtimes`]'s `bun`-then-`node` logic.
fn run_skill_installer(installer: &Installer) -> Result<Vec<String>, String> {
    match installer {
        Installer::Compiled {
            binary,
            resource_root,
        } => {
            let output = compiled_installer_command(binary, resource_root)
                .output()
                .map_err(|e| format!("Failed to launch the compiled skill installer: {e}"))?;
            parse_installer_output(output)
        }
        Installer::Script { path } => run_installer_with_runtimes(path, &INSTALLER_RUNTIMES),
    }
}

/// The runtime-fallthrough logic, parameterized over the runtime list so
/// tests can exercise it with fake, explicit-path "runtimes" instead of the
/// real `bun`/`node` resolved via the process's actual `$PATH`. Falls
/// through to the next runtime only on `NotFound` (a runtime that exists but
/// fails to spawn for some other reason is a real error, not a cue to try
/// the next one).
///
/// Deliberately does NOT test this via a mutated process-global `$PATH`:
/// `cargo test` runs the whole suite in one process, so a test that
/// temporarily narrows `$PATH` (even restored afterward) can race any other
/// test concurrently resolving a bare command name — nodespace-sync hit
/// exactly this class of bug with a mutated `umask` and lost a third of its
/// pre-push runs to it. Passing absolute paths as the "runtime" strings
/// sidesteps `$PATH` resolution entirely, so the test stays deterministic
/// under parallel execution.
fn run_installer_with_runtimes(
    installer_path: &Path,
    runtimes: &[&str],
) -> Result<Vec<String>, String> {
    let mut output = None;
    for runtime in runtimes {
        match installer_command(installer_path, runtime).output() {
            Ok(out) => {
                output = Some(out);
                break;
            }
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("Failed to launch {runtime}: {e}")),
        }
    }

    let Some(output) = output else {
        return Err(
            "Neither `bun` nor `node` was found on $PATH. One of them is required to install \
                 NodeSpace's AI-agent integrations (Claude Code, Codex, Gemini CLI, OpenCode). \
                 Install Node from https://nodejs.org (or Bun from https://bun.sh) and relaunch \
                 NodeSpace — or ignore this if you don't use one of those agents."
                .to_string(),
        );
    };

    parse_installer_output(output)
}

/// Parse a finished installer invocation's exit status and stdout into
/// installed-agent names, or an error string. Shared by both
/// [`Installer::Compiled`] and [`Installer::Script`] — the underlying
/// `packages/skill` `install()` call and its console output are identical
/// either way, only how the process got launched differs.
fn parse_installer_output(output: std::process::Output) -> Result<Vec<String>, String> {
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
         Install it with `curl -fsSL https://nodespace.ai/install.sh | sh`, \
         via the NodeSpace DMG, or `brew install --cask nodespaceai/nodespace/nodespace`, \
         then restart your terminal."
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The CLI-missing warning must point at the actual current install
    /// path (`install.sh`), not just the stale `cargo install`/DMG-only
    /// guidance — this is the message a user sees in the GUI when the CLI
    /// isn't on $PATH, and it must not drift from what `SKILL.md`'s
    /// failure-recovery table and `packages/skill`'s installer warning tell
    /// an agent to do for the exact same underlying problem.
    #[test]
    fn cli_warning_mentions_install_script() {
        let warning = cli_warning(false).expect("cli_warning(false) must return Some");
        assert!(
            warning.contains("nodespace.ai/install.sh"),
            "expected the install.sh one-liner in the warning, got: {warning}"
        );
    }

    #[test]
    fn cli_warning_is_none_when_cli_on_path() {
        assert!(cli_warning(true).is_none());
    }

    /// core#2350: `check_cli_on_path()` must search Homebrew's bin dirs and
    /// common per-user install dirs, not just whatever bare PATH the app
    /// process happened to inherit -- a LaunchServices-launched app never
    /// gets those from shell startup files. Uses injected fake values, not
    /// real env mutation, per this codebase's no-global-env-mutation-in-tests
    /// rule (cargo test runs many tests in one process).
    #[test]
    fn build_augmented_path_includes_homebrew_and_home_dirs() {
        let existing = OsStr::new("/usr/bin:/bin");
        let home = PathBuf::from("/Users/testuser");
        let result = build_augmented_path(existing, Some(&home), None);
        let paths: Vec<PathBuf> = std::env::split_paths(&result).collect();

        assert!(paths.contains(&PathBuf::from("/opt/homebrew/bin")));
        assert!(paths.contains(&PathBuf::from("/usr/local/bin")));
        assert!(paths.contains(&home.join(".cargo").join("bin")));
        assert!(paths.contains(&home.join(".local").join("bin")));
        assert!(paths.contains(&home.join(".npm-global").join("bin")));
        assert!(paths.contains(&home.join("go").join("bin")));
    }

    #[test]
    fn build_augmented_path_preserves_existing_path_entries() {
        let existing = OsStr::new("/custom/bin:/another/bin");
        let result = build_augmented_path(existing, None, None);
        let paths: Vec<PathBuf> = std::env::split_paths(&result).collect();

        assert!(paths.contains(&PathBuf::from("/custom/bin")));
        assert!(paths.contains(&PathBuf::from("/another/bin")));
    }

    #[test]
    fn build_augmented_path_works_with_no_home_dir_or_shell_path() {
        // Must not panic even when dirs::home_dir() or path_helper both
        // return None (e.g. a sandboxed or unusual environment).
        let existing = OsStr::new("/usr/bin");
        let result = build_augmented_path(existing, None, None);
        let paths: Vec<PathBuf> = std::env::split_paths(&result).collect();

        assert!(paths.contains(&PathBuf::from("/usr/bin")));
        assert!(paths.contains(&PathBuf::from("/opt/homebrew/bin")));
    }

    /// Shell-configured entries (from `path_helper` on macOS) must come
    /// before the raw inherited PATH's entries, so a Homebrew-installed
    /// `nodespace` is found ahead of any same-named binary already resolvable
    /// on the minimal inherited PATH.
    #[test]
    fn build_augmented_path_orders_shell_path_before_existing_path() {
        let existing = OsStr::new("/usr/bin");
        let shell_path = OsStr::new("/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin");
        let result = build_augmented_path(existing, None, Some(shell_path));
        let paths: Vec<PathBuf> = std::env::split_paths(&result).collect();

        let homebrew_idx = paths
            .iter()
            .position(|p| p == &PathBuf::from("/opt/homebrew/bin"))
            .expect("/opt/homebrew/bin must be present");
        let existing_usr_bin_idx = paths
            .iter()
            .rposition(|p| p == &PathBuf::from("/usr/bin"))
            .expect("/usr/bin must be present");
        assert!(
            homebrew_idx < existing_usr_bin_idx,
            "shell-configured entries must be searched before the raw inherited PATH"
        );
    }

    /// Closes the gap between `build_augmented_path` (well-tested, pure) and
    /// the real wiring in `check_cli_on_path` -- a regression that deletes
    /// the `.env("PATH", augmented_cli_path())` call from `check_cli_on_path`
    /// while leaving `build_augmented_path` untouched would otherwise pass
    /// every other test in this module. Only reads `PATH`/`HOME` (never
    /// mutates them), so this is safe alongside the parallel test run.
    #[test]
    fn augmented_cli_path_includes_homebrew_bin() {
        let result = augmented_cli_path();
        let paths: Vec<PathBuf> = std::env::split_paths(&result).collect();
        assert!(
            paths.contains(&PathBuf::from("/opt/homebrew/bin")),
            "augmented_cli_path() must include /opt/homebrew/bin, got: {result:?}"
        );
        assert!(
            paths.contains(&PathBuf::from("/usr/local/bin")),
            "augmented_cli_path() must include /usr/local/bin, got: {result:?}"
        );
    }

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

    /// `mock_app()` (used throughout this test module) has no bundled
    /// `externalBin` sidecar or `resources` tree, and the test binary's own
    /// `current_exe()` has no `nodespace-skill-installer` sitting beside it
    /// — so `resolve_compiled_installer_path` must return `None` and
    /// `resolve_installer` must fall through to `Installer::Script`, never
    /// silently produce a `Compiled` variant pointing at a binary that
    /// doesn't exist.
    #[test]
    fn resolve_installer_falls_back_to_script_when_no_compiled_binary_is_bundled() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();

        assert!(
            resolve_compiled_installer_path(&handle).is_none(),
            "mock_app() has no bundled compiled installer sidecar"
        );

        let installer =
            resolve_installer(&handle).expect("falls back to the script path successfully");
        assert!(
            matches!(installer, Installer::Script { .. }),
            "expected Installer::Script when no compiled binary is available"
        );
    }

    /// Same shape as `installer_command_invokes_bun_directly_not_npx_or_npm`
    /// but for the compiled-binary path: asserts the constructed `Command`
    /// invokes the binary directly (not through a shell) with `install
    /// --resource-root <path>`, via the `Command`'s `Debug` representation
    /// so a non-literal construction can't hide from this check either.
    #[test]
    fn compiled_installer_command_invokes_the_binary_directly_with_resource_root() {
        let cmd = compiled_installer_command(
            Path::new("/opt/nodespace/nodespace-skill-installer"),
            Path::new("/opt/nodespace/resources/skill"),
        );
        let debug_repr = format!("{cmd:?}");

        assert!(
            debug_repr.starts_with("\"/opt/nodespace/nodespace-skill-installer\""),
            "expected the compiled binary as the program, got: {debug_repr}"
        );
        assert!(
            debug_repr.contains("\"install\""),
            "expected the install subcommand, got: {debug_repr}"
        );
        assert!(
            debug_repr.contains("\"--resource-root\"")
                && debug_repr.contains("\"/opt/nodespace/resources/skill\""),
            "expected --resource-root and its value, got: {debug_repr}"
        );
    }

    /// End-to-end proof that the whole compiled-binary path actually works,
    /// not just that the pieces type-check: compiles a REAL standalone
    /// binary from the real `packages/skill/src/install.ts` (via `bun build
    /// --compile`, the exact command the real build pipeline runs), then runs
    /// it through the same `compiled_installer_command` + `parse_installer_output`
    /// pair `run_skill_installer`'s `Installer::Compiled` branch uses,
    /// against an isolated fake `$HOME` — asserting SKILL.md and the
    /// claude-code shim land where `packages/skill`'s agent config says they
    /// should, with ZERO bun/node on the child's `$PATH` (only the compiled
    /// binary itself, which needs no external runtime by construction).
    /// Requires `bun` on $PATH to run the compile step (this repo is
    /// Bun-only, so the test/pre-push environment always has it).
    #[test]
    fn run_skill_installer_compiled_variant_actually_installs_with_zero_runtime_on_path() {
        let skill_src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("skill")
            .join("src")
            .join("install.ts");
        assert!(
            skill_src.exists(),
            "packages/skill/src/install.ts must exist at {}",
            skill_src.display()
        );
        let resource_root = skill_src
            .parent()
            .and_then(Path::parent)
            .expect("install.ts is two levels under packages/skill")
            .to_path_buf();

        let scratch = tempfile::tempdir().expect("create scratch dir for the compiled binary");
        let binary_path = scratch.path().join("nodespace-skill-installer-test");

        let compile = Command::new("bun")
            .args(["build", "--compile"])
            .arg(&skill_src)
            .arg("--outfile")
            .arg(&binary_path)
            .output()
            .expect("bun must be on $PATH to compile the test binary");
        assert!(
            compile.status.success(),
            "bun build --compile failed — stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&compile.stdout),
            String::from_utf8_lossy(&compile.stderr)
        );
        assert!(binary_path.exists(), "compiled binary was not produced");

        let fake_home = tempfile::tempdir().expect("create isolated fake $HOME");
        std::fs::create_dir_all(fake_home.path().join(".claude"))
            .expect("create fake .claude dir so the installer detects claude-code");

        // Same construction the production Installer::Compiled branch of
        // run_skill_installer uses (compiled_installer_command +
        // parse_installer_output), with HOME/PATH overridden on the CHILD
        // process only via Command::env (matching the existing
        // run_skill_installer_actually_installs_into_an_isolated_home test's
        // pattern for the script variant) — never the parent test process's
        // own environment, so this stays safe under parallel test execution.
        // PATH is scrubbed to bare OS binaries to prove zero runtime
        // dependency, not just that it happens to work with bun on PATH.
        let output = compiled_installer_command(&binary_path, &resource_root)
            .env("HOME", fake_home.path())
            .env("PATH", "/usr/bin:/bin")
            .output()
            .expect("compiled binary must run standalone");
        let agents = parse_installer_output(output).expect("install succeeds with zero runtime");

        assert_eq!(agents, vec!["claude-code".to_string()]);
        let installed_skill = fake_home.path().join(".claude/skills/nodespace/SKILL.md");
        assert!(
            installed_skill.exists(),
            "SKILL.md was not installed into the fake $HOME at {}",
            installed_skill.display()
        );
        let installed_shim = fake_home
            .path()
            .join(".claude/skills/nodespace/nodespace-hook.ts");
        assert!(
            installed_shim.exists(),
            "claude-code shim was not installed"
        );
    }

    /// Inspects the actual constructed `Command`'s program (via its `Debug`
    /// impl, which prints the literal program string Rust will `exec`) —
    /// not the source text — so this can't be defeated by a non-literal
    /// construction (`Command::new(&format!("np{}", "x"))` would still show
    /// up here, unlike a source-text grep). Guards against a future edit
    /// re-introducing `npx`/`npm` — the exact invocation this issue fixes
    /// (exit 127, "command not found", because `@nodespaceai/skill` isn't
    /// published).
    #[test]
    fn installer_command_invokes_bun_directly_not_npx_or_npm() {
        let cmd = installer_command(Path::new("/tmp/fake/install.js"), "bun");
        let debug_repr = format!("{cmd:?}");

        assert!(
            debug_repr.starts_with("\"bun\""),
            "expected bun as the program, got: {debug_repr}"
        );
        assert!(
            !debug_repr.contains("npx") && !debug_repr.contains("npm"),
            "found npx/npm in the constructed command: {debug_repr}"
        );
    }

    /// `run_installer_with_runtimes` falls through from the first runtime to
    /// the second on `NotFound` rather than surfacing the failure
    /// immediately — the actual bug this issue fixes (a packaged app's end
    /// user has no reason to have `bun`, but is far more likely to have
    /// `node`). Uses absolute paths as the "runtimes" — a nonexistent one
    /// first, a real executable fake shim second — so this proves the
    /// fallback actually ran (not just that the function returns success for
    /// some other reason), without touching the process's real `$PATH` (see
    /// the mutation-safety note on `run_installer_with_runtimes`).
    #[test]
    fn run_installer_with_runtimes_falls_back_past_a_missing_first_runtime() {
        let fake_dir = tempfile::tempdir().expect("create scratch dir");
        let missing_runtime = fake_dir.path().join("definitely-does-not-exist-xyz");
        let working_runtime = fake_dir.path().join("fake-node-shim");
        std::fs::write(
            &working_runtime,
            "#!/bin/sh\necho '✓ claude-code: installed 1 file(s)'\nexit 0\n",
        )
        .expect("write fake node shim");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&working_runtime, std::fs::Permissions::from_mode(0o755))
                .expect("make fake node shim executable");
        }

        let result = run_installer_with_runtimes(
            Path::new("/tmp/fake/install.js"),
            &[
                missing_runtime.to_str().unwrap(),
                working_runtime.to_str().unwrap(),
            ],
        );

        assert_eq!(
            result,
            Ok(vec!["claude-code".to_string()]),
            "expected the fallback runtime to run and report success"
        );
    }

    /// When no runtime in the list resolves, the error names both `bun` and
    /// `node` (the real, hardcoded message — not the fake paths passed in
    /// for the fallthrough mechanics) — so the message stays accurate for
    /// the (more common, for a packaged app's end user) case where `node`
    /// was the one actually tried and missing.
    #[test]
    fn run_installer_with_runtimes_error_names_both_real_runtimes_when_none_resolve() {
        let fake_dir = tempfile::tempdir().expect("create scratch dir");
        let missing_a = fake_dir.path().join("does-not-exist-a");
        let missing_b = fake_dir.path().join("does-not-exist-b");

        let result = run_installer_with_runtimes(
            Path::new("/tmp/fake/install.js"),
            &[missing_a.to_str().unwrap(), missing_b.to_str().unwrap()],
        );

        let err = result.expect_err("neither fake runtime resolves");
        assert!(err.contains("bun"), "error should mention bun: {err}");
        assert!(err.contains("node"), "error should mention node: {err}");
    }

    /// End-to-end: actually runs the real built installer (via the same
    /// `installer_command` production code uses) against an isolated,
    /// throwaway `$HOME` — never the real one — and asserts SKILL.md and
    /// the claude-code shim land where `packages/skill`'s agent config says
    /// they should. Requires `bun` on $PATH (this repo is Bun-only, so the
    /// test/pre-push environment always has it — see CLAUDE.md) and
    /// `packages/skill` already built (`bun run build:skill`, staged by
    /// scripts/test-gate.ts before this runs).
    #[test]
    fn run_skill_installer_actually_installs_into_an_isolated_home() {
        let app = tauri::test::mock_app();
        let installer_path = resolve_installer_path(&app.handle().clone())
            .expect("dist/install.js must exist — run `bun run build:skill` first");

        let fake_home = tempfile::tempdir().expect("create isolated fake $HOME");
        std::fs::create_dir_all(fake_home.path().join(".claude"))
            .expect("create fake .claude dir so the installer detects claude-code");

        // .env() only affects this child process, not the test binary's own
        // environment — safe under parallel test execution (no risk to any
        // other test that resolves the real $HOME).
        let output = installer_command(&installer_path, "bun")
            .env("HOME", fake_home.path())
            .output()
            .expect("bun must be on $PATH to run this test");

        assert!(
            output.status.success(),
            "installer failed — stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let installed_skill = fake_home.path().join(".claude/skills/nodespace/SKILL.md");
        assert!(
            installed_skill.exists(),
            "SKILL.md was not installed into the fake $HOME at {}",
            installed_skill.display()
        );
        let installed_shim = fake_home
            .path()
            .join(".claude/skills/nodespace/nodespace-hook.ts");
        assert!(
            installed_shim.exists(),
            "claude-code shim was not installed"
        );
    }
}
