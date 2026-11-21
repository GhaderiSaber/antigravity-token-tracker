# Domain Glossary: antigravity-token-tracker (Python to Rust)

This glossary maps domain terms, entities, and data structures from the Python implementation to their canonical Rust types, modules, and conventions.

| Domain Term | Python Location / Representation | Rust Type / Path | Meaning & Context |
| :--- | :--- | :--- | :--- |
| **AccountProfile** | `dict` in `accounts.py` | `crate::models::AccountProfile` | Represents an authenticated user account (email, name, tier, access_token, refresh_token, last_known_quota, etc.) |
| **AccountsRegistry** | `dict` in `accounts.py` (`accounts.json`) | `crate::models::AccountsRegistry` | Master container containing registered accounts and the currently active email. Handles both legacy and v2 schemas. |
| **QuotaSnapshot** | `dict` with `groups` & `models` in `quota.py` | `crate::models::QuotaSnapshot` | Full quota payload retrieved from Google Cloud Controls Partner API / Antigravity internal endpoint. |
| **ModelQuota** | `dict` inside `models` array | `crate::models::ModelQuota` | Per-model quota: `modelId`, `remainingFraction`, `remainingPercent`, `resetTime`. |
| **QuotaGroup** | `dict` inside `groups` array | `crate::models::QuotaGroup` | Model grouping (e.g., "Gemini Models", "Claude and GPT models") with 5-hour and weekly bucket limits. |
| **QuotaBucket** | `dict` (`fiveHour` / `weekly`) | `crate::models::QuotaBucket` | Limits, remaining fraction/percent, and ISO reset timestamp for a rolling window. |
| **BurnRateRecord** | Entry in `token_history.jsonl` | `crate::models::BurnRateRecord` | Historical quota observation: timestamp, email, remaining percent, delta. |
| **BurnRateMetrics** | `dict` in `burnrate.py` | `crate::models::BurnRateMetrics` | Computed consumption velocity: burn rate (%/hr), hours until exhaustion, runout clock. |
| **EgressGeo** | `dict` in `geo.py` | `crate::models::EgressGeo` | Public egress IP address, country code, country flag emoji, city, ISP, and restricted status flag. |
| **GeoShieldState** | `dict` in `shield.py` | `crate::models::GeoShieldState` | Shield configuration: enabled, mode (`restricted` or `whitelist`), allowed countries list, last trigger timestamp. |
| **BalanceRecommendation** | `dict` in `balancer.py` | `crate::models::BalanceRecommendation` | Best alternative account suggestion based on watermark thresholds and remaining quota. |
| **DesktopAppSession** | Process & port info in `auth.py` | `crate::models::DesktopAppSession` | Extracted port, CSRF token, and user info from active Antigravity Desktop App process. |
| **IdeSessionSecret** | SQLite item in `state.vscdb` | `crate::models::IdeSessionSecret` | Stored credentials from Antigravity IDE `antigravityUnifiedStateSync.oauthToken`. |
| **FailoverDaemon** | Loop in `failover.py` | `crate::daemon::FailoverDaemon` | Background monitoring task executing quota checks, desktop toasts, and automatic failovers. |
| **TrayApplet** | GLib / StatusNotifierItem in `tray.py` | `crate::tray::TrayApplet` | Linux system tray indicator applet with live colored progress badge and popup menu. |
| **DashboardServer** | ThreadingHTTPServer in `server.py` | `crate::server::DashboardServer` | Axum HTTP server hosting REST endpoints, SSE stream, and embedded React frontend. |
