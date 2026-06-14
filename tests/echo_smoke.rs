//! Self-contained smoke test that drives `sh -c "echo ... && cat"` on
//! Unix and `cmd /C "echo ..."` on Windows. The point is to prove the
//! whole stack — PTY open + child spawn + reader thread + expect +
//! VT100 render — works end-to-end without any external dependency
//! beyond the platform's default shell.

use std::time::Duration;

use pty_expect::PtySession;

#[cfg(unix)]
fn echo_command() -> (&'static str, Vec<&'static str>) {
    ("sh", vec!["-c", "echo hello-from-pty-expect && cat"])
}

#[cfg(windows)]
fn echo_command() -> (&'static str, Vec<&'static str>) {
    // PowerShell with a deliberate `Start-Sleep` after the write. The
    // sleep is load-bearing: it keeps the child alive for ~1s after it
    // emits the line so the parent-side reader can drain the ConPTY
    // pipe before the child exits and the kernel closes the pipe.
    //
    // The `cmd /C echo` shape this code used to take exited within a
    // millisecond, and on Windows ConPTY that race (fast child exit
    // vs. reader scheduling) drops the output on the floor: the
    // reader.read() goes straight to EOF without ever seeing the line.
    (
        "powershell",
        vec![
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Write-Host 'hello-from-pty-expect'; Start-Sleep -Seconds 1",
        ],
    )
}

#[test]
fn echo_round_trip() {
    let (cmd, args) = echo_command();
    let mut sess = PtySession::spawn(cmd, &args).expect("spawn");
    sess.set_default_timeout(Duration::from_secs(10));

    // The echo lands on stdout — assert against the raw byte stream.
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
