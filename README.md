# pty-expect

Drive interactive CLI programs through a real PTY (Unix `/dev/pty` or
Windows ConPTY) and assert against either the raw byte stream or a
VT100-rendered view of *what the user would actually see*.

Built for end-to-end testing of REPL-style command-line tools where
piped-stdin / piped-stdout testing leaves too much behaviour
unexercised: tab completion, ANSI redraw, slash commands rendered into
a status line, signal handling (`Ctrl+C`, `Ctrl+D`), terminal resize,
and so on.

## What's in the box

```
┌──────────────────────────────────────────────────────────────┐
│  Your tests                                                  │
│                                                              │
│      let mut sess = PtySession::spawn("my-cli", &[])?;       │
│      sess.expect(r"prompt> ")?;                              │
│      sess.send_line("/help")?;                               │
│      sess.expect(r"Available commands")?;                    │
│      sess.send_ctrl('d')?;                                   │
│      sess.expect_eof()?;                                     │
│                                                              │
└──────────────────────────────────────────────────────────────┘
                            │
                            ▼  pty-expect (this crate, ~350 LOC)
                            │   thin: spawn / send / expect / render
                            │
              ┌─────────────┴─────────────┐
              ▼                           ▼
      ┌──────────────┐            ┌──────────────┐
      │ portable-pty │            │    vt100     │
      │  (wezterm)   │            │    (doy)     │
      │              │            │              │
      │ Real PTY     │            │ Interprets   │
      │ on Unix +    │            │ ANSI escape  │
      │ Windows      │            │ sequences as │
      │ ConPTY       │            │ a row × col  │
      │              │            │ cell grid    │
      └──────────────┘            └──────────────┘
```

The two leaf libraries do the platform-fiddly work. This crate adds
the expect / render API on top.

`vt100` is pinned at our fork at
[`sudoprivacy/vt100-rust`](https://github.com/sudoprivacy/vt100-rust)
so we control patches end-to-end. `portable-pty` is taken from the
upstream `wezterm` crate; if a Windows ConPTY quirk surfaces that the
upstream maintainers do not prioritise, the fork lives a `gh repo
fork` away.

## Quick start

```toml
[dev-dependencies]
pty-expect = { git = "https://github.com/sudoprivacy/pty-expect" }
```

```rust
use std::time::Duration;
use pty_expect::PtySession;

fn drive_echo() -> Result<(), pty_expect::Error> {
    let mut sess = PtySession::spawn("sh", &["-c", "echo hello && cat"])?;
    sess.set_default_timeout(Duration::from_secs(5));

    // Match against the raw PTY byte stream.
    sess.expect(r"hello")?;

    // Send a line of input as if the user typed it.
    sess.send_line("world")?;
    sess.expect(r"world")?;

    // Close stdin with Ctrl+D, wait for the child to exit cleanly.
    sess.send_ctrl('d')?;
    let exit_code = sess.expect_eof()?;
    assert_eq!(exit_code, 0);

    Ok(())
}
```

## Asserting against the rendered screen

Use `PtySession::spawn_with_size(command, args, rows, cols)` when startup must
observe a specific terminal size. It creates both the native PTY and the
VT100 screen at that size before launching the child; resizing after `spawn`
can race the child's first geometry query. Ordinary `spawn` keeps its 24x80
default. Later `resize(rows, cols)` calls still exercise a running child.

For checks that depend on cursor position, colour, or "what cell
contains what character" — say, a status line that overwrites itself
every turn — drop down to the VT100-rendered view:

```rust
sess.send_line("/status")?;
sess.expect(r"scode>")?;        // wait for the prompt to come back

sess.render(|screen| {
    // The rendered contents are what a user would see in their
    // terminal — ANSI escapes have already been applied.
    assert!(screen.contents().contains("permission: workspace-write"));

    // Cursor is back at column 0 of the new prompt line.
    let (_, col) = screen.cursor();
    assert_eq!(col, 0);
});
```

## Platforms

CI runs `cargo fmt`, `cargo clippy --all-targets`, and `cargo test
--all-targets` on macOS, Ubuntu Linux, and Windows. The PTY backend
is `portable-pty`, so the same code is built on all three.

On macOS, one shared lifecycle lock coordinates the complete native creation
transition (allocation, child attachment and stream setup) and release of master,
slave, reader and writer handles. Process execution and terminal I/O remain
concurrent. Teardown reaps a terminated child before releasing the terminal,
sharing the existing 500 ms cleanup budget with the reader's final drain. An
already reaped child is never signalled again.

The native-shell integration drives 16 callers through 4,000 sessions, including
input/output, normal exit and dropping a running child. Another real-process
case requires teardown to reap a child that ignores SIGHUP. macOS CI additionally
runs both cases in four fresh controllers; every controller must pass, and the
first failure stops the job.

### Native terminal handshake

The reader answers cursor-position queries (`CSI 6 n`) and primary device
attributes (`CSI c` / `CSI 0 c`) in order, once per request, including requests
split across reads. DA1 reports a basic VT100 without advanced features.
This completes ConPTY startup without its fixed three-second DA1 wait.
See [Microsoft's ConPTY startup sequence](https://github.com/microsoft/terminal/blob/v1.25.1171.0/src/host/VtIo.cpp#L179-L209).

All three CI platforms run real native process tests. Windows covers cmd,
PowerShell, initial geometry, final-output draining and four fresh native
startup processes, each required to become ready within two seconds. Linux
and macOS additionally exercise every split point of cursor/DA1 queries,
batched requests and duplicate-response detection through a real raw PTY.
The existing process cleanup and macOS concurrency checks remain required.

## License

Dual-licensed under [MIT](./LICENSE-MIT) or
[Apache-2.0](./LICENSE-APACHE), at your option.

The bundled `vt100` fork preserves its original MIT license; see
[`sudoprivacy/vt100-rust`](https://github.com/sudoprivacy/vt100-rust)
for that source tree.
