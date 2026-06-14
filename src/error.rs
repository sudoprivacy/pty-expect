use std::time::Duration;

/// Errors produced by [`PtySession`](crate::PtySession) operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Opening the PTY pair failed (kernel rejection, resource exhaustion,
    /// ConPTY unavailable on too-old Windows).
    #[error("pty open: {0}")]
    PtyOpen(String),

    /// Spawning the child command failed (command not found, permission denied).
    #[error("spawn: {0}")]
    Spawn(String),

    /// Resizing the PTY failed (kernel rejection).
    #[error("pty resize: {0}")]
    PtyResize(String),

    /// Underlying I/O error reading from or writing to the PTY.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// The regex passed to [`PtySession::expect`](crate::PtySession::expect)
    /// could not be compiled.
    #[error("regex: {0}")]
    Regex(#[from] regex::Error),

    /// The expected pattern did not appear within the timeout.
    /// The second field is the regex pattern that was being awaited.
    #[error("timeout after {0:?} waiting for pattern: {1}")]
    Timeout(Duration, String),

    /// The child exited before the expected pattern appeared.
    #[error("child exited before pattern matched: {0}")]
    Eof(String),

    /// `send_ctrl` was called with a non-letter character.
    #[error("send_ctrl requires an ASCII letter, got {0:?}")]
    InvalidCtrlChar(char),

    /// Wait for the child exit failed.
    #[error("wait: {0}")]
    Wait(String),
}

/// Convenience alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;
