use fs2::FileExt;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum TrayLockError {
    #[error("I/O error accessing lockfile: {0}")]
    Io(#[from] std::io::Error),
    #[error("Another instance is already running (PID: {0:?})")]
    AlreadyRunning(Option<u32>),
}

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

pub struct TrayLock {
    _file: std::fs::File,
    pub path: PathBuf,
}

impl TrayLock {
    pub fn acquire(custom_path: Option<&Path>) -> Result<Self, TrayLockError> {
        let path = match custom_path {
            Some(p) => p.to_path_buf(),
            None => default_lock_path(),
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        match file.try_lock_exclusive() {
            Ok(()) => {
                // We got the lock! Truncate and write PID
                let _ = file.set_len(0);
                let _ = file.seek(SeekFrom::Start(0));
                let pid = std::process::id();
                let _ = writeln!(file, "{pid}");
                let _ = file.flush();
                Ok(TrayLock { _file: file, path })
            }
            Err(_) => {
                // Another process holds the lock
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
        let _ = fs2::FileExt::unlock(&self._file);
        let _ = std::fs::remove_file(&self.path);
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

        let io_err = TrayLockError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));
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
}
