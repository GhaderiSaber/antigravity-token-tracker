use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BalancerStrategy {
    #[default]
    Watermark,
    ExpiryFirst,
    RoundRobin,
    Reactive,
}

impl std::fmt::Display for BalancerStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Watermark => write!(f, "watermark"),
            Self::ExpiryFirst => write!(f, "expiry_first"),
            Self::RoundRobin => write!(f, "round_robin"),
            Self::Reactive => write!(f, "reactive"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error)]
#[error(
    "Invalid balancer strategy: '{0}'. Valid strategies are: watermark, expiry_first, round_robin, reactive"
)]
pub struct ParseBalancerStrategyError(pub String);

impl std::str::FromStr for BalancerStrategy {
    type Err = ParseBalancerStrategyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().replace('-', "_").as_str() {
            "watermark" => Ok(Self::Watermark),
            "expiry_first" | "expiry" => Ok(Self::ExpiryFirst),
            "round_robin" | "roundrobin" => Ok(Self::RoundRobin),
            "reactive" => Ok(Self::Reactive),
            _ => Err(ParseBalancerStrategyError(s.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BalancerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub strategy: BalancerStrategy,
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
            strategy: BalancerStrategy::default(),
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

impl From<crate::balancer::switcher::SwitchOutcome> for SwitchResult {
    fn from(outcome: crate::balancer::switcher::SwitchOutcome) -> Self {
        match outcome {
            crate::balancer::switcher::SwitchOutcome::AlreadyActive { email } => Self {
                success: true,
                message: format!("Account '{email}' is already the active session."),
                from_email: None,
                to_email: email,
            },
            crate::balancer::switcher::SwitchOutcome::Switched { from, to, .. } => Self {
                success: true,
                message: format!("Successfully switched active session to '{to}'."),
                from_email: from,
                to_email: to,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SnapshotResult {
    pub desktop_saved: bool,
    pub ide_saved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RestartProcess {
    #[default]
    RestartIfRunning,
    DoNotRestart,
}

impl From<bool> for RestartProcess {
    fn from(restart: bool) -> Self {
        if restart {
            Self::RestartIfRunning
        } else {
            Self::DoNotRestart
        }
    }
}

impl RestartProcess {
    #[must_use]
    pub fn should_restart(self) -> bool {
        matches!(self, Self::RestartIfRunning)
    }
}
