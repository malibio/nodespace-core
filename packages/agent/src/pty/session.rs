//! One running agent process attached to a pseudo-terminal.
//!
//! A [`PtySession`] owns:
//!
//! * a per-session [`tempfile::TempDir`] (the agent's working directory),
//! * the master end of a PTY pair (`portable-pty`),
//! * a writer into the PTY's stdin,
//! * a [`portable_pty::ChildKiller`] handle for the spawned process,
//! * a background blocking task that reads the PTY's output and fans it out
//!   through `broadcast::Sender<OutputChunk>`,
//! * a watcher task (which owns the actual `Child`) that broadcasts the
//!   process exit status through `broadcast::Sender<ExitStatus>`.
//!
//! The temp dir is dropped when the session value is dropped, so any cleanup
//! path — explicit [`PtySession::terminate`] or natural process exit followed
//! by the manager removing the entry — also removes the working directory.
//!
//! ## Concurrency
//!
//! The exit-watcher task owns the [`portable_pty::Child`] exclusively so it
//! can block in `wait()` without holding any lock the session might need.
//! [`PtySession::terminate`] kills the child through a separately-cloned
//! [`portable_pty::ChildKiller`], avoiding the obvious deadlock of a kill
//! call waiting on a mutex the wait-loop already holds.

use std::io::{Read, Write};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::acp::context_assembly::GraphContextAssembler;
use crate::acp::registry::SystemAgentRegistry;
use crate::agent_types::{AgentType, ContextError};

/// Number of buffered chunks per output subscriber. Slow consumers that fall
/// behind by more than this many chunks will see `RecvError::Lagged`; that is
/// preferable to unbounded memory growth when an agent is streaming faster
/// than a UI can render.
const OUTPUT_CHANNEL_CAPACITY: usize = 256;

/// Read buffer for the PTY output thread. PTY data is byte-by-byte in the
/// worst case (one keystroke echo), but typical chunks are tens to a few
/// hundred bytes, so 4 KiB amortises syscalls without delaying small writes.
const READ_BUFFER_BYTES: usize = 4096;

/// Default PTY size used at launch. Callers reshape via [`PtySession::resize`]
/// as soon as they know their terminal geometry.
const DEFAULT_PTY_ROWS: u16 = 24;
const DEFAULT_PTY_COLS: u16 = 80;

/// One chunk of raw bytes read from the PTY master.
///
/// The PTY merges stdout and stderr into a single byte stream — there is no
/// way to distinguish them at this layer. Consumers (UI, capture pipeline)
/// are expected to render the stream as a terminal would.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputChunk {
    /// Raw bytes from the PTY. May contain partial UTF-8 sequences and
    /// terminal escape codes.
    pub data: Vec<u8>,
    /// Wall-clock time the chunk was read off the master FD.
    pub timestamp: DateTime<Utc>,
}

/// Exit status observed when the agent process terminates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ExitStatus {
    /// Raw exit code reported by `portable_pty::ExitStatus`. Zero on clean
    /// exit, non-zero on failure or signal termination.
    pub code: u32,
    /// `true` if the child exited cleanly (code == 0, no signal).
    pub success: bool,
}

/// One agent process running inside a PTY.
pub struct PtySession {
    /// Stable identifier for this session.
    pub id: Uuid,
    /// Which external agent (Claude Code, Codex, ...) is running.
    pub agent_type: AgentType,
    /// When [`PtySession::launch`] returned successfully.
    pub started_at: DateTime<Utc>,

    /// Master end of the PTY. Held under a mutex so [`resize`](Self::resize)
    /// can be called from any task without racing the output reader.
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    /// Writer to the PTY's stdin. Acquired once at launch via
    /// `master.take_writer()` so [`write_input`](Self::write_input) does not
    /// have to fight the reader for the master.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Kill handle for the child process. The actual [`portable_pty::Child`]
    /// lives in the exit-watcher task; this handle is what we use to send
    /// a signal without contending on that task's ownership.
    child_killer: Arc<Mutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>>,

    /// Fan-out for output chunks.
    output_tx: broadcast::Sender<OutputChunk>,
    /// Fan-out for process exit. A single value is sent when the child exits;
    /// subscribers created before exit will receive it.
    exit_tx: broadcast::Sender<ExitStatus>,

    /// Per-session working directory. Dropping this session drops the
    /// `TempDir`, which deletes the directory tree on disk.
    _session_dir: tempfile::TempDir,
}

impl PtySession {
    /// Spawn the agent binary for `agent_type` in a fresh PTY.
    ///
    /// Steps, in order:
    ///
    /// 1. Create a temp directory (auto-cleaned on session drop).
    /// 2. Have `assembler` write the context file (`CLAUDE.md` / `AGENTS.md`)
    ///    into the temp directory.
    /// 3. Resolve the agent binary on `PATH` via [`which::which`].
    /// 4. Open a PTY pair and spawn the binary with `cwd` set to the temp dir.
    /// 5. Start the reader and exit-watcher tasks.
    pub async fn launch(
        agent_type: AgentType,
        initial_prompt: Option<String>,
        assembler: &GraphContextAssembler,
    ) -> anyhow::Result<Self> {
        let session_dir = tempfile::tempdir()?;

        assembler
            .write_context_file(session_dir.path(), agent_type)
            .await
            .map_err(|e| match e {
                ContextError::Other(err) => err,
                other => anyhow::Error::new(other),
            })?;

        let definition = SystemAgentRegistry::new()
            .get(agent_type)
            .ok_or_else(|| anyhow::anyhow!("agent {:?} missing from catalog", agent_type))?;

        let binary_path = which::which(definition.binary).map_err(|e| {
            anyhow::anyhow!(
                "agent binary '{}' not found on PATH: {}",
                definition.binary,
                e
            )
        })?;

        Self::spawn_in_pty(
            agent_type,
            binary_path,
            initial_prompt,
            session_dir,
            DEFAULT_PTY_ROWS,
            DEFAULT_PTY_COLS,
        )
    }

    /// Inner helper that opens the PTY, spawns the process, and wires up
    /// reader / exit-watcher tasks. Split out from [`launch`](Self::launch)
    /// so tests can construct sessions without going through
    /// [`GraphContextAssembler`].
    fn spawn_in_pty(
        agent_type: AgentType,
        binary_path: std::path::PathBuf,
        initial_prompt: Option<String>,
        session_dir: tempfile::TempDir,
        rows: u16,
        cols: u16,
    ) -> anyhow::Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(binary_path);
        cmd.cwd(session_dir.path());
        if let Some(prompt) = initial_prompt {
            cmd.arg(prompt);
        }

        let child = pair.slave.spawn_command(cmd)?;
        // `portable-pty` recommends dropping the slave handle once the child
        // is spawned so closing the master tears down the PTY cleanly.
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let child_killer = child.clone_killer();

        let (output_tx, _) = broadcast::channel::<OutputChunk>(OUTPUT_CHANNEL_CAPACITY);
        let (exit_tx, _) = broadcast::channel::<ExitStatus>(1);

        let id = Uuid::new_v4();
        let started_at = Utc::now();

        let master = Arc::new(Mutex::new(pair.master));
        let writer = Arc::new(Mutex::new(writer));
        let child_killer = Arc::new(Mutex::new(child_killer));

        spawn_reader_task(reader, output_tx.clone());
        spawn_exit_watcher_task(child, exit_tx.clone());

        Ok(Self {
            id,
            agent_type,
            started_at,
            master,
            writer,
            child_killer,
            output_tx,
            exit_tx,
            _session_dir: session_dir,
        })
    }

    /// Subscribe to the PTY's output byte stream.
    ///
    /// Every subscriber sees the same stream from the point of subscription
    /// forward. Subscribers that fall too far behind will get
    /// `RecvError::Lagged`.
    pub fn subscribe_output(&self) -> broadcast::Receiver<OutputChunk> {
        self.output_tx.subscribe()
    }

    /// Subscribe to the child-exit signal.
    ///
    /// Exactly one [`ExitStatus`] value is broadcast when the child exits;
    /// subscribers that subscribed before exit will receive it.
    pub fn subscribe_exit(&self) -> broadcast::Receiver<ExitStatus> {
        self.exit_tx.subscribe()
    }

    /// Write `data` to the PTY's stdin.
    pub async fn write_input(&self, data: &[u8]) -> anyhow::Result<()> {
        let writer = self.writer.clone();
        let buf = data.to_vec();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let mut guard = writer.blocking_lock();
            guard.write_all(&buf)?;
            guard.flush()?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("write_input task panicked: {}", e))?
    }

    /// Resize the PTY to `cols` x `rows`.
    pub async fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        let master = self.master.lock().await;
        master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    /// Kill the child process and wait for the exit-watcher to observe it.
    ///
    /// Takes `&self` rather than consuming the session, because typical
    /// callers hold the session behind an `Arc` (the manager hands out
    /// `Arc<PtySession>` from `get()`). Temp-dir cleanup happens when the
    /// last `Arc` is dropped — usually the manager removing the entry
    /// after this returns.
    pub async fn terminate(&self) -> anyhow::Result<()> {
        // Subscribe BEFORE sending the kill so we cannot miss the broadcast
        // if the watcher fires immediately. (Late subscribers see Closed
        // once the watcher drops its sender, which would be a spurious
        // failure here.)
        let mut exit_rx = self.exit_tx.subscribe();

        // Send the kill signal. If the child has already exited, kill()
        // returns an error which we ignore — we just want to make sure the
        // process is gone before we drop the temp dir.
        {
            let killer = self.child_killer.clone();
            tokio::task::spawn_blocking(move || {
                let mut guard = killer.blocking_lock();
                let _ = guard.kill();
            })
            .await
            .map_err(|e| anyhow::anyhow!("terminate kill task panicked: {}", e))?;
        }

        match exit_rx.recv().await {
            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => Ok(()),
            Err(broadcast::error::RecvError::Closed) => Err(anyhow::anyhow!(
                "exit watcher closed without reporting status"
            )),
        }
    }
}

/// Background task: read from the PTY master and fan bytes out to subscribers.
///
/// Runs on the blocking pool because `portable-pty`'s reader is synchronous.
/// The task ends naturally when the reader returns EOF (child closed the PTY)
/// or hits an error.
fn spawn_reader_task(mut reader: Box<dyn Read + Send>, output_tx: broadcast::Sender<OutputChunk>) {
    tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; READ_BUFFER_BYTES];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF: child closed the PTY.
                Ok(n) => {
                    let chunk = OutputChunk {
                        data: buf[..n].to_vec(),
                        timestamp: Utc::now(),
                    };
                    // Send errors only happen when no receivers exist; that
                    // is fine — keep draining the PTY so the kernel buffer
                    // does not fill and block the child.
                    let _ = output_tx.send(chunk);
                }
                Err(e) => {
                    tracing::warn!("pty reader error: {}", e);
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
impl PtySession {
    /// Test-only constructor: spawn an arbitrary binary in a PTY without
    /// going through [`GraphContextAssembler`]. Lets tests use shell utilities
    /// (`cat`, `sh -c '...'`) instead of depending on a real agent binary.
    pub(crate) fn launch_for_test(binary: &str, args: Vec<String>) -> anyhow::Result<Self> {
        let session_dir = tempfile::tempdir()?;
        let binary_path = which::which(binary)
            .map_err(|e| anyhow::anyhow!("test binary '{}' not on PATH: {}", binary, e))?;
        let agent_type = AgentType::ClaudeCode;

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: DEFAULT_PTY_ROWS,
            cols: DEFAULT_PTY_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut cmd = CommandBuilder::new(binary_path);
        cmd.cwd(session_dir.path());
        for a in &args {
            cmd.arg(a);
        }
        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let child_killer = child.clone_killer();

        let (output_tx, _) = broadcast::channel::<OutputChunk>(OUTPUT_CHANNEL_CAPACITY);
        let (exit_tx, _) = broadcast::channel::<ExitStatus>(1);

        let id = Uuid::new_v4();
        let started_at = Utc::now();

        let master = Arc::new(Mutex::new(pair.master));
        let writer = Arc::new(Mutex::new(writer));
        let child_killer = Arc::new(Mutex::new(child_killer));

        spawn_reader_task(reader, output_tx.clone());
        spawn_exit_watcher_task(child, exit_tx.clone());

        Ok(Self {
            id,
            agent_type,
            started_at,
            master,
            writer,
            child_killer,
            output_tx,
            exit_tx,
            _session_dir: session_dir,
        })
    }

    /// Test-only accessor: where this session's temp dir lives.
    pub(crate) fn session_dir_path(&self) -> &std::path::Path {
        self._session_dir.path()
    }
}

/// Background task: own the child, wait for it to exit, broadcast the status.
fn spawn_exit_watcher_task(
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
    exit_tx: broadcast::Sender<ExitStatus>,
) {
    tokio::task::spawn_blocking(move || {
        let status = match child.wait() {
            Ok(s) => ExitStatus {
                code: s.exit_code(),
                success: s.success(),
            },
            Err(e) => {
                tracing::warn!("pty child wait error: {}", e);
                ExitStatus {
                    code: u32::MAX,
                    success: false,
                }
            }
        };
        let _ = exit_tx.send(status);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    /// Drain at most `max_chunks` chunks within `wait` from a subscriber and
    /// return the concatenated bytes. Used by tests that need to inspect the
    /// PTY's output stream without depending on exact chunk boundaries.
    async fn collect_output(
        rx: &mut broadcast::Receiver<OutputChunk>,
        wait: Duration,
        max_chunks: usize,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        for _ in 0..max_chunks {
            match timeout(wait, rx.recv()).await {
                Ok(Ok(chunk)) => out.extend_from_slice(&chunk.data),
                _ => break,
            }
        }
        out
    }

    #[tokio::test]
    async fn launch_for_test_runs_command_and_emits_output() {
        let session =
            PtySession::launch_for_test("sh", vec!["-c".into(), "echo hello-from-pty".into()])
                .expect("launch test session");

        assert_eq!(session.agent_type, AgentType::ClaudeCode);
        assert!(session.session_dir_path().exists());

        let mut rx = session.subscribe_output();
        let mut exit_rx = session.subscribe_exit();
        let output = collect_output(&mut rx, Duration::from_secs(2), 32).await;
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("hello-from-pty"),
            "expected echoed text in PTY output, got: {:?}",
            text
        );

        let status = timeout(Duration::from_secs(2), exit_rx.recv())
            .await
            .expect("exit broadcast within deadline")
            .expect("clean exit broadcast");
        assert!(status.success, "echo should exit successfully");
    }

    #[tokio::test]
    async fn write_input_is_echoed_back_through_pty() {
        // `cat` echoes stdin back to stdout, which the PTY then loops back
        // to its output stream.
        let session = PtySession::launch_for_test("cat", vec![]).expect("launch cat session");

        let mut rx = session.subscribe_output();

        session
            .write_input(b"ping\n")
            .await
            .expect("write input succeeds");

        let output = collect_output(&mut rx, Duration::from_secs(2), 32).await;
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("ping"),
            "expected echoed input in PTY output, got: {:?}",
            text
        );

        session.terminate().await.expect("terminate cat session");
    }

    #[tokio::test]
    async fn resize_does_not_error() {
        let session = PtySession::launch_for_test("sh", vec!["-c".into(), "sleep 1".into()])
            .expect("launch sleep session");

        session.resize(120, 40).await.expect("resize succeeds");
        session.resize(80, 24).await.expect("resize back succeeds");

        let mut exit_rx = session.subscribe_exit();
        timeout(Duration::from_secs(3), exit_rx.recv())
            .await
            .expect("sleep completes")
            .expect("clean exit");
    }

    #[tokio::test]
    async fn terminate_kills_long_running_process() {
        // `sleep 30` would normally outlive the test; terminate must end it.
        let session = PtySession::launch_for_test("sh", vec!["-c".into(), "sleep 30".into()])
            .expect("launch sleep session");

        let mut exit_rx = session.subscribe_exit();

        timeout(Duration::from_secs(3), session.terminate())
            .await
            .expect("terminate returns within deadline")
            .expect("terminate succeeds");

        let status = timeout(Duration::from_secs(1), exit_rx.recv())
            .await
            .expect("exit broadcast lands")
            .expect("exit status received");
        assert!(!status.success, "killed process should not report success");
    }

    #[tokio::test]
    async fn natural_exit_keeps_temp_dir_until_session_drops() {
        let session = PtySession::launch_for_test("sh", vec!["-c".into(), "echo done".into()])
            .expect("launch echo session");

        let temp_path = session.session_dir_path().to_path_buf();
        assert!(temp_path.exists(), "temp dir should exist immediately");

        let mut exit_rx = session.subscribe_exit();
        timeout(Duration::from_secs(2), exit_rx.recv())
            .await
            .expect("exit lands")
            .expect("clean exit");

        // Session still in scope: temp dir still exists.
        assert!(
            temp_path.exists(),
            "temp dir should outlive process exit until session drops"
        );

        drop(session);

        // After drop the TempDir's destructor removes the directory tree.
        assert!(
            !temp_path.exists(),
            "temp dir should be removed when session drops"
        );
    }
}
