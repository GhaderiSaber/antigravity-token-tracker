use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelQuota {
    #[serde(rename = "modelId")]
    pub model_id: String,
    #[serde(default, rename = "remainingFraction")]
    pub remaining_fraction: Option<f64>,
    #[serde(default, rename = "remainingPercent")]
    pub remaining_percent: Option<f64>,
    #[serde(default, rename = "resetTime")]
    pub reset_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct QuotaBucket {
    #[serde(default, rename = "bucketId")]
    pub bucket_id: Option<String>,
    #[serde(default)]
    pub window: Option<String>,
    #[serde(default, rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(default, rename = "remainingFraction")]
    pub remaining_fraction: Option<f64>,
    #[serde(default, rename = "remainingPercent")]
    pub remaining_percent: Option<f64>,
    #[serde(default, rename = "resetTime")]
    pub reset_time: Option<String>,
    #[serde(default)]
    pub description: Option<String>,

    // Fields populated by lifecycle analysis
    #[serde(default)]
    pub countdown: Option<String>,
    #[serde(default, rename = "secondsRemaining")]
    pub seconds_remaining: Option<f64>,
    #[serde(default, rename = "statusBadge")]
    pub status_badge: Option<String>,
    #[serde(default, rename = "statusStyle")]
    pub status_style: Option<String>,
    #[serde(default, rename = "isExhausted")]
    pub is_exhausted: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct QuotaGroup {
    #[serde(default, rename = "displayName")]
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub weekly: Option<QuotaBucket>,
    #[serde(default, rename = "fiveHour")]
    pub five_hour: Option<QuotaBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct QuotaSnapshot {
    #[serde(default, rename = "checked_at")]
    pub checked_at: Option<f64>,
    #[serde(default)]
    pub groups: Vec<QuotaGroup>,
    #[serde(default)]
    pub models: Vec<ModelQuota>,
    #[serde(default, rename = "is_cached")]
    pub is_cached: Option<bool>,
    #[serde(default, rename = "is_gemini_exhausted")]
    pub is_gemini_exhausted: Option<bool>,
    #[serde(default, rename = "is_3p_exhausted")]
    pub is_3p_exhausted: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AccountProfile {
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub picture: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub token_expires_in: Option<i64>,
    #[serde(default)]
    pub token_timestamp: Option<f64>,
    #[serde(default)]
    pub tier: Option<String>,
    #[serde(default)]
    pub added_at: Option<f64>,
    #[serde(default)]
    pub last_used: Option<f64>,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub is_current_ide_session: Option<bool>,
    #[serde(default)]
    pub is_current_desktop_session: Option<bool>,
    #[serde(default)]
    pub last_synced: Option<f64>,
    #[serde(default)]
    pub last_known_quota: Option<QuotaSnapshot>,
    #[serde(default)]
    pub app_port: Option<u16>,
    #[serde(default)]
    pub updated_at: Option<f64>,
    #[serde(default)]
    pub app_csrf_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AccountsRegistry {
    pub active_email: Option<String>,
    pub accounts: HashMap<String, AccountProfile>,
}

// Custom serializer and deserializer to support both v1 (flat dict) and v2 ({"active_email": ..., "accounts": ...}) schemas
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum AccountsFileRepresentation {
    V2 {
        #[serde(default)]
        active_email: Option<String>,
        accounts: HashMap<String, AccountProfile>,
    },
    V1(HashMap<String, AccountProfile>),
}

impl Serialize for AccountsRegistry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let repr = AccountsFileRepresentation::V2 {
            active_email: self.active_email.clone(),
            accounts: self.accounts.clone(),
        };
        repr.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AccountsRegistry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let repr = AccountsFileRepresentation::deserialize(deserializer)?;
        match repr {
            AccountsFileRepresentation::V2 {
                active_email,
                accounts,
            } => Ok(AccountsRegistry {
                active_email,
                accounts,
            }),
            AccountsFileRepresentation::V1(accounts) => Ok(AccountsRegistry {
                active_email: None,
                accounts,
            }),
        }
    }
}
