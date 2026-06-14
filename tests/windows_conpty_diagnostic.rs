//! Windows ConPTY raw-read diagnostic.
//!
//! Skips PtySession entirely and uses portable-pty directly. Spawns
//! a child that writes a known marker string, then reads from the
//! master in a tight loop printing each chunk's size and total
//! elapsed time. Surfaces hard data on what ConPTY actually
//! delivers (bytes? EOF? error?) so we can stop guessing what
//! pty-expect's Windows path is doing wrong.
//!
//! Only enabled on Windows. Run with:
//!   cargo test --test windows_conpty_diagnostic -- --nocapture
//! (the `--nocapture` is required to see the diagnostic prints in
//! real time; without it cargo swallows stdout until the test
//! returns.)

#![cfg(windows)]

use std::io::Read;
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

const MARKER: &str = "HELLO_FROM_CONPTY_DIAG_MARKER";

/// `#[ignore]` because this test deliberately reproduces the
/// pre-fix DSR(6) hang to surface diagnostic data — it never
/// participates in the handshake itself, so `reader.read()` blocks
/// indefinitely after the first 4 bytes (the DSR(6) request). The
/// deadline check sits *after* the blocking read, so it never
/// fires; on CI the test would hang for the runner's full
/// timeout. Run it manually with:
///
///   cargo test --test windows_conpty_diagnostic -- --ignored --nocapture
#[test]
#[ignore]
fn raw_read_from_cmd_echo() {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");

    let mut builder = CommandBuilder::new("cmd");
    builder.arg("/c");
    builder.arg(format!("echo {}", MARKER));

    let mut child = pair
        .slave
        .spawn_command(builder)
        .expect("spawn cmd /c echo");

    let mut reader = pair.master.try_clone_reader().expect("clone reader");

    // Drop slave per pty-expect's convention.
    drop(pair.slave);

    let start = Instant::now();
    let deadline = start + Duration::from_secs(15);
    let mut total: Vec<u8> = Vec::new();
    let mut chunk_idx = 0;
    let mut chunk = [0u8; 4096];
    let mut last_print = Instant::now();

    println!(
        "[t=0ms] starting reader loop, deadline {}s",
        (deadline - start).as_secs()
    );

    loop {
        // Don't block; check child + deadline between reads via a
        // bounded blocking read.
        match reader.read(&mut chunk) {
            Ok(0) => {
                println!(
                    "[t={:>5}ms] reader.read -> Ok(0) EOF after {} chunk(s), {} total bytes",
                    start.elapsed().as_millis(),
                    chunk_idx,
                    total.len()
                );
                break;
            }
            Ok(n) => {
                chunk_idx += 1;
                total.extend_from_slice(&chunk[..n]);
                println!(
                    "[t={:>5}ms] reader.read -> Ok({}) chunk #{} -- bytes: {:?}",
                    start.elapsed().as_millis(),
                    n,
                    chunk_idx,
                    String::from_utf8_lossy(&chunk[..n])
                );
            }
            Err(e) => {
                println!(
                    "[t={:>5}ms] reader.read -> Err({}) after {} chunks, {} total bytes",
                    start.elapsed().as_millis(),
                    e,
                    chunk_idx,
                    total.len()
                );
                break;
            }
        }

        if Instant::now() >= deadline {
            println!(
                "[t={:>5}ms] DEADLINE HIT — reader did not see EOF in 15s",
                start.elapsed().as_millis()
            );
            break;
        }

        // Print periodic heartbeat so we can see whether reader is
        // simply blocking or making progress.
        if last_print.elapsed() > Duration::from_secs(1) {
            println!(
                "[t={:>5}ms] heartbeat: chunks={}, total bytes={}",
                start.elapsed().as_millis(),
                chunk_idx,
                total.len()
            );
            last_print = Instant::now();
        }
    }

    // Inspect child status after the read loop ended.
    match child.try_wait() {
        Ok(Some(status)) => println!(
            "[t={:>5}ms] child exited: code={}",
            start.elapsed().as_millis(),
            status.exit_code()
        ),
        Ok(None) => println!(
            "[t={:>5}ms] child still running after read loop",
            start.elapsed().as_millis()
        ),
        Err(e) => println!(
            "[t={:>5}ms] child.try_wait -> Err({})",
            start.elapsed().as_millis(),
            e
        ),
    }

    let _ = child.kill();
    println!(
        "[t={:>5}ms] DIAGNOSTIC DONE. total bytes={}. marker present={}",
        start.elapsed().as_millis(),
        total.len(),
        String::from_utf8_lossy(&total).contains(MARKER)
    );

    // Diagnostic ALWAYS passes — its job is to print, not gate CI.
    // The reader's findings are inspected manually.
}
