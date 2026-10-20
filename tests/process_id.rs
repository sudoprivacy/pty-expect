//! `PtySession::process_id` returns the real OS pid of the spawned child.
//!
//! Verified by having the child print its own `$$` and comparing: a value
//! that merely looks like a pid is not enough, it has to be *the* child.
#![cfg(unix)]

use std::time::Duration;

use pty_expect::PtySession;

#[test]
fn process_id_is_the_spawned_child() {
    let mut session = PtySession::spawn(
        "sh",
        &["-c", r#"printf 'PTY-CHILD-PID:%s\n' "$$"; read -r _"#],
    )
    .expect("spawn a child that reports its own pid");
    session.set_default_timeout(Duration::from_secs(10));

    // Take the pid right after spawn, while the child is alive.
    let reported = session.process_id().expect("a live child has a process id");

    let line = session
        .expect(r"PTY-CHILD-PID:[0-9]+\r?\n")
        .expect("the child printed its own pid");
    let self_reported: u32 = line
        .rsplit_once("PTY-CHILD-PID:")
        .expect("pid marker")
        .1
        .trim()
        .parse()
        .expect("pid is numeric");

    assert_eq!(
        reported, self_reported,
        "process_id() must be the child's own pid, not a wrapper's"
    );

    // Release the child: Ctrl+D ends `read`, the shell exits.
    session.send_ctrl('d').expect("send ctrl+d");
    let _ = session.expect_eof();
}
