//! # Antigravity Token & Quota Lifecycle Tracker
//!
//! A high-performance, asynchronous Rust service and CLI daemon for monitoring,
//! balancing, and safeguarding Google Antigravity developer accounts, OAuth sessions,
//! and model rate-limit quotas.
//!
//! ## Architectural Overview
//!
//! - [`auth`]: Process discovery, IDE SQLite credential extraction, and system keyring OAuth sync.
//! - [`balancer`]: Intelligent multi-account rotation algorithms (Watermark, ExpiryFirst, RoundRobin) and session switching.
//! - [`cli`]: Interactive terminal dashboards, progress bars, and administrative commands.
//! - [`daemon`]: Autonomous background service loop, health doctor checks, token history snapshots, and failover daemon.
//! - [`quota`]: Quota parsing, burn-rate estimation, and model replenishment countdowns.
//! - [`shield`]: Egress IP Geo-Shield and emergency killswitch policy engine.
//! - [`storage`]: Resilient JSON registry storage, snapshot append logging, and atomic persistence.
//! - [`tray`]: Desktop system tray applet, menu callbacks, and live quota notifications.
//! - [`web`]: Local Axum HTTP dashboard and Server-Sent Events (SSE) live updates.

pub mod auth;
pub mod balancer;
pub mod cli;
pub mod daemon;
pub mod quota;
pub mod shield;
pub mod storage;
pub mod tray;
pub mod web;

// Compile-time static assertions ensuring core domain models satisfy Send + Sync.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<auth::DiscoveredSession>();
    assert_send_sync::<balancer::BalancerConfig>();
    assert_send_sync::<balancer::BalancerStrategy>();
    assert_send_sync::<balancer::SwitchOutcome>();
    assert_send_sync::<balancer::SwitchResult>();
    assert_send_sync::<balancer::SnapshotResult>();
    assert_send_sync::<shield::ShieldConfig>();
    assert_send_sync::<shield::ShieldEvaluation>();
    assert_send_sync::<shield::EgressGeo>();
    assert_send_sync::<storage::StorageManager>();
    assert_send_sync::<storage::Storage>();
    assert_send_sync::<storage::AccountsRegistry>();
    assert_send_sync::<quota::AccountQuotaResult>();
    assert_send_sync::<quota::PoolBurnrates>();
    assert_send_sync::<daemon::DoctorReport>();
    assert_send_sync::<daemon::FailoverState>();
    assert_send_sync::<daemon::FailoverDecision>();
};
