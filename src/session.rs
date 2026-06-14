//! Owns one PTY-backed child process and exposes the input / expect /
//! render surface.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use regex::Regex;

use crate::error::{Error, Result};
use crate::screen::Screen;

/// Initial PTY geometry. Matches the typical default for a terminal
/// emulator window — large enough that one-line wraps in tests are
/// unusual, small enough that a render snapshot stays readable.
const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;

/// Default time we wait for an expected pattern before giving up.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// How long the expect / wait loops sleep between checks. Short enough
/// to feel responsive in tests, long enough to keep CPU near zero.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// One PTY-backed child process.
///
/// Created via [`PtySession::spawn`]. A background reader thread
/// continuously pumps bytes from the PTY master into a shared buffer
/// and a VT100 parser, so [`PtySession::expect`] and
/// [`PtySession::render`] both see "the latest view" without the
/// caller doing any I/O scheduling.
///
/// Dropping the session kills the child (best effort) and joins the
/// reader thread.
pub struct PtySession {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    shared: Arc<Mutex<Shared>>,
    reader_handle: Option<JoinHandle<()>>,
    default_timeout: Duration,
}

/// Shared state between the background reader thread and the foreground
/// expect/render methods.
struct Shared {
    /// Every byte read from the PTY since the child started.
    raw: Vec<u8>,
    /// How far into `raw` the caller has already consumed via
    /// successful `expect()` calls. Lets each `expect` look only at
    /// "new" output, matching the behaviour users expect from
    /// pexpect-style libraries.
    consumed: usize,
    /// VT100 parser; the rendered screen is `parser.screen()`.
    parser: vt100::Parser,
    /// Set by the reader thread when the PTY read returns 0 or errors.
    eof: bool,
}

impl PtySession {
    /// Spawn `cmd` with the given arguments, wired to a fresh PTY.
    ///
    /// The PTY is created at the default size (24x80) and the child
    /// inherits the current process's environment except for any
    /// terminal-state variables that would confuse the child (see the
    /// documentation on `portable_pty::CommandBuilder` for details).
    pub fn spawn(cmd: &str, args: &[&str]) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: DEFAULT_ROWS,
                cols: DEFAULT_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| Error::PtyOpen(e.to_string()))?;

        let mut builder = CommandBuilder::new(cmd);
        for arg in args {
            builder.arg(arg);
        }

        let child = pair
            .slave
            .spawn_command(builder)
            .map_err(|e| Error::Spawn(e.to_string()))?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| Error::PtyOpen(e.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| Error::PtyOpen(e.to_string()))?;

        // Drop the slave side now that the child owns its end. Keeping
        // it open in the parent confuses some shells about whether the
        // session is interactive.
        drop(pair.slave);

        let shared = Arc::new(Mutex::new(Shared {
            raw: Vec::new(),
            consumed: 0,
            parser: vt100::Parser::new(DEFAULT_ROWS, DEFAULT_COLS, 0),
            eof: false,
        }));

        let shared_for_thread = Arc::clone(&shared);
        let reader_handle = thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) => {
                        let mut s = shared_for_thread.lock().unwrap();
                        s.eof = true;
                        break;
                    }
                    Ok(n) => {
                        let mut s = shared_for_thread.lock().unwrap();
                        s.raw.extend_from_slice(&chunk[..n]);
                        s.parser.process(&chunk[..n]);
                    }
                    Err(_) => {
                        let mut s = shared_for_thread.lock().unwrap();
                        s.eof = true;
                        break;
                    }
                }
            }
        });

        Ok(Self {
            child,
            writer,
            master: pair.master,
            shared,
            reader_handle: Some(reader_handle),
            default_timeout: DEFAULT_TIMEOUT,
        })
    }

    /// Override the default timeout used by [`PtySession::expect`] and
    /// [`PtySession::expect_eof`].
    pub fn set_default_timeout(&mut self, dur: Duration) {
        self.default_timeout = dur;
    }

    /// Send `text` followed by `\n` to the child's stdin.
    pub fn send_line(&mut self, text: &str) -> Result<()> {
        self.writer.write_all(text.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        Ok(())
    }

    /// Send `text` as-is to the child's stdin (no newline appended).
    pub fn send(&mut self, text: &str) -> Result<()> {
        self.writer.write_all(text.as_bytes())?;
        self.writer.flush()?;
        Ok(())
    }

    /// Send a control character (`Ctrl+<letter>`).
    ///
    /// `c` must be an ASCII letter; case-insensitive. `Ctrl+C` is byte
    /// `0x03`, `Ctrl+D` is `0x04`, and so on. Returns
    /// [`Error::InvalidCtrlChar`] for anything else.
    pub fn send_ctrl(&mut self, c: char) -> Result<()> {
        let upper = c.to_ascii_uppercase();
        if !upper.is_ascii_alphabetic() {
            return Err(Error::InvalidCtrlChar(c));
        }
        let byte = (upper as u8) - b'A' + 1;
        self.writer.write_all(&[byte])?;
        self.writer.flush()?;
        Ok(())
    }

    /// Resize the PTY to `rows` x `cols`. The child sees `SIGWINCH` on
    /// Unix or the equivalent ConPTY notification on Windows.
    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| Error::PtyResize(e.to_string()))?;
        let mut s = self.shared.lock().unwrap();
        s.parser.screen_mut().set_size(rows, cols);
        Ok(())
    }

    /// Block until `pattern` (a regex) appears in the unconsumed PTY
    /// byte stream, or the default timeout elapses.
    ///
    /// On success, returns the matched substring and advances the
    /// consumed cursor past it, so a subsequent `expect()` looks only
    /// at output that arrives after the match.
    pub fn expect(&mut self, pattern: &str) -> Result<String> {
        self.expect_within(pattern, self.default_timeout)
    }

    /// Like [`PtySession::expect`] but with an explicit timeout.
    pub fn expect_within(&mut self, pattern: &str, timeout: Duration) -> Result<String> {
        let re = Regex::new(pattern)?;
        let deadline = Instant::now() + timeout;
        loop {
            {
                let mut s = self.shared.lock().unwrap();
                let view = String::from_utf8_lossy(&s.raw[s.consumed..]);
                if let Some(m) = re.find(&view) {
                    let matched = view[..m.end()].to_string();
                    s.consumed += m.end();
                    return Ok(matched);
                }
                if s.eof {
                    return Err(Error::Eof(pattern.to_string()));
                }
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout(timeout, pattern.to_string()));
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    /// Inspect the current VT100-rendered screen.
    ///
    /// `f` is called while holding the shared state lock, so keep it
    /// short (e.g. extract any state you need with `to_string()` /
    /// `to_owned()` and return it; do not do expensive work inside).
    pub fn render<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Screen<'_>) -> R,
    {
        let s = self.shared.lock().unwrap();
        let screen = s.parser.screen();
        let wrapper = Screen::from_vt(screen);
        f(&wrapper)
    }

    /// Wait for the child to exit and return its exit code.
    ///
    /// Uses the default timeout. Calls to `expect_eof` consume the
    /// child handle, so this is the last operation on the session.
    pub fn expect_eof(&mut self) -> Result<u32> {
        let deadline = Instant::now() + self.default_timeout;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Ok(status.exit_code()),
                Ok(None) => {}
                Err(e) => return Err(Error::Wait(e.to_string())),
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout(
                    self.default_timeout,
                    "<child exit>".to_string(),
                ));
            }
            thread::sleep(POLL_INTERVAL);
        }
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        // Best-effort cleanup. Errors are ignored because the child may
        // already be gone, and we are in a Drop.
        let _ = self.child.kill();
        if let Some(h) = self.reader_handle.take() {
            // The reader thread exits when the PTY read returns
            // EOF/error after the child dies. We give it a moment to
            // notice; if it has not, drop the JoinHandle without
            // joining — the thread will outlive us briefly but cannot
            // touch anything we own.
            let _ = h.join();
        }
    }
}
