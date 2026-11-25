use super::keyring::get_desktop_keyring_token;
use super::models::DiscoveredSession;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

static PID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"pid=(\d+)").expect("valid PID regex pattern"));
static PORT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"127\.0\.0\.1:(\d+)").expect("valid port regex pattern"));
static CSRF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"--csrf_token[=\s]+([^\s]+)").expect("valid CSRF token regex pattern")
});

#[must_use]
pub fn listening_ports_by_pid() -> HashMap<i32, Vec<u16>> {
    let mut pid_ports: HashMap<i32, Vec<u16>> = HashMap::new();

    // Use `ss -tulpn` if available
    if let Ok(output) = std::process::Command::new("ss")
        .args(["-tulpn"])
        .stderr(std::process::Stdio::null())
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            if let (Some(m_pid), Some(m_port)) = (PID_RE.captures(line), PORT_RE.captures(line))
                && let (Ok(pid), Ok(port)) = (m_pid[1].parse::<i32>(), m_port[1].parse::<u16>())
            {
                pid_ports.entry(pid).or_default().push(port);
            }
        }
    }

    pid_ports
}

#[inline]
#[must_use]
pub fn get_listening_ports_by_pid() -> HashMap<i32, Vec<u16>> {
    listening_ports_by_pid()
}

pub async fn query_local_user_status(
    port: u16,
    csrf_token: &str,
) -> Option<(String, String, String)> {
    let url = format!(
        "https://127.0.0.1:{port}/exa.language_server_pb.LanguageServerService/GetUserStatus"
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(1500))
        .danger_accept_invalid_certs(true)
        .build()
        .ok()?;

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("X-Codeium-Csrf-Token", csrf_token)
        .header("Connect-Protocol-Version", "1")
        .body("{}")
        .send()
        .await
        .ok()?;

    if resp.status().is_success() {
        let data: Value = resp.json().await.ok()?;
        let user_status = data.get("userStatus")?;
        let email = user_status
            .get("email")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_lowercase();
        let name = user_status
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tier = user_status
            .get("userTier")
            .and_then(|t| t.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("Standard")
            .to_string();

        return Some((email, name, tier));
    }

    None
}

pub async fn fetch_quota_from_desktop_app(port: u16, csrf_token: &str) -> Option<Value> {
    let url = format!(
        "https://127.0.0.1:{port}/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary"
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .build()
        .ok()?;

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("X-Codeium-Csrf-Token", csrf_token)
        .header("Connect-Protocol-Version", "1")
        .body("{}")
        .send()
        .await
        .ok()?;

    if resp.status().is_success() {
        let data: Value = resp.json().await.ok()?;
        return Some(data.get("response").cloned().unwrap_or(data));
    }

    None
}

fn normalize_email(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_lowercase())
    }
}

/// Returns `true` when a process command line belongs to the Antigravity Desktop App language server.
#[must_use]
pub fn is_desktop_language_server_cmdline(cmd: &str) -> bool {
    cmd.contains("language_server")
        && (cmd.contains("subclient_type hub")
            || (cmd.contains("app_data_dir antigravity")
                && !cmd.contains("app_data_dir antigravity-ide"))
            || (cmd.contains("/snap/antigravity/") && !cmd.contains("/snap/antigravity-ide")))
        && !(cmd.contains("antigravity-ide") || cmd.contains("subclient_type ide"))
}

/// Reads the last logged-in desktop username from `app_storage.json`.
///
/// Prefers `agy.onboarding.lastLoginUsername` (Antigravity v2.35+) and falls back to
/// `jetski.onboarding.lastLoginUsername`, returning a trimmed lowercase email when non-empty.
#[must_use]
pub fn read_desktop_last_login_username(app_storage_path: &Path) -> Option<String> {
    let content = fs::read_to_string(app_storage_path).ok()?;
    let data: Value = serde_json::from_str(&content).ok()?;

    [
        "agy.onboarding.lastLoginUsername",
        "jetski.onboarding.lastLoginUsername",
    ]
    .iter()
    .find_map(|key| {
        data.get(*key)
            .and_then(|v| v.as_str())
            .and_then(normalize_email)
    })
}

/// Resolves the active desktop session email from live status, keyring, or `app_storage.json`.
#[must_use]
pub fn resolve_desktop_session_email(
    live_email: &str,
    keyring_email: Option<&str>,
    app_storage_email: Option<&str>,
) -> Option<String> {
    [Some(live_email), keyring_email, app_storage_email]
        .into_iter()
        .flatten()
        .find_map(normalize_email)
}

pub async fn discover_antigravity_desktop_app() -> Option<DiscoveredSession> {
    let fallback_email = dirs::home_dir().and_then(|h| {
        read_desktop_last_login_username(&h.join(".config/Antigravity/app_storage.json"))
    });

    let pid_ports = listening_ports_by_pid();
    let mut headless_pid: Option<i32> = None;

    // Check /proc/[0-9]*/cmdline
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(name_str) = file_name.to_str() else {
                continue;
            };
            let Ok(pid) = name_str.parse::<i32>() else {
                continue;
            };

            let cmdline_file = format!("/proc/{pid}/cmdline");
            let Ok(raw_bytes) = fs::read(&cmdline_file) else {
                continue;
            };
            let cmd = String::from_utf8_lossy(&raw_bytes).replace('\0', " ");

            if !is_desktop_language_server_cmdline(&cmd) {
                continue;
            }

            if headless_pid.is_none() {
                headless_pid = Some(pid);
            }

            let csrf = CSRF_RE
                .captures(&cmd)
                .map(|c| c[1].to_string())
                .unwrap_or_default();

            if !csrf.is_empty()
                && let Some(ports) = pid_ports.get(&pid)
            {
                for &port in ports {
                    if let Some((live_email, live_name, tier)) =
                        query_local_user_status(port, &csrf).await
                    {
                        let keyring_email = if live_email.is_empty() {
                            get_desktop_keyring_token().await.map(|rec| rec.email)
                        } else {
                            None
                        };

                        if let Some(final_email) = resolve_desktop_session_email(
                            &live_email,
                            keyring_email.as_deref(),
                            fallback_email.as_deref(),
                        ) {
                            return Some(DiscoveredSession {
                                email: final_email,
                                name: Some(live_name),
                                port: Some(port),
                                csrf_token: Some(csrf),
                                tier: Some(tier),
                                surface: Some("Antigravity Desktop App".into()),
                                pid: Some(pid),
                            });
                        }
                    }
                }
            }
        }
    }

    // Antigravity v2.35+ passes --read_host_bridge_token_from_stdin instead of --csrf_token
    // on the language_server command line; fall back to keyring / app_storage.json when the
    // desktop language_server process is running.
    if let Some(pid) = headless_pid {
        let keyring_email = get_desktop_keyring_token().await.map(|rec| rec.email);
        if let Some(final_email) =
            resolve_desktop_session_email("", keyring_email.as_deref(), fallback_email.as_deref())
        {
            return Some(DiscoveredSession {
                email: final_email,
                name: None,
                port: None,
                csrf_token: None,
                tier: None,
                surface: Some("Antigravity Desktop App".into()),
                pid: Some(pid),
            });
        }
    }

    // Secondary fallback: main.log
    let log_file: Option<PathBuf> =
        dirs::home_dir().map(|h| h.join(".config/Antigravity/logs/main.log"));
    if let Some(path) = log_file
        && path.exists()
        && let Ok(f) = File::open(path)
    {
        let reader = BufReader::new(f);
        let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();

        let mut csrf_token = None;
        let mut ports = Vec::new();

        for line in lines.iter().rev() {
            if csrf_token.is_none()
                && let Some(m_csrf) = CSRF_RE.captures(line)
            {
                csrf_token = Some(m_csrf[1].to_string());
            }
            if let Some(m_port) = PORT_RE.captures(line)
                && let Ok(p) = m_port[1].parse::<u16>()
                && !ports.contains(&p)
            {
                ports.push(p);
            }
        }

        if let Some(csrf) = csrf_token {
            for port in ports {
                if let Some((live_email, live_name, tier)) =
                    query_local_user_status(port, &csrf).await
                {
                    let keyring_email = if live_email.is_empty() {
                        get_desktop_keyring_token().await.map(|rec| rec.email)
                    } else {
                        None
                    };

                    if let Some(final_email) = resolve_desktop_session_email(
                        &live_email,
                        keyring_email.as_deref(),
                        fallback_email.as_deref(),
                    ) {
                        return Some(DiscoveredSession {
                            email: final_email,
                            name: Some(live_name),
                            port: Some(port),
                            csrf_token: Some(csrf),
                            tier: Some(tier),
                            surface: Some("Antigravity Desktop App".into()),
                            pid: None,
                        });
                    }
                }
            }
        }
    }

    None
}

pub async fn discover_antigravity_ide() -> Option<DiscoveredSession> {
    let pid_ports = listening_ports_by_pid();

    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(name_str) = file_name.to_str() else {
                continue;
            };
            let Ok(pid) = name_str.parse::<i32>() else {
                continue;
            };

            let cmdline_file = format!("/proc/{pid}/cmdline");
            if let Ok(raw_bytes) = fs::read(&cmdline_file) {
                let cmd = String::from_utf8_lossy(&raw_bytes).replace('\0', " ");

                if cmd.contains("language_server")
                    && (cmd.contains("antigravity-ide") || cmd.contains("subclient_type ide"))
                {
                    let csrf = CSRF_RE
                        .captures(&cmd)
                        .map(|c| c[1].to_string())
                        .unwrap_or_default();

                    if let Some(ports) = pid_ports.get(&pid) {
                        for &port in ports {
                            if let Some((live_email, live_name, tier)) =
                                query_local_user_status(port, &csrf).await
                                && !live_email.is_empty()
                            {
                                return Some(DiscoveredSession {
                                    email: live_email,
                                    name: Some(live_name),
                                    port: Some(port),
                                    csrf_token: Some(csrf),
                                    tier: Some(tier),
                                    surface: Some("Antigravity IDE".into()),
                                    pid: Some(pid),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    None
}
