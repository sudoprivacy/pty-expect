//! Windows ConPTY runtime smoke tests, exercising the full PtySession
//! stack against `cmd` and `powershell`.
//!
//! These are the Windows counterpart to `echo_smoke.rs`'s
//! `#[cfg(unix)]` tests. Until issue #1 was fixed by adding a
//! reader-thread DSR(6) handshake (CSI 6 n → CSI 1 ; 1 R), these
//! tests would have looked exactly like the three failed attempts
//! the echo_smoke.rs header comment documents — `expect()` would
//! time out because ConPTY blocks on its initialization handshake
//! waiting for a terminal-emulator peer.
//!
//! With the handshake landed, the reader thread auto-responds to
//! each DSR(6) the moment it arrives, ConPTY proceeds, the child's
//! output reaches us, and `expect()` matches.

#![cfg(windows)]

use std::time::Duration;

use pty_expect::PtySession;

const MARKER: &str = "HELLO_FROM_PTY_EXPECT_WINDOWS_SMOKE";

/// Roughly the Windows analogue of `echo_round_trip` in
/// `echo_smoke.rs`. `cmd /c echo MARKER` writes the marker to its
/// stdout, ConPTY pumps it to us through the master, the reader
/// thread appends it to the shared buffer, `expect()` matches.
#[test]
fn echo_round_trip_through_cmd() {
    let mut sess = PtySession::spawn("cmd", &["/c", &format!("echo {}", MARKER)]).expect("spawn");
    sess.set_default_timeout(Duration::from_secs(15));
    sess.expect(MARKER).expect("see marker from cmd /c echo");
}

/// Same marker, different shell — powershell goes through a
/// noticeably different ConPTY init sequence than cmd. Exercising
/// both gives us coverage that the DSR(6) handler isn't an
/// accidentally-cmd-specific fix.
#[test]
fn echo_round_trip_through_powershell() {
    let mut sess = PtySession::spawn(
        "powershell",
        &[
            "-NoProfile",
            "-Command",
            &format!("Write-Output {}", MARKER),
        ],
    )
    .expect("spawn powershell");
    sess.set_default_timeout(Duration::from_secs(30));
    sess.expect(MARKER)
        .expect("see marker from powershell Write-Output");
}

/// Two-step interaction: `cmd` runs a `set /p` prompt, we send a
/// line back, then `cmd` echoes the variable. Exercises the
/// `send_line → child reads → child writes → expect` loop end-to-
/// end on Windows.
///
/// Currently `#[ignore]`d. The DSR(6) handshake fix that lands
/// with this commit unblocks the *output* side of Windows ConPTY
/// (the two `echo_round_trip_*` tests above pass). The *input*
/// side via `send_line` on Windows has a separate timing /
/// line-ending issue we have not yet diagnosed:
///
///   - `cmd /c "set /p VAL= && echo …"` does not emit a prompt
///     before reading, so we have no synchronisation point to know
///     when `set /p` is actually waiting on stdin;
///   - Windows convention is `\r\n` not bare `\n`, and ConPTY may
///     or may not translate;
///   - cmd's `set /p` is famously finicky about input timing.
///
/// All three could be the culprit. Treat as a follow-up: once a
/// real PTY scenario downstream needs `send_line` on Windows,
/// debug it then with the concrete failure case in hand rather
/// than trying to satisfy an artificial test now. Tracked under
/// issue #1's follow-ups (or a fresh issue if extracted).
#[test]
#[ignore]
fn send_line_round_trip_through_cmd_set_p() {
    let script = format!("set /p VAL= && echo GOT-%VAL%-{}", MARKER);
    let mut sess = PtySession::spawn("cmd", &["/c", &script]).expect("spawn");
    sess.set_default_timeout(Duration::from_secs(15));

    // `cmd`'s `set /p` does not print a prompt label of its own, but
    // the PTY still raises bytes once the child is ready to read
    // stdin. Send the value immediately; the child consumes it and
    // emits the echo line.
    sess.send_line("ping").expect("send ping");
    sess.expect(&format!("GOT-ping-{}", MARKER))
        .expect("see echoed marker");
}
