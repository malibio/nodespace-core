//! `nodespace mcp` — a stdio MCP server exposing exactly one passthrough
//! tool, for bash-less MCP surfaces (e.g. Claude Desktop's Chat tab, which
//! has no shell, file I/O, or code execution and can only reach the local
//! machine through a stdio MCP connector).
//!
//! Claude Desktop spawns this as a child process and speaks JSON-RPC 2.0
//! over its stdin/stdout — the MCP stdio transport, one message per line,
//! no embedded newlines. The daemon is not involved in this transport: this
//! process is just one more `nodespace` invocation against `daemon.sock`,
//! same as every other subcommand, using the same `--socket`/`--database`
//! resolution.
//!
//! The server exposes exactly one tool, `nodespace(args: string)`, where
//! `args` is the argument list that would follow `nodespace` on a shell
//! line (e.g. `args: "search \"auth tokens\""`). Each call:
//!
//! 1. Splits `args` the way a POSIX shell would ([`shell_words::split`]), so
//!    quoting behaves the same as every other CLI invocation.
//! 2. Prepends the resolved `--socket`/`--database` this `mcp` process was
//!    started with, and appends `--json` (unless already present), so the
//!    tool result is always structured.
//! 3. Shells out to this same compiled `nodespace` binary with that argv.
//!
//! Step 3 is deliberately a subprocess, not an in-process call to the
//! existing command handlers: those handlers `println!` their output, and
//! that would land on this process's own stdout — the same stream carrying
//! the JSON-RPC transport — interleaving CLI output into the middle of
//! protocol messages. Spawning the compiled binary keeps the exact same
//! code path (clap parsing, dispatch, the one gRPC client) while capturing
//! its output cleanly, so this stays a transport adapter rather than a
//! second client or a reimplementation of the subcommands.
//!
//! No consent gate is added here: the CLI has none today (`node delete`
//! deletes immediately, no confirmation prompt), and every call dispatches
//! through the same unmodified handlers. The resolve-then-confirm discipline
//! the skill documents for destructive verbs is enforced by the calling
//! model reading that guidance before it decides to invoke the tool — the
//! same as it is for every other shell-capable surface — not by anything in
//! this transport.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command as ChildCommand;

/// MCP protocol revision this server speaks. Always advertised as-is,
/// regardless of what a client requests in `initialize` — the surface is
/// fixed (one tool, no resources/prompts/sampling), so there is nothing to
/// negotiate.
const PROTOCOL_VERSION: &str = "2024-11-05";

const TOOL_NAME: &str = "nodespace";

const TOOL_DESCRIPTION: &str = "Run a `nodespace` CLI command. `args` is the exact argument list \
    that would follow `nodespace` on a shell line (e.g. `search \"auth tokens\"`, `node get <id>`). \
    Returns the command's --json output. See the NodeSpace skill for available commands.";

/// A single dispatched invocation may run for this long before being killed
/// and reported as a timeout. Generous enough for a slow `import`/`query`,
/// but finite: `session launch`/`session attach` are designed to stream and
/// block indefinitely (interactive PTY attachment), which would otherwise
/// wedge this server's single-threaded request loop forever.
const DISPATCH_TIMEOUT: Duration = Duration::from_secs(120);

/// Hosts the stdio MCP server: reads JSON-RPC requests from stdin, one per
/// line, dispatches them, and writes JSON-RPC responses to stdout, one per
/// line, until stdin closes (the client disconnected).
///
/// `sock` is the already-resolved daemon socket path — honoring `--socket` /
/// `NODESPACED_SOCKET` / auto-discovery, exactly as every other subcommand
/// resolves it via [`crate::resolve_socket_path`] — and `database` is the
/// raw `--database` selection, if any. Both are forwarded to every
/// dispatched child invocation, so the whole MCP session targets one
/// consistent daemon/database: the one `nodespace mcp` itself was started
/// with.
pub async fn run(sock: PathBuf, database: Option<String>) -> Result<()> {
    let exe = std::env::current_exe().context("resolve the nodespace executable's own path")?;

    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = lines.next_line().await.context("read stdin")? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(response) = handle_message(line, &exe, &sock, database.as_deref()).await {
            let text = serde_json::to_string(&response).context("serialize JSON-RPC response")?;
            stdout.write_all(text.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }

    Ok(())
}

/// Handles one JSON-RPC message (a single stdin line). Returns `None` for
/// notifications (no `id`) and whenever a notification-shaped message would
/// otherwise trigger a response — JSON-RPC forbids responding to those.
/// Never panics: every failure becomes either a JSON-RPC error object or,
/// for a recognized tool call that fails, a tool result with
/// `isError: true`.
async fn handle_message(
    line: &str,
    exe: &Path,
    sock: &Path,
    database: Option<&str>,
) -> Option<Value> {
    let request: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(error_response(
                Value::Null,
                -32700,
                &format!("Parse error: {e}"),
            ));
        }
    };

    let id = request.get("id").cloned();
    let is_notification = id.is_none();

    let method = match request.get("method").and_then(Value::as_str) {
        Some(m) => m,
        None => {
            return if is_notification {
                None
            } else {
                Some(error_response(
                    id.unwrap_or(Value::Null),
                    -32600,
                    "Invalid Request: missing \"method\"",
                ))
            };
        }
    };

    match method {
        "initialize" => id.map(|id| success_response(id, initialize_result())),
        "notifications/initialized" | "notifications/cancelled" => None,
        "ping" => id.map(|id| success_response(id, json!({}))),
        "tools/list" => id.map(|id| success_response(id, tools_list_result())),
        "tools/call" => {
            let id = id?;
            let params = request.get("params").cloned().unwrap_or(Value::Null);
            Some(match extract_tool_call(&params) {
                Ok(args_str) => {
                    let result = call_tool(exe, &args_str, sock, database).await;
                    success_response(id, result)
                }
                Err(msg) => error_response(id, -32602, &msg),
            })
        }
        other => {
            if is_notification {
                None
            } else {
                Some(error_response(
                    id.unwrap_or(Value::Null),
                    -32601,
                    &format!("Method not found: {other}"),
                ))
            }
        }
    }
}

fn success_response(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {"tools": {}},
        "serverInfo": {"name": "nodespace", "version": env!("CARGO_PKG_VERSION")},
    })
}

fn tools_list_result() -> Value {
    json!({
        "tools": [{
            "name": TOOL_NAME,
            "description": TOOL_DESCRIPTION,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "args": {
                        "type": "string",
                        "description": "The argument list that would follow `nodespace` on a shell line.",
                    }
                },
                "required": ["args"],
            },
        }],
    })
}

/// Validates a `tools/call` request's `params` and extracts the `args`
/// string, without running anything yet. The returned `Err` is the message
/// for a JSON-RPC `Invalid params` error: an unknown tool name or a
/// missing/non-string `args` is a malformed call, not a tool execution
/// failure, so it is reported at the protocol level rather than through
/// [`call_tool`]'s `isError` result.
fn extract_tool_call(params: &Value) -> Result<String, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call params missing string \"name\"".to_string())?;
    if name != TOOL_NAME {
        return Err(format!("Unknown tool: {name}"));
    }
    params
        .get("arguments")
        .and_then(|a| a.get("args"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("the \"{TOOL_NAME}\" tool requires a string \"args\" argument"))
}

/// Builds the child argv for one dispatched call: the resolved
/// `--socket`/`--database` this `mcp` process was started with, then the
/// shell-split `args`, then `--json` (unless already present) so the tool
/// result is always structured.
///
/// Pure and synchronous so quoting/splitting behavior is unit-testable
/// without spawning a process. If the caller's `args` explicitly repeats
/// `--socket`/`--database`, its value wins over the prefix (clap keeps the
/// last occurrence of a non-repeatable flag), so an explicit override in the
/// tool call is still honored.
fn build_child_args(
    args_str: &str,
    sock: &Path,
    database: Option<&str>,
) -> Result<Vec<String>, String> {
    let tail = shell_words::split(args_str)
        .map_err(|e| format!("could not parse \"args\" as shell-style arguments: {e}"))?;

    let mut argv = vec!["--socket".to_string(), sock.display().to_string()];
    if let Some(db) = database {
        argv.push("--database".to_string());
        argv.push(db.to_string());
    }
    argv.extend(tail);

    if !argv.iter().any(|a| a == "--json") {
        argv.push("--json".to_string());
    }

    Ok(argv)
}

async fn call_tool(exe: &Path, args_str: &str, sock: &Path, database: Option<&str>) -> Value {
    match build_child_args(args_str, sock, database) {
        Ok(argv) => run_child(exe, &argv, DISPATCH_TIMEOUT).await,
        Err(msg) => tool_error(msg),
    }
}

/// Spawns `exe` with `argv`, waits up to `timeout`, and turns the outcome
/// into an MCP tool-call result. Never returns an `Err`: every failure mode
/// — spawn failure, non-zero exit, timeout — becomes a normal
/// `isError: true` result, so a failed dispatch reads as an actionable
/// answer rather than crashing the server or the caller's turn.
///
/// `kill_on_drop(true)` ensures a child that outlives `timeout` (e.g. a
/// `session launch` that never exits on its own) is actually killed when the
/// timed-out future is dropped, rather than left running as an orphan.
///
/// `stdin(Stdio::null())` is load-bearing, not cosmetic: unlike
/// `std::process::Command::output()`, which nulls stdin by default,
/// `tokio::process::Command::output()` leaves stdin untouched — i.e.
/// inherited from this `mcp` process, which is this server's real JSON-RPC
/// transport. Without this call, dispatching a subcommand that itself reads
/// stdin (`session attach`/`session launch`, which stream a PTY) would race
/// this function's own stdin loop for bytes off the same pipe, silently
/// diverting a live client's subsequent JSON-RPC messages into an unrelated
/// terminal session instead of the request loop. Do not remove it.
async fn run_child(exe: &Path, argv: &[String], timeout: Duration) -> Value {
    let mut command = ChildCommand::new(exe);
    command
        .args(argv)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null());

    match tokio::time::timeout(timeout, command.output()).await {
        Ok(Ok(output)) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            json!({"content": [{"type": "text", "text": text}], "isError": false})
        }
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let text = if !stderr.is_empty() {
                clean_cli_error(&stderr)
            } else {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if stdout.is_empty() {
                    format!("nodespace exited with {} and no output", output.status)
                } else {
                    stdout
                }
            };
            tool_error(text)
        }
        Ok(Err(e)) => tool_error(format!(
            "Failed to run the nodespace CLI at {}: {e}. Reinstall NodeSpace or confirm its CLI \
             binary is present and executable.",
            exe.display()
        )),
        Err(_) => tool_error(format!(
            "`nodespace {}` did not complete within {}s. Commands that stream or block (e.g. \
             `session launch`/`session attach`) are not supported through this passthrough — use \
             a shell-capable surface (Claude Code, the Claude Desktop Code tab) for interactive \
             sessions.",
            argv.join(" "),
            timeout.as_secs(),
        )),
    }
}

fn tool_error(text: String) -> Value {
    json!({"content": [{"type": "text", "text": text}], "isError": true})
}

/// Cleans up a dispatched invocation's raw stderr into a tool-friendly
/// message: strips the `Error: ` prefix `std::process::Termination` adds for
/// a failing `anyhow::Result`, and drops the `Caused by:` chain of internal
/// (often OS-level) detail that follows it — the headline anyhow message is
/// already the actionable part (e.g. `connect_error_context`'s "Is the
/// daemon running?"); the chain exists for human debugging, not for a model
/// deciding what to do next. Anything not shaped like that (e.g. clap's own
/// usage/parse errors, which start with a lowercase `error:`) is left as-is.
fn clean_cli_error(stderr: &str) -> String {
    let text = stderr.strip_prefix("Error: ").unwrap_or(stderr);
    let text = text.split("\n\nCaused by:").next().unwrap_or(text);
    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_child_args_splits_quoted_args_and_appends_json() {
        let argv = build_child_args(
            r#"search "auth tokens""#,
            Path::new("/tmp/daemon.sock"),
            None,
        )
        .expect("valid shell syntax");
        assert_eq!(
            argv,
            vec![
                "--socket",
                "/tmp/daemon.sock",
                "search",
                "auth tokens",
                "--json"
            ]
        );
    }

    #[test]
    fn build_child_args_forwards_database_selection() {
        let argv = build_child_args(
            "node get abc-123",
            Path::new("/tmp/daemon.sock"),
            Some("work"),
        )
        .expect("valid shell syntax");
        assert_eq!(
            argv,
            vec![
                "--socket",
                "/tmp/daemon.sock",
                "--database",
                "work",
                "node",
                "get",
                "abc-123",
                "--json",
            ]
        );
    }

    #[test]
    fn build_child_args_does_not_duplicate_an_explicit_json_flag() {
        let argv = build_child_args("node get abc-123 --json", Path::new("/s"), None)
            .expect("valid shell syntax");
        assert_eq!(argv.iter().filter(|a| *a == "--json").count(), 1);
    }

    #[test]
    fn build_child_args_rejects_unterminated_quotes() {
        let err = build_child_args(r#"search "unterminated"#, Path::new("/s"), None)
            .expect_err("unterminated quote must not panic or silently truncate");
        assert!(err.contains("could not parse"));
    }

    #[test]
    fn build_child_args_handles_empty_args_string() {
        // A bare tool call with an empty `args` string is well-formed shell
        // syntax (zero tokens) — the dispatched binary is responsible for
        // rejecting "no subcommand given", the same as running `nodespace`
        // alone.
        let argv = build_child_args("", Path::new("/s"), None).expect("empty is valid");
        assert_eq!(argv, vec!["--socket", "/s", "--json"]);
    }

    #[test]
    fn build_child_args_lets_an_explicit_socket_override_win() {
        // shell_words splits "--socket" and its value as ordinary tokens; they
        // land after our own --socket/value pair, and clap keeps the last
        // occurrence of a non-repeatable flag, so the caller's explicit
        // override is what the dispatched binary actually uses.
        let argv = build_child_args(
            "--socket /explicit/other.sock node get abc",
            Path::new("/default.sock"),
            None,
        )
        .expect("valid shell syntax");
        assert_eq!(
            argv,
            vec![
                "--socket",
                "/default.sock",
                "--socket",
                "/explicit/other.sock",
                "node",
                "get",
                "abc",
                "--json",
            ]
        );
    }

    #[test]
    fn tools_list_exposes_exactly_one_tool_with_a_string_args_parameter() {
        let result = tools_list_result();
        let tools = result["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 1, "exactly one tool must be exposed");
        assert_eq!(tools[0]["name"], TOOL_NAME);
        assert_eq!(tools[0]["inputSchema"]["type"], "object");
        assert_eq!(
            tools[0]["inputSchema"]["properties"]["args"]["type"],
            "string"
        );
        assert_eq!(tools[0]["inputSchema"]["required"][0], "args");
    }

    #[test]
    fn extract_tool_call_rejects_unknown_tool_name() {
        let params = json!({"name": "not-nodespace", "arguments": {"args": "search x"}});
        let err = extract_tool_call(&params).expect_err("unknown tool must be rejected");
        assert!(err.contains("Unknown tool"));
    }

    #[test]
    fn extract_tool_call_rejects_missing_args_string() {
        let params = json!({"name": TOOL_NAME, "arguments": {}});
        let err = extract_tool_call(&params).expect_err("missing args must be rejected");
        assert!(err.contains("args"));
    }

    #[test]
    fn extract_tool_call_rejects_non_string_args() {
        let params = json!({"name": TOOL_NAME, "arguments": {"args": 5}});
        assert!(extract_tool_call(&params).is_err());
    }

    #[test]
    fn extract_tool_call_accepts_well_formed_params() {
        let params = json!({"name": TOOL_NAME, "arguments": {"args": "search x"}});
        assert_eq!(
            extract_tool_call(&params).expect("valid params"),
            "search x"
        );
    }

    #[test]
    fn clean_cli_error_strips_the_termination_prefix_and_caused_by_chain() {
        let raw = "Error: Could not connect to nodespaced at /tmp/x.sock.\nIs the daemon running? Start it with `nodespaced` in another terminal.\n\nCaused by:\n    0: transport error\n    1: No such file or directory (os error 2)";
        let cleaned = clean_cli_error(raw);
        assert_eq!(
            cleaned,
            "Could not connect to nodespaced at /tmp/x.sock.\nIs the daemon running? Start it with `nodespaced` in another terminal."
        );
        assert!(!cleaned.to_lowercase().contains("os error"));
    }

    #[test]
    fn clean_cli_error_leaves_non_anyhow_shaped_errors_untouched() {
        // clap's own usage errors start with a lowercase `error:`, not the
        // `Error: ` prefix std's Termination impl adds — must pass through.
        let raw = "error: unrecognized subcommand 'bogus'\n\nUsage: nodespace <COMMAND>";
        assert_eq!(clean_cli_error(raw), raw);
    }

    #[test]
    fn clean_cli_error_handles_a_bare_single_line_message() {
        assert_eq!(clean_cli_error("Error: invalid status"), "invalid status");
    }

    #[tokio::test]
    async fn handle_message_initialize_returns_protocol_version_and_server_info() {
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            Path::new("/bin/true"),
            Path::new("/tmp/daemon.sock"),
            None,
        )
        .await
        .expect("initialize is a request, not a notification");
        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(response["result"]["serverInfo"]["name"], "nodespace");
    }

    #[tokio::test]
    async fn handle_message_notification_gets_no_response() {
        let response = handle_message(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            Path::new("/bin/true"),
            Path::new("/tmp/daemon.sock"),
            None,
        )
        .await;
        assert!(
            response.is_none(),
            "JSON-RPC forbids responding to notifications"
        );
    }

    #[tokio::test]
    async fn handle_message_unknown_method_is_method_not_found() {
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":7,"method":"resources/list","params":{}}"#,
            Path::new("/bin/true"),
            Path::new("/tmp/daemon.sock"),
            None,
        )
        .await
        .expect("a request always gets a response");
        assert_eq!(response["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn handle_message_malformed_json_is_parse_error_with_null_id() {
        let response = handle_message(
            "not json at all",
            Path::new("/bin/true"),
            Path::new("/tmp/daemon.sock"),
            None,
        )
        .await
        .expect("a parse failure must still be reported");
        assert_eq!(response["error"]["code"], -32700);
        assert_eq!(response["id"], Value::Null);
    }

    #[tokio::test]
    async fn handle_message_tools_call_with_unknown_tool_is_invalid_params() {
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"bogus","arguments":{}}}"#,
            Path::new("/bin/true"),
            Path::new("/tmp/daemon.sock"),
            None,
        )
        .await
        .expect("a request always gets a response");
        assert_eq!(response["error"]["code"], -32602);
    }

    #[tokio::test]
    async fn handle_message_tools_list_advertises_the_one_tool() {
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}"#,
            Path::new("/bin/true"),
            Path::new("/tmp/daemon.sock"),
            None,
        )
        .await
        .expect("a request always gets a response");
        assert_eq!(response["result"]["tools"][0]["name"], TOOL_NAME);
    }
}

#[cfg(all(test, unix))]
mod process_tests {
    use super::*;

    #[tokio::test]
    async fn run_child_captures_stdout_on_success() {
        let result = run_child(
            Path::new("/bin/echo"),
            &["hello".to_string()],
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(result["isError"], false);
        assert_eq!(result["content"][0]["text"], "hello");
    }

    #[tokio::test]
    async fn run_child_surfaces_stderr_on_nonzero_exit() {
        let result = run_child(
            Path::new("/bin/sh"),
            &["-c".to_string(), "echo boom >&2; exit 1".to_string()],
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(result["isError"], true);
        assert_eq!(result["content"][0]["text"], "boom");
    }

    #[tokio::test]
    async fn run_child_times_out_and_kills_a_blocking_child() {
        let result = run_child(
            Path::new("/bin/sleep"),
            &["5".to_string()],
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(result["isError"], true);
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("did not complete within"));
        assert!(text.contains("session launch"));
    }

    #[tokio::test]
    async fn run_child_reports_spawn_failure_actionably() {
        let result = run_child(
            Path::new("/no/such/binary-at-all"),
            &["node".to_string()],
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(result["isError"], true);
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Failed to run the nodespace CLI"));
    }

    /// `cat` with no arguments reads its stdin until EOF and echoes it back.
    /// A null stdin (what `run_child` must give the dispatched process — see
    /// its doc comment for why) hits EOF immediately, so this returns fast
    /// with empty output; a real regression (dropping `stdin(Stdio::null())`
    /// so the child inherits this test process's own stdin) is only
    /// observable here when that inherited stream is itself open and
    /// unclosed, which this harness does not control — the primary
    /// protection against that regression is `run_child`'s explanatory doc
    /// comment plus the source-level fact that `tokio::process::Command`'s
    /// `output()`, unlike `std::process::Command`'s, does not null stdin on
    /// its own. This test still exercises the exact code path and pins the
    /// intended fast/empty behavior.
    #[tokio::test]
    async fn run_child_gives_the_dispatched_process_a_null_stdin() {
        let result = run_child(Path::new("/bin/cat"), &[], Duration::from_secs(5)).await;
        assert_eq!(result["isError"], false);
        assert_eq!(result["content"][0]["text"], "");
    }
}
