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

### v0.1 status by platform

| Platform | Compile + clippy | Runtime tests |
|---|---|---|
| Linux (Ubuntu) | ✅ | ✅ verified |
| macOS | ✅ | ✅ verified |
| Windows | ✅ | ⚠️ deferred to v0.2 |

The Windows compile path is exercised by CI on every commit. The
Windows runtime path against ConPTY needs focused diagnosis — early
attempts to drive a child via `cmd /C echo`, then `powershell
Write-Host`, then `powershell Write-Output` each failed with `expect`
never seeing the child's output, which points at something deeper
than command choice. Rather than keep patching the test, we are
tracking the runtime verification in
<https://github.com/sudoprivacy/pty-expect/issues/1>.

## License

Dual-licensed under [MIT](./LICENSE-MIT) or
[Apache-2.0](./LICENSE-APACHE), at your option.

The bundled `vt100` fork preserves its original MIT license; see
[`sudoprivacy/vt100-rust`](https://github.com/sudoprivacy/vt100-rust)
for that source tree.
