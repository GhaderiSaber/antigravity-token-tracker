use crate::balancer::balancer::{
    evaluate_and_execute_balancer, get_eligible_pool_accounts, load_balancer_config,
    load_balancer_state, save_balancer_config,
};
use crate::balancer::models::BalancerStrategy;
use crate::balancer::switcher::switch_to_account;
use crate::daemon::doctor::run_doctor_suite;
use crate::daemon::failover::{get_active_sessions, get_failover_status};
use crate::quota::burnrate::{calculate_pool_burnrates, load_snapshots};
use crate::quota::quota::fetch_all_accounts_quota;
use crate::shield::geo::get_ip_geo;
use crate::shield::models::{ShieldAction, ShieldMode};
use crate::shield::shield::{
    is_ip_allowed, load_shield_config, save_shield_config, set_shield_enabled,
};
use crate::storage::storage::{default_config_dir, load_accounts};
use crate::web::assets::static_handler;
use axum::Router;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::LazyLock;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::cors::CorsLayer;

static SSE_TX: LazyLock<broadcast::Sender<String>> = LazyLock::new(|| {
    let (tx, _rx) = broadcast::channel(100);
    tx
});

pub fn broadcast_event(event_type: &str, data: serde_json::Value) {
    let payload = json!({
        "type": event_type,
        "data": data,
    });
    let _ = SSE_TX.send(payload.to_string());
}

#[derive(Deserialize, Default)]
pub struct QuotaQueryParams {
    pub force: Option<bool>,
}

#[derive(Deserialize, Default)]
pub struct HistoryQueryParams {
    pub hours: Option<f64>,
}

#[derive(Deserialize, Default)]
pub struct IpQueryParams {
    pub refresh: Option<bool>,
}

#[derive(Deserialize, Default)]
pub struct DoctorQueryParams {
    pub validate: Option<bool>,
}

#[derive(Deserialize, Default)]
pub struct SwitchParams {
    pub email: Option<String>,
    pub surface: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct SyncSurfacesParams {
    pub target: Option<String>,
}

pub fn create_router() -> Router {
    Router::new()
        .route("/api/events", get(sse_handler))
        .route("/api/quota", get(get_quota_handler))
        .route("/api/history", get(get_history_handler))
        .route("/api/burnrate", get(get_burnrate_handler))
        .route(
            "/api/shield",
            get(get_shield_handler).post(post_shield_handler),
        )
        .route("/api/ip", get(get_ip_handler))
        .route(
            "/api/balancer",
            get(get_balancer_handler).post(post_balancer_handler),
        )
        .route("/api/balancer/rotate", post(post_balancer_rotate_handler))
        .route("/api/switch", get(switch_handler).post(switch_handler))
        .route("/api/switch-best", post(switch_best_handler))
        .route(
            "/api/sync-surfaces",
            get(sync_surfaces_handler).post(sync_surfaces_handler),
        )
        .route("/api/doctor", get(get_doctor_handler))
        .fallback(static_handler)
        .layer(CorsLayer::permissive())
}

async fn sse_handler() -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = SSE_TX.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(data) => Some(Ok(Event::default().data(data))),
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn get_quota_handler(Query(params): Query<QuotaQueryParams>) -> Json<serde_json::Value> {
    let force = params.force.unwrap_or(false);
    let all_quotas = fetch_all_accounts_quota(force).await;

    let now_dt = chrono::Utc::now();
    let mut analyzed = HashMap::new();
    for (email, q) in &all_quotas {
        let snap = crate::storage::models::QuotaSnapshot {
            checked_at: Some(q.checked_at),
            groups: q.groups.clone(),
            models: q.models.clone(),
            is_cached: Some(q.is_cached),
            ..Default::default()
        };
        let analyzed_snap = crate::storage::lifecycle::analyze_account_lifecycle(&snap, now_dt);
        let mut analyzed_q = q.clone();
        analyzed_q.groups = analyzed_snap.groups;
        analyzed_q.models = analyzed_snap.models;
        analyzed.insert(email.clone(), analyzed_q);
    }

    let mut snapshots_map = HashMap::new();
    for (email, q) in &analyzed {
        snapshots_map.insert(
            email.clone(),
            crate::storage::models::QuotaSnapshot {
                checked_at: Some(q.checked_at),
                groups: q.groups.clone(),
                models: q.models.clone(),
                is_cached: Some(q.is_cached),
                ..Default::default()
            },
        );
    }
    let active_pair = crate::daemon::failover::get_active_account(&analyzed);
    let active_email = active_pair.as_ref().map(|(e, _)| e.as_str());
    let rec =
        crate::storage::lifecycle::compute_switching_recommendation(&snapshots_map, active_email);

    let failover_info = get_failover_status(&analyzed);
    let active_sessions = get_active_sessions(&analyzed);
    let geo_info = get_ip_geo(force).await;
    let config_dir = default_config_dir();
    let shield_cfg = load_shield_config(&config_dir);
    let bal_cfg = load_balancer_config();
    let bal_state = load_balancer_state();
    let now_epoch = now_dt.timestamp_millis() as f64 / 1000.0;
    let burnrate_info = calculate_pool_burnrates(&analyzed, None, None, now_epoch);

    let mut active_sessions_dict = HashMap::new();
    for (surf, (email, acc_data)) in active_sessions {
        let mut rem_pct = 0.0;
        for g in &acc_data.groups {
            if g.display_name.to_lowercase().contains("gemini")
                && let Some(w) = &g.weekly
            {
                rem_pct = w.remaining_percent.unwrap_or(0.0);
                break;
            }
        }
        active_sessions_dict.insert(
            surf,
            json!({
                "email": email,
                "quota_pct": rem_pct,
                "name": acc_data.name
            }),
        );
    }

    Json(json!({
        "accounts": analyzed,
        "recommendation": rec,
        "failover": failover_info,
        "active_sessions": active_sessions_dict,
        "geo": geo_info,
        "shield": shield_cfg,
        "balancer": bal_cfg,
        "balancer_state": bal_state,
        "burnrate": burnrate_info,
        "server_time_utc": now_dt.to_rfc3339(),
        "server_time_local": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
    }))
}

async fn get_history_handler(Query(params): Query<HistoryQueryParams>) -> Json<serde_json::Value> {
    let hours = params.hours.unwrap_or(24.0);
    let max_age = hours * 3600.0;
    let now_epoch = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
    let default_hist = crate::quota::burnrate::default_history_file();
    let snapshots = load_snapshots(&default_hist, max_age, 500, now_epoch);

    Json(json!({
        "snapshots": snapshots,
        "count": snapshots.len()
    }))
}

async fn get_burnrate_handler() -> Json<serde_json::Value> {
    let all_quotas = fetch_all_accounts_quota(false).await;
    let now_epoch = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
    let burn_info = calculate_pool_burnrates(&all_quotas, None, None, now_epoch);
    Json(json!(burn_info))
}

async fn get_shield_handler() -> Json<serde_json::Value> {
    let config_dir = default_config_dir();
    let cfg = load_shield_config(&config_dir);
    let geo = get_ip_geo(false).await;
    let (allowed, reason) = is_ip_allowed(&geo, &cfg);

    Json(json!({
        "config": cfg,
        "allowed": allowed,
        "reason": reason,
        "geo": geo
    }))
}

async fn post_shield_handler(Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let config_dir = default_config_dir();
    let mut cfg = load_shield_config(&config_dir);

    if let Some(enabled) = body.get("enabled").and_then(serde_json::Value::as_bool) {
        cfg.enabled = enabled;
        let _ = set_shield_enabled(&config_dir, enabled);
    }
    if let Some(mode) = body
        .get("mode")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<ShieldMode>().ok())
    {
        cfg.mode = mode;
    }
    if let Some(allowed) = body.get("allowed_countries").and_then(|v| v.as_array()) {
        cfg.allowed_countries = allowed
            .iter()
            .filter_map(|c| c.as_str().map(std::string::ToString::to_string))
            .collect();
    }
    if let Some(action) = body
        .get("action")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<ShieldAction>().ok())
    {
        cfg.action = action;
    }
    let _ = save_shield_config(&config_dir, &cfg);

    broadcast_event(
        "shield_updated",
        json!({ "enabled": cfg.enabled, "mode": cfg.mode }),
    );

    get_shield_handler().await
}

async fn get_ip_handler(Query(params): Query<IpQueryParams>) -> Json<serde_json::Value> {
    let refresh = params.refresh.unwrap_or(false);
    let geo_info = get_ip_geo(refresh).await;
    Json(json!(geo_info))
}

async fn get_balancer_handler() -> Json<serde_json::Value> {
    let cfg = load_balancer_config();
    let state = load_balancer_state();
    let all_quotas = fetch_all_accounts_quota(false).await;
    let pool = get_eligible_pool_accounts(&all_quotas, cfg.min_quota_pct);

    Json(json!({
        "config": cfg,
        "state": state,
        "pool": pool
    }))
}

async fn post_balancer_handler(Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let mut cfg = load_balancer_config();

    if let Some(enabled) = body.get("enabled").and_then(serde_json::Value::as_bool) {
        cfg.enabled = enabled;
    }
    if let Some(strat_val) = body.get("strategy") {
        if let Ok(strat) = serde_json::from_value::<BalancerStrategy>(strat_val.clone()) {
            cfg.strategy = strat;
        } else if let Some(s) = strat_val.as_str() {
            if let Ok(strat) = s.parse::<BalancerStrategy>() {
                cfg.strategy = strat;
            }
        }
    }
    if let Some(spread) = body
        .get("watermark_spread_pct")
        .and_then(serde_json::Value::as_f64)
    {
        cfg.watermark_spread_pct = spread;
    }
    if let Some(interval) = body
        .get("round_robin_interval_minutes")
        .and_then(serde_json::Value::as_u64)
    {
        cfg.round_robin_interval_minutes = interval;
    }
    if let Some(exp) = body
        .get("expiry_window_hours")
        .and_then(serde_json::Value::as_u64)
    {
        cfg.expiry_window_hours = exp;
    }
    let _ = save_balancer_config(&cfg);

    broadcast_event(
        "balancer_updated",
        json!({ "enabled": cfg.enabled, "strategy": cfg.strategy }),
    );

    get_balancer_handler().await
}

async fn post_balancer_rotate_handler() -> Json<serde_json::Value> {
    let all_quotas = fetch_all_accounts_quota(false).await;
    let res = evaluate_and_execute_balancer(&all_quotas, false, true, None).await;
    if res.triggered {
        broadcast_event("balancer_rotated", json!(res));
    }
    Json(json!(res))
}

async fn switch_handler(
    Query(query_params): Query<SwitchParams>,
    body: Option<Json<SwitchParams>>,
) -> Response {
    let email = query_params
        .email
        .or_else(|| body.as_ref().and_then(|b| b.email.clone()));
    let surface = query_params
        .surface
        .or_else(|| body.as_ref().and_then(|b| b.surface.clone()))
        .unwrap_or_else(|| "both".into());

    let Some(target_email) = email else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"success": false, "error": "Missing email parameter"})),
        )
            .into_response();
    };

    let (success, msg) = switch_to_account(&target_email, true, &surface).await;
    if success {
        broadcast_event(
            "account_switched",
            json!({"target": target_email, "surface": surface}),
        );
        (
            StatusCode::OK,
            Json(json!({"success": true, "message": msg})),
        )
            .into_response()
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"success": false, "error": msg})),
        )
            .into_response()
    }
}

async fn switch_best_handler() -> Response {
    let all_quotas = fetch_all_accounts_quota(false).await;
    let failover_info = get_failover_status(&all_quotas);
    let best_target = failover_info
        .get("backup_candidate_email")
        .and_then(|v| v.as_str());

    let Some(target) = best_target else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"success": false, "error": "No backup candidate with available quota."})),
        )
            .into_response();
    };

    let (success, msg) = switch_to_account(target, true, "both").await;
    if success {
        broadcast_event(
            "account_switched",
            json!({"target": target, "surface": "both"}),
        );
        (
            StatusCode::OK,
            Json(json!({"success": true, "message": msg, "target": target})),
        )
            .into_response()
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"success": false, "error": msg})),
        )
            .into_response()
    }
}

async fn sync_surfaces_handler(Query(params): Query<SyncSurfacesParams>) -> Response {
    let target = params.target.unwrap_or_else(|| "ide".into()).to_lowercase();
    let reg = load_accounts().unwrap_or_default();
    let mut desktop_acc = None;
    let mut ide_acc = None;

    for (em, acc) in reg.accounts {
        if acc.is_current_desktop_session == Some(true) {
            desktop_acc = Some(em.clone());
        }
        if acc.is_current_ide_session == Some(true) {
            ide_acc = Some(em.clone());
        }
    }

    let (success, msg) = if (target == "ide" || target == "align_ide")
        && let Some(d_acc) = desktop_acc
    {
        switch_to_account(&d_acc, false, "ide").await
    } else if (target == "desktop" || target == "app" || target == "align_desktop")
        && let Some(i_acc) = ide_acc
    {
        switch_to_account(&i_acc, true, "desktop").await
    } else {
        (
            false,
            "Source session not detected for alignment or invalid target.".into(),
        )
    };

    if success {
        broadcast_event("surfaces_synced", json!({"target": target}));
        (
            StatusCode::OK,
            Json(json!({"success": true, "message": msg})),
        )
            .into_response()
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"success": false, "error": msg})),
        )
            .into_response()
    }
}

async fn get_doctor_handler(Query(_params): Query<DoctorQueryParams>) -> Json<serde_json::Value> {
    let diag = run_doctor_suite().await;
    Json(json!(diag))
}

pub async fn start_web_server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let app = create_router();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("\n[Antigravity Token Dashboard] Serving at http://localhost:{port}/");
    axum::serve(listener, app).await?;
    Ok(())
}
