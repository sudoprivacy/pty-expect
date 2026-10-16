//! Native terminal teardown includes the child process's completed exit.
#![cfg(unix)]

use std::process::Command;

use pty_expect::PtySession;

#[test]
fn drop_reaps_a_child_that_requires_forced_termination() {
    let mut session = PtySession::spawn(
        "sh",
        &[
            "-c",
            r#"trap '' HUP; printf 'PTY-CHILD-PID:%s\n' "$$"; read -r value"#,
        ],
    )
    .expect("spawn a child that ignores SIGHUP");
    let output = session
        .expect(r"PTY-CHILD-PID:[0-9]+\r?\n")
        .expect("the child installed its signal disposition");
    let pid = output
        .rsplit_once("PTY-CHILD-PID:")
        .expect("child PID marker")
        .1
        .trim()
        .parse::<u32>()
        .expect("numeric child PID");
    drop(session);

    let status = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "ppid="])
        .output()
        .expect("inspect the actual child after terminal teardown");
    assert!(status.stderr.is_empty(), "{:?}", status.stderr);
    assert!(status.status.success() || status.status.code() == Some(1));
    let parents = String::from_utf8(status.stdout).expect("ASCII process IDs");
    let owner = std::process::id().to_string();
    assert!(
        !parents.lines().any(|parent| parent.trim() == owner),
        "terminal drop left owned child {pid} unreaped"
    );
}
