//! Daemon lifecycle management — macOS (launchd), Linux (systemd), Windows (direct spawn).
//!
//! On first launch:
//!   1. Locate sidecar binaries bundled beside the app's own executable (Tauri's
//!      `externalBin` convention — see `resolve_sidecar_path`'s doc comment for
//!      why this is NOT the `Resources`/resource-resolver tree).
//!   2. Copy them to ~/.nodespace/bin/ (skipped if dest already matches bundled size).
//!   3. Register the daemon as a user service:
//!      - macOS: write ~/Library/LaunchAgents/<plist_filename()> and bootstrap it.
//!        The filename and launchd label vary by build variant (debug/release × community/Pro)
//!        so dev builds and the production app never collide on the same launchd job or socket.
//!      - Linux: write ~/.config/systemd/user/nodespace.service and enable it.
//!      - Windows: spawn the daemon process directly (stdout/stderr routed to
//!        ~/.nodespace/logs/nodespaced.log and nodespaced-error.log, mirroring
//!        launchd/systemd on the other two platforms) and write an HKCU autorun key.
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
use tauri::AppHandle;
use tokio::time::timeout;

const DAEMON_BIN_DIR: &str = ".nodespace/bin";
const DAEMON_DB_DIR: &str = ".nodespace/database";
const DAEMON_LOG_DIR: &str = ".nodespace/logs";
const DAEMON_BINARY_NAME: &str = "nodespaced";
const PRO_DAEMON_BINARY_NAME: &str = "nodespaced-pro";
const CLI_BINARY_NAME: &str = "nodespace";

#[cfg(target_os = "linux")]
const SYSTEMD_SERVICE_NAME: &str = "nodespace.service";

// ── Pro edition, baked at compile time ───────────────────────────────
// A Pro build sets these env vars when running `tauri build` (see
// nodespace-sync/scripts/build-pro-dmg.sh); a community build leaves them unset.
// When set, the app installs + launches the `nodespaced-pro` sync daemon with the
// Supabase cloud env instead of the community `nodespaced`. The anon key is the
// project's PUBLISHABLE key (safe to bake into the binary).
const PRO_SUPABASE_URL: Option<&str> = option_env!("NODESPACE_PRO_SUPABASE_URL");
const PRO_ANON_KEY: Option<&str> = option_env!("NODESPACE_PRO_ANON_KEY");

/// True when this binary was built as the Pro edition (cloud env baked in).
/// `pub(crate)` so the update check can pick the Pro release source (see
/// `update_check`) off the same discriminator the daemon setup uses.
pub(crate) fn is_pro_build() -> bool {
    PRO_SUPABASE_URL.is_some()
}

/// Pure function form of `daemon_binary_name()`, taking edition as a
/// parameter instead of reading it from `is_pro_build()`'s compile-time-baked
/// constant. This lets both editions' binary-name logic be exercised by an
/// ordinary `#[test]` on any development machine — a real Pro build can only
/// ever produce one edition per binary, so `is_pro_build()` itself can't be
/// flipped at test time.
fn daemon_binary_name_for(is_pro: bool) -> &'static str {
    if is_pro {
        PRO_DAEMON_BINARY_NAME
    } else {
        DAEMON_BINARY_NAME
    }
}

/// The daemon sidecar this edition installs + launches.
fn daemon_binary_name() -> &'static str {
    daemon_binary_name_for(is_pro_build())
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

/// macOS plist filename — label + ".plist", so label and filename are always in sync.
#[cfg(target_os = "macos")]
fn plist_filename() -> String {
    format!("{}.plist", launch_agent_label())
}

/// Synchronously kill the running daemon if the installed binary differs in size
/// from the bundled sidecar. Called before the gRPC client is managed so the
/// frontend never connects to a stale daemon.
///
/// This is intentionally synchronous and cheap (two stat calls + maybe lsof) —
/// it is a size-only pre-check, not a signature-aware gate. It does not call
/// `codesign --verify`, so a same-size install with a corrupted signature is
/// left running here; that self-heal happens moments later in the async
/// `extract_sidecar_if_changed`, which re-extracts and kills the daemon via
/// `binary_updated` once verification fails. `ensure_daemon_running` handles
/// extraction and restart afterward.
#[cfg(unix)]
pub fn kill_stale_daemon_sync() {
    let home = match home_dir() {
        Some(h) => h,
        None => return,
    };
    let bin_dir = home.join(DAEMON_BIN_DIR);
    let socket_path = home.join(daemon_socket_relative());
    let installed = bin_dir.join(daemon_binary_name());

    let bundled_size = match resolve_sidecar_path_sync() {
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
                    let argv0 = args.split_whitespace().next().unwrap_or("");
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

fn resolve_sidecar_path_sync() -> Option<PathBuf> {
    sidecar_path_from_exe(
        &std::env::current_exe().ok()?,
        &bundled_sidecar_name(daemon_binary_name()),
    )
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
    // stdout/stderr are routed to log files in log_dir (mirroring launchd's
    // StandardOutPath/StandardErrorPath on macOS and systemd's StandardOutput=/
    // StandardError=append: on Linux) rather than Stdio::null() — otherwise any
    // diagnostic the daemon writes to stdout/stderr (tracing's default writer,
    // or an eprintln!-based diagnostic like SchemaNode::from_node's fields-parse
    // failure message) is silently discarded.
    #[cfg(windows)]
    {
        spawn_daemon_windows(&daemon_bin, &log_dir).context("Failed to spawn daemon on Windows")?;
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
                        let argv0 = args.split_whitespace().next().unwrap_or("");
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

/// Windows taskkill `/IM` image name for a given edition's daemon binary.
///
/// Pure function form, gated on `any(windows, test)` rather than `windows`
/// alone (mirroring `daemon_log_paths`/`open_daemon_log` below), so the exact
/// bug class this fixes — an image name that can silently drift from
/// `daemon_binary_name_for()` — is directly testable on any platform, not
/// just compile-checked against the Windows target.
///
/// The `.exe` suffix mirrors the literal this replaces (`"nodespaced.exe"`);
/// it assumes the installed daemon binary carries that extension on Windows,
/// which — like everything else `#[cfg(windows)]` in this file — has not
/// been confirmed by an actual Windows run.
#[cfg(any(windows, test))]
fn daemon_image_name_for(is_pro: bool) -> String {
    format!("{}.exe", daemon_binary_name_for(is_pro))
}

/// Kill the running daemon on Windows via taskkill and wait for it to exit.
///
/// Targets the edition-correct image name (`nodespaced.exe` or
/// `nodespaced-pro.exe`, via `daemon_binary_name_for()`) rather than a
/// hardcoded community-edition literal — a Pro build's `nodespaced-pro.exe`
/// would otherwise never be matched and killed before launching the updated
/// binary.
#[cfg(windows)]
async fn kill_running_daemon(socket_path: &Path) {
    if check_daemon_socket(socket_path).await != DaemonStatus::Healthy {
        return;
    }

    let image_name = daemon_image_name_for(is_pro_build());
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/IM", &image_name])
        .output();
    tracing::info!(%image_name, "Sent taskkill to daemon");

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
///
/// `pub` (not `pub(crate)`) so the readiness integration test — which lives
/// in `tests/`, a separate crate — can drive it against a real daemon.
pub async fn wait_for_daemon(socket_path: &Path, max_wait: Duration) -> DaemonStatus {
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
/// but only re-copy it if the destination is missing, has a different file
/// size than the bundled source, or fails code-signature verification.
/// Returns true if the binary was (re-)extracted.
///
/// Quarantine is cleared on `dest` unconditionally, even when re-copying is
/// skipped: an ad-hoc signature stays valid regardless of quarantine state
/// (see `clear_quarantine`'s doc comment), so a binary already on disk from
/// before this check existed can be same-size and validly signed and still
/// carry the flag that gets it killed by syspolicyd — a size/signature match
/// alone would otherwise let that binary sit there untouched forever on any
/// upgrade whose bundled daemon binary happens not to change size.
///
/// When re-copying does happen, it writes to a temp path in the same
/// directory, re-seals the ad-hoc signature and clears quarantine there too,
/// then renames into place — so a concurrently-launched launchd `KeepAlive`
/// daemon can never `mmap` a partially-written, unsigned, or quarantined
/// image.
async fn extract_sidecar_if_changed(app: &AppHandle, name: &str, bin_dir: &Path) -> Result<bool> {
    let src = resolve_sidecar_path(app, name)?;
    let dest = bin_dir.join(name);

    let src_size = tokio::fs::metadata(&src)
        .await
        .with_context(|| format!("Cannot stat bundled sidecar {}", src.display()))?
        .len();

    if let Ok(dest_meta) = tokio::fs::metadata(&dest).await {
        if dest_meta.len() == src_size && verify_signature(&dest) {
            // A signature check alone doesn't rule out a lingering quarantine
            // flag: an ad-hoc signature stays valid regardless of quarantine
            // state, so a binary extracted before this fix existed can sit
            // here indefinitely — same size, validly signed, still killed by
            // syspolicyd — and every upgrade whose bundled daemon binary
            // happens not to change size would otherwise skip extraction and
            // never reach the one call that clears it. Always clear it here
            // too, not just on the extraction path below.
            clear_quarantine(&dest)?;
            tracing::debug!(
                "{} is up-to-date (size={}) and validly signed, skipping extraction",
                name,
                src_size
            );
            return Ok(false);
        }
        if dest_meta.len() == src_size {
            tracing::warn!(
                "{} matches bundled size but has an invalid code signature — re-extracting",
                name
            );
        }
    }

    tracing::info!(
        "Extracting {} ({} bytes) to {}",
        name,
        src_size,
        dest.display()
    );

    let tmp_dest = bin_dir.join(format!("{}.tmp-{}", name, std::process::id()));

    tokio::fs::copy(&src, &tmp_dest)
        .await
        .with_context(|| format!("Failed to copy {} to {}", src.display(), tmp_dest.display()))?;

    // If anything below fails, remove the temp file rather than leaving a stray
    // `.tmp-<pid>` behind in ~/.nodespace/bin/ — harmless on its own, but it
    // would otherwise accumulate silently across repeated failures.
    if let Err(e) = set_executable(&tmp_dest)
        .and_then(|_| resign_binary(&tmp_dest))
        .and_then(|_| clear_quarantine(&tmp_dest))
    {
        let _ = std::fs::remove_file(&tmp_dest);
        return Err(e);
    }

    tokio::fs::rename(&tmp_dest, &dest)
        .await
        .with_context(|| format!("Failed to install {} to {}", name, dest.display()))?;

    Ok(true)
}

/// Re-establish the ad-hoc seal on an extracted binary so its code signature
/// matches the bundled sidecar's `Signature=adhoc, linker-signed`. Without
/// this, macOS invalidates the seal on write and SIGKILLs the process the
/// moment execution faults into a modified page (`CODESIGNING`/`Invalid Page`).
#[cfg(target_os = "macos")]
fn resign_binary(path: &Path) -> Result<()> {
    let status = std::process::Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(path)
        .status()
        .context("Failed to invoke codesign")?;
    anyhow::ensure!(
        status.success(),
        "codesign --force --sign - failed for {}",
        path.display()
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn resign_binary(_path: &Path) -> Result<()> {
    Ok(())
}

/// Strip the `com.apple.quarantine` extended attribute an extracted sidecar
/// can pick up from the copy that produced it.
///
/// The sidecar's signature is ad-hoc, not Developer-ID (see `resign_binary`'s
/// doc comment) — it runs fine *inside* the bundle because Gatekeeper's
/// notarization check happens once, at the bundle level, when the user
/// launches `NodeSpace.app` itself, and everything inside is implicitly
/// trusted in that context. Once copied out to `~/.nodespace/bin/` and
/// registered as an independent `launchd` service — not spawned as a child of
/// the already-trusted running app — `syspolicyd` evaluates it on its own:
/// an ad-hoc-only signature fails that independent check outright if the file
/// also carries a quarantine flag, which is exactly what a plain copy of a
/// quarantined source produces (confirmed directly: even `cp` on a quarantined
/// file leaves a freshly-quarantined destination, not an unquarantined one).
/// The observed failure is `syspolicyd: rejecting due to lack of matching
/// active rule`, `bundle_id: NOT_A_BUNDLE` — the daemon never stays up long
/// enough to do anything (core#2287).
///
/// `xattr -d` exits non-zero both when the attribute is simply absent (the
/// common case: a source that was never quarantined) and on a genuine
/// failure (permission denied, read-only filesystem) — the exit code alone
/// doesn't distinguish them. Checking presence first with a separate, plain
/// `xattr` invocation (no `-d`) sidesteps that ambiguity entirely: skip the
/// removal altogether when the attribute isn't there, and treat any failure
/// of the removal itself — now known to be acting on a real attribute — as
/// the genuine error it is, rather than swallowing it at debug level.
#[cfg(target_os = "macos")]
fn has_quarantine_attribute(path: &Path) -> bool {
    std::process::Command::new("xattr")
        .arg(path)
        .output()
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .any(|line| line == "com.apple.quarantine")
        })
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn clear_quarantine(path: &Path) -> Result<()> {
    if !has_quarantine_attribute(path) {
        return Ok(());
    }
    let status = std::process::Command::new("xattr")
        .args(["-d", "com.apple.quarantine"])
        .arg(path)
        .status()
        .context("Failed to invoke xattr")?;
    anyhow::ensure!(
        status.success(),
        "xattr -d com.apple.quarantine failed for {} despite the attribute being present",
        path.display()
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn clear_quarantine(_path: &Path) -> Result<()> {
    Ok(())
}

/// Verify an installed binary's code signature is intact. Always `true` on
/// non-macOS platforms (no code-signing enforcement to check).
#[cfg(target_os = "macos")]
fn verify_signature(path: &Path) -> bool {
    std::process::Command::new("codesign")
        .args(["--verify", "--strict"])
        .arg(path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn verify_signature(_path: &Path) -> bool {
    true
}

/// Resolve a sidecar's path inside the installed Tauri bundle.
///
/// `nodespaced`/`nodespace` are declared under `bundle.externalBin` in
/// `tauri.conf.json`, not `bundle.resources` — two things distinguish that
/// from what the code here used to assume:
///
/// 1. **Directory.** Tauri places `externalBin` sidecars next to the app's own
///    executable (`Contents/MacOS/` on macOS), not under `Contents/Resources/`.
///    Resolving via `BaseDirectory::Resource` (the API for `resources` entries,
///    which is where `resources/skill` and `resources/models` — the *other*
///    kind of bundled file this app ships — correctly land) looked in a
///    directory these binaries were never copied into, so every daemon-setup
///    call failed to even stat the sidecar on a freshly installed app.
///    `current_exe()`'s directory is the correct base on every platform Tauri
///    targets.
/// 2. **Filename.** The *source* tree under `src-tauri/binaries/` holds one
///    triple-suffixed file per target (`nodespaced-aarch64-apple-darwin`, so a
///    cross-compilation source tree can hold several), but the bundler copies
///    the one matching the current build into the bundle **renamed to the bare
///    name** — confirmed directly against a real build output
///    (`Contents/MacOS/nodespaced`, no triple). The runtime lookup has to match
///    that renamed, installed file, not the source-tree name.
fn resolve_sidecar_path(_app: &AppHandle, name: &str) -> Result<PathBuf> {
    let exe = std::env::current_exe().context("Cannot resolve current executable path")?;
    sidecar_path_from_exe(&exe, &bundled_sidecar_name(name))
        .with_context(|| format!("Cannot resolve sidecar path for '{}'", name))
}

/// The installed sidecar's filename: the bare name Tauri renames it to when
/// bundling, plus the platform's native executable extension. `.exe` on
/// Windows; no extension on macOS/Linux, where executables don't carry one.
pub(crate) fn bundled_sidecar_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Pure form of the sidecar-path computation: the installed sidecar lives
/// beside the running executable, since Tauri bundles `externalBin` entries
/// into the same directory as the app binary itself on every target
/// (`Contents/MacOS/` on macOS, alongside the `.exe` on Windows, alongside the
/// main binary in the AppImage/deb layout on Linux) — never under a Resources
/// tree. Takes the executable path as a parameter (rather than calling
/// `current_exe()` itself) so this is exercisable with a synthetic path in a
/// unit test, independent of where cargo actually places the test binary.
pub(crate) fn sidecar_path_from_exe(exe_path: &Path, sidecar_name: &str) -> Option<PathBuf> {
    Some(exe_path.parent()?.join(sidecar_name))
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
///
/// `KeepAlive` is the conditional `{SuccessfulExit: false}` form, NOT bare
/// `true` (core#2353). Bare `true` restarts the job on *any* exit, with no
/// way to distinguish a crash from a deliberate, successful shutdown --
/// which meant the tray's "Quit" item was undone by launchd relaunching the
/// process within about half a second, regardless of how cleanly it had
/// just shut down. `SuccessfulExit: false` only restarts on a nonzero/
/// crash exit, matching `main`'s `Ok(())` (exit 0) vs `Err(...)` (exit
/// nonzero, same as a panic) behavior on the verified tray-Quit path -- and
/// matches the Linux systemd unit's `Restart=on-failure` in
/// `write_systemd_service`, which never had this bug because it was never
/// unconditional to begin with.
///
/// This does NOT by itself guarantee a SIGTERM'd or internally-panicked
/// daemon always reaches that `Ok(())`/`Err(...)` split while the tray is
/// up -- `main`'s tray-mode event loop only reacts to the tray's own "Quit"
/// click today, a separate, pre-existing gap tracked in a follow-up issue.
/// This fix is specifically about the path that IS wired end to end: a
/// deliberate tray "Quit."
#[cfg(target_os = "macos")]
fn write_plist(home: &Path, plist_path: &Path, daemon_bin: &Path) -> Result<()> {
    let launch_agents = plist_path
        .parent()
        .context("plist_path has no parent directory")?;
    std::fs::create_dir_all(launch_agents).context("Failed to create ~/Library/LaunchAgents")?;

    let home_str = home.to_string_lossy();
    let bin_str = daemon_bin.to_string_lossy();
    let socket_path = xml_escape(&format!("{}/{}", home_str, daemon_socket_relative()));
    let log_out = xml_escape(&format!("{}/{}/nodespaced.log", home_str, DAEMON_LOG_DIR));
    let log_err = xml_escape(&format!(
        "{}/{}/nodespaced-error.log",
        home_str, DAEMON_LOG_DIR
    ));
    let bin_escaped = xml_escape(&bin_str);
    let label_escaped = xml_escape(launch_agent_label());

    // The UI binary path: this function runs inside nodespace-app, so current_exe()
    // returns the path the daemon needs to re-launch the GUI from the tray.
    // canonicalize() resolves symlinks/wrapper paths that some macOS launch contexts produce.
    let ui_binary = xml_escape(
        &std::env::current_exe()
            .context("Cannot resolve current executable path for NODESPACE_UI_BINARY")?
            .canonicalize()
            .context("Cannot canonicalize current executable path for NODESPACE_UI_BINARY")?
            .to_string_lossy(),
    );

    // Pro edition: inject the deployment-wide Supabase endpoint the sync daemon
    // needs. Only the project URL and publishable anon key are baked in — both are
    // deployment-wide, not tenant-specific. The tenant a database syncs to (schema +
    // collection) is bound per database at runtime and is deliberately NOT injected
    // here (ADR-053 per-database cloud sync). Both values are XML-safe (JWT chars /
    // URLs contain no `<>&`). Empty for a community build.
    let pro_env = if is_pro_build() {
        format!(
            "        <key>NODESPACED_PRO_SUPABASE_URL</key>\n        <string>{url}</string>\n\
             \x20       <key>NODESPACED_PRO_ANON_KEY</key>\n        <string>{key}</string>\n",
            url = xml_escape(PRO_SUPABASE_URL.unwrap_or_default()),
            key = xml_escape(PRO_ANON_KEY.unwrap_or_default()),
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
        <key>NODESPACE_UI_BINARY</key>
        <string>{ui_binary}</string>
{pro_env}    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
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
        ui_binary = ui_binary,
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
/// attempt `enable` + `bootout` to clear stale launchd state and then retry
/// `bootstrap`. This makes recovery from arbitrary launchctl failures (e.g.
/// exit code 5 I/O error from a diverged plist) self-healing rather than
/// requiring manual `launchctl bootout`/`enable` or a reboot.
///
/// `enable` matters on its own: a disabled label is a *persistent per-user*
/// launchd override that survives deleting the plist and reinstalling --
/// `bootout` does not clear it, so without this a disabled label makes every
/// bootstrap attempt fail identically forever, with launchd's own error
/// ("Input/output error") giving no hint that the label is disabled at all.
/// Harmless when the label isn't disabled -- `launchctl enable` on an
/// already-enabled label is a no-op.
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
        "launchctl bootstrap exited with status {} ({}); attempting enable + bootout + retry",
        output.status,
        stderr.trim()
    );

    // Clear a disabled-label override before bootout -- bootout alone
    // cannot, and a still-disabled label would make the retry below fail
    // with the exact same error.
    let _ = std::process::Command::new("launchctl")
        .args(["enable", &service_target])
        .output();

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
    let retry_stderr = String::from_utf8_lossy(&retry.stderr);
    tracing::warn!(
        "launchctl bootstrap retry failed ({}); attempting kickstart",
        retry_stderr.trim()
    );
    let kickstart = std::process::Command::new("launchctl")
        .args(["kickstart", "-k", &service_target])
        .output()
        .context("Failed to run launchctl kickstart")?;

    if !kickstart.status.success() {
        let ks_err = String::from_utf8_lossy(&kickstart.stderr);
        // Every fallback exhausted -- bootstrap, enable + bootout + retry,
        // and kickstart all failed. Returning here (rather than swallowing
        // this into Ok, as before) is what lets `ensure_daemon_running`'s
        // `?` actually surface a reason instead of silently falling through
        // to a later, unrelated "daemon not running" check with no context
        // on why -- the exact "Retry button that cannot succeed" symptom
        // this is meant to fix.
        return Err(anyhow::anyhow!(
            "launchd agent {label} failed to start: bootstrap, bootout+retry, and kickstart all \
             failed (last error: {err})",
            label = label,
            err = ks_err.trim()
        ));
    }
    tracing::info!("launchd agent kickstarted successfully");
    Ok(())
}

/// `bootstrap_launchd_agent` shells out to the real `launchctl` and would,
/// under test, operate on this machine's actual dev-build agent label
/// (tests compile with `cfg(debug_assertions)`) -- risking disruption of a
/// developer's own running dev daemon rather than a safe, isolated
/// fixture. Pinned at the source level instead, the same technique
/// `lib.rs`'s `persist_window_geometry_captures_inner_size_not_outer_size...`
/// test uses for the same reason (no safe way to exercise real OS-level
/// window/process state under test).
#[cfg(all(test, target_os = "macos"))]
mod bootstrap_launchd_agent_recovery_order_tests {
    /// Slices out exactly `bootstrap_launchd_agent`'s own body by counting
    /// balanced braces from its opening one, rather than searching for the
    /// next item's name -- a name-based end marker is fragile to whatever
    /// happens to be declared next in the file (this test module itself
    /// used to sit between the function and `fn get_uid`, which made an
    /// earlier version of this helper's slice silently include the test
    /// module's own source, including its own assertion strings).
    fn function_source() -> &'static str {
        let source = include_str!("daemon_setup.rs");
        let start = source
            .find("fn bootstrap_launchd_agent")
            .expect("bootstrap_launchd_agent not found in daemon_setup.rs");
        let body_start = source[start..]
            .find('{')
            .map(|offset| start + offset)
            .expect("bootstrap_launchd_agent has no opening brace");
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in source[body_start..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(body_start + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = end.expect("bootstrap_launchd_agent's braces never balance to zero");
        &source[start..end]
    }

    #[test]
    fn enable_runs_before_bootout_on_a_bootstrap_failure() {
        let src = function_source();
        let enable_pos = src
            .find(r#"["enable", &service_target]"#)
            .expect("must call `launchctl enable` on the failed label before bootout+retry");
        let first_bootout_pos = src
            .find(r#"["bootout""#)
            .expect("must still attempt bootout after enable");
        assert!(
            enable_pos < first_bootout_pos,
            "`launchctl enable` must run BEFORE the first `bootout` -- a disabled label \
             survives bootout, so enabling it after bootout (or not at all) leaves the \
             identical failure on retry"
        );
    }

    #[test]
    fn a_kickstart_failure_returns_an_error_instead_of_being_swallowed() {
        let src = function_source();
        assert!(
            src.contains("return Err(anyhow::anyhow!"),
            "a kickstart failure -- the last fallback -- must return Err so \
             `ensure_daemon_running`'s `?` can surface a real reason, instead of always \
             returning Ok(()) and leaving the caller to discover the daemon never started \
             from an unrelated, contextless check further downstream"
        );
    }
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
    let log_out = format!("{}/{}/nodespaced.log", home_str, DAEMON_LOG_DIR);
    let log_err = format!("{}/{}/nodespaced-error.log", home_str, DAEMON_LOG_DIR);

    // This function runs inside nodespace-app, so current_exe() is the UI binary
    // the daemon needs to re-launch the GUI from the tray.
    // canonicalize() resolves any symlinks that some Linux launch contexts produce.
    let ui_binary = std::env::current_exe()
        .context("Cannot resolve current executable path for NODESPACE_UI_BINARY")?
        .canonicalize()
        .context("Cannot canonicalize current executable path for NODESPACE_UI_BINARY")?
        .to_string_lossy()
        .into_owned();

    // systemd Environment= values with spaces must be single-quoted. Escape embedded
    // single quotes as '\'' (end quote, literal single quote, reopen quote).
    let sq_escape = |s: &str| s.replace('\'', r"'\''");

    let unit = format!(
        "[Unit]\n\
         Description=NodeSpace daemon\n\
         After=network.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={bin}\n\
         Environment=NODESPACED_SOCKET='{socket}'\n\
         Environment=NODESPACE_UI_BINARY='{ui_binary}'\n\
         StandardOutput=append:{log_out}\n\
         StandardError=append:{log_err}\n\
         Restart=on-failure\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        bin = bin_str,
        socket = sq_escape(&socket_path),
        ui_binary = sq_escape(&ui_binary),
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

/// Compute the (stdout, stderr) log file paths for the daemon, given its log
/// directory. Filenames are identical to the ones the macOS `launchd` plist
/// (`write_plist`'s `StandardOutPath`/`StandardErrorPath`) and the Linux
/// `systemd` unit (`write_systemd_service`'s `StandardOutput=`/`StandardError=`)
/// already write to, so support/debugging habits transfer across platforms.
///
/// Kept as a plain, platform-independent function (no `#[cfg(windows)]`) so
/// it can be exercised by an ordinary `#[test]` on any development machine,
/// even though only the Windows direct-spawn path calls it today.
#[cfg(any(windows, test))]
fn daemon_log_paths(log_dir: &Path) -> (PathBuf, PathBuf) {
    (
        log_dir.join("nodespaced.log"),
        log_dir.join("nodespaced-error.log"),
    )
}

/// Open a daemon log file for append, creating it if it doesn't exist yet.
///
/// Uses `append` (not `truncate`) so log history survives across daemon
/// restarts — this file is reopened every time `ensure_daemon_running` spawns
/// a fresh daemon process (binary updates, crash recovery, etc.), which would
/// otherwise silently drop everything logged before the most recent restart.
/// This mirrors the Linux systemd unit's explicit `append:` prefix on
/// `StandardOutput=`/`StandardError=` in `write_systemd_service`.
///
/// Pure `std::fs` — no Windows-specific API — so it's directly testable here.
#[cfg(any(windows, test))]
fn open_daemon_log(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
}

/// Open a daemon log file for the spawned child's stdio, falling back to
/// `Stdio::null()` (the pre-existing behavior this PR replaces) with a
/// warning if the file can't be opened — e.g. a locked-by-AV or permissions
/// edge case. A log-file-open failure must not block daemon startup itself:
/// on macOS/Linux, a failed `launchd`/`systemd` log write never prevents the
/// service from running (the app only ever writes the plist/unit text; the
/// service manager owns opening the log), so treating it as fatal here would
/// be a Windows-only regression relative to that behavior.
#[cfg(windows)]
fn daemon_log_stdio(path: &Path) -> std::process::Stdio {
    match open_daemon_log(path) {
        Ok(f) => std::process::Stdio::from(f),
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "Failed to open daemon log file — falling back to discarding this stream"
            );
            std::process::Stdio::null()
        }
    }
}

/// Spawn the nodespaced binary as a detached background process on Windows.
///
/// Uses `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP` so the child is not a
/// member of the parent's job object. Without these flags, Windows terminates
/// all job members (including nodespaced) when the Tauri app exits.
///
/// stdout/stderr are opened as log files in `log_dir` and handed to the child
/// via `Stdio::from(File)` rather than `Stdio::null()` — mirroring `launchd`'s
/// `StandardOutPath`/`StandardErrorPath` (macOS) and `systemd`'s
/// `StandardOutput=`/`StandardError=append:` (Linux), both defined earlier in
/// this file. Without this, any diagnostic the daemon writes to stdout/stderr
/// — including `tracing_subscriber::fmt()`'s default stdout writer and
/// `eprintln!`-based diagnostics such as `SchemaNode::from_node`'s
/// fields-parse-failure message — is discarded outright, since a Windows
/// child spawned with `Stdio::null()` has no destination at all for that
/// output (unlike `/dev/null`, there's no dropped-but-consistent sink; the
/// handle is simply absent).
///
/// A log file that fails to open (see `daemon_log_stdio`) falls back to
/// `Stdio::null()` for that stream rather than aborting the spawn — daemon
/// startup must not fail just because logging couldn't be set up.
#[cfg(windows)]
fn spawn_daemon_windows(daemon_bin: &Path, log_dir: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

    let (stdout_path, stderr_path) = daemon_log_paths(log_dir);

    Command::new(daemon_bin)
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .stdin(Stdio::null())
        .stdout(daemon_log_stdio(&stdout_path))
        .stderr(daemon_log_stdio(&stderr_path))
        .spawn()
        .with_context(|| format!("Failed to spawn {}", daemon_bin.display()))?;
    tracing::info!(
        log_dir = %log_dir.display(),
        "nodespaced spawned (Windows), stdout/stderr routed to log files"
    );
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

#[cfg(all(test, target_os = "macos"))]
mod macos_codesign_tests {
    use super::{clear_quarantine, has_quarantine_attribute, resign_binary, verify_signature};
    use std::path::PathBuf;

    /// Copy a real Mach-O binary into a scratch dir so tampering with its
    /// signature (via a byte flip) doesn't touch anything on the real system.
    fn scratch_copy(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ns-codesign-test-{}-{}", tag, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("bin");
        std::fs::copy("/bin/cat", &dest).unwrap();
        dest
    }

    #[test]
    fn resign_then_verify_succeeds() {
        let bin = scratch_copy("resign-ok");
        resign_binary(&bin).expect("codesign --force --sign - should succeed");
        assert!(
            verify_signature(&bin),
            "freshly re-signed binary should pass --verify --strict"
        );
    }

    #[test]
    fn tampering_after_sign_invalidates_signature() {
        let bin = scratch_copy("tamper");
        resign_binary(&bin).expect("codesign should succeed");
        assert!(
            verify_signature(&bin),
            "should be valid right after signing"
        );

        // Flip a byte past the Mach-O header to invalidate the seal without
        // corrupting it so badly the file won't open at all.
        let mut bytes = std::fs::read(&bin).unwrap();
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xFF;
        std::fs::write(&bin, bytes).unwrap();

        assert!(
            !verify_signature(&bin),
            "tampered binary must fail --verify --strict — this is exactly the \
             corruption class that causes the CODESIGNING SIGKILL crash-loop"
        );
    }

    /// Reads the `com.apple.quarantine` xattr directly via the `xattr` CLI —
    /// deliberately not through `clear_quarantine` itself, so the assertion
    /// can't pass merely because both sides share a bug.
    fn has_quarantine_attr(path: &std::path::Path) -> bool {
        std::process::Command::new("xattr")
            .arg(path)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("com.apple.quarantine"))
            .unwrap_or(false)
    }

    #[test]
    fn clear_quarantine_removes_a_real_quarantine_attribute() {
        let bin = scratch_copy("clear-quarantine");
        // `xattr -w` on a freshly-copied scratch file — not a claim about how
        // the daemon's own extraction path acquires the attribute (that's a
        // plain `cp` of an already-quarantined source, verified separately
        // during core#2287's investigation), just a controlled fixture for
        // this function's own removal behavior.
        std::process::Command::new("xattr")
            .args(["-w", "com.apple.quarantine", "0081;00000000;test;"])
            .arg(&bin)
            .status()
            .expect("xattr -w should succeed on a scratch file");
        assert!(
            has_quarantine_attr(&bin),
            "fixture setup must actually apply the attribute, or this test proves nothing"
        );

        clear_quarantine(&bin).expect("clear_quarantine must not error on a real attribute");

        assert!(
            !has_quarantine_attr(&bin),
            "com.apple.quarantine must be gone after clear_quarantine — this is the exact \
             attribute that makes syspolicyd reject an ad-hoc-signed extracted binary \
             (core#2287)"
        );
    }

    #[test]
    fn clear_quarantine_on_an_unquarantined_file_is_not_an_error() {
        // The common case: a source that was never quarantined in the first
        // place. `xattr -d` on an absent attribute exits non-zero — this must
        // not surface as a hard failure that aborts extraction.
        let bin = scratch_copy("no-quarantine");
        assert!(
            !has_quarantine_attr(&bin),
            "fixture must start with no quarantine attribute"
        );
        clear_quarantine(&bin).expect(
            "clearing an absent attribute is success, not failure — extraction must not abort \
             over a file that was never quarantined",
        );
    }

    #[test]
    fn has_quarantine_attribute_reflects_real_xattr_state() {
        // Production `has_quarantine_attribute`, checked directly — this is
        // the presence check `clear_quarantine` now runs before attempting
        // removal, so a bug here would either skip a real removal (leaving a
        // binary quarantined) or attempt — and potentially fail loudly on —
        // a removal that was never needed.
        let unquarantined = scratch_copy("presence-check-absent");
        assert!(
            !has_quarantine_attribute(&unquarantined),
            "must report false when the attribute was never set"
        );

        let quarantined = scratch_copy("presence-check-present");
        std::process::Command::new("xattr")
            .args(["-w", "com.apple.quarantine", "0081;00000000;test;"])
            .arg(&quarantined)
            .status()
            .expect("xattr -w should succeed on a scratch file");
        assert!(
            has_quarantine_attribute(&quarantined),
            "must report true once the attribute is actually present"
        );
    }

    #[test]
    fn extraction_skip_path_still_clears_a_pre_existing_quarantine_flag() {
        // Regresses the gap an adversarial review of PR#2290 caught: a
        // same-size, validly-signed binary can still be quarantined if it was
        // extracted before this fix existed, and a naive skip-when-unchanged
        // check would leave it that way forever on any upgrade whose bundled
        // daemon binary happens not to change size. `extract_sidecar_if_changed`
        // itself needs a live AppHandle to exercise end-to-end, so this pins
        // the same behavior at the level that can run in an ordinary `#[test]`:
        // `clear_quarantine` must be safe and effective to call on a file that
        // is already up-to-date, not just on a freshly-copied temp file.
        let bin = scratch_copy("skip-path-quarantine");
        std::process::Command::new("xattr")
            .args(["-w", "com.apple.quarantine", "0081;00000000;test;"])
            .arg(&bin)
            .status()
            .expect("xattr -w should succeed on a scratch file");
        resign_binary(&bin).expect("codesign should succeed");
        assert!(
            verify_signature(&bin),
            "fixture must be validly signed — the exact state that hits the skip path"
        );
        assert!(
            has_quarantine_attribute(&bin),
            "fixture setup must actually apply the attribute, or this test proves nothing"
        );

        clear_quarantine(&bin)
            .expect("clear_quarantine must succeed on an already-signed, up-to-date binary");

        assert!(
            !has_quarantine_attribute(&bin),
            "a signature match must not be treated as proof the binary is unquarantined"
        );
    }
}

/// core#2353 regression guard: launchd's `KeepAlive` must be the conditional
/// `{SuccessfulExit: false}` form, not bare `true`. Bare `true` restarted the
/// daemon on ANY exit -- crash or a fully clean, deliberate shutdown (e.g.
/// the tray "Quit" item) alike -- since launchd has no way to distinguish
/// them under that form. A future edit that reverts to `<true/>` for
/// simplicity would silently reintroduce the exact bug this module exists to
/// prevent, so this asserts on the literal rendered XML rather than just
/// checking the plist parses.
#[cfg(all(test, target_os = "macos"))]
mod macos_plist_keepalive_tests {
    use super::write_plist;
    use std::path::PathBuf;

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ns-plist-keepalive-test-{}-{}",
            tag,
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn keep_alive_is_conditional_on_successful_exit_not_bare_true() {
        let home = scratch_dir("keepalive");
        let plist_path = home.join("Library/LaunchAgents/app.nodespace.daemon.plist");
        let daemon_bin = home.join("bin/nodespaced");

        write_plist(&home, &plist_path, &daemon_bin).expect("write_plist should succeed");
        let contents = std::fs::read_to_string(&plist_path).expect("plist should be written");

        assert!(
            !contents.contains("<key>KeepAlive</key>\n    <true/>"),
            "KeepAlive must not be the unconditional bare `true` form -- that restarts the \
             daemon on every exit, including a clean, deliberate shutdown (core#2353): {contents}"
        );
        assert!(
            contents.contains("<key>SuccessfulExit</key>\n        <false/>"),
            "KeepAlive must use the conditional {{SuccessfulExit: false}} form so launchd only \
             restarts on a nonzero/crash exit: {contents}"
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn keep_alive_dict_is_well_formed_xml() {
        let home = scratch_dir("wellformed");
        let plist_path = home.join("Library/LaunchAgents/app.nodespace.daemon.plist");
        let daemon_bin = home.join("bin/nodespaced");

        write_plist(&home, &plist_path, &daemon_bin).expect("write_plist should succeed");
        assert!(
            plist_path.exists(),
            "write_plist should have created the file"
        );

        // `plutil -lint` validates real plist XML structure (matching quotes,
        // balanced tags, valid DOCTYPE) without needing to actually load it
        // as a launchd job -- a lightweight correctness check beyond string
        // matching, catching e.g. an unbalanced <dict>/</dict> from the
        // KeepAlive change above.
        let status = std::process::Command::new("plutil")
            .args(["-lint", &plist_path.to_string_lossy()])
            .status()
            .expect("plutil should be available on macOS");
        assert!(status.success(), "written plist must be valid XML/plist");

        let _ = std::fs::remove_dir_all(&home);
    }
}

/// These exercise the platform-independent halves of the Windows daemon-stdio
/// fix (path construction, append-not-truncate file semantics) that can be
/// verified on any development machine. They deliberately do NOT and cannot
/// cover `spawn_daemon_windows` itself — `Command::creation_flags`,
/// `DETACHED_PROCESS`/`CREATE_NEW_PROCESS_GROUP`, and `Stdio::from(File)`
/// handle-inheritance into a Windows child process have no macOS/Linux
/// equivalent to run against, so that code path is compile-checked only
/// (against the `x86_64-pc-windows-msvc` target) and awaits validation on a
/// real Windows machine.
#[cfg(test)]
mod windows_daemon_stdio_tests {
    use super::{daemon_log_paths, open_daemon_log};
    use std::io::{Read, Write};
    use std::path::Path;

    #[test]
    fn daemon_log_paths_match_macos_linux_filenames() {
        let (stdout, stderr) = daemon_log_paths(Path::new("/home/user/.nodespace/logs"));
        assert_eq!(
            stdout,
            Path::new("/home/user/.nodespace/logs/nodespaced.log"),
            "stdout log filename must match write_plist/write_systemd_service's nodespaced.log"
        );
        assert_eq!(
            stderr,
            Path::new("/home/user/.nodespace/logs/nodespaced-error.log"),
            "stderr log filename must match write_plist/write_systemd_service's nodespaced-error.log"
        );
    }

    #[test]
    fn daemon_log_paths_are_distinct_files_under_the_given_dir() {
        let dir = Path::new("/tmp/example-nodespace-logs");
        let (stdout, stderr) = daemon_log_paths(dir);
        assert_ne!(
            stdout, stderr,
            "stdout and stderr must not collide on one file"
        );
        assert!(stdout.starts_with(dir));
        assert!(stderr.starts_with(dir));
    }

    #[test]
    fn open_daemon_log_creates_missing_file() {
        let dir =
            std::env::temp_dir().join(format!("ns-daemon-log-test-create-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("nodespaced.log");
        assert!(!path.exists());

        let mut f = open_daemon_log(&path).expect("should create and open the log file");
        writeln!(f, "first line").unwrap();
        drop(f);

        assert!(path.exists());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "first line\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_daemon_log_appends_rather_than_truncates_on_reopen() {
        // Regression guard for the exact behavior spawn_daemon_windows depends
        // on: each daemon restart reopens the same log path, and prior runs'
        // output must survive rather than being wiped on every relaunch.
        let dir =
            std::env::temp_dir().join(format!("ns-daemon-log-test-append-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("nodespaced-error.log");

        {
            let mut f = open_daemon_log(&path).unwrap();
            writeln!(f, "run 1").unwrap();
        }
        {
            let mut f = open_daemon_log(&path).unwrap();
            writeln!(f, "run 2").unwrap();
        }

        let mut contents = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert_eq!(
            contents, "run 1\nrun 2\n",
            "reopening the log across restarts must append, not truncate"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Unit coverage for the edition-selection and image-name-formatting logic
/// `kill_running_daemon` (Windows) consumes: `daemon_binary_name_for` and
/// `daemon_image_name_for` are pure functions that take edition as a
/// parameter, so both editions' exact output strings are pinned here even
/// though a real build only ever bakes in one edition via `is_pro_build()`.
///
/// This does NOT exercise `kill_running_daemon` itself or the actual
/// `taskkill` invocation — that remains compile-check-only against the
/// Windows target (see the `windows_daemon_stdio_tests` module docs above
/// for why: no macOS/Linux equivalent exists to run it against).
#[cfg(test)]
mod windows_taskkill_image_name_tests {
    use super::{daemon_binary_name_for, daemon_image_name_for};

    #[test]
    fn community_edition_image_name_matches_community_binary() {
        assert_eq!(daemon_binary_name_for(false), "nodespaced");
        assert_eq!(daemon_image_name_for(false), "nodespaced.exe");
    }

    #[test]
    fn pro_edition_image_name_matches_pro_binary() {
        assert_eq!(daemon_binary_name_for(true), "nodespaced-pro");
        assert_eq!(daemon_image_name_for(true), "nodespaced-pro.exe");
    }
}

/// `externalBin` sidecars are bundled beside the app's own executable —
/// `Contents/MacOS/` on macOS, alongside the `.exe` on Windows, alongside the
/// main binary in the AppImage/deb layout on Linux — never under a Resources
/// tree. This pins that against the three bundle shapes directly, so a future
/// change back to a Resources-relative lookup (the bug this module exists to
/// fix — see `resolve_sidecar_path`'s doc comment) fails a test immediately
/// instead of shipping silently, the way it did the first time.
#[cfg(test)]
mod sidecar_path_tests {
    use super::{bundled_sidecar_name, sidecar_path_from_exe};
    use std::path::Path;

    #[test]
    fn sidecar_sits_beside_the_macos_app_executable() {
        let exe = Path::new("/Applications/NodeSpace.app/Contents/MacOS/nodespace-app");
        // Bare name, no target-triple suffix: confirmed against a real
        // `tauri build` output — the bundler renames the triple-suffixed
        // source-tree file to the bare name when staging it into the bundle
        // (`Contents/MacOS/nodespaced`, not `nodespaced-aarch64-apple-darwin`).
        let sidecar = sidecar_path_from_exe(exe, "nodespaced").unwrap();
        assert_eq!(
            sidecar,
            Path::new("/Applications/NodeSpace.app/Contents/MacOS/nodespaced"),
            "must resolve inside Contents/MacOS/, beside the app binary — NOT under \
             Contents/Resources/, which is where `resources` entries (skill, models) land, \
             not `externalBin` entries"
        );
    }

    #[test]
    fn sidecar_sits_beside_the_windows_exe() {
        // Forward slashes deliberately: `Path` only parses `\` as a separator
        // when compiled FOR Windows, so a `C:\...` literal parses as a single
        // opaque component (no parent) on this test's actual host. `/` is a
        // valid separator to `Path` on every target, Windows included, so this
        // exercises the same join logic portably rather than skipping the
        // platform on non-Windows hosts.
        let exe = Path::new("C:/Program Files/NodeSpace/nodespace-app.exe");
        let sidecar = sidecar_path_from_exe(exe, "nodespaced.exe").unwrap();
        assert_eq!(
            sidecar,
            Path::new("C:/Program Files/NodeSpace/nodespaced.exe")
        );
    }

    #[test]
    fn sidecar_sits_beside_the_linux_binary() {
        let exe = Path::new("/opt/nodespace/nodespace-app");
        let sidecar = sidecar_path_from_exe(exe, "nodespaced").unwrap();
        assert_eq!(sidecar, Path::new("/opt/nodespace/nodespaced"));
    }

    #[test]
    fn a_rootless_executable_path_has_no_sidecar_directory() {
        // Pathological input (current_exe() returning a bare filename with no
        // parent) — must degrade to None rather than panic.
        let exe = Path::new("nodespace-app");
        assert_eq!(
            sidecar_path_from_exe(exe, "nodespaced"),
            Some(Path::new("nodespaced").to_path_buf()),
            "an empty parent (bare relative filename) is itself a valid, if unusual, base \
             directory — Path::parent() returns Some(\"\") here, not None"
        );
    }

    #[test]
    fn bundled_name_adds_exe_only_on_windows() {
        let name = bundled_sidecar_name("nodespaced");
        if cfg!(windows) {
            assert_eq!(name, "nodespaced.exe");
        } else {
            assert_eq!(name, "nodespaced");
        }
    }
}
