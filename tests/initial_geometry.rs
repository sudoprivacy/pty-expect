//! The child's first native size query and the screen agree before any resize.

use pty_expect::PtySession;

fn query_initial_size(rows: u16, cols: u16, use_default: bool) {
    #[cfg(unix)]
    let (command, args) = ("sh", vec!["-c", "stty size; printf 'SIZE-DONE\\n'"]);
    #[cfg(windows)]
    let (command, args) = (
        "powershell",
        vec![
            "-NoProfile",
            "-Command",
            "[Console]::WriteLine(('{0} {1}' -f [Console]::WindowHeight, [Console]::WindowWidth)); [Console]::WriteLine('SIZE-DONE')",
        ],
    );
    let mut session = if use_default {
        PtySession::spawn(command, &args)
    } else {
        PtySession::spawn_with_size(command, &args, rows, cols)
    }
    .expect("spawn native geometry query");
    session
        .expect("SIZE-DONE")
        .expect("first size query finished");
    assert_eq!(session.expect_eof().expect("native query exits"), 0);
    session.render(|screen| {
        assert_eq!(screen.size(), (rows, cols));
        let expected = format!("{rows} {cols}");
        assert!(
            screen
                .contents()
                .lines()
                .any(|line| line.trim() == expected),
            "first native size must be {expected}: {:?}",
            screen.contents(),
        );
    });
}

#[test]
fn initial_geometry_is_visible_to_the_child_and_the_screen() {
    for (rows, cols) in [(40, 120), (100, 20), (30, 90)] {
        query_initial_size(rows, cols, false);
    }
}

#[test]
fn default_geometry_remains_24_by_80() {
    query_initial_size(24, 80, true);
}
