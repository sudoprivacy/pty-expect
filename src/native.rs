//! One owner for native PTY allocation and handle release. Sessions keep
//! independent process execution and terminal I/O.

use std::sync::MutexGuard;

use portable_pty::{native_pty_system, PtyPair, PtySize};

use crate::error::{Error, Result};

fn transition() -> Option<MutexGuard<'static, ()>> {
    #[cfg(target_os = "macos")]
    {
        static LIFECYCLE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        Some(
            LIFECYCLE
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

pub(crate) fn allocate(size: PtySize) -> Result<PtyPair> {
    let _transition = transition();
    native_pty_system()
        .openpty(size)
        .map_err(|error| Error::PtyOpen(error.to_string()))
}

#[cfg(target_os = "macos")]
pub(crate) struct Resource<T>(Option<T>);

#[cfg(not(target_os = "macos"))]
pub(crate) type Resource<T> = T;

pub(crate) fn own<T>(value: T) -> Resource<T> {
    #[cfg(target_os = "macos")]
    {
        Resource(Some(value))
    }
    #[cfg(not(target_os = "macos"))]
    {
        value
    }
}

#[cfg(target_os = "macos")]
impl<T> std::ops::Deref for Resource<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0.as_ref().expect("native resource already released")
    }
}

#[cfg(target_os = "macos")]
impl<T> std::ops::DerefMut for Resource<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.0.as_mut().expect("native resource already released")
    }
}

#[cfg(target_os = "macos")]
impl<T> Drop for Resource<T> {
    fn drop(&mut self) {
        let _transition = transition();
        drop(self.0.take());
    }
}
