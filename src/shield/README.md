# Geo-Shield & Egress Guard (`src/shield`)

## Overview
Monitors outbound egress IP address and geographic location, detects VPN disconnects or route changes into restricted regions, and automatically enforces emergency process termination (killswitch) against running Antigravity processes to safeguard account security.

## Interface
- `pub struct EgressGeo`: Holds public IP, country code, country name, ISP, flag emoji, and restriction status `[src/shield/models.rs:EgressGeo]`
- `pub struct ShieldConfig`: Configuration model for shield enablement, policy mode (`restricted` vs `whitelist`), allowed countries list, action, and cooldown `[src/shield/models.rs:ShieldConfig]`
- `pub struct ShieldEvaluation`: Result of shield evaluation indicating whether a policy violation occurred and what enforcement action was taken `[src/shield/models.rs:ShieldEvaluation]`
- `pub const RESTRICTED_COUNTRIES: [&str; 7]`: Standard restricted country ISO codes (`IR`, `RU`, `BY`, `KP`, `CU`, `SY`, `CN`) `[src/shield/geo.rs:RESTRICTED_COUNTRIES]`
- `pub async fn get_ip_geo(force_refresh: bool) -> EgressGeo`: Retrieves egress IP and geolocation with in-memory caching `[src/shield/geo.rs:get_ip_geo]`
- `pub fn is_restricted_country(country_code: &str) -> bool`: Verifies whether a given country code is restricted `[src/shield/geo.rs:is_restricted_country]`
- `pub fn load_shield_config(config_dir: &Path) -> ShieldConfig`: Loads shield configuration from disk `[src/shield/shield.rs:load_shield_config]`
- `pub fn save_shield_config(config_dir: &Path, cfg: &ShieldConfig) -> std::io::Result<()>`: Atomically saves shield configuration to disk `[src/shield/shield.rs:save_shield_config]`
- `pub fn set_shield_enabled(config_dir: &Path, enabled: bool) -> std::io::Result<ShieldConfig>`: Toggles shield enablement state `[src/shield/shield.rs:set_shield_enabled]`
- `pub fn get_running_antigravity_pids() -> Vec<i32>`: Enumerates PIDs of active Antigravity Desktop and Language Server processes `[src/shield/shield.rs:get_running_antigravity_pids]`
- `pub fn kill_antigravity_processes(force: bool) -> (usize, Vec<i32>)`: Terminates running Antigravity processes via SIGTERM and fallback SIGKILL `[src/shield/shield.rs:kill_antigravity_processes]`
- `pub fn is_ip_allowed(geo_info: &EgressGeo, cfg: &ShieldConfig) -> (bool, String)`: Evaluates IP geolocation against shield policy `[src/shield/shield.rs:is_ip_allowed]`
- `pub async fn evaluate_and_enforce_shield(config_dir: &Path, geo_override: Option<EgressGeo>) -> ShieldEvaluation`: Evaluates egress IP and executes killswitch actions on policy breach `[src/shield/shield.rs:evaluate_and_enforce_shield]`

## Invariants
- Hardcoded list of restricted countries: `["IR", "RU", "BY", "KP", "CU", "SY", "CN"]` `[src/shield/geo.rs:RESTRICTED_COUNTRIES]`
- Cache validity duration is fixed at 60.0 seconds `[src/shield/geo.rs:CACHE_TTL_SECS]`
- Returns `'🌐'` if trimmed country code length is not 2 or contains non-ASCII alphabetic characters; otherwise calculates Unicode Regional Indicator symbols from base 0x1F1E6 `[src/shield/geo.rs:get_flag_emoji]`
- Country code matching is normalized to trimmed uppercase before checking RESTRICTED_SET `[src/shield/geo.rs:is_restricted_country]`
- Cached geolocation is returned without network calls if `!force_refresh` and `now - cache.timestamp < CACHE_TTL_SECS`; falls back to stale cache or fallback struct if network query fails `[src/shield/geo.rs:get_ip_geo]`
- Default configuration values: `enabled=true`, `mode="restricted"`, `allowed_countries=["US", "DE", "GB", "NL", "CA", "FR", "JP", "CH"]`, `action="kill_process"`, `auto_relaunch=false`, `cooldown_seconds=60` `[src/shield/models.rs:ShieldConfig::default]`
- Default struct values: `ip="Unknown"`, `country_name="Checking..."`, `flag="🌐"`, `is_restricted=false`, `checked_at=0.0`, `summary="🌐 Egress IP Checking..."` `[src/shield/models.rs:EgressGeo::default]`
- If `shield.json` does not exist, saves and returns `ShieldConfig::default()`; on read/deserialization failure returns `ShieldConfig::default()` `[src/shield/shield.rs:load_shield_config]`
- Ensures config directory has 0o700 permissions (Unix); writes JSON to a temporary file (`.tmp`) with 0o600 permissions (Unix) and flushes before atomically renaming to target `shield.json` `[src/shield/shield.rs:save_shield_config]`
- Excludes the running process PID and command lines containing `'antigravity-token-tracker'` or `'agy-token'`; filters processes by name containing `'antigravity'`, cmd containing `'/antigravity'`, or cmd containing `'language_server'` and `'antigravity'`; returns sorted, deduplicated PIDs `[src/shield/shield.rs:get_running_antigravity_pids]`
- Always sends SIGTERM first; polls `/proc/{pid}` for up to 15 iterations of 100ms (1.5s total) for process exit; sends SIGKILL only if `force=true` and processes remain `[src/shield/shield.rs:kill_antigravity_processes]`
- Allows traffic if `cfg.enabled` is false or ip is `'Unknown'` or `country_code` is empty; in `'restricted'` mode blocks if `is_restricted=true` or country is in `RESTRICTED_COUNTRIES`; in `'whitelist'` mode blocks if uppercase country is not in `cfg.allowed_countries` `[src/shield/shield.rs:is_ip_allowed]`
- If `!cfg.enabled`, violation is false and action is None; if enabled and `!allowed`, sets `WAS_VIOLATING=true`, executes `kill_antigravity_processes(true)` if action is `'kill_process'` or `'both'`, respects cooldown before updating `LAST_TRIGGER_TIMESTAMP`, and returns violation=true; if allowed, clears `WAS_VIOLATING` `[src/shield/shield.rs:evaluate_and_enforce_shield]`

## Side Effects
- Mutates global static in-memory cache `GEO_CACHE` mutex `[src/shield/geo.rs:GEO_CACHE]`
- Dispatches outbound HTTP GET requests to `https://ipwho.is/` and `https://ipapi.co/json/` `[src/shield/geo.rs:query_ip_geo]`
- Writes and renames `shield.json` with Unix permissions 0o600 `[src/shield/shield.rs:save_shield_config]`
- Queries OS process table via `sysinfo::System` `[src/shield/shield.rs:get_running_antigravity_pids]`
- Sends POSIX signals (SIGTERM, SIGKILL) to processes via `nix::sys::signal::kill` `[src/shield/shield.rs:kill_antigravity_processes]`
- Mutates global static state `LAST_TRIGGER_TIMESTAMP` and `WAS_VIOLATING` `[src/shield/shield.rs:evaluate_and_enforce_shield]`

## Verification
- Run: `cargo test`
