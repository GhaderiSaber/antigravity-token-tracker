# Web Server & REST API (`src/web`)

## Overview
Hosts the embedded Axum HTTP web server, provides REST API endpoints, streams live Server-Sent Events (SSE) updates to connected browser clients, and delivers embedded frontend SPA assets.

## Interface
- Module exports (`src/web/mod.rs`): `pub mod assets`, `pub mod server`, with explicit re-exports `FrontendAssets`, `static_handler`, `DoctorQueryParams`, `HistoryQueryParams`, `IpQueryParams`, `QuotaQueryParams`, `SwitchParams`, `SyncSurfacesParams`, `WebError`, `broadcast_event`, `create_router`, and `start_web_server` `[src/web/mod.rs]`
- `pub struct FrontendAssets`: `RustEmbed` provider serving static assets embedded from `src/web/dist/` `[src/web/assets.rs:FrontendAssets]`
- `pub async fn static_handler(uri: axum::http::Uri) -> Response`: Serves embedded static assets or SPA `index.html` fallback `[src/web/assets.rs:static_handler]`
- `pub enum WebError`: Typed web server error (`Io(#[from] std::io::Error)`) `[src/web/server.rs:WebError]`
- `pub fn broadcast_event(event_type: &str, data: serde_json::Value)`: Broadcasts real-time JSON events across open SSE connections `[src/web/server.rs:broadcast_event]`
- `pub struct QuotaQueryParams`, `pub struct HistoryQueryParams`, `pub struct IpQueryParams`, `pub struct DoctorQueryParams`, `pub struct SwitchParams`, `pub struct SyncSurfacesParams`: Query/payload parameter structs for REST endpoints `[src/web/server.rs]`
- `pub fn create_router() -> Router`: Configures Axum router with localhost-only CORS, SPA fallback, and routes:
  - `GET`: `/api/events`, `/api/quota`, `/api/history`, `/api/burnrate`, `/api/ip`, `/api/doctor`
  - `GET`, `POST`: `/api/shield`, `/api/balancer`, `/api/switch`, `/api/sync-surfaces`
  - `POST`: `/api/balancer/rotate`, `/api/switch-best`
  `[src/web/server.rs:create_router]`
- `pub async fn start_web_server(port: u16) -> Result<(), WebError>`: Binds TCP listener on `127.0.0.1:{port}` and launches the HTTP service `[src/web/server.rs:start_web_server]`

## Invariants
- Empty path or `'index.html'` resolves to `'index.html'`; leading slashes are trimmed `[src/web/assets.rs:static_handler]`
- Paths starting with `'assets/'` are given `'Cache-Control: public, max-age=31536000, immutable'`; all other files are given `'Cache-Control: no-cache'` `[src/web/assets.rs:static_handler]`
- Unmatched paths fall back to serving embedded `'index.html'` with `'Content-Type: text/html; charset=utf-8'` and `'Cache-Control: no-cache'` for client-side routing; returns 404 if `'index.html'` is missing `[src/web/assets.rs:static_handler]`
- Broadcast channel capacity invariant is fixed at 100 messages via `broadcast::channel(100)` `[src/web/server.rs:SSE_TX]`
- Router applies localhost-only CORS via `AllowOrigin::predicate(|origin, _| is_localhost_origin(origin))`, permitting only `http://localhost`, `http://localhost:<u16>`, `http://127.0.0.1`, and `http://127.0.0.1:<u16>` while rejecting non-localhost and spoofed subdomain origins `[src/web/server.rs:is_localhost_origin, src/web/server.rs:create_router]`
- `params.force` defaults to `false` if `None` `[src/web/server.rs:get_quota_handler]`
- `params.hours` defaults to `24.0` if `None`; snapshot count load limit is fixed to 500 `[src/web/server.rs:get_history_handler]`
- `params.refresh` defaults to `false` if `None` `[src/web/server.rs:get_ip_handler]`
- `get_doctor_handler` accepts `Query(_params): Query<DoctorQueryParams>` (`validate: Option<bool>`) and ignores `validate` `[src/web/server.rs:get_doctor_handler]`
- `'balancer_rotated'` SSE event is emitted only when `res.triggered` is `true` `[src/web/server.rs:post_balancer_rotate_handler]`
- Accepts `email` and `surface` from query params or JSON body (query takes precedence); `surface` defaults to `'both'` if unspecified; returns 400 Bad Request if `surface` is invalid, `email` parameter is absent, or `switch_to_account` fails `[src/web/server.rs:switch_handler]`
- Requires `backup_candidate_email` from failover status; returns 400 Bad Request if `None`, and 500 Internal Server Error if `switch_to_account` fails `[src/web/server.rs:switch_best_handler]`
- `sync_surfaces_handler` accepts query params only (`Query(params)`); `target` defaults to `'ide'` if unspecified, passes `restart = false` for `SwitchSurface::Ide` (`'ide'`/`'align_ide'`) and `restart = true` for `SwitchSurface::Desktop` (`'desktop'`/`'app'`/`'align_desktop'`), and returns 400 Bad Request if the source session is absent or the target is invalid `[src/web/server.rs:sync_surfaces_handler]`
- Server binds strictly to IPv4 localhost address `127.0.0.1` at the specified port `[src/web/server.rs:start_web_server]`

## Side Effects
- Emits serialized JSON payloads to SSE broadcast channel subscribers `[src/web/server.rs:broadcast_event]`
- Dispatches async network queries to Google CloudCode PA and IP geolocation endpoints `[src/web/server.rs:get_quota_handler]`
- Reads and updates `shield.json` on disk and broadcasts `shield_updated` SSE event `[src/web/server.rs:post_shield_handler]`
- Reads and updates `balancer.json` on disk and broadcasts `balancer_updated` SSE event `[src/web/server.rs:post_balancer_handler]`
- Executes account switching and session file manipulation via `switch_to_account`, broadcasting `account_switched` (`switch_handler`, `switch_best_handler`) or `surfaces_synced` (`sync_surfaces_handler`) SSE events `[src/web/server.rs:switch_handler, src/web/server.rs:switch_best_handler, src/web/server.rs:sync_surfaces_handler]`
- Binds TCP socket listener on `127.0.0.1:{port}` and executes `axum::serve` `[src/web/server.rs:start_web_server]`

## Verification
- Run: `cargo test`
