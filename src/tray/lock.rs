//! Single-instance lockfile and typed error definitions for the system tray service.

use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Top-level error type returned by the system tray runner.
#[must_use]
#[derive(Debug, thiserror::Error)]
pub enum TrayError {
    /// Failed to acquire the single-instance tray lockfile.
    #[error("tray lock error: {0}")]
    Lock(#[from] TrayLockError),
    /// Failed to register or update the D-Bus StatusNotifierItem service.
    #[error("D-Bus tray service error: {0}")]
    Dbus(#[from] ksni::Error),
    /// Failed due to an underlying filesystem or I/O error.
    #[error("tray I/O error: {0}")]
    Io(#[from] io::Error),
}

/// Errors produced when acquiring a [`TrayLock`].
#[must_use]
#[derive(Debug, thiserror::Error)]
pub enum TrayLockError {
    /// Underlying filesystem error while opening, locking, or writing the lockfile.
    #[error("I/O error accessing lockfile: {0}")]
    Io(#[from] io::Error),
    /// Another tray instance already holds the exclusive lock on the lockfile.
    #[error("Another instance is already running (PID: {0:?})")]
    AlreadyRunning(Option<u32>),
}

/// Returns the default path for the tray singleton lockfile.
#[must_use]
pub fn default_lock_path() -> PathBuf {
    dirs::config_dir().map_or_else(
        || {
            PathBuf::from(".config")
                .join("antigravity-token-tracker")
                .join("tray.lock")
        },
        |c| c.join("antigravity-token-tracker").join("tray.lock"),
    )
}

/// Exclusive advisory lock ensuring only one tray indicator runs at a time.
///
/// Dropping the lock releases the OS file lock without unlinking the lockfile,
/// preventing inode races between concurrent contenders.
#[derive(Debug)]
pub struct TrayLock {
    _file: File,
    /// Filesystem path of the locked file.
    pub path: PathBuf,
}

impl TrayLock {
    /// Acquires an exclusive non-blocking lock on `custom_path` or [`default_lock_path()`].
    ///
    /// # Errors
    ///
    /// Returns [`TrayLockError::AlreadyRunning`] if another process holds the lock,
    /// or [`TrayLockError::Io`] if creating parent directories, opening, or updating
    /// the lockfile fails.
    pub fn acquire(custom_path: Option<&Path>) -> Result<Self, TrayLockError> {
        let path = custom_path.map_or_else(default_lock_path, Path::to_path_buf);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        match file.try_lock_exclusive() {
            Ok(()) => {
                file.set_len(0)?;
                file.seek(SeekFrom::Start(0))?;
                let pid = std::process::id();
                writeln!(file, "{pid}")?;
                file.flush()?;
                Ok(Self { _file: file, path })
            }
            Err(_) => {
                let _ = file.seek(SeekFrom::Start(0));
                let mut content = String::new();
                let _ = file.read_to_string(&mut content);
                let existing_pid = content.trim().parse::<u32>().ok();
                Err(TrayLockError::AlreadyRunning(existing_pid))
            }
        }
    }
}

impl Drop for TrayLock {
    fn drop(&mut self) {
        // Unlock without unlinking so concurrent contenders always lock the same inode.
        let _ = FileExt::unlock(&self._file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tray_lock_error_display() {
        let err = TrayLockError::AlreadyRunning(Some(1234));
        assert_eq!(
            err.to_string(),
            "Another instance is already running (PID: Some(1234))"
        );

        let err_none = TrayLockError::AlreadyRunning(None);
        assert_eq!(
            err_none.to_string(),
            "Another instance is already running (PID: None)"
        );

        let io_err = TrayLockError::Io(io::Error::new(io::ErrorKind::NotFound, "file not found"));
        assert!(io_err.to_string().contains("I/O error accessing lockfile"));
    }

    #[test]
    fn test_default_lock_path() {
        let path = default_lock_path();
        assert!(
            path.to_string_lossy()
                .ends_with("antigravity-token-tracker/tray.lock")
        );
    }

    #[test]
    fn test_tray_error_variants() {
        let lock_err = TrayError::from(TrayLockError::AlreadyRunning(Some(42)));
        assert!(lock_err.to_string().contains("42"));

        let io_err = TrayError::from(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        assert!(io_err.to_string().contains("denied"));
    }

    #[test]
    fn test_tray_lock_contends_reads_pid_and_reacquires_after_drop() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("tray.lock");

        let lock1 = TrayLock::acquire(Some(&lock_path)).unwrap();
        let lock2 = TrayLock::acquire(Some(&lock_path));
        match lock2 {
            Err(TrayLockError::AlreadyRunning(Some(pid))) => {
                assert_eq!(pid, std::process::id());
            }
            other => panic!("expected AlreadyRunning(Some(self_pid)), got {other:?}"),
        }

        drop(lock1);
        assert!(
            lock_path.exists(),
            "lockfile stays on disk to avoid unlink race"
        );
        let lock3 = TrayLock::acquire(Some(&lock_path));
        assert!(lock3.is_ok());
    }
}
