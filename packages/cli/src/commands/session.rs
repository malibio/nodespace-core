//! `nodespace session ...` subcommands — PTY agent session management via gRPC.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use crossterm::terminal;
use nodespace_daemon::nodespace::{
    LaunchSessionRequest, ListSessionsRequest, StreamOutputRequest, TerminateSessionRequest,
    WriteInputRequest,
};
use nodespace_daemon::AgentSessionServiceClient;
use tokio::io::AsyncReadExt;
use tonic::transport::Channel;
use tonic::Request;

use crate::terminal::{write_stdout, RawMode};

#[derive(Subcommand, Debug)]
pub enum SessionAction {
    /// Launch a new agent session and stream its output to stdout.
    Launch(LaunchArgs),
    /// Attach to an existing session's output stream.
    Attach(AttachArgs),
    /// List active agent sessions.
    #[command(name = "list")]
    List(ListArgs),
    /// Terminate a running session.
    Kill(KillArgs),
}

#[derive(Args, Debug)]
pub struct LaunchArgs {
    /// Agent to launch: claude-code, codex, gemini, pi, opencode
    pub agent: String,

    /// Initial prompt passed to the agent at launch time.
    #[arg(long)]
    pub prompt: Option<String>,

    /// Terminal width in columns (defaults to current terminal width).
    #[arg(long)]
    pub cols: Option<u32>,

    /// Terminal height in rows (defaults to current terminal height).
    #[arg(long)]
    pub rows: Option<u32>,
}

#[derive(Args, Debug)]
pub struct AttachArgs {
    /// Session ID to attach to.
    pub session_id: String,
}

#[derive(Args, Debug)]
pub struct ListArgs {}

#[derive(Args, Debug)]
pub struct KillArgs {
    /// Session ID to terminate.
    pub session_id: String,
}

pub async fn run(
    client: &mut AgentSessionServiceClient<Channel>,
    action: SessionAction,
    _json: bool,
) -> Result<()> {
    match action {
        SessionAction::Launch(args) => launch(client, args).await,
        SessionAction::Attach(args) => attach(client, args).await,
        SessionAction::List(_) => list(client).await,
        SessionAction::Kill(args) => kill(client, args).await,
    }
}

async fn launch(client: &mut AgentSessionServiceClient<Channel>, args: LaunchArgs) -> Result<()> {
    let (cols, rows) = detect_terminal_size(args.cols, args.rows);

    let resp = client
        .launch_session(Request::new(LaunchSessionRequest {
            agent_type: args.agent,
            prompt: args.prompt,
            cols,
            rows,
        }))
        .await
        .context("LaunchSession RPC failed")?
        .into_inner();

    // Print session ID to stderr so scripts can capture it separately from output.
    eprintln!("session: {}", resp.session_id);

    stream_bridge(client, resp.session_id).await
}

async fn attach(client: &mut AgentSessionServiceClient<Channel>, args: AttachArgs) -> Result<()> {
    stream_bridge(client, args.session_id).await
}

async fn list(client: &mut AgentSessionServiceClient<Channel>) -> Result<()> {
    let resp = client
        .list_sessions(Request::new(ListSessionsRequest {}))
        .await
        .context("ListSessions RPC failed")?
        .into_inner();

    if resp.sessions.is_empty() {
        println!("No active sessions.");
        return Ok(());
    }

    println!("{:<38}  {:<12}  STARTED", "SESSION ID", "AGENT");
    for s in &resp.sessions {
        let started = format_unix_time(s.started_at);
        println!("{:<38}  {:<12}  {}", s.session_id, s.agent_type, started);
    }
    Ok(())
}

async fn kill(client: &mut AgentSessionServiceClient<Channel>, args: KillArgs) -> Result<()> {
    let resp = client
        .terminate_session(Request::new(TerminateSessionRequest {
            session_id: args.session_id.clone(),
        }))
        .await
        .context("TerminateSession RPC failed")?
        .into_inner();

    if resp.was_running {
        println!("Session {} terminated.", args.session_id);
    } else {
        println!(
            "Session {} was not running (already cleaned up).",
            args.session_id
        );
    }
    Ok(())
}

/// Concurrently streams output from the session to stdout and forwards raw
/// stdin to the session's PTY input. Runs until the output stream ends,
/// the user presses Ctrl+D, or the user presses Ctrl+C (detach, no kill).
async fn stream_bridge(
    client: &mut AgentSessionServiceClient<Channel>,
    session_id: String,
) -> Result<()> {
    let _raw = RawMode::enter()?;

    // Open the output stream.
    let mut output_stream = client
        .stream_output(Request::new(StreamOutputRequest {
            session_id: session_id.clone(),
        }))
        .await
        .context("StreamOutput RPC failed")?
        .into_inner();

    // Clone the channel for the input writer task.
    let mut input_client = client.clone();
    let input_session = session_id.clone();

    // Spawn stdin → WriteInput task.
    let input_task = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let mut buf = [0u8; 256];
        loop {
            let n = match stdin.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let data = buf[..n].to_vec();

            // Ctrl+D (0x04) — detach without killing.
            // Ctrl+C (0x03) — also detach without killing.
            let should_detach = data.iter().any(|&b| b == 0x04 || b == 0x03);

            let _ = input_client
                .write_input(Request::new(WriteInputRequest {
                    session_id: input_session.clone(),
                    data,
                }))
                .await;

            if should_detach {
                break;
            }
        }
    });

    // Drive the output stream to stdout.
    loop {
        match output_stream.message().await {
            Ok(Some(chunk)) => {
                write_stdout(&chunk.data)?;
            }
            Ok(None) => break, // stream ended cleanly
            Err(e) => {
                // Abort input task then surface the error.
                input_task.abort();
                return Err(e).context("StreamOutput error");
            }
        }
    }

    input_task.abort();
    Ok(())
}

fn detect_terminal_size(cols_override: Option<u32>, rows_override: Option<u32>) -> (u32, u32) {
    let (detected_cols, detected_rows) = terminal::size().unwrap_or((80, 24));
    let cols = cols_override.unwrap_or(detected_cols as u32);
    let rows = rows_override.unwrap_or(detected_rows as u32);
    (cols, rows)
}

fn format_unix_time(unix_secs: i64) -> String {
    // Simple HH:MM:SS display — good enough for a session list.
    let secs = unix_secs % 86400;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}
