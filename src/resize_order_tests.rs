use super::*;

const SENTINEL: &str = "GEOMETRY_ORDER_SENTINEL";
type ObservedGeometry = Arc<Mutex<Option<(u16, u16)>>>;

#[derive(Clone, Copy)]
enum Mode {
    ObserveNewFrame,
    Refuse,
    Backpressure,
}

struct ObservedMaster {
    inner: Box<dyn MasterPty + Send>,
    shared: Arc<Mutex<Shared>>,
    writer: Arc<Mutex<native::Resource<Box<dyn Write + Send>>>>,
    observed: ObservedGeometry,
    mode: Mode,
}

impl ObservedMaster {
    fn command(&self, command: &[u8]) -> anyhow::Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.write_all(command)?;
        // ConPTY's cooked console input accepts Enter as carriage return.
        writer.write_all(if cfg!(windows) { b"\r" } else { b"\n" })?;
        writer.flush()?;
        Ok(())
    }
}

impl MasterPty for ObservedMaster {
    fn resize(&self, size: PtySize) -> anyhow::Result<()> {
        if matches!(self.mode, Mode::Refuse) {
            anyhow::bail!("CONTROLLED_RESIZE_REFUSAL");
        }
        if matches!(self.mode, Mode::Backpressure) {
            self.command(b"flood")?;
            thread::sleep(Duration::from_millis(100));
        }
        self.inner.resize(size)?;
        if matches!(self.mode, Mode::ObserveNewFrame) {
            self.command(b"paint")?;
            let deadline = Instant::now() + Duration::from_millis(500);
            while Instant::now() < deadline {
                match self.shared.try_lock() {
                    Ok(state) => {
                        if state
                            .raw
                            .windows(SENTINEL.len())
                            .any(|b| b == SENTINEL.as_bytes())
                        {
                            *self.observed.lock().unwrap() = Some(state.parser.screen().size());
                            break;
                        }
                    }
                    Err(std::sync::TryLockError::WouldBlock) => {}
                    Err(std::sync::TryLockError::Poisoned(_)) => panic!("reader state poisoned"),
                }
                thread::sleep(Duration::from_millis(2));
            }
        }
        Ok(())
    }

    fn get_size(&self) -> anyhow::Result<PtySize> {
        self.inner.get_size()
    }

    fn try_clone_reader(&self) -> anyhow::Result<Box<dyn Read + Send>> {
        self.inner.try_clone_reader()
    }

    fn take_writer(&self) -> anyhow::Result<Box<dyn Write + Send>> {
        self.inner.take_writer()
    }

    #[cfg(unix)]
    fn process_group_leader(&self) -> Option<i32> {
        self.inner.process_group_leader()
    }

    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<i32> {
        self.inner.as_raw_fd()
    }

    #[cfg(unix)]
    fn tty_name(&self) -> Option<std::path::PathBuf> {
        self.inner.tty_name()
    }
}

fn session(mode: Mode) -> (PtySession, ObservedGeometry) {
    let executable = std::env::current_exe().unwrap();
    let module = module_path!().split_once("::").unwrap().1;
    let child_name = format!("{module}::pty_fixture_child");
    let mut session = PtySession::spawn_with_size(
        executable.to_str().unwrap(),
        &["--exact", &child_name, "--ignored", "--nocapture"],
        6,
        12,
    )
    .unwrap();
    session
        .expect_within("PTY_ORDER_READY", Duration::from_secs(5))
        .unwrap();
    let observed = Arc::new(Mutex::new(None));
    let inner = native::unown_for_test(session.master.take().unwrap());
    session.master = Some(native::own(Box::new(ObservedMaster {
        inner,
        shared: session.shared.clone(),
        writer: session.writer.clone(),
        observed: observed.clone(),
        mode,
    }) as Box<dyn MasterPty + Send>));
    (session, observed)
}

fn expected_line() -> String {
    format!("{}{SENTINEL}", " ".repeat(32))
}

#[test]
#[ignore = "explicit bounded native resize acceptance entry in CI"]
fn resize_observation_new_output_uses_new_geometry() {
    let (mut session, observed) = session(Mode::ObserveNewFrame);
    session.resize(24, 80).unwrap();
    session
        .expect_within(SENTINEL, Duration::from_secs(5))
        .unwrap();
    let screen = session.render(|s| s.contents());
    eprintln!(
        "reader geometry before native return: {:?}; screen: {screen:?}",
        *observed.lock().unwrap()
    );
    assert!(
        screen.lines().any(|line| line == expected_line()),
        "new output used prior viewport: {screen:?}"
    );
}

#[test]
#[ignore = "explicit bounded native resize acceptance entry in CI"]
fn resize_observation_original_order_is_detected() {
    let (mut session, observed) = session(Mode::ObserveNewFrame);
    // Original implementation, retained as a real native negative control.
    session
        .master
        .as_ref()
        .unwrap()
        .resize(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    session
        .shared
        .lock()
        .unwrap()
        .parser
        .screen_mut()
        .set_size(24, 80);
    session
        .expect_within(SENTINEL, Duration::from_secs(5))
        .unwrap();
    let screen = session.render(|s| s.contents());
    assert_eq!(*observed.lock().unwrap(), Some((6, 12)));
    assert!(
        !screen.lines().any(|line| line == expected_line()),
        "old ordering control unexpectedly preserved new geometry"
    );
}

#[test]
#[ignore = "explicit bounded native resize acceptance entry in CI"]
fn resize_observation_backend_refusal_preserves_state() {
    let (mut session, _) = session(Mode::Refuse);
    let before = {
        let s = session.shared.lock().unwrap();
        (s.parser.screen().size(), s.parser.screen().contents())
    };
    assert!(session.resize(24, 80).is_err());
    let after = {
        let s = session.shared.lock().unwrap();
        (s.parser.screen().size(), s.parser.screen().contents())
    };
    assert_eq!(before, after);
}

#[test]
#[ignore = "explicit bounded native resize acceptance entry in CI"]
fn resize_observation_native_output_backpressure_finishes() {
    let (mut session, _) = session(Mode::Backpressure);
    session.resize(24, 80).unwrap();
    session
        .expect_within("PTY_ORDER_FLOOD_DONE", Duration::from_secs(5))
        .unwrap();
}

#[test]
#[ignore = "isolated PTY fixture child, invoked by its owning native control"]
fn pty_fixture_child() {
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        #[link(name = "kernel32")]
        extern "system" {
            fn GetStdHandle(kind: u32) -> *mut c_void;
            fn GetConsoleMode(handle: *mut c_void, mode: *mut u32) -> i32;
            fn SetConsoleMode(handle: *mut c_void, mode: u32) -> i32;
        }
        let mut mode = 0;
        // SAFETY: this child owns its stdout handle and passes valid storage.
        unsafe {
            let output = GetStdHandle(-11i32 as u32);
            assert_ne!(GetConsoleMode(output, &mut mode), 0);
            assert_ne!(SetConsoleMode(output, mode | 4), 0);
        }
    }
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    out.write_all(b"\x1b[2J\x1b[HPTY_ORDER_READY\n").unwrap();
    out.flush().unwrap();
    let stdin = std::io::stdin();
    let mut line = String::new();
    while stdin.read_line(&mut line).unwrap() != 0 {
        match line.trim() {
            "paint" => out
                .write_all(b"\x1b[2J\x1b[2;33HGEOMETRY_ORDER_SENTINEL")
                .unwrap(),
            "flood" => {
                for _ in 0..64 {
                    out.write_all(&[b'X'; 4096]).unwrap();
                }
                out.write_all(b"\nPTY_ORDER_FLOOD_DONE\n").unwrap();
            }
            _ => {}
        }
        out.flush().unwrap();
        line.clear();
    }
}
