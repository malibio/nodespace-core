//! End-to-end test of `nodespace mcp`'s stdio transport.
//!
//! Spawns the real compiled `nodespace` binary (via `CARGO_BIN_EXE_nodespace`,
//! not the library surface used by `cli_integration.rs`) and speaks JSON-RPC
//! over its actual stdin/stdout — this is the one CLI surface where the OS
//! process boundary and stdio framing are the thing under test, not
//! incidental plumbing to route around.
//!
//! Unix-only: `nodespace` itself refuses to run on Windows (Unix socket
//! transport only — see `nodespace_cli::run`'s `#[cfg(windows)]` stub), so
//! there is nothing for this binary to do on that platform.
#![cfg(unix)]

use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;

/// Sends one JSON-RPC request line to the child's stdin and reads exactly
/// one response line from its stdout.
async fn send_and_read(
    stdin: &mut (impl tokio::io::AsyncWrite + Unpin),
    stdout: &mut (impl AsyncBufReadExt + Unpin),
    request: Value,
) -> Value {
    let line = serde_json::to_string(&request).expect("serialize request");
    stdin
        .write_all(line.as_bytes())
        .await
        .expect("write request");
    stdin.write_all(b"\n").await.expect("write newline");
    stdin.flush().await.expect("flush stdin");

    let mut response_line = String::new();
    timeout(
        Duration::from_secs(15),
        stdout.read_line(&mut response_line),
    )
    .await
    .expect("mcp server did not respond in time")
    .expect("read response line");
    assert!(
        !response_line.trim().is_empty(),
        "expected a JSON-RPC response line, got EOF"
    );
    serde_json::from_str(response_line.trim())
        .unwrap_or_else(|e| panic!("response line was not valid JSON ({e}): {response_line:?}"))
}

async fn wait_for_clean_exit(mut child: Child) {
    let status = timeout(Duration::from_secs(15), child.wait())
        .await
        .expect("mcp server did not exit after stdin closed")
        .expect("wait on child");
    assert!(
        status.success(),
        "mcp server must exit 0 once stdin closes cleanly, got {status}"
    );
}

#[tokio::test]
async fn mcp_speaks_stdio_jsonrpc_and_exposes_exactly_one_tool() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_nodespace"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn nodespace mcp");

    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    let init = send_and_read(
        &mut stdin,
        &mut stdout,
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
    )
    .await;
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["serverInfo"]["name"], "nodespace");
    assert_eq!(init["result"]["capabilities"]["tools"], json!({}));

    // A notification (no "id") must draw no response line at all — send it,
    // then immediately follow with a real request and confirm the very next
    // line answers that request, not a stray reply to the notification.
    let notify =
        serde_json::to_string(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .unwrap();
    stdin.write_all(notify.as_bytes()).await.unwrap();
    stdin.write_all(b"\n").await.unwrap();
    stdin.flush().await.unwrap();

    let list = send_and_read(
        &mut stdin,
        &mut stdout,
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
    )
    .await;
    assert_eq!(list["id"], 2);
    let tools = list["result"]["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 1, "exactly one tool must be exposed");
    assert_eq!(tools[0]["name"], "nodespace");
    assert_eq!(
        tools[0]["inputSchema"]["properties"]["args"]["type"],
        "string"
    );
    assert_eq!(tools[0]["inputSchema"]["required"][0], "args");

    drop(stdin);
    wait_for_clean_exit(child).await;
}

#[tokio::test]
async fn mcp_tool_call_reports_daemon_unreachable_actionably_not_as_a_raw_connection_error() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    // A path inside a fresh, empty tempdir guarantees nothing is listening —
    // the daemon-unreachable path triggers deterministically, regardless of
    // whether some other daemon happens to be running on the host executing
    // this test.
    let sock = tempdir.path().join("no-such-daemon.sock");

    let mut child = Command::new(env!("CARGO_BIN_EXE_nodespace"))
        .arg("--socket")
        .arg(&sock)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn nodespace mcp");

    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    let call = send_and_read(
        &mut stdin,
        &mut stdout,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "nodespace", "arguments": {"args": "node get some-id"}},
        }),
    )
    .await;

    assert_eq!(
        call["result"]["isError"], true,
        "an unreachable daemon must surface as a tool error, not a JSON-RPC error or success: {call}"
    );
    let text = call["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    assert!(
        text.contains("Could not connect to nodespaced"),
        "expected the CLI's own friendly connection error, got: {text}"
    );
    assert!(
        text.contains("Is the daemon running?") && text.contains("nodespaced"),
        "expected an actionable hint naming the `nodespaced` start step, got: {text}"
    );
    assert!(
        !text.to_lowercase().contains("os error")
            && !text.to_lowercase().contains("connection refused")
            && !text.to_lowercase().contains("no such file or directory"),
        "must not leak the raw OS-level connection error text: {text}"
    );

    drop(stdin);
    wait_for_clean_exit(child).await;
}

#[tokio::test]
async fn mcp_tool_call_rejects_an_unterminated_quote_without_dispatching_anything() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_nodespace"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn nodespace mcp");

    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    let call = send_and_read(
        &mut stdin,
        &mut stdout,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "nodespace", "arguments": {"args": "search \"unterminated"}},
        }),
    )
    .await;

    assert_eq!(call["result"]["isError"], true);
    let text = call["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    assert!(text.contains("could not parse"), "got: {text}");

    drop(stdin);
    wait_for_clean_exit(child).await;
}
