//! Real native allocation, shell I/O and teardown under concurrent callers.
#![cfg(unix)]

use std::sync::{Arc, Barrier};
use std::thread;

use pty_expect::PtySession;

fn round_trip(worker: usize, round: usize) -> Result<(), String> {
    let mut session = PtySession::spawn(
        "sh",
        &[
            "-c",
            r#"printf 'PTY-READY\n'; read -r value; printf 'PTY-DONE:%s\n' "$value""#,
        ],
    )
    .map_err(|error| format!("allocate/spawn: {error:?}"))?;
    session
        .expect("PTY-READY")
        .map_err(|error| format!("ready: {error:?}"))?;
    if round % 2 == 0 {
        drop(session);
        return Ok(());
    }
    let value = format!("{worker}-{round}");
    session
        .send_line(&value)
        .map_err(|error| format!("input: {error:?}"))?;
    session
        .expect(&format!("PTY-DONE:{value}"))
        .map_err(|error| format!("output: {error:?}"))?;
    let code = session
        .expect_eof()
        .map_err(|error| format!("exit: {error:?}"))?;
    if code != 0 {
        return Err(format!("unexpected exit code {code}"));
    }
    Ok(())
}

#[test]
fn concurrent_native_shells_preserve_input_across_exit_and_drop() {
    let barrier = Arc::new(Barrier::new(16));
    let workers: Vec<_> = (0..16)
        .map(|worker| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let mut failures = Vec::new();
                for round in 0..50 {
                    if let Err(error) = round_trip(worker, round) {
                        failures.push(format!("worker {worker}, round {round}: {error}"));
                    }
                }
                failures
            })
        })
        .collect();
    let failures: Vec<_> = workers
        .into_iter()
        .flat_map(|worker| worker.join().expect("shell driver panicked"))
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
