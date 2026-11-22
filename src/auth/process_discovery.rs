use super::keyring::get_desktop_keyring_token;
use super::models::DiscoveredSession;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

static PID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"pid=(\d+)").unwrap());
static PORT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"127\.0\.0\.1:(\d+)").unwrap());
static CSRF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"--csrf_token[=\s]+([^\s]+)").unwrap());

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

pub async fn discover_antigravity_desktop_app() -> Option<DiscoveredSession> {
    let mut fallback_email = String::new();
    let storage_file = dirs::home_dir().map(|h| h.join(".config/Antigravity/app_storage.json"));
    if let Some(path) = storage_file
        && path.exists()
        && let Ok(content) = fs::read_to_string(&path)
        && let Ok(data) = serde_json::from_str::<Value>(&content)
        && let Some(em) = data
            .get("jetski.onboarding.lastLoginUsername")
            .and_then(|v| v.as_str())
    {
        fallback_email = em.trim().to_lowercase();
    }

    let pid_ports = listening_ports_by_pid();

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
            if let Ok(raw_bytes) = fs::read(&cmdline_file) {
                let cmd = String::from_utf8_lossy(&raw_bytes).replace('\0', " ");

                let is_desktop_ls = cmd.contains("language_server")
                    && (cmd.contains("subclient_type hub")
                        || (cmd.contains("app_data_dir antigravity")
                            && !cmd.contains("app_data_dir antigravity-ide"))
                        || (cmd.contains("/snap/antigravity/")
                            && !cmd.contains("/snap/antigravity-ide")))
                    && !(cmd.contains("antigravity-ide") || cmd.contains("subclient_type ide"));

                if is_desktop_ls {
                    let csrf = CSRF_RE
                        .captures(&cmd)
                        .map(|c| c[1].to_string())
                        .unwrap_or_default();

                    if let Some(ports) = pid_ports.get(&pid) {
                        for &port in ports {
                            if let Some((live_email, live_name, tier)) =
                                query_local_user_status(port, &csrf).await
                            {
                                let keyring_email =
                                    get_desktop_keyring_token().await.map(|rec| rec.email);
                                let final_email = if !live_email.is_empty() {
                                    live_email
                                } else if let Some(k_em) = keyring_email {
                                    if !k_em.is_empty() {
                                        k_em
                                    } else {
                                        fallback_email.clone()
                                    }
                                } else {
                                    fallback_email.clone()
                                };

                                if !final_email.is_empty() {
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
                    let keyring_email = get_desktop_keyring_token().await.map(|rec| rec.email);
                    let final_email = if !live_email.is_empty() {
                        live_email
                    } else if let Some(k_em) = keyring_email {
                        if !k_em.is_empty() {
                            k_em
                        } else {
                            fallback_email.clone()
                        }
                    } else {
                        fallback_email.clone()
                    };

                    if !final_email.is_empty() {
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
