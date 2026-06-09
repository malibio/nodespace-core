//! Daemon lifecycle management — macOS (launchd) and Linux (systemd).
//!
//! On first launch:
//!   1. Locate sidecar binaries bundled inside the .app via Tauri's resource resolver.
//!   2. Copy them to ~/.nodespace/bin/ (skipped if dest already matches bundled size).
//!   3. Register the daemon as a user service:
//!      - macOS: write ~/Library/LaunchAgents/app.nodespace.daemon.plist and bootstrap it.
//!      - Linux: write ~/.config/systemd/user/nodespace.service and enable it.
//!   4. Wait for the Unix Domain Socket to appear (cold-start model load can take ~9s).
//!
//! On subsequent launches:
//!   - Check if the socket exists and the daemon responds (cheap path).
//!   - If already healthy: no-op.
//!   - If service is registered but daemon crashed: restart it.
//!   - If service is missing (e.g. clean install): re-run first-launch setup.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use tauri::{AppHandle, Manager};
use tokio::time::timeout;

use crate::constants::DAEMON_SOCKET_RELATIVE;

const DAEMON_BIN_DIR: &str = ".nodespace/bin";
const DAEMON_DB_DIR: &str = ".nodespace/database";
const DAEMON_LOG_DIR: &str = ".nodespace/logs";
const DAEMON_BINARY_NAME: &str = "nodespaced";
const CLI_BINARY_NAME: &str = "nodespace";

#[cfg(target_os = "macos")]
const LAUNCH_AGENT_LABEL: &str = "app.nodespace.daemon";
#[cfg(target_os = "macos")]
const PLIST_FILENAME: &str = "app.nodespace.daemon.plist";

#[cfg(target_os = "linux")]
const SYSTEMD_SERVICE_NAME: &str = "nodespace.service";

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

/// Ensure nodespaced is installed as a user service (launchd/systemd) and running.
///
/// Call this from the Tauri setup block. It is non-fatal: logs errors
/// and returns them so the caller can emit an appropriate UI error state.
pub async fn ensure_daemon_running(app: &AppHandle) -> Result<DaemonStatus> {
    let home = home_dir().context("Cannot resolve home directory")?;
    let bin_dir = home.join(DAEMON_BIN_DIR);
    let log_dir = home.join(DAEMON_LOG_DIR);
    let db_dir = home.join(DAEMON_DB_DIR);
    let socket_path = home.join(DAEMON_SOCKET_RELATIVE);
    let daemon_bin = bin_dir.join(DAEMON_BINARY_NAME);

    // Check current daemon health first (cheap path for subsequent launches).
    let status = check_daemon_socket(&socket_path).await;
    if status == DaemonStatus::Healthy {
        tracing::info!("nodespaced is already running and healthy");
        return Ok(DaemonStatus::Healthy);
    }

    // Need to (re)start the daemon. Ensure all directories exist.
    tokio::fs::create_dir_all(&bin_dir)
        .await
        .context("Failed to create ~/.nodespace/bin")?;
    tokio::fs::create_dir_all(&log_dir)
        .await
        .context("Failed to create ~/.nodespace/logs")?;
    tokio::fs::create_dir_all(&db_dir)
        .await
        .context("Failed to create ~/.nodespace/database")?;

    // Extract sidecar binaries from the app bundle if missing or outdated.
    extract_sidecar_if_changed(app, DAEMON_BINARY_NAME, &bin_dir).await?;
    extract_sidecar_if_changed(app, CLI_BINARY_NAME, &bin_dir).await?;

    // Register and/or start the daemon user service.
    #[cfg(target_os = "macos")]
    {
        let plist_path = launch_agents_dir(&home).join(PLIST_FILENAME);
        write_plist(&home, &plist_path, &daemon_bin).context("Failed to write launchd plist")?;
        bootstrap_launchd_agent(&plist_path)?;
    }

    #[cfg(target_os = "linux")]
    {
        let service_path = systemd_user_service_dir(&home).join(SYSTEMD_SERVICE_NAME);
        write_systemd_service(&home, &service_path, &daemon_bin)
            .context("Failed to write systemd service file")?;
        enable_systemd_service()?;
    }

    // The daemon loads the embedding model before binding the socket (~9s on M2 Pro).
    // 30s covers cold-start model load on slower machines.
    let status = wait_for_daemon(&socket_path, Duration::from_secs(30)).await;
    Ok(status)
}

/// Check daemon health by testing whether the Unix Domain Socket is reachable.
///
/// A successful UDS connect is sufficient — the OS rejects the connect
/// if no process is listening.
pub async fn check_daemon_socket(socket_path: &Path) -> DaemonStatus {
    if !socket_path.exists() {
        return DaemonStatus::NotRunning;
    }
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
/// the bundled source.
async fn extract_sidecar_if_changed(app: &AppHandle, name: &str, bin_dir: &Path) -> Result<()> {
    let src = resolve_sidecar_path(app, name)?;
    let dest = bin_dir.join(name);

    let src_size = tokio::fs::metadata(&src)
        .await
        .with_context(|| format!("Cannot stat bundled sidecar {}", src.display()))?
        .len();

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

// ── macOS: launchd ────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn launch_agents_dir(home: &Path) -> PathBuf {
    home.join("Library/LaunchAgents")
}

/// Write the launchd plist for the nodespaced user agent.
#[cfg(target_os = "macos")]
fn write_plist(home: &Path, plist_path: &Path, daemon_bin: &Path) -> Result<()> {
    let launch_agents = plist_path
        .parent()
        .context("plist_path has no parent directory")?;
    std::fs::create_dir_all(launch_agents).context("Failed to create ~/Library/LaunchAgents")?;

    let home_str = home.to_string_lossy();
    let bin_str = daemon_bin.to_string_lossy();
    let socket_path = format!("{}/{}", home_str, DAEMON_SOCKET_RELATIVE);
    let db_path = format!("{}/{}/nodespace.db", home_str, DAEMON_DB_DIR);
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
/// Uses the modern `launchctl bootstrap gui/<uid>` API (macOS 10.10+).
/// If already bootstrapped, falls back to `launchctl kickstart -k`.
#[cfg(target_os = "macos")]
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

    // Error 37 = EALREADY / Error 36 = ENOTSUP: service already registered.
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

    tracing::warn!(
        "launchctl bootstrap exited with status {}: {}",
        output.status,
        stderr
    );
    Ok(())
}

#[cfg(target_os = "macos")]
fn get_uid() -> u32 {
    // SAFETY: getuid() is always safe to call.
    unsafe { libc::getuid() }
}

// ── Linux: systemd user service ───────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn systemd_user_service_dir(home: &Path) -> PathBuf {
    home.join(".config/systemd/user")
}

/// Write the systemd user service unit for nodespaced.
#[cfg(target_os = "linux")]
fn write_systemd_service(home: &Path, service_path: &Path, daemon_bin: &Path) -> Result<()> {
    let service_dir = service_path
        .parent()
        .context("service_path has no parent directory")?;
    std::fs::create_dir_all(service_dir)
        .context("Failed to create ~/.config/systemd/user directory")?;

    let home_str = home.to_string_lossy();
    let bin_str = daemon_bin.to_string_lossy();
    let socket_path = format!("{}/{}", home_str, DAEMON_SOCKET_RELATIVE);
    let db_path = format!("{}/{}/nodespace.db", home_str, DAEMON_DB_DIR);
    let log_out = format!("{}/{}/nodespaced.log", home_str, DAEMON_LOG_DIR);
    let log_err = format!("{}/{}/nodespaced-error.log", home_str, DAEMON_LOG_DIR);

    let unit = format!(
        "[Unit]\n\
         Description=NodeSpace daemon\n\
         After=network.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={bin}\n\
         Environment=NODESPACED_SOCKET={socket}\n\
         Environment=NODESPACED_DB_PATH={db}\n\
         StandardOutput=append:{log_out}\n\
         StandardError=append:{log_err}\n\
         Restart=on-failure\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        bin = bin_str,
        socket = socket_path,
        db = db_path,
        log_out = log_out,
        log_err = log_err,
    );

    std::fs::write(service_path, unit)
        .with_context(|| format!("Cannot write service file to {}", service_path.display()))
}

/// Enable and start (or restart) the systemd user service.
#[cfg(target_os = "linux")]
fn enable_systemd_service() -> Result<()> {
    let service_name = SYSTEMD_SERVICE_NAME;

    // Reload unit files so systemd picks up the newly written service.
    let reload = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output()
        .context("Failed to run systemctl --user daemon-reload")?;
    if !reload.status.success() {
        let err = String::from_utf8_lossy(&reload.stderr);
        tracing::warn!("systemctl daemon-reload failed: {}", err);
    }

    // Enable so it starts on login.
    let enable = std::process::Command::new("systemctl")
        .args(["--user", "enable", service_name])
        .output()
        .context("Failed to run systemctl --user enable")?;
    if !enable.status.success() {
        let err = String::from_utf8_lossy(&enable.stderr);
        tracing::warn!("systemctl enable failed: {}", err);
    }

    // Start or restart the service.
    let start = std::process::Command::new("systemctl")
        .args(["--user", "restart", service_name])
        .output()
        .context("Failed to run systemctl --user restart")?;
    if !start.status.success() {
        let err = String::from_utf8_lossy(&start.stderr);
        tracing::warn!(
            "systemctl restart failed (daemon may start on next login): {}",
            err
        );
    } else {
        tracing::info!("systemd user service started: {}", service_name);
    }

    Ok(())
}
