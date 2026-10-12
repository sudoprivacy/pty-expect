use pty_expect::PtySession;
use std::time::Duration;

#[test]
fn process_exit_includes_the_final_rendered_output() {
    #[cfg(windows)]
    let (command, args) = ("powershell", vec!["-NoProfile", "-Command", "1..1200 | ForEach-Object { Write-Output ('output-' + $_) }; Write-Output 'FINAL_PTY_OUTPUT_COMPLETE'; exit 37"]);
    #[cfg(unix)]
    let (command, args) = ("sh", vec!["-c", "i=0; while [ $i -lt 1200 ]; do echo output-$i; i=$((i+1)); done; echo FINAL_PTY_OUTPUT_COMPLETE; exit 37"]);

    for _ in 0..3 {
        let mut session = PtySession::spawn(command, &args).expect("spawn output burst");
        session.set_default_timeout(Duration::from_secs(20));
        assert_eq!(session.expect_eof().expect("process and output EOF"), 37);
        assert!(session
            .render(|screen| screen.contents())
            .contains("FINAL_PTY_OUTPUT_COMPLETE"));
        assert!(
            session.resize(30, 90).is_err(),
            "the terminal is closed after EOF"
        );
    }
}
