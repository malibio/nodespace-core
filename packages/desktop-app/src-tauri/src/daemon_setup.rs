//! launchd-based daemon lifecycle management (Issue #1179).
//!
//! On first launch:
//!   1. Locate sidecar binaries bundled inside the .app via Tauri's resource resolver.
//!   2. Copy them to ~/.nodespace/bin/ with executable permissions.
//!   3. Write a launchd user-agent plist to ~/Library/LaunchAgents/.
//!   4. Load the plist via `launchctl load` so the daemon starts immediately.
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

const LAUNCH_AGENT_LABEL: &str = "app.nodespace.daemon";
const DAEMON_SOCKET_PATH: &str = ".nodespace/daemon.sock";
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
    let socket_path = home.join(DAEMON_SOCKET_PATH);
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
    extract_sidecar(app, DAEMON_BINARY_NAME, &bin_dir).await?;
    extract_sidecar(app, CLI_BINARY_NAME, &bin_dir).await?;

    // Write (or overwrite) the launchd plist with the current username baked in.
    write_plist(&home, &plist_path, &daemon_bin).context("Failed to write launchd plist")?;

    // Load or reload the plist.
    load_launchd_agent(&plist_path)?;

    // Wait briefly for the daemon to come up.
    let status = wait_for_daemon(&socket_path, Duration::from_secs(5)).await;
    Ok(status)
}

/// Check daemon health by testing whether the Unix Domain Socket is reachable.
///
/// A full gRPC ping would require importing the proto types; for the health
/// check a successful TCP-style connect to the UDS is sufficient — the OS
/// rejects the connect if no process is listening.
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

/// Extract a sidecar binary from the Tauri bundle to `~/.nodespace/bin/`.
///
/// Tauri names sidecar binaries `<name>-<target-triple>` inside the bundle.
/// We copy the file and set the executable bit.
async fn extract_sidecar(app: &AppHandle, name: &str, bin_dir: &Path) -> Result<()> {
    // Tauri resolves the sidecar path using the `externalBin` entries in tauri.conf.json.
    // The binary is stored as `binaries/<name>-<target-triple>` inside Resources.
    let src = resolve_sidecar_path(app, name)?;
    let dest = bin_dir.join(name);

    tracing::info!(
        "Extracting {} from {} to {}",
        name,
        src.display(),
        dest.display()
    );

    tokio::fs::copy(&src, &dest)
        .await
        .with_context(|| format!("Failed to copy {} to {}", src.display(), dest.display()))?;

    // Mark executable (rwxr-xr-x).
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
    // Ensure ~/Library/LaunchAgents exists (it normally does on macOS).
    std::fs::create_dir_all(plist_path.parent().unwrap())
        .context("Failed to create ~/Library/LaunchAgents")?;

    let home_str = home.to_string_lossy();
    let bin_str = daemon_bin.to_string_lossy();
    let socket_path = format!("{}/{}", home_str, DAEMON_SOCKET_PATH);
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

/// Load (or reload) the launchd agent plist.
///
/// If already loaded, `launchctl load` exits with an error. We detect that and
/// use `launchctl kickstart` to restart a crashed daemon instead.
fn load_launchd_agent(plist_path: &Path) -> Result<()> {
    tracing::info!("Loading launchd agent: {}", plist_path.display());

    let output = std::process::Command::new("launchctl")
        .args(["load", "-w", &plist_path.to_string_lossy()])
        .output()
        .context("Failed to run launchctl load")?;

    if output.status.success() {
        tracing::info!("launchd agent loaded successfully");
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);

    // If the agent is already loaded, kick it to restart a stopped/crashed instance.
    if stderr.contains("already loaded") || stderr.contains("service already loaded") {
        tracing::info!("Agent already loaded; using kickstart to restart");
        let kickstart = std::process::Command::new("launchctl")
            .args([
                "kickstart",
                "-k",
                &format!("gui/{}/{}", get_uid(), LAUNCH_AGENT_LABEL),
            ])
            .output()
            .context("Failed to run launchctl kickstart")?;

        if !kickstart.status.success() {
            let ks_err = String::from_utf8_lossy(&kickstart.stderr);
            tracing::warn!("launchctl kickstart failed: {}", ks_err);
        }
        return Ok(());
    }

    // Non-fatal: log but do not propagate — the daemon may already be running
    // from a previous invocation and `launchctl load` just doesn't like duplicates.
    tracing::warn!(
        "launchctl load exited with status {}: {}",
        output.status,
        stderr
    );
    Ok(())
}

fn get_uid() -> u32 {
    // SAFETY: getuid() is always safe to call.
    unsafe { libc::getuid() }
}
