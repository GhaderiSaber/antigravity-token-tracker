use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EgressGeo {
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub country_code: String,
    #[serde(default)]
    pub country_name: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub isp: String,
    #[serde(default)]
    pub org: String,
    #[serde(default)]
    pub flag: String,
    #[serde(default)]
    pub is_restricted: bool,
    #[serde(default)]
    pub checked_at: f64,
    #[serde(default)]
    pub summary: String,
}

impl Default for EgressGeo {
    fn default() -> Self {
        Self {
            ip: "Unknown".into(),
            country_code: "".into(),
            country_name: "Checking...".into(),
            region: "".into(),
            city: "".into(),
            isp: "".into(),
            org: "".into(),
            flag: "🌐".into(),
            is_restricted: false,
            checked_at: 0.0,
            summary: "🌐 Egress IP Checking...".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum IpDetail {
    #[default]
    WithIp,
    OmitIp,
}

impl From<bool> for IpDetail {
    fn from(include_ip: bool) -> Self {
        if include_ip {
            Self::WithIp
        } else {
            Self::OmitIp
        }
    }
}

impl IpDetail {
    pub fn includes_ip(self) -> bool {
        matches!(self, Self::WithIp)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EgressCachePolicy {
    #[default]
    UseCache,
    ForceRefresh,
}

impl From<bool> for EgressCachePolicy {
    fn from(force_refresh: bool) -> Self {
        if force_refresh {
            Self::ForceRefresh
        } else {
            Self::UseCache
        }
    }
}

impl EgressCachePolicy {
    pub fn is_force_refresh(self) -> bool {
        matches!(self, Self::ForceRefresh)
    }
}

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShieldMode {
    #[default]
    Restricted,
    Whitelist,
}

impl fmt::Display for ShieldMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Restricted => write!(f, "restricted"),
            Self::Whitelist => write!(f, "whitelist"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("unknown shield mode: '{0}', expected 'restricted' or 'whitelist'")]
pub struct ParseShieldModeError(pub String);

impl FromStr for ShieldMode {
    type Err = ParseShieldModeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "restricted" => Ok(Self::Restricted),
            "whitelist" | "white_list" | "white-list" => Ok(Self::Whitelist),
            other => Err(ParseShieldModeError(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShieldAction {
    #[default]
    KillProcess,
    KillNetwork,
    Both,
}

impl fmt::Display for ShieldAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KillProcess => write!(f, "kill_process"),
            Self::KillNetwork => write!(f, "kill_network"),
            Self::Both => write!(f, "both"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("unknown shield action: '{0}', expected 'kill_process', 'kill_network', or 'both'")]
pub struct ParseShieldActionError(pub String);

impl FromStr for ShieldAction {
    type Err = ParseShieldActionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "kill_process" | "kill-process" | "killprocess" => Ok(Self::KillProcess),
            "kill_network" | "kill-network" | "killnetwork" => Ok(Self::KillNetwork),
            "both" => Ok(Self::Both),
            other => Err(ParseShieldActionError(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShieldConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub mode: ShieldMode,
    #[serde(default = "default_allowed_countries")]
    pub allowed_countries: Vec<String>,
    #[serde(default)]
    pub action: ShieldAction,
    #[serde(default)]
    pub auto_relaunch: bool,
    #[serde(default = "default_cooldown")]
    pub cooldown_seconds: u64,
}

fn default_true() -> bool {
    true
}

fn default_cooldown() -> u64 {
    60
}

fn default_allowed_countries() -> Vec<String> {
    vec![
        "US".into(),
        "DE".into(),
        "GB".into(),
        "NL".into(),
        "CA".into(),
        "FR".into(),
        "JP".into(),
        "CH".into(),
    ]
}

impl Default for ShieldConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            mode: ShieldMode::default(),
            allowed_countries: default_allowed_countries(),
            action: ShieldAction::default(),
            auto_relaunch: false,
            cooldown_seconds: default_cooldown(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShieldEvaluation {
    pub enabled: bool,
    pub violation: bool,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub action_taken: Option<String>,
    #[serde(default)]
    pub processes_killed: Option<usize>,
    #[serde(default)]
    pub pids_killed: Option<Vec<i32>>,
    #[serde(default)]
    pub network_killed: Option<bool>,
    #[serde(default)]
    pub geo: Option<EgressGeo>,
}
