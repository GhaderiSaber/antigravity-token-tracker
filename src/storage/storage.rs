use crate::storage::models::{AccountProfile, AccountsRegistry};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

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

pub fn default_config_dir() -> PathBuf {
    dirs::home_dir().map_or_else(
        || PathBuf::from(".config/antigravity-token-tracker"),
        |h| h.join(".config").join("antigravity-token-tracker"),
    )
}

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

pub struct StorageManager {
    config_dir: PathBuf,
}

impl Default for StorageManager {
    fn default() -> Self {
        Self::new(default_config_dir())
    }
}

impl StorageManager {
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: config_dir.into(),
        }
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn accounts_file(&self) -> PathBuf {
        self.config_dir.join("accounts.json")
    }

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

    pub fn load_accounts(&self) -> Result<AccountsRegistry> {
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

    pub fn save_accounts(&self, registry: &AccountsRegistry) -> Result<()> {
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

    pub fn get_account(&self, email: &str) -> Result<Option<AccountProfile>> {
        let registry = self.load_accounts()?;
        let key = email.trim().to_lowercase();
        Ok(registry.accounts.get(&key).cloned())
    }

    pub fn list_accounts(&self) -> Result<Vec<AccountProfile>> {
        let registry = self.load_accounts()?;
        let mut list: Vec<AccountProfile> = registry.accounts.into_values().collect();
        list.sort_by(|a, b| a.email.cmp(&b.email));
        Ok(list)
    }

    pub fn upsert_account(&self, mut account: AccountProfile) -> Result<AccountProfile> {
        let email = account.email.trim().to_lowercase();
        if email.is_empty() {
            return Err(StorageError::InvalidData(
                "Account must contain a valid email".into(),
            ));
        }
        account.email.clone_from(&email);

        let mut registry = self.load_accounts()?;
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
        self.save_accounts(&registry)?;
        Ok(account)
    }

    pub fn remove_account(&self, identifier: &str) -> Result<Option<AccountProfile>> {
        let mut registry = self.load_accounts()?;
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
                self.save_accounts(&registry)?;
                return Ok(removed);
            }
        }

        // 2. Direct email match
        let key = trimmed.to_lowercase();
        if let Some(removed) = registry.accounts.remove(&key) {
            if registry.active_email.as_deref() == Some(&key) {
                registry.active_email = None;
            }
            self.save_accounts(&registry)?;
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
            self.save_accounts(&registry)?;
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

pub fn get_account(email: &str) -> Result<Option<AccountProfile>> {
    StorageManager::default().get_account(email)
}

pub fn remove_account(identifier: &str) -> Result<Option<AccountProfile>> {
    StorageManager::default().remove_account(identifier)
}
