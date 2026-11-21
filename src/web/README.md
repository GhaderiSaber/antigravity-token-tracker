# Web Server & REST API (`src/web`)

## Overview
Hosts the embedded Axum HTTP web server, provides REST API endpoints, streams live Server-Sent Events (SSE) updates to connected browser clients, and delivers embedded frontend SPA assets.

## Interface
- `pub struct FrontendAssets`: `RustEmbed` provider serving static assets embedded from `src/web/dist/` `[src/web/assets.rs:FrontendAssets]`
- `pub async fn static_handler(uri: axum::http::Uri) -> Response`: Serves embedded static assets or SPA fallback `[src/web/assets.rs:static_handler]`
- `pub fn broadcast_event(event_type: &str, data: serde_json::Value)`: Broadcasts real-time events across open SSE connections `[src/web/server.rs:broadcast_event]`
- `pub fn create_router() -> Router`: Configures Axum router with REST and SSE endpoints (`/api/events`, `/api/quota`, `/api/history`, `/api/burnrate`, `/api/shield`, `/api/ip`, `/api/balancer`, `/api/switch`, `/api/doctor`) `[src/web/server.rs:create_router]`
- `pub async fn start_web_server(port: u16) -> Result<(), Box<dyn std::error::Error>>`: Binds TCP listener on `127.0.0.1:{port}` and launches the HTTP service `[src/web/server.rs:start_web_server]`

## Invariants
- Empty path or `'index.html'` resolves to `'index.html'`; leading slashes are trimmed `[src/web/assets.rs:static_handler]`
- Paths starting with `'assets/'` are given `'Cache-Control: public, max-age=31536000, immutable'`; all other files are given `'Cache-Control: no-cache'` `[src/web/assets.rs:static_handler]`
- Unmatched paths fall back to serving embedded `'index.html'` with `'Content-Type: text/html; charset=utf-8'` and `'Cache-Control: no-cache'` for client-side routing; returns 404 if `'index.html'` is missing `[src/web/assets.rs:static_handler]`
- Broadcast channel capacity invariant is fixed at 100 messages via `broadcast::channel(100)` `[src/web/server.rs:SSE_TX]`
- `params.force` defaults to false if None `[src/web/server.rs:get_quota_handler]`
- `params.hours` defaults to 24.0 if None; snapshot count load limit is fixed to 500 `[src/web/server.rs:get_history_handler]`
- `params.refresh` defaults to false if None `[src/web/server.rs:get_ip_handler]`
- `'balancer_rotated'` SSE event is emitted only when `res.triggered` is true `[src/web/server.rs:post_balancer_rotate_handler]`
- Surface defaults to `'both'` if unspecified; returns 400 Bad Request if email parameter is absent `[src/web/server.rs:switch_handler]`
- Requires `backup_candidate_email` from failover status; returns 400 Bad Request if None, and 500 Internal Server Error if `switch_to_account` fails `[src/web/server.rs:switch_best_handler]`
- Target defaults to `'ide'` if unspecified; requires corresponding desktop/ide active session in accounts registry or returns 400 Bad Request `[src/web/server.rs:sync_surfaces_handler]`
- Server binds strictly to IPv4 localhost address `127.0.0.1` at the specified port `[src/web/server.rs:start_web_server]`

## Side Effects
- Emits serialized JSON payloads to SSE broadcast channel subscribers `[src/web/server.rs:broadcast_event]`
- Dispatches async network queries to Google CloudCode PA and IP geolocation endpoints `[src/web/server.rs:get_quota_handler]`
- Reads and updates shield and balancer configurations on disk; broadcasts `shield_updated` and `balancer_updated` SSE events `[src/web/server.rs:post_shield_handler]`
- Executes account switching and session file manipulation via `switch_to_account` `[src/web/server.rs:switch_handler]`
- Binds TCP socket listener on `127.0.0.1:{port}` and executes `axum::serve` `[src/web/server.rs:start_web_server]`

## Verification
- Run: `cargo test`
