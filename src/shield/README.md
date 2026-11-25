# Geo-Shield & Egress Guard (`src/shield`)

## Overview
Monitors outbound egress IP address and geographic location, detects VPN disconnects or route changes into restricted regions, and automatically enforces emergency process termination (killswitch) against running Antigravity processes to safeguard account security.

## Interface
- `pub struct EgressGeo`: Holds public IP, country code, country name, region, city, ISP, org, flag emoji, restriction status, timestamp, and summary `[src/shield/models.rs:EgressGeo]`
- `pub enum IpDetail`: Typed flag (`WithIp`, `OmitIp`) for IP summary formatting with `From<bool>` `[src/shield/models.rs:IpDetail]`
- `pub enum EgressCachePolicy`: Typed flag (`UseCache`, `ForceRefresh`) for egress geo queries with `From<bool>` `[src/shield/models.rs:EgressCachePolicy]`
- `pub enum ShieldMode` / `pub struct ParseShieldModeError`: Policy mode (`Restricted`, `Whitelist`) with snake_case serde, `Display`, and `FromStr` `[src/shield/models.rs:ShieldMode]`
- `pub enum ShieldAction` / `pub struct ParseShieldActionError`: Enforcement action (`KillProcess`, `KillNetwork`, `Both`) with snake_case serde, `Display`, and `FromStr` `[src/shield/models.rs:ShieldAction]`
- `pub struct ShieldConfig`: Configuration model for shield enablement, `ShieldMode`, `allowed_countries`, `ShieldAction`, `auto_relaunch`, and `cooldown_seconds` `[src/shield/models.rs:ShieldConfig]`
- `pub struct ShieldEvaluation`: Result of shield evaluation indicating policy violation status and enforcement action taken `[src/shield/models.rs:ShieldEvaluation]`
- `pub const RESTRICTED_COUNTRIES: [&str; 7]` / `pub const CACHE_TTL_SECS: f64`: Restricted ISO codes (`IR`, `RU`, `BY`, `KP`, `CU`, `SY`, `CN`) and 60.0s cache TTL `[src/shield/geo.rs:RESTRICTED_COUNTRIES]`
- `pub fn flag_emoji(country_code: &str) -> String` / `get_flag_emoji`: Computes Regional Indicator flag emoji or `'🌐'` `[src/shield/geo.rs:flag_emoji]`
- `pub fn is_restricted_country(country_code: &str) -> bool`: Verifies whether a country code is in `RESTRICTED_SET` `[src/shield/geo.rs:is_restricted_country]`
- `pub fn format_ip_summary(geo: &EgressGeo, detail: impl Into<IpDetail>) -> String`: Formats human-readable location and optional IP summary `[src/shield/geo.rs:format_ip_summary]`
- `pub fn now_epoch() -> f64` / `pub async fn query_ip_geo() -> Option<EgressGeo>`: Returns UNIX epoch seconds and queries external IP geo APIs `[src/shield/geo.rs:query_ip_geo]`
- `pub async fn ip_geo(policy: impl Into<EgressCachePolicy>) -> EgressGeo` / `get_ip_geo`: Retrieves egress IP and geolocation with in-memory caching `[src/shield/geo.rs:get_ip_geo]`
- `pub fn default_config_dir() -> PathBuf` / `pub fn shield_config_file(config_dir: &Path) -> PathBuf`: Resolves config directory and `shield.json` path `[src/shield/shield.rs:shield_config_file]`
- `pub fn load_shield_config(config_dir: &Path) -> ShieldConfig`: Loads shield configuration from disk `[src/shield/shield.rs:load_shield_config]`
- `pub fn save_shield_config(config_dir: &Path, cfg: &ShieldConfig) -> std::io::Result<()>`: Atomically saves shield configuration to disk `[src/shield/shield.rs:save_shield_config]`
- `pub fn enable_shield(config_dir: &Path) -> std::io::Result<ShieldConfig>` / `disable_shield` / `set_shield_enabled`: Updates and persists shield enablement state `[src/shield/shield.rs:set_shield_enabled]`
- `pub fn running_antigravity_pids() -> Vec<i32>` / `get_running_antigravity_pids`: Enumerates PIDs of active Antigravity Desktop and Language Server processes `[src/shield/shield.rs:running_antigravity_pids]`
- `pub fn kill_antigravity_processes(force: bool) -> (usize, Vec<i32>)`: Terminates running Antigravity processes via SIGTERM and fallback SIGKILL `[src/shield/shield.rs:kill_antigravity_processes]`
- `pub fn is_ip_allowed(geo_info: &EgressGeo, cfg: &ShieldConfig) -> (bool, String)`: Evaluates IP geolocation against `ShieldMode` policy `[src/shield/shield.rs:is_ip_allowed]`
- `pub async fn evaluate_and_enforce_shield(config_dir: &Path, geo_override: Option<EgressGeo>) -> ShieldEvaluation`: Evaluates egress IP and executes killswitch actions on policy breach `[src/shield/shield.rs:evaluate_and_enforce_shield]`

## Invariants
- Restricted country list is `["IR", "RU", "BY", "KP", "CU", "SY", "CN"]`; cache TTL is `60.0` seconds `[src/shield/geo.rs:RESTRICTED_COUNTRIES]`
- Returns `'🌐'` if trimmed country code length is not 2 or contains non-ASCII alphabetic characters; otherwise calculates Unicode Regional Indicator symbols from base `0x1F1E6` `[src/shield/geo.rs:flag_emoji]`
- Country code matching is normalized to trimmed uppercase before checking `RESTRICTED_SET` `[src/shield/geo.rs:is_restricted_country]`
- Cached geolocation is returned without network calls if `!policy.into().is_force_refresh()` and `now - cache.timestamp < CACHE_TTL_SECS`; falls back to stale cache or default `EgressGeo` if network query fails `[src/shield/geo.rs:ip_geo]`
- Default `ShieldConfig` values: `enabled=true`, `mode=ShieldMode::Restricted`, `allowed_countries=["US", "DE", "GB", "NL", "CA", "FR", "JP", "CH"]`, `action=ShieldAction::KillProcess`, `auto_relaunch=false`, `cooldown_seconds=60` `[src/shield/models.rs:ShieldConfig::default]`
- Default `EgressGeo` values: `ip="Unknown"`, `country_name="Checking..."`, `flag="🌐"`, `is_restricted=false`, `checked_at=0.0`, `summary="🌐 Egress IP Checking..."` `[src/shield/models.rs:EgressGeo::default]`
- If `shield.json` does not exist, saves and returns `ShieldConfig::default()`; on read/deserialization failure returns `ShieldConfig::default()` `[src/shield/shield.rs:load_shield_config]`
- Ensures config directory has `0o700` permissions on creation (Unix); writes JSON to `.tmp` with `0o600` permissions (Unix) and flushes before atomically renaming to `shield.json` `[src/shield/shield.rs:save_shield_config]`
- Excludes the running process PID and command lines containing `'antigravity-token-tracker'` or `'agy-token'`; filters processes by name containing `'antigravity'`, cmd containing `'/antigravity'`, or cmd containing `'language_server'` and `'antigravity'`; returns sorted, deduplicated PIDs `[src/shield/shield.rs:running_antigravity_pids]`
- Sends SIGTERM first, polls `/proc/{pid}` for up to 15 iterations of 100ms (1.5s total) for exit, and sends SIGKILL to remaining PIDs only if `force=true` `[src/shield/shield.rs:kill_antigravity_processes]`
- Allows traffic if `!cfg.enabled` or `ip == "Unknown"` or `country_code` is empty; in `ShieldMode::Restricted` blocks if `geo_info.is_restricted || is_restricted_country(&country_code)`; in `ShieldMode::Whitelist` blocks if uppercase country is not in `cfg.allowed_countries` `[src/shield/shield.rs:is_ip_allowed]`
- If `!cfg.enabled`, returns `violation=false` and `action_taken=None`; if enabled and `!allowed`, sets `WAS_VIOLATING=true`, executes `kill_antigravity_processes(true)` when `cfg.action` is `KillProcess | Both`, updates `LAST_TRIGGER_TIMESTAMP_BITS` if cooldown elapsed, and returns `violation=true` (`network_killed: Some(false)`); if allowed, clears `WAS_VIOLATING` `[src/shield/shield.rs:evaluate_and_enforce_shield]`

## Side Effects
- Mutates global static in-memory cache `GEO_CACHE` mutex `[src/shield/geo.rs:GEO_CACHE]`
- Dispatches outbound HTTP GET requests to `https://ipwho.is/` and `https://ipapi.co/json/` `[src/shield/geo.rs:query_ip_geo]`
- Creates config directory (`0o700`) and writes/renames `shield.json` (`0o600`) `[src/shield/shield.rs:save_shield_config]`
- Queries OS process table via `sysinfo::System` `[src/shield/shield.rs:running_antigravity_pids]`
- Sends POSIX signals (`SIGTERM`, `SIGKILL`) to processes via `nix::sys::signal::kill` `[src/shield/shield.rs:kill_antigravity_processes]`
- Mutates global static atomics `LAST_TRIGGER_TIMESTAMP_BITS` and `WAS_VIOLATING` `[src/shield/shield.rs:evaluate_and_enforce_shield]`

## Verification
- Run: `cargo test`
