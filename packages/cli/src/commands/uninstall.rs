use anyhow::{Context, Result};
use clap::Args;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

#[derive(Args, Debug)]
pub struct UninstallArgs {}

fn home_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("$HOME is unset — cannot determine home directory")?;
    Ok(PathBuf::from(home))
}

fn read_yn_prompt() -> bool {
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    let trimmed = input.trim();
    trimmed.is_empty() || trimmed.eq_ignore_ascii_case("y")
}

pub async fn run(_args: UninstallArgs) -> Result<()> {
    stop_daemon();

    let home = home_dir()?;

    remove_bin_dir(&home);
    remove_sock(&home);
    remove_claude_skill(&home);
    prompt_remove_mcp(&home)?;

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

fn remove_bin_dir(home: &PathBuf) {
    let bin_dir = home.join(".nodespace").join("bin");
    let _ = fs::remove_dir_all(&bin_dir);
}

fn remove_sock(home: &PathBuf) {
    let sock = home.join(".nodespace").join("daemon.sock");
    let _ = fs::remove_file(&sock);
}

fn remove_claude_skill(home: &PathBuf) {
    let skill_dir = home.join(".claude").join("skills").join("nodespace");
    let _ = fs::remove_dir_all(&skill_dir);
}

fn prompt_remove_mcp(home: &PathBuf) -> Result<()> {
    print!("Remove MCP entry from Claude Desktop config? [Y/n] ");
    io::stdout().flush().context("flush stdout")?;

    if !read_yn_prompt() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    let config_path = home
        .join("Library")
        .join("Application Support")
        .join("Claude")
        .join("claude_desktop_config.json");

    #[cfg(target_os = "linux")]
    let config_path = home
        .join(".config")
        .join("Claude")
        .join("claude_desktop_config.json");

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Ok(());

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        if !config_path.exists() {
            return Ok(());
        }

        let contents = fs::read_to_string(&config_path)
            .with_context(|| format!("read {}", config_path.display()))?;

        let mut value: serde_json::Value = serde_json::from_str(&contents)
            .with_context(|| format!("parse {}", config_path.display()))?;

        if let Some(mcp_servers) = value.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
            mcp_servers.remove("nodespace");
        }

        let updated =
            serde_json::to_string_pretty(&value).context("serialize claude desktop config")?;
        fs::write(&config_path, updated)
            .with_context(|| format!("write {}", config_path.display()))?;
    }

    Ok(())
}
