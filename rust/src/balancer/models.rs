use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BalancerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default = "default_watermark_spread")]
    pub watermark_spread_pct: f64,
    #[serde(default = "default_round_robin_interval")]
    pub round_robin_interval_minutes: u64,
    #[serde(default = "default_expiry_window")]
    pub expiry_window_hours: u64,
    #[serde(default = "default_min_quota")]
    pub min_quota_pct: f64,
    #[serde(default = "default_cooldown")]
    pub cooldown_seconds: u64,
    #[serde(default = "default_auto_restart")]
    pub auto_restart: bool,
}

fn default_strategy() -> String {
    "watermark".to_string()
}
fn default_watermark_spread() -> f64 {
    20.0
}
fn default_round_robin_interval() -> u64 {
    60
}
fn default_expiry_window() -> u64 {
    36
}
fn default_min_quota() -> f64 {
    5.0
}
fn default_cooldown() -> u64 {
    300
}
fn default_auto_restart() -> bool {
    true
}

impl Default for BalancerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            strategy: default_strategy(),
            watermark_spread_pct: default_watermark_spread(),
            round_robin_interval_minutes: default_round_robin_interval(),
            expiry_window_hours: default_expiry_window(),
            min_quota_pct: default_min_quota(),
            cooldown_seconds: default_cooldown(),
            auto_restart: default_auto_restart(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BalancerState {
    #[serde(default)]
    pub last_rotation_timestamp: Option<f64>,
    #[serde(default)]
    pub last_from_email: Option<String>,
    #[serde(default)]
    pub last_to_email: Option<String>,
    #[serde(default)]
    pub rotation_count: u64,
    #[serde(default)]
    pub round_robin_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SessionSnapshotMeta {
    pub email: String,
    pub snapshotted_at: f64,
    #[serde(default)]
    pub has_desktop_session: bool,
    #[serde(default)]
    pub has_keyring_token: bool,
    #[serde(default)]
    pub has_ide_session: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SwitchResult {
    pub success: bool,
    pub message: String,
    pub from_email: Option<String>,
    pub to_email: String,
}
