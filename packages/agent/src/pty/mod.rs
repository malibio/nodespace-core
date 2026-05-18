//! PTY-based agent session engine (ADR-032).
//!
//! [`PtySession`] owns one running external agent process attached to a
//! pseudo-terminal. [`PtySessionManager`] owns the collection of active
//! sessions and is meant to be held in `nodespaced`'s shared state.
//!
//! The session lifecycle is:
//!
//! 1. Create a per-session temp directory.
//! 2. Write the context file ([`GraphContextAssembler::write_context_file`])
//!    so the agent picks up its `CLAUDE.md` / `AGENTS.md` on launch.
//! 3. Spawn the agent binary inside a freshly opened PTY rooted at the temp dir.
//! 4. Stream stdout/stderr bytes through a `broadcast::Sender<OutputChunk>`.
//! 5. Accept stdin via [`PtySession::write_input`] and resize via [`PtySession::resize`].
//! 6. On [`PtySession::terminate`] (or when the child exits naturally), drop the
//!    [`tempfile::TempDir`] so the working directory is cleaned up.

pub mod capture;
pub mod manager;
pub mod session;

pub use capture::SessionCapture;
pub use manager::{PtySessionManager, SessionMetadata};
pub use session::{ExitStatus, OutputChunk, PtySession};
