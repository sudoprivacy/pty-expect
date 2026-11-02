//! Closing a real ConPTY must release its reader before the reaper budget.
#![cfg(windows)]

use std::time::{Duration, Instant};

use pty_expect::PtySession;

#[test]
fn drop_does_not_wait_for_a_reader_held_open_by_its_own_terminal() {
    let mut fastest = Duration::MAX;
    for sample in 0..5 {
        let mut session = PtySession::spawn(
            "powershell",
            &[
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Write-Output PTY_DROP_READY; $null = [Console]::ReadLine()",
            ],
        )
        .expect("spawn a real child waiting for terminal input");
        session.set_default_timeout(Duration::from_secs(15));
        session.expect("PTY_DROP_READY").expect("child is ready");

        let started = Instant::now();
        drop(session);
        let elapsed = started.elapsed();
        eprintln!("ConPTY drop sample {sample}: {elapsed:?}");
        fastest = fastest.min(elapsed);
    }

    // Individual samples may be delayed by the host. Requiring one prompt
    // drain still rejects waiting out the existing 500 ms reaper budget on
    // every session while keeping its master handle open.
    assert!(
        fastest < Duration::from_millis(450),
        "every terminal drop waited for its own reader: fastest={fastest:?}"
    );
}
