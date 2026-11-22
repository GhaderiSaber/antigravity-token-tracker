use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FailoverState {
    #[serde(default)]
    pub last_failover_timestamp: Option<f64>,
    #[serde(default)]
    pub last_from_email: Option<String>,
    #[serde(default)]
    pub last_to_email: Option<String>,
    #[serde(default)]
    pub all_exhausted_alerted_at: Option<f64>,
    #[serde(default)]
    pub history: Vec<FailoverEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FailoverEvent {
    pub timestamp: f64,
    pub from: String,
    pub to: String,
    pub from_pct: f64,
    pub to_pct: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FailoverDecision {
    pub triggered: bool,
    pub reason: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_quota_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum DiagnosticStatus {
    #[default]
    Pass,
    Warn,
    Fail,
    Info,
}

impl std::fmt::Display for DiagnosticStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pass => write!(f, "PASS"),
            Self::Warn => write!(f, "WARN"),
            Self::Fail => write!(f, "FAIL"),
            Self::Info => write!(f, "INFO"),
        }
    }
}

impl DiagnosticStatus {
    #[must_use]
    pub fn is_pass(self) -> bool {
        matches!(self, Self::Pass)
    }

    #[must_use]
    pub fn is_warn(self) -> bool {
        matches!(self, Self::Warn)
    }

    #[must_use]
    pub fn is_fail(self) -> bool {
        matches!(self, Self::Fail)
    }

    #[must_use]
    pub fn is_info(self) -> bool {
        matches!(self, Self::Info)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticItem {
    pub component: String,
    pub status: DiagnosticStatus,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DoctorReport {
    pub items: Vec<DiagnosticItem>,
    pub overall_status: DiagnosticStatus,
    pub timestamp: f64,
}
