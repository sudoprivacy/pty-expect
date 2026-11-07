//! Real child processes exercise terminal replies and ConPTY startup.

use pty_expect::PtySession;

#[cfg(windows)]
#[test]
#[ignore = "native child fixture, explicitly launched by its parent"]
fn startup_child() {
    println!("NATIVE-STARTUP-READY");
}

#[cfg(windows)]
#[test]
fn conpty_startup_completes_without_the_three_second_da1_timeout() {
    use std::time::{Duration, Instant};
    let executable = std::env::current_exe().unwrap();
    for sample in 0..4 {
        let started = Instant::now();
        let mut session = PtySession::spawn(
            executable.to_str().unwrap(),
            &["--ignored", "--exact", "startup_child", "--nocapture"],
        )
        .unwrap();
        session.expect("NATIVE-STARTUP-READY").unwrap();
        let elapsed = started.elapsed();
        assert_eq!(session.expect_eof().unwrap(), 0);
        eprintln!("ConPTY ready sample {sample}: {elapsed:?}");
        assert!(
            elapsed < Duration::from_secs(2),
            "DA1 startup handshake stalled: {elapsed:?}"
        );
    }
}

#[cfg(unix)]
const QUERIES: &[(&[u8], &[u8])] = &[
    (b"\x1b[6n", b"\x1b[1;1R"),
    (b"\x1b[c", b"\x1b[?1;0c"),
    (b"\x1b[0c", b"\x1b[?1;0c"),
];

#[cfg(unix)]
#[test]
#[ignore = "native child fixture, explicitly launched by its parent"]
fn query_child() {
    use std::io::{self, Read, Write};
    assert!(std::process::Command::new("stty")
        .args(["-echo", "-icanon", "min", "1", "time", "0"])
        .status()
        .unwrap()
        .success());
    let mut input = io::stdin();
    let mut output = io::stdout();
    let mut sample = 0;
    for &(query, reply) in QUERIES {
        for split in 1..query.len() {
            writeln!(output, "QUERY:{sample}").unwrap();
            output.write_all(&query[..split]).unwrap();
            output.flush().unwrap();
            let mut ack = [0];
            input.read_exact(&mut ack).unwrap();
            assert_eq!(ack, *b"A", "previous query must not be answered twice");
            output.write_all(&query[split..]).unwrap();
            output.flush().unwrap();
            let mut received = vec![0; reply.len()];
            input.read_exact(&mut received).unwrap();
            assert_eq!(received, reply);
            writeln!(output, "ANSWERED:{sample}").unwrap();
            output.flush().unwrap();
            sample += 1;
        }
    }
    for &(query, _) in QUERIES {
        output.write_all(query).unwrap();
    }
    output.flush().unwrap();
    for &(_, reply) in QUERIES {
        let mut received = vec![0; reply.len()];
        input.read_exact(&mut received).unwrap();
        assert_eq!(received, reply, "batched queries retain wire order");
    }
    println!("QUERIES-DONE");
}

#[cfg(unix)]
#[test]
fn fragmented_and_batched_queries_receive_one_ordered_reply() {
    let executable = std::env::current_exe().unwrap();
    let mut session = PtySession::spawn(
        executable.to_str().unwrap(),
        &["--ignored", "--exact", "query_child", "--nocapture"],
    )
    .unwrap();
    let mut sample = 0;
    for &(query, _) in QUERIES {
        for split in 1..query.len() {
            session.expect(&format!("QUERY:{sample}")).unwrap();
            session
                .expect(&regex::escape(
                    std::str::from_utf8(&query[..split]).unwrap(),
                ))
                .unwrap();
            session.send("A").unwrap();
            session.expect(&format!("ANSWERED:{sample}")).unwrap();
            sample += 1;
        }
    }
    session.expect("QUERIES-DONE").unwrap();
    assert_eq!(session.expect_eof().unwrap(), 0);
}
