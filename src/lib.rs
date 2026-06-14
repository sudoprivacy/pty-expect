//! # pty-expect
//!
//! Drive an interactive CLI program through a real PTY — Unix
//! `/dev/pty` or Windows ConPTY — and assert against either the raw
//! byte stream or a VT100-rendered view of "what the user would see".
//!
//! Built for end-to-end testing of REPL-style command-line tools where
//! piped-stdin / piped-stdout testing leaves too much behaviour
//! unexercised: tab completion, ANSI redraw, slash commands rendered
//! into a status line, signal handling (`Ctrl+C`, `Ctrl+D`), terminal
//! resize, and so on. The PTY layer comes from `portable-pty`
//! (wezterm); the rendered view comes from `vt100`.
//!
//! ## Example
//!
//! ```no_run
//! use std::time::Duration;
//! use pty_expect::PtySession;
//!
//! let mut sess = PtySession::spawn("sh", &["-c", "echo hello && cat"])?;
//! sess.set_default_timeout(Duration::from_secs(5));
//!
//! sess.expect(r"hello")?;
//! sess.send_line("world")?;
//! sess.expect(r"world")?;
//!
//! sess.send_ctrl('d')?;
//! sess.expect_eof()?;
//! # Ok::<(), pty_expect::Error>(())
//! ```

mod error;
mod screen;
mod session;

pub use error::{Error, Result};
pub use screen::Screen;
pub use session::PtySession;
