//! One owner for complete native PTY creation, process reaping and handle
//! release. Sessions keep independent process execution and terminal I/O.

use std::io::{Read, Write};
use std::sync::MutexGuard;
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use crate::error::{Error, Result};

pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(10);
const REAPER_BUDGET: Duration = Duration::from_millis(500);

pub(crate) struct Spawned {
    pub child: Box<dyn Child + Send + Sync>,
    pub master: Resource<Box<dyn MasterPty + Send>>,
    pub reader: Resource<Box<dyn Read + Send>>,
    pub writer: Resource<Box<dyn Write + Send>>,
}

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

pub(crate) fn spawn(command: &str, args: &[&str], size: PtySize) -> Result<Spawned> {
    let _transition = transition();
    let pair = native_pty_system()
        .openpty(size)
        .map_err(|error| Error::PtyOpen(error.to_string()))?;
    let mut builder = CommandBuilder::new(command);
    for arg in args {
        builder.arg(arg);
    }
    let mut child = pair
        .slave
        .spawn_command(builder)
        .map_err(|error| Error::Spawn(error.to_string()))?;
    let streams: Result<_> = (|| {
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| Error::PtyOpen(error.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| Error::PtyOpen(error.to_string()))?;
        Ok((reader, writer))
    })();
    let (reader, writer) = match streams {
        Ok(streams) => streams,
        Err(error) => {
            let _ = terminate(child.as_mut());
            return Err(error);
        }
    };
    drop(pair.slave);
    Ok(Spawned {
        child,
        master: own(pair.master),
        reader: own(reader),
        writer: own(writer),
    })
}

/// Reap within the existing cleanup budget while the caller still owns the
/// terminal. A cached exit must never send a signal to a recycled PID.
pub(crate) fn terminate(child: &mut (dyn Child + Send + Sync)) -> Instant {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return Instant::now() + REAPER_BUDGET;
    }
    let _ = child.kill();
    let deadline = Instant::now() + REAPER_BUDGET;
    while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
        thread::sleep(POLL_INTERVAL);
    }
    deadline
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
