use anyhow::Result;
use clap::Args;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Args, Debug)]
pub struct UninstallArgs {}

fn home_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| anyhow::anyhow!("$HOME is unset — cannot determine home directory"))?;
    Ok(PathBuf::from(home))
}

pub fn run(_args: UninstallArgs) -> Result<()> {
    stop_daemon();

    let home = home_dir()?;

    remove_bin_dir(&home);
    remove_sock(&home);
    remove_claude_skill(&home);

    println!("NodeSpace uninstalled. Your data at ~/.nodespace/database/ has been preserved.");

    Ok(())
}

#[cfg(target_os = "macos")]
fn stop_daemon() {
    let uid = unsafe { libc::getuid() };
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("gui/{uid}"), "app.nodespace.daemon"])
        .status();

    if let Ok(home) = std::env::var("HOME") {
        let plist = PathBuf::from(home)
            .join("Library")
            .join("LaunchAgents")
            .join("app.nodespace.daemon.plist");
        let _ = fs::remove_file(&plist);
    }
}

#[cfg(target_os = "linux")]
fn stop_daemon() {
    let _ = Command::new("systemctl")
        .args(["--user", "stop", "nodespace"])
        .status();
    let _ = Command::new("systemctl")
        .args(["--user", "disable", "nodespace"])
        .status();

    if let Ok(home) = std::env::var("HOME") {
        let service = PathBuf::from(home)
            .join(".config")
            .join("systemd")
            .join("user")
            .join("nodespace.service");
        let _ = fs::remove_file(&service);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn stop_daemon() {}

fn remove_bin_dir(home: &Path) {
    let bin_dir = home.join(".nodespace").join("bin");
    let _ = fs::remove_dir_all(&bin_dir);
}

/// Remove every build variant's socket, not just the community one. A single
/// `nodespace` binary uninstalls whichever app is installed, and it cannot tell
/// from its own build which variant left a socket behind — a stale Pro or dev
/// socket file would otherwise survive an uninstall.
fn remove_sock(home: &Path) {
    let dir = home.join(nodespace_proto::socket::STATE_DIR);
    for name in nodespace_proto::socket::DAEMON_SOCKET_NAMES {
        let _ = fs::remove_file(dir.join(name));
    }
}

fn remove_claude_skill(home: &Path) {
    let skill_dir = home.join(".claude").join("skills").join("nodespace");
    let _ = fs::remove_dir_all(&skill_dir);
}
