//! launchd-based daemon lifecycle management (Issue #1179).
//!
//! On first launch:
//!   1. Locate sidecar binaries bundled inside the .app via Tauri's resource resolver.
//!   2. Copy them to ~/.nodespace/bin/ (skipped if dest already matches bundled size).
//!   3. Write a launchd user-agent plist to ~/Library/LaunchAgents/.
//!   4. Bootstrap the agent via `launchctl bootstrap gui/<uid> <plist>`.
//!
//! On subsequent launches:
//!   - Check if the Unix Domain Socket exists and the daemon responds to a gRPC ping.
//!   - If already healthy: no-op.
//!   - If plist is registered but daemon crashed: `launchctl kickstart` it.
//!   - If plist is missing (e.g. clean install): re-run first-launch setup.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use tauri::{AppHandle, Manager};
use tokio::time::timeout;

use crate::constants::DAEMON_SOCKET_RELATIVE;

const LAUNCH_AGENT_LABEL: &str = "app.nodespace.daemon";
const DAEMON_BIN_DIR: &str = ".nodespace/bin";
const DAEMON_LOG_DIR: &str = ".nodespace/logs";
const DAEMON_DB_DIR: &str = ".nodespace/database";
const PLIST_FILENAME: &str = "app.nodespace.daemon.plist";
const DAEMON_BINARY_NAME: &str = "nodespaced";
const CLI_BINARY_NAME: &str = "nodespace";

/// Result of the daemon health check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonStatus {
    /// Daemon is running and responding on the socket.
    Healthy,
    /// Socket exists but daemon is unresponsive (started but not ready yet).
    Starting,
    /// Daemon is not running.
    NotRunning,
}

/// Ensure nodespaced is installed as a launchd user agent and running.
///
/// Call this from the Tauri setup block. It is non-fatal: logs errors
/// and returns them so the caller can emit an appropriate UI error state.
pub async fn ensure_daemon_running(app: &AppHandle) -> Result<DaemonStatus> {
    let home = home_dir().context("Cannot resolve home directory")?;
    let bin_dir = home.join(DAEMON_BIN_DIR);
    let log_dir = home.join(DAEMON_LOG_DIR);
    let plist_path = launch_agents_dir(&home).join(PLIST_FILENAME);
    let socket_path = home.join(DAEMON_SOCKET_RELATIVE);
    let daemon_bin = bin_dir.join(DAEMON_BINARY_NAME);

    // Check current daemon health first (cheap path for subsequent launches).
    let status = check_daemon_socket(&socket_path).await;
    if status == DaemonStatus::Healthy {
        tracing::info!("nodespaced is already running and healthy");
        return Ok(DaemonStatus::Healthy);
    }

    // Need to (re)start the daemon. Ensure directories exist.
    tokio::fs::create_dir_all(&bin_dir)
        .await
        .context("Failed to create ~/.nodespace/bin")?;
    tokio::fs::create_dir_all(&log_dir)
        .await
        .context("Failed to create ~/.nodespace/logs")?;
    tokio::fs::create_dir_all(home.join(DAEMON_DB_DIR))
        .await
        .context("Failed to create ~/.nodespace/database")?;

    // Extract sidecar binaries from the .app bundle if missing or outdated.
    extract_sidecar_if_changed(app, DAEMON_BINARY_NAME, &bin_dir).await?;
    extract_sidecar_if_changed(app, CLI_BINARY_NAME, &bin_dir).await?;

    // Write (or overwrite) the launchd plist with the current username baked in.
    write_plist(&home, &plist_path, &daemon_bin).context("Failed to write launchd plist")?;

    // Bootstrap or restart the launchd agent.
    bootstrap_launchd_agent(&plist_path)?;

    // Wait briefly for the daemon to come up.
    let status = wait_for_daemon(&socket_path, Duration::from_secs(5)).await;
    Ok(status)
}

/// Check daemon health by testing whether the Unix Domain Socket is reachable.
///
/// A full gRPC ping would require importing the proto types; for the health
/// check a successful UDS connect is sufficient — the OS rejects the connect
/// if no process is listening.
pub async fn check_daemon_socket(socket_path: &Path) -> DaemonStatus {
    if !socket_path.exists() {
        return DaemonStatus::NotRunning;
    }
    // Attempt a UDS connection with a short timeout.
    match timeout(
        Duration::from_millis(500),
        tokio::net::UnixStream::connect(socket_path),
    )
    .await
    {
        Ok(Ok(_)) => DaemonStatus::Healthy,
        Ok(Err(_)) => DaemonStatus::NotRunning,
        Err(_) => DaemonStatus::Starting,
    }
}

/// Poll the socket until the daemon is healthy or the timeout expires.
async fn wait_for_daemon(socket_path: &Path, max_wait: Duration) -> DaemonStatus {
    let deadline = tokio::time::Instant::now() + max_wait;
    loop {
        let status = check_daemon_socket(socket_path).await;
        if status == DaemonStatus::Healthy {
            tracing::info!("nodespaced is up and healthy");
            return DaemonStatus::Healthy;
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!("nodespaced did not respond within {:?}", max_wait);
            return status;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Extract a sidecar binary from the Tauri bundle to `~/.nodespace/bin/`,
/// but only if the destination is missing or has a different file size than
/// the bundled source. Size comparison is a lightweight proxy for version change:
/// a real binary size change on every Tauri build guarantees re-extraction.
async fn extract_sidecar_if_changed(app: &AppHandle, name: &str, bin_dir: &Path) -> Result<()> {
    let src = resolve_sidecar_path(app, name)?;
    let dest = bin_dir.join(name);

    let src_size = tokio::fs::metadata(&src)
        .await
        .with_context(|| format!("Cannot stat bundled sidecar {}", src.display()))?
        .len();

    // Skip extraction if dest exists and matches bundled binary size.
    if let Ok(dest_meta) = tokio::fs::metadata(&dest).await {
        if dest_meta.len() == src_size {
            tracing::debug!(
                "{} is up-to-date (size={}), skipping extraction",
                name,
                src_size
            );
            return Ok(());
        }
    }

    tracing::info!(
        "Extracting {} ({} bytes) to {}",
        name,
        src_size,
        dest.display()
    );

    tokio::fs::copy(&src, &dest)
        .await
        .with_context(|| format!("Failed to copy {} to {}", src.display(), dest.display()))?;

    set_executable(&dest)?;
    Ok(())
}

/// Resolve the platform-tagged sidecar path inside the Tauri bundle.
///
/// Tauri appends the current target triple to sidecar binary names, e.g.:
/// `binaries/nodespaced-aarch64-apple-darwin` on Apple Silicon.
fn resolve_sidecar_path(app: &AppHandle, name: &str) -> Result<PathBuf> {
    use tauri::path::BaseDirectory;

    let triple =
        tauri::utils::platform::target_triple().context("Cannot determine target triple")?;
    let sidecar_name = format!("binaries/{}-{}", name, triple);

    app.path()
        .resolve(&sidecar_name, BaseDirectory::Resource)
        .with_context(|| format!("Cannot resolve sidecar path for '{}'", name))
}

fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)
        .with_context(|| format!("Cannot stat {}", path.display()))?
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("Cannot set executable bit on {}", path.display()))
}

fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

fn launch_agents_dir(home: &Path) -> PathBuf {
    home.join("Library/LaunchAgents")
}

/// Write the launchd plist for the nodespaced user agent.
fn write_plist(home: &Path, plist_path: &Path, daemon_bin: &Path) -> Result<()> {
    let launch_agents = plist_path
        .parent()
        .context("plist_path has no parent directory")?;
    std::fs::create_dir_all(launch_agents).context("Failed to create ~/Library/LaunchAgents")?;

    let home_str = home.to_string_lossy();
    let bin_str = daemon_bin.to_string_lossy();
    let socket_path = format!("{}/{}", home_str, DAEMON_SOCKET_RELATIVE);
    let db_path = format!("{}/{}/nodespace", home_str, DAEMON_DB_DIR);
    let log_out = format!("{}/{}/nodespaced.log", home_str, DAEMON_LOG_DIR);
    let log_err = format!("{}/{}/nodespaced-error.log", home_str, DAEMON_LOG_DIR);

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>NODESPACED_SOCKET</key>
        <string>{socket}</string>
        <key>NODESPACED_DB_PATH</key>
        <string>{db}</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{log_out}</string>
    <key>StandardErrorPath</key>
    <string>{log_err}</string>
</dict>
</plist>
"#,
        label = LAUNCH_AGENT_LABEL,
        bin = bin_str,
        socket = socket_path,
        db = db_path,
        log_out = log_out,
        log_err = log_err,
    );

    std::fs::write(plist_path, plist)
        .with_context(|| format!("Cannot write plist to {}", plist_path.display()))
}

/// Register or restart the launchd user agent.
///
/// Uses the modern `launchctl bootstrap gui/<uid> <plist>` API (macOS 10.10+).
/// If already bootstrapped, falls back to `launchctl kickstart -k` to restart
/// a stopped or crashed instance. The legacy `launchctl load -w` is intentionally
/// avoided — it was deprecated in macOS 10.11 and generates log noise on macOS 15+.
fn bootstrap_launchd_agent(plist_path: &Path) -> Result<()> {
    let uid = get_uid();
    let gui_target = format!("gui/{}", uid);
    tracing::info!("Bootstrapping launchd agent for {}", gui_target);

    let output = std::process::Command::new("launchctl")
        .args(["bootstrap", &gui_target, &plist_path.to_string_lossy()])
        .output()
        .context("Failed to run launchctl bootstrap")?;

    if output.status.success() {
        tracing::info!("launchd agent bootstrapped successfully");
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Error 37 = EALREADY: service is already registered in this bootstrap context.
    // Error 36 = ENOTSUP: also seen for "already bootstrapped" on some versions.
    let already_bootstrapped = output.status.code().is_some_and(|c| c == 37 || c == 36)
        || stderr.contains("already bootstrapped")
        || stderr.contains("service already exists");

    if already_bootstrapped {
        tracing::info!("Agent already bootstrapped; kickstarting to restart");
        let kickstart = std::process::Command::new("launchctl")
            .args([
                "kickstart",
                "-k",
                &format!("{}/{}", gui_target, LAUNCH_AGENT_LABEL),
            ])
            .output()
            .context("Failed to run launchctl kickstart")?;

        if !kickstart.status.success() {
            let ks_err = String::from_utf8_lossy(&kickstart.stderr);
            tracing::warn!(
                "launchctl kickstart failed (daemon may start on next login): {}",
                ks_err
            );
        }
        return Ok(());
    }

    // Non-fatal: log and continue — daemon may still be running from a prior launch.
    tracing::warn!(
        "launchctl bootstrap exited with status {}: {}",
        output.status,
        stderr
    );
    Ok(())
}

fn get_uid() -> u32 {
    // SAFETY: getuid() is always safe to call.
    unsafe { libc::getuid() }
}
