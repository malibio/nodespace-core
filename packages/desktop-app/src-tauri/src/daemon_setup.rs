//! Daemon lifecycle management — macOS (launchd), Linux (systemd), Windows (direct spawn).
//!
//! On first launch:
//!   1. Locate sidecar binaries bundled inside the .app via Tauri's resource resolver.
//!   2. Copy them to ~/.nodespace/bin/ (skipped if dest already matches bundled size).
//!   3. Register the daemon as a user service:
//!      - macOS: write ~/Library/LaunchAgents/app.nodespace.daemon.plist and bootstrap it.
//!      - Linux: write ~/.config/systemd/user/nodespace.service and enable it.
//!      - Windows: spawn the daemon process directly and write an HKCU autorun key.
//!   4. Wait for the IPC endpoint to appear (UDS on Unix, Named Pipe on Windows).
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

const DAEMON_BIN_DIR: &str = ".nodespace/bin";
const DAEMON_DB_DIR: &str = ".nodespace/database";
const DAEMON_LOG_DIR: &str = ".nodespace/logs";
const DAEMON_BINARY_NAME: &str = "nodespaced";
const PRO_DAEMON_BINARY_NAME: &str = "nodespaced-pro";
const CLI_BINARY_NAME: &str = "nodespace";

#[cfg(target_os = "linux")]
const SYSTEMD_SERVICE_NAME: &str = "nodespace.service";

// ── Pro edition, baked at compile time (#156) ───────────────────────────────
// A Pro build sets these env vars when running `tauri build` (see
// nodespace-sync/scripts/build-pro-dmg.sh); a community build leaves them unset.
// When set, the app installs + launches the `nodespaced-pro` sync daemon with the
// Supabase cloud env instead of the community `nodespaced`. The anon key is the
// project's PUBLISHABLE key (safe to bake into the binary).
const PRO_SUPABASE_URL: Option<&str> = option_env!("NODESPACE_PRO_SUPABASE_URL");
const PRO_ANON_KEY: Option<&str> = option_env!("NODESPACE_PRO_ANON_KEY");
const PRO_SCHEMA: Option<&str> = option_env!("NODESPACE_PRO_SCHEMA");
const PRO_COLLECTION: Option<&str> = option_env!("NODESPACE_PRO_COLLECTION");

/// True when this binary was built as the Pro edition (cloud env baked in).
fn is_pro_build() -> bool {
    PRO_SUPABASE_URL.is_some()
}

/// The daemon sidecar this edition installs + launches.
fn daemon_binary_name() -> &'static str {
    if is_pro_build() {
        PRO_DAEMON_BINARY_NAME
    } else {
        DAEMON_BINARY_NAME
    }
}

/// Relative path from HOME to the daemon socket, scoped by build variant.
///
/// Scoping prevents dev builds from colliding with the production app and prevents
/// community builds from colliding with Pro builds on the same machine:
///   - Release community: `.nodespace/daemon.sock`
///   - Release Pro:       `.nodespace/daemon-pro.sock`
///   - Debug community:   `.nodespace/daemon-dev.sock`
///   - Debug Pro:         `.nodespace/daemon-dev-pro.sock`
///
/// grpc_client::resolve_socket_path() calls this function for its fallback, so
/// the GUI app always dials the same socket the plist points the daemon to.
pub(crate) fn daemon_socket_relative() -> &'static str {
    match (cfg!(debug_assertions), is_pro_build()) {
        (false, false) => ".nodespace/daemon.sock",
        (false, true) => ".nodespace/daemon-pro.sock",
        (true, false) => ".nodespace/daemon-dev.sock",
        (true, true) => ".nodespace/daemon-dev-pro.sock",
    }
}

/// macOS launchd label, scoped by build variant (mirrors daemon_socket_relative).
#[cfg(target_os = "macos")]
fn launch_agent_label() -> &'static str {
    match (cfg!(debug_assertions), is_pro_build()) {
        (false, false) => "app.nodespace.daemon",
        (false, true) => "app.nodespace.daemon.pro",
        (true, false) => "app.nodespace.daemon.dev",
        (true, true) => "app.nodespace.daemon.dev.pro",
    }
}

/// macOS plist filename, derived from the label (dots → dots, .plist appended).
#[cfg(target_os = "macos")]
fn plist_filename() -> &'static str {
    match (cfg!(debug_assertions), is_pro_build()) {
        (false, false) => "app.nodespace.daemon.plist",
        (false, true) => "app.nodespace.daemon.pro.plist",
        (true, false) => "app.nodespace.daemon.dev.plist",
        (true, true) => "app.nodespace.daemon.dev.pro.plist",
    }
}

/// Synchronously kill the running daemon if the installed binary differs in size
/// from the bundled sidecar. Called before the gRPC client is managed so the
/// frontend never connects to a stale daemon.
///
/// This is intentionally synchronous and cheap (two stat calls + maybe lsof).
/// The async `ensure_daemon_running` handles extraction and restart afterward.
#[cfg(unix)]
pub fn kill_stale_daemon_sync(app: &tauri::App) {
    let handle = app.handle();
    let home = match home_dir() {
        Some(h) => h,
        None => return,
    };
    let bin_dir = home.join(DAEMON_BIN_DIR);
    let socket_path = home.join(daemon_socket_relative());
    let installed = bin_dir.join(daemon_binary_name());

    let bundled_size = match resolve_sidecar_path_sync(handle) {
        Some(p) => match std::fs::metadata(&p) {
            Ok(m) => m.len(),
            Err(_) => return,
        },
        None => return,
    };

    let installed_size = match std::fs::metadata(&installed) {
        Ok(m) => m.len(),
        Err(_) => return, // not installed yet — ensure_daemon_running handles this
    };

    if bundled_size == installed_size {
        return; // up to date
    }

    tracing::info!(
        bundled = bundled_size,
        installed = installed_size,
        "Installed daemon binary is stale — killing before gRPC client connects"
    );

    // Kill only nodespaced processes using the socket (not gRPC clients like nodespace-app).
    // Parse lsof -F pn output to collect unique PIDs, then verify each is nodespaced
    // before SIGKILLing. Deduplicating into a HashSet avoids spawning ps more than once
    // per PID when the daemon has multiple FDs open on the socket.
    let sock = socket_path.to_string_lossy();
    if let Ok(out) = std::process::Command::new("lsof")
        .args(["-F", "pn", "-U", sock.as_ref()])
        .output()
    {
        use std::collections::HashSet;
        let output = String::from_utf8_lossy(&out.stdout);
        let mut current_pid: Option<i32> = None;
        let mut pids_to_check: HashSet<i32> = HashSet::new();
        for line in output.lines() {
            if let Some(pid_str) = line.strip_prefix('p') {
                current_pid = pid_str.parse::<i32>().ok();
            } else if line.strip_prefix('n').is_some() {
                if let Some(pid) = current_pid {
                    pids_to_check.insert(pid);
                }
            }
        }
        let installed_str = installed.to_string_lossy().into_owned();
        for pid in pids_to_check {
            // Verify the PID's argv[0] matches our installed binary path exactly,
            // not just a trailing-substring heuristic that could match unrelated processes.
            let exe_check = std::process::Command::new("ps")
                .args(["-p", &pid.to_string(), "-o", "args="])
                .output()
                .ok();
            let is_our_daemon = exe_check
                .as_ref()
                .map(|o| {
                    let args = String::from_utf8_lossy(&o.stdout);
                    let argv0 = args.trim().split_whitespace().next().unwrap_or("");
                    argv0 == installed_str
                })
                .unwrap_or(false);
            if is_our_daemon {
                unsafe { libc::kill(pid, libc::SIGKILL) };
                tracing::info!(pid, "Sent SIGKILL to stale nodespaced");
            }
        }
    }

    // Remove stale socket so the health check in ensure_daemon_running sees NotRunning
    let _ = std::fs::remove_file(&socket_path);
}

fn resolve_sidecar_path_sync(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::path::BaseDirectory;
    let triple = tauri::utils::platform::target_triple().ok()?;
    let name = format!("binaries/{}-{}", daemon_binary_name(), triple);
    app.path().resolve(&name, BaseDirectory::Resource).ok()
}

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
    // On Windows the "socket path" is a Named Pipe path, not a filesystem path.
    // check_daemon_socket and wait_for_daemon both dispatch on cfg(windows) so they
    // probe the pipe correctly as long as we pass the right path here.
    #[cfg(unix)]
    let socket_path = home.join(daemon_socket_relative());
    #[cfg(windows)]
    let socket_path = PathBuf::from(crate::services::grpc_client::resolve_pipe_name());
    let daemon_bin = bin_dir.join(daemon_binary_name());

    // Ensure all directories exist before any binary checks.
    tokio::fs::create_dir_all(&bin_dir)
        .await
        .context("Failed to create ~/.nodespace/bin")?;
    tokio::fs::create_dir_all(&log_dir)
        .await
        .context("Failed to create ~/.nodespace/logs")?;
    tokio::fs::create_dir_all(&db_dir)
        .await
        .context("Failed to create ~/.nodespace/database")?;

    // Always check whether the bundled sidecar differs from the installed binary.
    // If the daemon is already running but the binary changed (e.g. dev rebuild),
    // kill it so the updated binary gets launched below.
    let binary_updated = extract_sidecar_if_changed(app, daemon_binary_name(), &bin_dir).await?;
    extract_sidecar_if_changed(app, CLI_BINARY_NAME, &bin_dir).await?;

    if binary_updated {
        tracing::info!("nodespaced binary updated — restarting daemon");
        kill_running_daemon(&socket_path).await;
    } else {
        // Binary unchanged: if already healthy, nothing to do.
        let status = check_daemon_socket(&socket_path).await;
        if status == DaemonStatus::Healthy {
            tracing::info!("nodespaced is already running and healthy");
            return Ok(DaemonStatus::Healthy);
        }
    }

    // Register and/or start the daemon user service.
    #[cfg(target_os = "macos")]
    {
        let plist_path = launch_agents_dir(&home).join(plist_filename());
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

    // Windows: spawn the daemon process directly and register it in HKCU autorun
    // so it restarts automatically on next login. Full SCM registration requires
    // elevation which a normal user app cannot assume — direct spawn is used instead.
    #[cfg(windows)]
    {
        spawn_daemon_windows(&daemon_bin).context("Failed to spawn daemon on Windows")?;
        register_autorun_windows(&daemon_bin);
    }

    // The daemon loads the embedding model before binding the socket (~9s on M2 Pro).
    // 30s covers cold-start model load on slower machines.
    let status = wait_for_daemon(&socket_path, Duration::from_secs(30)).await;
    Ok(status)
}

/// Send SIGTERM to the process listening on the socket and wait for it to exit.
///
/// Uses `-F pn` output and verifies each PID's full binary path against our
/// installed daemon before signalling — avoids accidentally terminating gRPC
/// clients or unrelated processes that happen to share the socket.
#[cfg(unix)]
async fn kill_running_daemon(socket_path: &Path) {
    if check_daemon_socket(socket_path).await != DaemonStatus::Healthy {
        return;
    }

    // Resolve the PID via lsof rather than storing it, so this works whether the
    // daemon was started by launchd, a previous app launch, or manually.
    // Parse -F pn output to extract PIDs, then verify each against our installed
    // binary path before sending SIGTERM.
    {
        use std::collections::HashSet;
        use std::process::Command;
        let installed = home_dir()
            .map(|h| h.join(DAEMON_BIN_DIR).join(daemon_binary_name()))
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let sock = socket_path.to_string_lossy();
        if let Ok(out) = Command::new("lsof")
            .args(["-F", "pn", "-U", sock.as_ref()])
            .output()
        {
            let output = String::from_utf8_lossy(&out.stdout);
            let mut current_pid: Option<i32> = None;
            let mut pids_to_kill: HashSet<i32> = HashSet::new();
            for line in output.lines() {
                if let Some(pid_str) = line.strip_prefix('p') {
                    current_pid = pid_str.parse::<i32>().ok();
                } else if line.strip_prefix('n').is_some() {
                    if let Some(pid) = current_pid {
                        pids_to_kill.insert(pid);
                    }
                }
            }
            for pid in pids_to_kill {
                let exe_check = Command::new("ps")
                    .args(["-p", &pid.to_string(), "-o", "args="])
                    .output()
                    .ok();
                let is_our_daemon = exe_check
                    .as_ref()
                    .map(|o| {
                        let args = String::from_utf8_lossy(&o.stdout);
                        let argv0 = args.trim().split_whitespace().next().unwrap_or("");
                        !installed.is_empty() && argv0 == installed
                    })
                    .unwrap_or(false);
                if is_our_daemon {
                    // SAFETY: kill() is always safe to call with a valid pid and signal.
                    unsafe { libc::kill(pid, libc::SIGTERM) };
                    tracing::info!("Sent SIGTERM to old nodespaced (pid {})", pid);
                }
            }
        }
    }

    // Give the daemon up to 5 s to exit cleanly before proceeding.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if check_daemon_socket(socket_path).await == DaemonStatus::NotRunning {
            break;
        }
    }

    // Remove a stale socket file so launchd can bind the new one.
    let _ = std::fs::remove_file(socket_path);
}

/// Kill the running daemon on Windows via taskkill and wait for it to exit.
#[cfg(windows)]
async fn kill_running_daemon(socket_path: &Path) {
    if check_daemon_socket(socket_path).await != DaemonStatus::Healthy {
        return;
    }

    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/IM", "nodespaced.exe"])
        .output();
    tracing::info!("Sent taskkill to nodespaced.exe");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if check_daemon_socket(socket_path).await == DaemonStatus::NotRunning {
            break;
        }
    }
}

/// Check daemon health by testing whether the Unix Domain Socket is reachable.
///
/// A successful UDS connect is sufficient — the OS rejects the connect
/// if no process is listening.
#[cfg(unix)]
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

/// Check daemon health on Windows by probing the Named Pipe.
///
/// Uses a blocking `std::fs::OpenOptions` probe (pipes are accessible via the
/// filesystem namespace on Windows) wrapped in `spawn_blocking`.
#[cfg(windows)]
pub async fn check_daemon_socket(socket_path: &Path) -> DaemonStatus {
    let pipe_path = socket_path.to_string_lossy().to_string();
    let result = tokio::task::spawn_blocking(move || {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&pipe_path)
    })
    .await;
    match result {
        Ok(Ok(_)) => DaemonStatus::Healthy,
        // ERROR_PIPE_BUSY (231): daemon is running but all instances are busy with
        // existing clients. The daemon is healthy — treat as Starting so the caller
        // retries rather than spawning a second instance.
        Ok(Err(e)) if e.raw_os_error() == Some(231) => DaemonStatus::Starting,
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
/// the bundled source. Returns true if the binary was updated.
async fn extract_sidecar_if_changed(app: &AppHandle, name: &str, bin_dir: &Path) -> Result<bool> {
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
            return Ok(false);
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
    Ok(true)
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

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)
        .with_context(|| format!("Cannot stat {}", path.display()))?
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("Cannot set executable bit on {}", path.display()))
}

#[cfg(windows)]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

// ── macOS: launchd ────────────────────────────────────────────────────────────

/// Escape a string for safe embedding inside an XML `<string>` element.
/// Handles the three characters that are special in XML character data.
#[cfg(target_os = "macos")]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

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
    let socket_path = xml_escape(&format!("{}/{}", home_str, daemon_socket_relative()));
    let db_path = xml_escape(&format!("{}/{}/nodespace.db", home_str, DAEMON_DB_DIR));
    let log_out = xml_escape(&format!("{}/{}/nodespaced.log", home_str, DAEMON_LOG_DIR));
    let log_err = xml_escape(&format!(
        "{}/{}/nodespaced-error.log",
        home_str, DAEMON_LOG_DIR
    ));
    let bin_escaped = xml_escape(&bin_str);
    let label_escaped = xml_escape(launch_agent_label());

    // Pro edition: inject the Supabase cloud env the sync daemon needs (#156). The
    // values are baked at build time; all are XML-safe (JWT chars / URLs / schema
    // names contain no `<>&`). Empty for a community build.
    let pro_env = if is_pro_build() {
        format!(
            "        <key>NODESPACED_PRO_SUPABASE_URL</key>\n        <string>{url}</string>\n\
             \x20       <key>NODESPACED_PRO_ANON_KEY</key>\n        <string>{key}</string>\n\
             \x20       <key>NODESPACED_PRO_SCHEMA</key>\n        <string>{schema}</string>\n\
             \x20       <key>NODESPACED_PRO_COLLECTION</key>\n        <string>{coll}</string>\n",
            url = xml_escape(PRO_SUPABASE_URL.unwrap_or_default()),
            key = xml_escape(PRO_ANON_KEY.unwrap_or_default()),
            schema = xml_escape(PRO_SCHEMA.unwrap_or_default()),
            coll = xml_escape(PRO_COLLECTION.unwrap_or_default()),
        )
    } else {
        String::new()
    };

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
{pro_env}    </dict>
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
        label = label_escaped,
        bin = bin_escaped,
        socket = socket_path,
        db = db_path,
        pro_env = pro_env,
        log_out = log_out,
        log_err = log_err,
    );

    std::fs::write(plist_path, plist)
        .with_context(|| format!("Cannot write plist to {}", plist_path.display()))
}

/// Register or restart the launchd user agent.
///
/// Uses the modern `launchctl bootstrap gui/<uid>` API (macOS 10.10+).
/// Any non-success exit code is treated as "might already be registered" — we
/// attempt `bootout` to clear stale launchd state and then retry `bootstrap`.
/// This makes recovery from arbitrary launchctl failures (e.g. exit code 5 I/O
/// error from a diverged plist) self-healing rather than requiring manual
/// `launchctl bootout` or a reboot.
#[cfg(target_os = "macos")]
fn bootstrap_launchd_agent(plist_path: &Path) -> Result<()> {
    let uid = get_uid();
    let gui_target = format!("gui/{}", uid);
    let label = launch_agent_label();
    let service_target = format!("{}/{}", gui_target, label);
    tracing::info!("Bootstrapping launchd agent {} for {}", label, gui_target);

    let output = std::process::Command::new("launchctl")
        .args(["bootstrap", &gui_target, &plist_path.to_string_lossy()])
        .output()
        .context("Failed to run launchctl bootstrap")?;

    if output.status.success() {
        tracing::info!("launchd agent bootstrapped successfully");
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    tracing::warn!(
        "launchctl bootstrap exited with status {} ({}); attempting bootout + retry",
        output.status,
        stderr.trim()
    );

    // Attempt to remove any stale launchd registration (ignoring errors — the
    // service may not actually be registered, which is fine).
    let _ = std::process::Command::new("launchctl")
        .args(["bootout", &gui_target, &plist_path.to_string_lossy()])
        .output();

    // Also try by service target label in case the plist path changed.
    let _ = std::process::Command::new("launchctl")
        .args(["bootout", &service_target])
        .output();

    // Retry the bootstrap now that stale state has been cleared.
    let retry = std::process::Command::new("launchctl")
        .args(["bootstrap", &gui_target, &plist_path.to_string_lossy()])
        .output()
        .context("Failed to run launchctl bootstrap (retry)")?;

    if retry.status.success() {
        tracing::info!("launchd agent bootstrapped successfully (after bootout)");
        return Ok(());
    }

    // If bootstrap still fails, try kickstart as a last resort (handles the
    // case where bootout succeeded but the job is immediately re-registered by
    // launchd's KeepAlive before our retry can bootstrap it).
    let ks_err_str = String::from_utf8_lossy(&retry.stderr);
    tracing::warn!(
        "launchctl bootstrap retry failed ({}); attempting kickstart",
        ks_err_str.trim()
    );
    let kickstart = std::process::Command::new("launchctl")
        .args(["kickstart", "-k", &service_target])
        .output()
        .context("Failed to run launchctl kickstart")?;

    if !kickstart.status.success() {
        let ks_err = String::from_utf8_lossy(&kickstart.stderr);
        tracing::warn!(
            "launchctl kickstart failed (daemon may start on next login): {}",
            ks_err.trim()
        );
    } else {
        tracing::info!("launchd agent kickstarted successfully");
    }
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
    let socket_path = format!("{}/{}", home_str, daemon_socket_relative());
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

/// Enable and start the systemd user service.
#[cfg(target_os = "linux")]
fn enable_systemd_service() -> Result<()> {
    // Reload unit files so systemd picks up the newly written service.
    let reload = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output()
        .context("Failed to run systemctl --user daemon-reload")?;
    if !reload.status.success() {
        let err = String::from_utf8_lossy(&reload.stderr);
        tracing::warn!("systemctl daemon-reload failed: {}", err);
    }

    // Enable and start atomically; --now avoids a separate start call and
    // handles the "never started before" case correctly on all systemd versions.
    let enable = std::process::Command::new("systemctl")
        .args(["--user", "enable", "--now", SYSTEMD_SERVICE_NAME])
        .output()
        .context("Failed to run systemctl --user enable --now")?;
    if !enable.status.success() {
        let err = String::from_utf8_lossy(&enable.stderr);
        tracing::warn!(
            "systemctl enable --now failed (daemon may start on next login): {}",
            err
        );
    } else {
        tracing::info!(
            "systemd user service enabled and started: {}",
            SYSTEMD_SERVICE_NAME
        );
    }

    Ok(())
}

// ── Windows: direct spawn + HKCU autorun ─────────────────────────────────────

#[cfg(windows)]
const WINDOWS_AUTORUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const WINDOWS_AUTORUN_VALUE: &str = "NodeSpaceDaemon";

/// Spawn the nodespaced binary as a detached background process on Windows.
///
/// Uses `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP` so the child is not a
/// member of the parent's job object. Without these flags, Windows terminates
/// all job members (including nodespaced) when the Tauri app exits.
#[cfg(windows)]
fn spawn_daemon_windows(daemon_bin: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    Command::new(daemon_bin)
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("Failed to spawn {}", daemon_bin.display()))?;
    tracing::info!("nodespaced spawned (Windows)");
    Ok(())
}

/// Register the daemon binary in HKCU autorun via reg.exe so it starts on next login.
/// Best-effort: logs on failure but does not propagate the error.
#[cfg(windows)]
fn register_autorun_windows(daemon_bin: &Path) {
    let bin_str = daemon_bin.to_string_lossy().to_string();
    // Wrap the path in quotes so the Windows Run registry evaluator handles
    // paths with spaces (e.g. C:\Users\John Smith\AppData\...) correctly.
    let quoted = format!("\"{}\"", bin_str);
    let result = std::process::Command::new("reg")
        .args([
            "add",
            &format!("HKCU\\{}", WINDOWS_AUTORUN_KEY),
            "/v",
            WINDOWS_AUTORUN_VALUE,
            "/t",
            "REG_SZ",
            "/d",
            &quoted,
            "/f",
        ])
        .output();
    match result {
        Ok(out) if out.status.success() => {
            tracing::info!("nodespaced registered in HKCU autorun");
        }
        Ok(out) => {
            tracing::warn!(
                "Failed to register autorun: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Err(e) => {
            tracing::warn!("Failed to run reg.exe for autorun: {}", e);
        }
    }
}
