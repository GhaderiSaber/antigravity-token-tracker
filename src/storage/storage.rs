use crate::storage::models::{AccountProfile, AccountsRegistry};
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors returned by [`StorageManager`] operations.
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON serialization/deserialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid account data: {0}")]
    InvalidData(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// Returns the default configuration directory using XDG config home when available.
#[must_use]
pub fn default_config_dir() -> PathBuf {
    dirs::config_dir().map_or_else(
        || PathBuf::from(".config").join("antigravity-token-tracker"),
        |c| c.join("antigravity-token-tracker"),
    )
}

#[must_use]
pub fn default_history_file() -> PathBuf {
    dirs::home_dir().map_or_else(
        || PathBuf::from(".gemini/antigravity/token_history.jsonl"),
        |h| {
            h.join(".gemini")
                .join("antigravity")
                .join("token_history.jsonl")
        },
    )
}

#[derive(Debug)]
struct AccountsLockGuard(File);

impl Drop for AccountsLockGuard {
    fn drop(&mut self) {
        // Unlock without unlinking so concurrent contenders always lock the same inode.
        let _ = FileExt::unlock(&self.0);
    }
}

#[derive(Debug, Clone, Copy)]
enum LockMode {
    Shared,
    Exclusive,
}

#[derive(Debug, Clone)]
pub struct StorageManager {
    config_dir: PathBuf,
}

/// Idiomatic alias for [`StorageManager`] to omit the Manager suffix.
pub type Storage = StorageManager;

impl Default for StorageManager {
    fn default() -> Self {
        Self::new(default_config_dir())
    }
}

impl StorageManager {
    #[must_use]
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: config_dir.into(),
        }
    }

    #[must_use]
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    #[must_use]
    pub fn accounts_file(&self) -> PathBuf {
        self.config_dir.join("accounts.json")
    }

    /// Returns the path to the advisory lockfile coordinating access to `accounts.json`.
    #[must_use]
    pub fn accounts_lock_file(&self) -> PathBuf {
        self.config_dir.join(".accounts.lock")
    }

    /// Ensures `self.config_dir` exists on disk with restrictive permissions on Unix.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if directory creation fails.
    pub fn ensure_config_dir(&self) -> Result<()> {
        if !self.config_dir.exists() {
            fs::create_dir_all(&self.config_dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&self.config_dir, fs::Permissions::from_mode(0o700));
            }
        }
        Ok(())
    }

    fn acquire_lock(&self, mode: LockMode) -> Result<AccountsLockGuard> {
        self.ensure_config_dir()?;
        let lock_path = self.accounts_lock_file();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o600));
        }

        match mode {
            LockMode::Shared => FileExt::lock_shared(&file)?,
            LockMode::Exclusive => FileExt::lock_exclusive(&file)?,
        }

        Ok(AccountsLockGuard(file))
    }

    fn load_accounts_unlocked(&self) -> Result<AccountsRegistry> {
        let file_path = self.accounts_file();
        if !file_path.exists() {
            return Ok(AccountsRegistry::default());
        }

        let content = fs::read_to_string(&file_path)?;
        if content.trim().is_empty() {
            return Ok(AccountsRegistry::default());
        }

        let registry: AccountsRegistry = serde_json::from_str(&content)?;
        Ok(registry)
    }

    fn save_accounts_unlocked(&self, registry: &AccountsRegistry) -> Result<()> {
        self.ensure_config_dir()?;
        let target_file = self.accounts_file();
        let tmp_file = target_file.with_extension("tmp");

        let json_data = serde_json::to_string_pretty(registry)?;
        {
            let mut file = File::create(&tmp_file)?;
            file.write_all(json_data.as_bytes())?;
            file.flush()?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&tmp_file, fs::Permissions::from_mode(0o600));
            }
        }

        fs::rename(&tmp_file, &target_file)?;
        Ok(())
    }

    /// Loads the account registry under a shared advisory file lock.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] on lock/read failure or [`StorageError::Json`] if
    /// `accounts.json` contains invalid JSON.
    pub fn load_accounts(&self) -> Result<AccountsRegistry> {
        if !self.config_dir.exists() {
            return Ok(AccountsRegistry::default());
        }
        let _guard = self.acquire_lock(LockMode::Shared)?;
        self.load_accounts_unlocked()
    }

    /// Atomically persists the account registry under an exclusive advisory file lock.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] or [`StorageError::Json`] if locking, serialization,
    /// or atomic rename fails.
    pub fn save_accounts(&self, registry: &AccountsRegistry) -> Result<()> {
        let _guard = self.acquire_lock(LockMode::Exclusive)?;
        self.save_accounts_unlocked(registry)
    }

    pub fn account(&self, email: &str) -> Result<Option<AccountProfile>> {
        let registry = self.load_accounts()?;
        let key = email.trim().to_lowercase();
        Ok(registry.accounts.get(&key).cloned())
    }

    #[inline]
    pub fn get_account(&self, email: &str) -> Result<Option<AccountProfile>> {
        self.account(email)
    }

    pub fn list_accounts(&self) -> Result<Vec<AccountProfile>> {
        let registry = self.load_accounts()?;
        let mut list: Vec<AccountProfile> = registry.accounts.into_values().collect();
        list.sort_by(|a, b| a.email.cmp(&b.email));
        Ok(list)
    }

    /// Inserts or updates an account profile atomically under an exclusive advisory file lock.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidData`] if `account.email` is empty, or
    /// [`StorageError::Io`] / [`StorageError::Json`] if reading or saving fails.
    pub fn upsert_account(&self, mut account: AccountProfile) -> Result<AccountProfile> {
        let email = account.email.trim().to_lowercase();
        if email.is_empty() {
            return Err(StorageError::InvalidData(
                "Account must contain a valid email".into(),
            ));
        }
        account.email.clone_from(&email);

        let _guard = self.acquire_lock(LockMode::Exclusive)?;
        let mut registry = self.load_accounts_unlocked()?;
        let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
        account.updated_at = Some(now);

        if let Some(existing) = registry.accounts.get(&email) {
            // Preserve existing refresh token if new one is empty
            if account
                .refresh_token
                .as_ref()
                .is_none_or(std::string::String::is_empty)
            {
                account.refresh_token.clone_from(&existing.refresh_token);
            }
            if account.tier.is_none() {
                account.tier.clone_from(&existing.tier);
            }
            if account.name.is_none() {
                account.name.clone_from(&existing.name);
            }
        }

        registry.accounts.insert(email, account.clone());
        self.save_accounts_unlocked(&registry)?;
        Ok(account)
    }

    /// Removes an account by 1-based index, exact email, or name/email substring under an exclusive lock.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] or [`StorageError::Json`] if reading or saving fails.
    pub fn remove_account(&self, identifier: &str) -> Result<Option<AccountProfile>> {
        let _guard = self.acquire_lock(LockMode::Exclusive)?;
        let mut registry = self.load_accounts_unlocked()?;
        let trimmed = identifier.trim();

        // 1. Try numeric 1-based index
        if let Ok(num) = trimmed.parse::<usize>() {
            let mut sorted_keys: Vec<String> = registry.accounts.keys().cloned().collect();
            sorted_keys.sort();
            if num >= 1 && num <= sorted_keys.len() {
                let key = &sorted_keys[num - 1];
                let removed = registry.accounts.remove(key);
                if registry.active_email.as_deref() == Some(key) {
                    registry.active_email = None;
                }
                self.save_accounts_unlocked(&registry)?;
                return Ok(removed);
            }
        }

        // 2. Direct email match
        let key = trimmed.to_lowercase();
        if let Some(removed) = registry.accounts.remove(&key) {
            if registry.active_email.as_deref() == Some(&key) {
                registry.active_email = None;
            }
            self.save_accounts_unlocked(&registry)?;
            return Ok(Some(removed));
        }

        // 3. Match by name or substring
        let matching_key = registry.accounts.iter().find_map(|(email, profile)| {
            if let Some(name) = &profile.name
                && name.to_lowercase().contains(&key)
            {
                return Some(email.clone());
            }
            if email.contains(&key) {
                return Some(email.clone());
            }
            None
        });

        if let Some(matched_email) = matching_key {
            let removed = registry.accounts.remove(&matched_email);
            if registry.active_email.as_deref() == Some(&matched_email) {
                registry.active_email = None;
            }
            self.save_accounts_unlocked(&registry)?;
            return Ok(removed);
        }

        Ok(None)
    }
}

pub fn load_accounts() -> Result<AccountsRegistry> {
    StorageManager::default().load_accounts()
}

pub fn save_accounts(registry: &AccountsRegistry) -> Result<()> {
    StorageManager::default().save_accounts(registry)
}

pub fn upsert_account(account: &AccountProfile) -> Result<AccountProfile> {
    StorageManager::default().upsert_account(account.clone())
}

pub fn list_accounts() -> Result<Vec<AccountProfile>> {
    StorageManager::default().list_accounts()
}

pub fn account(email: &str) -> Result<Option<AccountProfile>> {
    StorageManager::default().account(email)
}

#[inline]
pub fn get_account(email: &str) -> Result<Option<AccountProfile>> {
    account(email)
}

pub fn remove_account(identifier: &str) -> Result<Option<AccountProfile>> {
    StorageManager::default().remove_account(identifier)
}
