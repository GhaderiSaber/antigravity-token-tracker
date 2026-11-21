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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticItem {
    pub component: String,
    pub status: String, // "PASS", "WARN", "FAIL"
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DoctorReport {
    pub items: Vec<DiagnosticItem>,
    pub overall_status: String, // "PASS", "WARN", "FAIL"
    pub timestamp: f64,
}
