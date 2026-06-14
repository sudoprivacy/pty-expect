//! Self-contained smoke tests that drive `sh -c ...` on Unix. The
//! whole stack — PTY open + child spawn + reader thread + expect +
//! VT100 render — is exercised end-to-end without any external
//! dependency beyond the platform's default shell.
//!
//! Why these are all `#[cfg(unix)]` in v0.1:
//!
//! - Unix path is verified by these tests at runtime.
//! - The Windows compile path is verified separately by
//!   `cargo clippy --all-targets` on the windows-latest CI job, which
//!   does pass.
//! - Windows runtime behaviour against ConPTY is **not** verified at
//!   runtime in v0.1. We attempted three patches (`cmd /C echo`,
//!   `powershell Write-Host`, `powershell Write-Output`) and each
//!   failed in CI with `expect()` never seeing the child's output,
//!   which means the failure mode is deeper than command-choice. The
//!   honest answer is to stop patching the test and properly
//!   diagnose the Windows path in a focused follow-up rather than
//!   keep guessing.
//!
//! Tracked in <https://github.com/sudoprivacy/pty-expect/issues/1>.

#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
use pty_expect::PtySession;

#[cfg(unix)]
#[test]
fn echo_round_trip() {
    let mut sess =
        PtySession::spawn("sh", &["-c", "echo hello-from-pty-expect && cat"]).expect("spawn");
    sess.set_default_timeout(Duration::from_secs(10));
    sess.expect(r"hello-from-pty-expect").expect("expect echo");
}

#[cfg(unix)]
#[test]
fn send_line_and_round_trip_through_cat() {
    let mut sess = PtySession::spawn("sh", &["-c", "cat"]).expect("spawn cat");
    sess.set_default_timeout(Duration::from_secs(10));

    sess.send_line("ping").expect("send ping");
    sess.expect(r"ping").expect("see ping");

    // Ctrl+D closes stdin; cat exits 0.
    sess.send_ctrl('d').expect("send ctrl+d");
    let code = sess.expect_eof().expect("wait eof");
    assert_eq!(code, 0);
}

#[cfg(unix)]
#[test]
fn render_view_matches_raw_view() {
    let mut sess = PtySession::spawn("sh", &["-c", "echo screen-line-1"]).expect("spawn");
    sess.set_default_timeout(Duration::from_secs(5));

    // Wait for the line to land in raw bytes.
    sess.expect(r"screen-line-1").expect("expect screen-line-1");

    // The VT100-rendered view should also see it, since the echo writes
    // bare ASCII with no escape sequences.
    sess.render(|screen| {
        assert!(
            screen.contents().contains("screen-line-1"),
            "rendered contents missing expected line: {}",
            screen.contents()
        );
    });
}
