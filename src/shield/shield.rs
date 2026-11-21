use crate::shield::geo::{get_ip_geo, is_restricted_country, now_epoch};
use crate::shield::models::{EgressGeo, ShieldAction, ShieldConfig, ShieldEvaluation, ShieldMode};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::sleep;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

static LAST_TRIGGER_TIMESTAMP_BITS: AtomicU64 = AtomicU64::new(0);
static WAS_VIOLATING: AtomicBool = AtomicBool::new(false);

pub fn default_config_dir() -> PathBuf {
    crate::storage::default_config_dir()
}

pub fn shield_config_file(config_dir: &Path) -> PathBuf {
    config_dir.join("shield.json")
}

pub fn load_shield_config(config_dir: &Path) -> ShieldConfig {
    let file = shield_config_file(config_dir);
    if !file.exists() {
        let def = ShieldConfig::default();
        let _ = save_shield_config(config_dir, &def);
        return def;
    }

    match fs::read_to_string(&file) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => ShieldConfig::default(),
    }
}

pub fn save_shield_config(config_dir: &Path, cfg: &ShieldConfig) -> std::io::Result<()> {
    if !config_dir.exists() {
        fs::create_dir_all(config_dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(config_dir, fs::Permissions::from_mode(0o700));
        }
    }

    let file = shield_config_file(config_dir);
    let tmp = file.with_extension("tmp");
    let json_data = serde_json::to_string_pretty(cfg)?;

    {
        let mut f = File::create(&tmp)?;
        f.write_all(json_data.as_bytes())?;
        f.flush()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
        }
    }

    fs::rename(&tmp, &file)?;
    Ok(())
}

pub fn set_shield_enabled(config_dir: &Path, enabled: bool) -> std::io::Result<ShieldConfig> {
    let mut cfg = load_shield_config(config_dir);
    cfg.enabled = enabled;
    save_shield_config(config_dir, &cfg)?;
    Ok(cfg)
}

pub fn get_running_antigravity_pids() -> Vec<i32> {
    let mut s = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()),
    );
    s.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let current_pid = std::process::id() as usize;
    let mut pids = Vec::new();

    for (pid, process) in s.processes() {
        let pid_u = pid.as_u32() as usize;
        if pid_u == current_pid {
            continue;
        }

        let name = process.name().to_string_lossy().to_lowercase();
        let cmd = process
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");

        // Ignore our own binaries / trackers
        if cmd.contains("antigravity-token-tracker") || cmd.contains("agy-token") {
            continue;
        }

        let is_match = name.contains("antigravity")
            || cmd.contains("/antigravity")
            || (cmd.contains("language_server") && cmd.contains("antigravity"));

        if is_match {
            pids.push(pid.as_u32() as i32);
        }
    }

    pids.sort();
    pids.dedup();
    pids
}

pub fn kill_antigravity_processes(force: bool) -> (usize, Vec<i32>) {
    let pids = get_running_antigravity_pids();
    if pids.is_empty() {
        return (0, Vec::new());
    }

    // 1. Send SIGTERM first
    for &pid in &pids {
        let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
    }

    // 2. Wait up to 1.5 seconds for processes to exit
    let mut remaining = pids.clone();
    for _ in 0..15 {
        sleep(Duration::from_millis(100));
        remaining.retain(|&p| Path::new(&format!("/proc/{p}")).exists());
        if remaining.is_empty() {
            break;
        }
    }

    // 3. Send SIGKILL if force
    if force {
        for &pid in &remaining {
            let _ = kill(Pid::from_raw(pid), Signal::SIGKILL);
        }
    }

    (pids.len(), pids)
}

pub fn is_ip_allowed(geo_info: &EgressGeo, cfg: &ShieldConfig) -> (bool, String) {
    if !cfg.enabled {
        return (true, "Shield is disabled".to_string());
    }

    let country_code = geo_info.country_code.trim().to_uppercase();
    let country_name = &geo_info.country_name;
    let ip = &geo_info.ip;

    if ip == "Unknown" || country_code.is_empty() {
        return (true, "IP detection pending or offline".to_string());
    }

    match cfg.mode {
        ShieldMode::Restricted => {
            if geo_info.is_restricted || is_restricted_country(&country_code) {
                return (
                    false,
                    format!(
                        "Egress IP {ip} is located in restricted region: {country_name} ({country_code})"
                    ),
                );
            }
            (true, format!("Country {country_code} is safe"))
        }
        ShieldMode::Whitelist => {
            let allowed: Vec<String> = cfg
                .allowed_countries
                .iter()
                .map(|c| c.trim().to_uppercase())
                .collect();
            if !allowed.contains(&country_code) {
                return (
                    false,
                    format!(
                        "Country {} ({}) is outside allowed whitelist: {}",
                        country_code,
                        country_name,
                        allowed.join(", ")
                    ),
                );
            }
            (
                true,
                format!("Country {country_code} is in allowed whitelist"),
            )
        }
    }
}

pub async fn evaluate_and_enforce_shield(
    config_dir: &Path,
    geo_override: Option<EgressGeo>,
) -> ShieldEvaluation {
    let cfg = load_shield_config(config_dir);

    let geo_info = match geo_override {
        Some(g) => g,
        None => get_ip_geo(false).await,
    };

    if !cfg.enabled {
        let (allowed, reason) = is_ip_allowed(&geo_info, &cfg);
        let now = now_epoch();
        let last_trigger = f64::from_bits(LAST_TRIGGER_TIMESTAMP_BITS.load(Ordering::Relaxed));
        if !allowed && (now - last_trigger) > cfg.cooldown_seconds as f64 {
            LAST_TRIGGER_TIMESTAMP_BITS.store(now.to_bits(), Ordering::Relaxed);
            // Alert can be sent
        }
        return ShieldEvaluation {
            enabled: false,
            violation: false,
            message: Some("Shield disabled".to_string()),
            reason: Some(reason),
            action_taken: None,
            processes_killed: None,
            pids_killed: None,
            network_killed: None,
            geo: Some(geo_info),
        };
    }

    let (allowed, reason) = is_ip_allowed(&geo_info, &cfg);
    let now = now_epoch();

    if !allowed {
        WAS_VIOLATING.store(true, Ordering::SeqCst);

        let (killed_count, killed_pids) =
            if matches!(cfg.action, ShieldAction::KillProcess | ShieldAction::Both) {
                kill_antigravity_processes(true)
            } else {
                (0, Vec::new())
            };

        let last_trigger = f64::from_bits(LAST_TRIGGER_TIMESTAMP_BITS.load(Ordering::Relaxed));
        if (now - last_trigger) > cfg.cooldown_seconds as f64 {
            LAST_TRIGGER_TIMESTAMP_BITS.store(now.to_bits(), Ordering::Relaxed);
        }

        ShieldEvaluation {
            enabled: true,
            violation: true,
            message: None,
            reason: Some(reason),
            action_taken: Some(cfg.action.to_string()),
            processes_killed: Some(killed_count),
            pids_killed: Some(killed_pids),
            network_killed: Some(false),
            geo: Some(geo_info),
        }
    } else {
        WAS_VIOLATING.swap(false, Ordering::SeqCst);

        ShieldEvaluation {
            enabled: true,
            violation: false,
            message: Some("Safe egress".to_string()),
            reason: Some(reason),
            action_taken: None,
            processes_killed: None,
            pids_killed: None,
            network_killed: None,
            geo: Some(geo_info),
        }
    }
}
