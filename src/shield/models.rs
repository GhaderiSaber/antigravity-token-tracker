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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShieldConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_mode")]
    pub mode: String, // "restricted" or "whitelist"
    #[serde(default = "default_allowed_countries")]
    pub allowed_countries: Vec<String>,
    #[serde(default = "default_action")]
    pub action: String, // "kill_process", "kill_network", or "both"
    #[serde(default)]
    pub auto_relaunch: bool,
    #[serde(default = "default_cooldown")]
    pub cooldown_seconds: u64,
}

fn default_true() -> bool {
    true
}

fn default_mode() -> String {
    "restricted".into()
}

fn default_action() -> String {
    "kill_process".into()
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
            mode: default_mode(),
            allowed_countries: default_allowed_countries(),
            action: default_action(),
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
