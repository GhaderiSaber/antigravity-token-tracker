use fs2::FileExt;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub fn default_lock_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join(".config")
        .join("antigravity-token-tracker")
        .join("tray.lock")
}

pub struct TrayLock {
    _file: std::fs::File,
    pub path: PathBuf,
}

impl TrayLock {
    pub fn acquire(custom_path: Option<&Path>) -> Result<Self, Option<u32>> {
        let path = match custom_path {
            Some(p) => p.to_path_buf(),
            None => default_lock_path(),
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let Ok(mut file) = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
        else {
            return Err(None);
        };

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
                Err(existing_pid)
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
