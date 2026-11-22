use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use crate::storage::models::{ModelQuota, QuotaBucket, QuotaGroup, QuotaSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum QuotaRefreshPolicy {
    #[default]
    UseCache,
    ForceRefresh,
}

impl From<bool> for QuotaRefreshPolicy {
    fn from(force_refresh: bool) -> Self {
        if force_refresh {
            Self::ForceRefresh
        } else {
            Self::UseCache
        }
    }
}

impl QuotaRefreshPolicy {
    pub fn is_force_refresh(self) -> bool {
        matches!(self, Self::ForceRefresh)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountQuotaResult {
    pub email: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub tier: String,
    #[serde(default)]
    pub is_current_ide_session: bool,
    #[serde(default)]
    pub is_current_desktop_session: bool,
    #[serde(default)]
    pub groups: Vec<QuotaGroup>,
    #[serde(default)]
    pub models: Vec<ModelQuota>,
    #[serde(default)]
    pub checked_at: f64,
    #[serde(default)]
    pub is_cached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountVelocity {
    pub email: String,
    pub current_quota_pct: f64,
    pub velocity_pct_per_hour: f64,
    pub quota_consumed_in_window: f64,
    pub window_seconds: f64,
    pub datapoints: usize,
    pub pace_status: String,
    pub pace_icon: String,
    pub pace_style: String,
    pub eta_seconds: Option<f64>,
    pub eta_human: String,
    pub depletion_timestamp: Option<f64>,
    pub depletion_time_str: String,
    pub is_depleting: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MultiwindowVelocity {
    pub email: String,
    pub primary: AccountVelocity,
    pub windows: HashMap<String, AccountVelocity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PoolBurnrates {
    pub active_email: Option<String>,
    pub active_metrics: Option<MultiwindowVelocity>,
    pub accounts: HashMap<String, MultiwindowVelocity>,
    pub snapshot_count: usize,
}
