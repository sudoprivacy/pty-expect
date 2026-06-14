//! Read-only view over the VT100-rendered terminal screen produced by
//! everything the child has written so far.
//!
//! Pass a closure to [`PtySession::render`](crate::PtySession::render)
//! to inspect the current screen — its full contents, the cursor
//! position, individual cell attributes, and so on. This is the right
//! layer to assert against when checking "what the user actually
//! sees", since it accounts for cursor movement, screen clearing,
//! scrollback, and colour / attribute escape sequences.

/// A rendered terminal screen.
///
/// Borrows the underlying [`vt100::Screen`] held by the session's
/// parser. Methods here cover the common assertion needs; for anything
/// not exposed you can call [`Screen::raw`] to drop down to the
/// underlying `vt100::Screen` directly.
pub struct Screen<'a> {
    inner: &'a vt100::Screen,
}

impl<'a> Screen<'a> {
    pub(crate) fn from_vt(inner: &'a vt100::Screen) -> Self {
        Self { inner }
    }

    /// Underlying `vt100::Screen`, for any inspection not exposed here
    /// (cell-level attributes, scrollback, formatted output, etc.).
    pub fn raw(&self) -> &'a vt100::Screen {
        self.inner
    }

    /// `(rows, cols)` — the screen geometry.
    pub fn size(&self) -> (u16, u16) {
        self.inner.size()
    }

    /// The full screen contents as a UTF-8 string, with line breaks
    /// between rows. Trailing whitespace on each row is preserved.
    pub fn contents(&self) -> String {
        self.inner.contents()
    }

    /// `(row, col)` of the cursor (0-indexed).
    pub fn cursor(&self) -> (u16, u16) {
        self.inner.cursor_position()
    }

    /// `true` if the rendered screen contents contain `needle`.
    pub fn contains(&self, needle: &str) -> bool {
        self.contents().contains(needle)
    }

    /// The character at `(row, col)`, or `None` if the cell is empty or
    /// out of bounds.
    pub fn char_at(&self, row: u16, col: u16) -> Option<String> {
        self.inner.cell(row, col).map(|c| c.contents().to_string())
    }
}
