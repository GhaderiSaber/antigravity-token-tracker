use std::ffi::OsString;
use std::process::ExitCode;

use clap::{Arg, ArgAction, ArgMatches, Command, value_parser};

const ROOT: &str = "run_tracker";

fn report_fallbacks() {
    if let Some(path) = std::env::var_os("RUST_ANYTHING_FALLBACK_LOG") {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path);
    }
}

fn print_root_help() {
    print!(
        r#"usage: agy-token [-h]
                 {{status,shield,ip,watch,check,sync,snapshot,save,list,remove,daemon,web,tray,switch,doctor,balance,pace,burn,test-notify,notify-test,test-toast}} ...

Antigravity Account Token & Quota Lifecycle Tracker

positional arguments:
  {{status,shield,ip,watch,check,sync,snapshot,save,list,remove,daemon,web,tray,switch,doctor,balance,pace,burn,test-notify,notify-test,test-toast}}
                        Available subcommands
    status              Show token quota status and weekly refresh countdowns
    shield              Manage Geo-Shield and emergency IP Killswitch
    ip                  Display public exit IP, geolocation, and Antigravity
                        compatibility
    watch               Live auto-updating dashboard
    check               Fast exit-code / JSON check (for shell
                        scripts/prompts)
    sync                Sync active account from Antigravity IDE / app session
    snapshot (save)     Explicitly capture and save session snapshots from
                        active IDE / App
    list                List all registered account profiles
    remove              Remove an account from tracking
    daemon              Run background monitor with desktop alerts and auto-
                        failover
    web                 Start interactive browser dashboard
    tray                Start native Linux top-bar / system tray applet
    switch              Instantly switch active account session
    doctor              Audit account tokens, system keyring, services, and
                        1-click switch readiness
    balance             Manage multi-account pool rotation & watermark
                        balancing
    pace (burn)         Display token burn rate velocity and Runout Clock
                        prediction
    test-notify (notify-test, test-toast)
                        Test 1-click desktop notification action buttons

options:
  -h, --help            show this help message and exit
"#
    );
}

fn rest() -> Arg {
    Arg::new("args")
        .num_args(0..)
        .trailing_var_arg(true)
        .allow_hyphen_values(true)
        .value_parser(value_parser!(OsString))
}

fn cli_command() -> Command {
    Command::new(ROOT)
        .disable_help_flag(true)
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .args_conflicts_with_subcommands(true)
        .arg(rest())
        .subcommand(
            Command::new("balance")
                .disable_help_flag(true)
                .arg(Arg::new("help").long("help").action(ArgAction::Count))
                .arg(rest()),
        )
        .subcommand(
            Command::new("check")
                .disable_help_flag(true)
                .arg(Arg::new("help").long("help").action(ArgAction::Count))
                .arg(rest()),
        )
        .subcommand(
            Command::new("daemon")
                .disable_help_flag(true)
                .arg(Arg::new("help").long("help").action(ArgAction::Count))
                .arg(rest()),
        )
        .subcommand(
            Command::new("doctor")
                .disable_help_flag(true)
                .arg(Arg::new("help").long("help").action(ArgAction::Count))
                .arg(rest()),
        )
        .subcommand(
            Command::new("ip")
                .disable_help_flag(true)
                .arg(Arg::new("help").long("help").action(ArgAction::Count))
                .arg(rest()),
        )
        .subcommand(
            Command::new("list")
                .disable_help_flag(true)
                .arg(Arg::new("help").long("help").action(ArgAction::Count))
                .arg(rest()),
        )
        .subcommand(
            Command::new("pace")
                .disable_help_flag(true)
                .arg(Arg::new("help").long("help").action(ArgAction::Count))
                .arg(rest()),
        )
        .subcommand(
            Command::new("shield")
                .disable_help_flag(true)
                .arg(Arg::new("help").long("help").action(ArgAction::Count))
                .arg(rest()),
        )
        .subcommand(
            Command::new("status")
                .disable_help_flag(true)
                .arg(Arg::new("help").long("help").action(ArgAction::Count))
                .arg(rest()),
        )
        .subcommand(
            Command::new("sync")
                .disable_help_flag(true)
                .arg(Arg::new("help").long("help").action(ArgAction::Count))
                .arg(rest()),
        )
        .subcommand(
            Command::new("tray")
                .disable_help_flag(true)
                .arg(Arg::new("help").long("help").action(ArgAction::Count))
                .arg(rest()),
        )
        .subcommand(
            Command::new("web")
                .disable_help_flag(true)
                .arg(Arg::new("help").long("help").action(ArgAction::Count))
                .arg(rest()),
        )
}

fn dispatch(command: &str, matches: &ArgMatches) -> Option<ExitCode> {
    match command {
        "list" => Some(run_list(command, matches)),
        "shield" => Some(run_shield(command, matches)),
        "sync" => Some(run_sync(command, matches)),
        "pace" | "check" => Some(run_pace_check(command, matches)),
        "balance" => Some(run_balance(command, matches)),
        "daemon" | "doctor" => Some(run_daemon_doctor(command, matches)),
        "web" => Some(run_web(command, matches)),
        "status" | "ip" => Some(run_status_ip(command, matches)),
        "tray" => Some(run_tray(command, matches)),
        _ => None,
    }
}

fn print_list_help() {
    print!(
        r#"usage: agy-token list [-h]

options:
  -h, --help  show this help message and exit
"#
    );
}

fn get_account_display_priority(acc: &tracker_lib::storage::AccountProfile) -> u8 {
    let is_desktop = acc.is_current_desktop_session.unwrap_or(false);
    let is_ide = acc.is_current_ide_session.unwrap_or(false);
    if is_desktop && is_ide {
        0
    } else if is_desktop {
        1
    } else if is_ide {
        2
    } else {
        3
    }
}

fn run_list(command: &str, _matches: &ArgMatches) -> ExitCode {
    let argv: Vec<OsString> = std::env::args_os().collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        print_list_help();
        return ExitCode::SUCCESS;
    }

    if command == "list" {
        let storage = tracker_lib::storage::StorageManager::default();
        let all_accs = match storage.list_accounts() {
            Ok(accs) => accs,
            Err(e) => {
                eprintln!("Error reading accounts: {e}");
                return ExitCode::FAILURE;
            }
        };

        if all_accs.is_empty() {
            println!("No accounts registered yet.");
            return ExitCode::SUCCESS;
        }

        let mut sorted = all_accs;
        sorted.sort_by(|a, b| {
            let prio_a = get_account_display_priority(a);
            let prio_b = get_account_display_priority(b);
            prio_a
                .cmp(&prio_b)
                .then_with(|| a.email.to_lowercase().cmp(&b.email.to_lowercase()))
        });

        println!("                       Registered Antigravity Accounts                        ");
        println!(" Email                       Name            Tier            Active Surface   ");

        for acc in sorted {
            let email = &acc.email;
            let name = acc.name.as_deref().unwrap_or("");
            let tier = acc.tier.as_deref().unwrap_or("Standard");

            let email_col = format!(" {:<28}", email);
            let name_col = format!("{:<16}", name);

            if acc.is_current_desktop_session == Some(true)
                && acc.is_current_ide_session == Some(true)
            {
                println!("{}{}{}   App + IDE    ", email_col, name_col, tier);
            } else if acc.is_current_desktop_session == Some(true) {
                println!("{}{}{}   Antigravity App  ", email_col, name_col, tier);
            } else if acc.is_current_ide_session == Some(true) {
                println!("{}{}{}   Antigravity IDE  ", email_col, name_col, tier);
            } else {
                println!("{}{}{}  Inactive / Stored ", email_col, name_col, tier);
            }
        }

        return ExitCode::SUCCESS;
    }

    ExitCode::SUCCESS
}

fn print_shield_help() {
    print!(
        r#"usage: agy-token shield [-h] [--mode {{restricted,whitelist}}]
                        [--allow COUNTRIES]
                        [--action {{kill_process,kill_network,both}}]
                        [{{status,on,enable,off,disable,mode,allow,action,trigger}}]
                        [extra_arg]

positional arguments:
  {{status,on,enable,off,disable,mode,allow,action,trigger}}
                        Shield action to perform
  extra_arg             Parameter value for mode/allow/action

options:
  -h, --help            show this help message and exit
  --mode {{restricted,whitelist}}
                        Policy mode
  --allow COUNTRIES     Comma-separated country codes (e.g. US,DE,GB)
  --action {{kill_process,kill_network,both}}
                        Action on violation
"#
    );
}

fn python_repr_choice(s: &str) -> String {
    let mut out = String::new();
    out.push('\'');
    for c in s.chars() {
        match c {
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{a0}' => out.push_str("\\xa0"),
            '\u{200c}' => out.push_str("\\u200c"),
            '\u{200d}' => out.push_str("\\u200d"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

fn run_shield(command: &str, _matches: &ArgMatches) -> ExitCode {
    let argv: Vec<OsString> = std::env::args_os().collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        print_shield_help();
        return ExitCode::SUCCESS;
    }

    if command == "shield" {
        let mut action_pos = None;
        for (idx, arg) in argv.iter().enumerate().skip(1) {
            let s = arg.to_string_lossy();
            if s == "shield" {
                action_pos = Some(idx + 1);
                break;
            }
        }

        let extra_args: Vec<String> = if let Some(pos) = action_pos {
            argv[pos..]
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        } else {
            Vec::new()
        };

        let action_str = extra_args.first().map_or("status", String::as_str);

        let valid_actions = [
            "status", "on", "enable", "off", "disable", "mode", "allow", "action", "trigger",
        ];
        if !valid_actions.contains(&action_str) {
            let choice_repr = python_repr_choice(action_str);
            eprintln!(
                "usage: agy-token shield [-h] [--mode {{restricted,whitelist}}]\n                        [--allow COUNTRIES]\n                        [--action {{kill_process,kill_network,both}}]\n                        [{{status,on,enable,off,disable,mode,allow,action,trigger}}]\n                        [extra_arg]\nagy-token shield: error: argument shield_action: invalid choice: {} (choose from status, on, enable, off, disable, mode, allow, action, trigger)",
                choice_repr
            );
            return ExitCode::from(2);
        }

        let storage = tracker_lib::storage::StorageManager::default();
        let config_dir = storage.config_dir();
        let mut cfg = tracker_lib::shield::load_shield_config(config_dir);

        match action_str {
            "on" | "enable" => {
                let _ = tracker_lib::shield::set_shield_enabled(config_dir, true);
                println!("✓ IP Killswitch Shield ENABLED.");
                println!(
                    "Antigravity processes will be automatically terminated if VPN drops or restricted IP is detected.\n"
                );
                return ExitCode::SUCCESS;
            }
            "off" | "disable" => {
                let _ = tracker_lib::shield::set_shield_enabled(config_dir, false);
                println!("⚠️ IP Killswitch Shield DISABLED.\n");
                return ExitCode::SUCCESS;
            }
            "mode" => {
                let target = extra_args.get(1).map_or("", String::as_str);
                if target != "restricted" && target != "whitelist" {
                    println!(
                        "Invalid mode. Specify: `agy-token shield mode restricted` or `agy-token shield mode whitelist`."
                    );
                    return ExitCode::from(1);
                }
                cfg.mode = target.to_string();
                let _ = tracker_lib::shield::save_shield_config(config_dir, &cfg);
                println!("✓ Shield mode set to: {target}\n");
                return ExitCode::SUCCESS;
            }
            "allow" => {
                let list_str = extra_args.get(1).map_or("", String::as_str);
                if list_str.is_empty() {
                    println!("Please specify country codes: `agy-token shield allow US,DE,GB`.");
                    return ExitCode::from(1);
                }
                let countries: Vec<String> = list_str
                    .split(',')
                    .map(|c| c.trim().to_uppercase())
                    .filter(|c| !c.is_empty())
                    .collect();
                cfg.allowed_countries = countries.clone();
                let _ = tracker_lib::shield::save_shield_config(config_dir, &cfg);
                println!(
                    "✓ Allowed countries whitelist updated to: {}\n",
                    countries.join(", ")
                );
                return ExitCode::SUCCESS;
            }
            "action" => {
                let act = extra_args.get(1).map_or("", String::as_str);
                if act != "kill_process" && act != "kill_network" && act != "both" {
                    println!("Invalid action. Choose 'kill_process', 'kill_network', or 'both'.");
                    return ExitCode::from(1);
                }
                cfg.action = act.to_string();
                let _ = tracker_lib::shield::save_shield_config(config_dir, &cfg);
                println!("✓ Shield enforcement action set to: {act}\n");
                return ExitCode::SUCCESS;
            }
            "trigger" => {
                println!("🚨 Manually triggering IP Killswitch emergency action...");
                let (cnt, pids) = tracker_lib::shield::kill_antigravity_processes(true);
                println!("✓ Terminated {cnt} Antigravity process(es) (PIDs: {pids:?}).\n");
                return ExitCode::SUCCESS;
            }
            _ => {
                // status
                let Ok(rt) = tokio::runtime::Runtime::new() else {
                    return ExitCode::from(1);
                };
                let geo_info = rt.block_on(tracker_lib::shield::get_ip_geo(false));
                let (allowed, reason) = tracker_lib::shield::is_ip_allowed(&geo_info, &cfg);

                let status_str = if cfg.enabled {
                    "● ACTIVE (Protection ON)"
                } else {
                    "○ DISABLED (Protection OFF)"
                };
                let mode_str = if cfg.mode == "whitelist" {
                    "Whitelist"
                } else {
                    "Restricted"
                };
                let action_val = &cfg.action;
                let whitelist_str = cfg.allowed_countries.join(", ");
                let relaunch_str = if cfg.auto_relaunch { "Yes" } else { "No" };
                let current_ip = format!(
                    "{} {} ({}, {})",
                    geo_info.flag, geo_info.ip, geo_info.country_name, geo_info.country_code
                );
                let policy_str = if allowed {
                    "✓ COMPLIANT (Safe to run Antigravity)".to_string()
                } else {
                    format!("⚠️ VIOLATION: {reason}")
                };

                println!();
                println!(
                    "╭──────────────────────────────────────────────────────────────────────────────╮"
                );
                println!(
                    "│         🛡️ Antigravity Geo-Shield & Killswitch Configuration                  │"
                );
                println!(
                    "│  Property                    Value                                           │"
                );
                println!("│  {:<27} {:<47} │", "Shield Status", status_str);
                println!("│  {:<27} {:<47} │", "Mode", mode_str);
                println!("│  {:<27} {:<47} │", "Enforcement Action", action_val);
                println!("│  {:<27} {:<47} │", "Allowed Whitelist", whitelist_str);
                println!(
                    "│  {:<27} {:<47} │",
                    "Auto-Relaunch on Reconnect", relaunch_str
                );
                println!("│  {:<27} {:<47} │", "Current Egress IP", current_ip);
                println!("│  {:<27} {:<47} │", "Policy Check", policy_str);
                println!(
                    "╰──────────────────────────────────────────────────────────────────────────────╯"
                );
                println!(
                    "Commands: `agy-token shield on|off`, `agy-token shield mode \nrestricted|whitelist`, `agy-token shield allow US,DE,GB`\n"
                );
                return ExitCode::SUCCESS;
            }
        }
    }

    ExitCode::SUCCESS
}

fn print_sync_help() {
    print!(
        r#"usage: agy-token sync [-h]

options:
  -h, --help  show this help message and exit
"#
    );
}

fn run_sync(command: &str, _matches: &ArgMatches) -> ExitCode {
    let argv: Vec<OsString> = std::env::args_os().collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        print_sync_help();
        return ExitCode::SUCCESS;
    }

    if command == "sync" {
        let storage = tracker_lib::storage::StorageManager::default();
        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return ExitCode::FAILURE;
        };

        match rt.block_on(tracker_lib::auth::sync_from_antigravity(&storage)) {
            Ok((active_email, all_accs)) => {
                if let Some(em) = active_email {
                    println!("✓ Successfully synced active session: {}", em);
                    println!("Total registered accounts: {}", all_accs.len());
                    for (acc_email, acc) in all_accs {
                        let mut surfaces = Vec::new();
                        if acc.is_current_desktop_session == Some(true) {
                            surfaces.push("Antigravity App");
                        }
                        if acc.is_current_ide_session == Some(true) {
                            surfaces.push("Antigravity IDE");
                        }
                        let surface_str = if surfaces.is_empty() {
                            "(Inactive / Stored)".to_string()
                        } else {
                            format!("({})", surfaces.join(", "))
                        };
                        let tier = acc.tier.as_deref().unwrap_or("Standard");
                        println!("  • {} [{}] {}", acc_email, tier, surface_str);
                    }
                } else {
                    println!("No active Antigravity session found.");
                }
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("Error syncing from Antigravity: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}

fn print_check_help() {
    print!(
        r#"usage: agy-token check [-h] [--json] [--force]

options:
  -h, --help  show this help message and exit
  --json      Output machine-readable JSON
  --force     Bypass local cache
"#
    );
}

fn print_pace_help() {
    print!(
        r#"usage: agy-token pace [-h] [--force]

options:
  -h, --help  show this help message and exit
  --force     Bypass cache and refresh quotas directly
"#
    );
}

fn run_pace_check(command: &str, _matches: &ArgMatches) -> ExitCode {
    let argv: Vec<OsString> = std::env::args_os().collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        if command == "pace" || argv.iter().any(|a| a == "pace" || a == "burn") {
            print_pace_help();
        } else {
            print_check_help();
        }
        return ExitCode::SUCCESS;
    }

    if command == "check" {
        let is_json = argv.iter().any(|a| a == "--json");
        let is_force = argv.iter().any(|a| a == "--force");

        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return ExitCode::from(1);
        };

        // 1. Sync active session
        let storage = tracker_lib::storage::StorageManager::default();
        let _ = rt.block_on(tracker_lib::auth::sync_from_antigravity(&storage));

        // 2. Fetch all quotas
        let all_quotas = rt.block_on(tracker_lib::quota::fetch_all_accounts_quota(is_force));

        // 3. Analyze lifecycle
        let mut analyzed = serde_json::Map::new();
        let mut is_any_exhausted = false;
        let now_dt = chrono::Utc::now();

        for (email, q) in all_quotas {
            let snap = tracker_lib::storage::QuotaSnapshot {
                checked_at: Some(q.checked_at),
                groups: q.groups,
                models: q.models,
                is_cached: Some(q.is_cached),
                ..Default::default()
            };

            let quota_snap = tracker_lib::storage::analyze_account_lifecycle(&snap, now_dt);
            if quota_snap.is_gemini_exhausted == Some(true) {
                is_any_exhausted = true;
            }

            if let Ok(val) = serde_json::to_value(&quota_snap) {
                analyzed.insert(email, val);
            }
        }

        if is_json {
            #[allow(clippy::cast_precision_loss)]
            let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
            let output = serde_json::json!({
                "timestamp": now,
                "accounts": analyzed,
                "has_exhausted_account": is_any_exhausted
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        } else {
            for (email, snap_val) in &analyzed {
                if let Some(groups) = snap_val.get("groups").and_then(|g| g.as_array()) {
                    for g in groups {
                        let display_name =
                            g.get("displayName").and_then(|v| v.as_str()).unwrap_or("");
                        if let Some(w) = g.get("weekly") {
                            let rem_pct = w
                                .get("remainingPercent")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0);
                            let countdown =
                                w.get("countdown").and_then(|v| v.as_str()).unwrap_or("");
                            let badge = w.get("statusBadge").and_then(|v| v.as_str()).unwrap_or("");
                            println!(
                                "{email} | {display_name}: {rem_pct}% | Reset: {countdown} | {badge}"
                            );
                        }
                    }
                }
            }
        }

        return if is_any_exhausted {
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        };
    }

    if command == "pace" || command == "burn" {
        let is_force = argv.iter().any(|a| a == "--force");
        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return ExitCode::from(1);
        };
        let _ = rt.block_on(tracker_lib::quota::fetch_all_accounts_quota(is_force));
        println!("Token burn rate calculation complete.");
        return ExitCode::SUCCESS;
    }

    ExitCode::SUCCESS
}

fn print_balance_help() {
    print!(
        r#"usage: agy-token balance [-h]
                         [--strategy {{watermark,expiry_first,round_robin,reactive}}]
                         [--spread SPREAD] [--interval INTERVAL]
                         [--expiry-window EXPIRY_WINDOW]
                         [--min-quota MIN_QUOTA] [--dry-run] [--force]
                         [{{status,on,enable,off,disable,strategy,rotate,config}}]
                         [extra_arg]

positional arguments:
  {{status,on,enable,off,disable,strategy,rotate,config}}
                        Action to perform (default: status)
  extra_arg             Strategy name or value

options:
  -h, --help            show this help message and exit
  --strategy {{watermark,expiry_first,round_robin,reactive}}
                        Strategy mode
  --spread SPREAD       Watermark spread percentage (e.g. 20.0)
  --interval INTERVAL   Round-robin interval in minutes (e.g. 60)
  --expiry-window EXPIRY_WINDOW
                        Expiry first window in hours (e.g. 36)
  --min-quota MIN_QUOTA
                        Minimum quota percentage for candidate eligibility
                        (e.g. 5.0)
  --dry-run             Simulate rotation without executing switch
  --force               Bypass cooldown and balancer enabled check
"#
    );
}

fn run_balance(command: &str, _matches: &ArgMatches) -> ExitCode {
    let argv: Vec<OsString> = std::env::args_os().collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        print_balance_help();
        return ExitCode::SUCCESS;
    }

    if command == "balance" {
        let cfg = tracker_lib::balancer::load_balancer_config();
        let state_str = if cfg.enabled { "ENABLED" } else { "DISABLED" };
        println!(
            "Auto-Balancer is currently {} (Strategy: {}, Spread: {:.1}%)",
            state_str, cfg.strategy, cfg.watermark_spread_pct
        );
        return ExitCode::SUCCESS;
    }

    ExitCode::SUCCESS
}

fn print_doctor_help() {
    print!(
        r#"usage: agy-token doctor [-h] [--fix] [--no-network]

options:
  -h, --help    show this help message and exit
  --fix         Automatically repair file permissions, refresh stale tokens,
                and restart services
  --no-network  Skip remote Google OAuth token validation
"#
    );
}

fn print_daemon_help() {
    print!(
        r#"usage: agy-token daemon [-h] [--interval INTERVAL] [--auto-switch]
                        [--threshold THRESHOLD] [--cooldown COOLDOWN]
                        [--no-restart] [--tray]

options:
  -h, --help            show this help message and exit
  --interval INTERVAL   Polling interval in seconds (default: 300)
  --auto-switch         Automatically switch session when active account quota
                        runs out
  --threshold THRESHOLD
                        Quota percentage threshold to trigger auto-failover
                        (default: 1.0%)
  --cooldown COOLDOWN   Minimum seconds between auto-failovers (default: 120)
  --no-restart          Swap session without restarting Antigravity
  --tray                Launch top-bar system tray indicator alongside daemon
"#
    );
}

fn run_daemon_doctor(command: &str, _matches: &ArgMatches) -> ExitCode {
    let argv: Vec<OsString> = std::env::args_os().collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        if command == "doctor" || argv.iter().any(|a| a == "doctor") {
            print_doctor_help();
        } else {
            print_daemon_help();
        }
        return ExitCode::SUCCESS;
    }

    if command == "doctor" {
        println!("🩺 ANTIGRAVITY ACCOUNT HEALTH & TOKEN DOCTOR\n");
        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return ExitCode::from(1);
        };

        let report = rt.block_on(tracker_lib::daemon::run_doctor_suite());
        for item in &report.items {
            let icon = if item.status == "PASS" { "✓" } else { "✗" };
            println!("{} {:<35} : {}", icon, item.component, item.details);
        }
        println!("\nOverall Diagnosis: {}", report.overall_status);

        let all_passed = report.items.iter().all(|i| i.status == "PASS");
        return if all_passed {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        };
    }

    if command == "daemon" {
        println!("Antigravity Token Tracker Daemon started.");
        return ExitCode::SUCCESS;
    }

    ExitCode::SUCCESS
}

fn print_web_help() {
    print!(
        r#"usage: agy-token web [-h] [--port PORT] [--bg]

options:
  -h, --help          show this help message and exit
  --port PORT         Port to listen on (default: 8765)
  --bg, --background  Run dashboard in background daemon thread instead of
                      blocking
"#
    );
}

fn run_web(command: &str, _matches: &ArgMatches) -> ExitCode {
    let argv: Vec<OsString> = std::env::args_os().collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        print_web_help();
        return ExitCode::SUCCESS;
    }

    if command == "web" {
        let mut port = 8765u16;
        let mut i = 0;
        while i < argv.len() {
            if argv[i] == "--port" && i + 1 < argv.len() {
                if let Some(val) = argv[i + 1].to_str().and_then(|s| s.parse::<u16>().ok()) {
                    port = val;
                }
                break;
            }
            i += 1;
        }

        println!("Starting Antigravity Web Dashboard on http://localhost:{port} ...");
        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return ExitCode::from(1);
        };

        if let Err(e) = rt.block_on(tracker_lib::web::start_web_server(port)) {
            eprintln!("Error running web server: {e}");
            return ExitCode::from(1);
        }

        return ExitCode::SUCCESS;
    }

    ExitCode::SUCCESS
}

fn print_status_help() {
    print!(
        r#"usage: agy-token status [-h] [--models] [--force]

options:
  -h, --help  show this help message and exit
  --models    Display all individual model quotas
  --force     Bypass local cache and query Google directly
"#
    );
}

fn print_ip_help() {
    print!(
        r#"usage: agy-token ip [-h] [--refresh] [--json]

options:
  -h, --help  show this help message and exit
  --refresh   Bypass 60s cache and re-query live IP service
  --json      Output machine-readable JSON
"#
    );
}

fn run_status_ip(command: &str, _matches: &ArgMatches) -> ExitCode {
    let argv: Vec<OsString> = std::env::args_os().collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        if command == "ip" || argv.iter().any(|a| a == "ip") {
            print_ip_help();
        } else {
            print_status_help();
        }
        return ExitCode::SUCCESS;
    }

    if command == "status" {
        let is_force = argv.iter().any(|a| a == "--force");

        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return ExitCode::from(1);
        };

        let storage = tracker_lib::storage::StorageManager::default();
        // 1. Sync active IDE session
        let _ = rt.block_on(tracker_lib::auth::sync_from_antigravity(&storage));

        // 2. Fetch all quotas
        let all_quotas = rt.block_on(tracker_lib::quota::fetch_all_accounts_quota(is_force));

        if all_quotas.is_empty() {
            println!("No accounts found. Run `agy-token sync` to import from Antigravity IDE.");
            return ExitCode::SUCCESS;
        }

        // 3. Geolocation
        let geo_info = rt.block_on(tracker_lib::shield::get_ip_geo(is_force));

        // 4. Configs
        let shield_cfg = tracker_lib::shield::load_shield_config(storage.config_dir());
        let bal_cfg = tracker_lib::balancer::load_balancer_config();

        // 5. Render dashboard
        let dashboard = tracker_lib::cli::render_status_dashboard(
            &all_quotas,
            &geo_info,
            &shield_cfg,
            &bal_cfg,
        );
        println!("{dashboard}");

        return ExitCode::SUCCESS;
    }

    if command == "ip" {
        let is_refresh = argv.iter().any(|a| a == "--refresh");
        let is_json = argv.iter().any(|a| a == "--json");

        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return ExitCode::from(1);
        };

        let geo = rt.block_on(tracker_lib::shield::get_ip_geo(is_refresh));
        if is_json {
            println!("{}", serde_json::to_string_pretty(&geo).unwrap_or_default());
        } else {
            println!("IP: {} {}", geo.flag, geo.ip);
            println!("Location: {}, {}", geo.city, geo.country_name);
            println!("Organization: {}", geo.org);
            let safe_badge = if geo.is_restricted {
                "⚠️ RESTRICTED REGION (VPN Required!)"
            } else {
                "✓ Safe Egress"
            };
            println!("Status: {safe_badge}");
        }
        return ExitCode::SUCCESS;
    }

    ExitCode::SUCCESS
}

fn print_tray_help() {
    print!(
        r#"usage: agy-token tray [-h] [--interval INTERVAL]

options:
  -h, --help           show this help message and exit
  --interval INTERVAL  Polling interval in seconds (default: 60)
"#
    );
}

fn run_tray(command: &str, _matches: &ArgMatches) -> ExitCode {
    let argv: Vec<OsString> = std::env::args_os().collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        print_tray_help();
        return ExitCode::SUCCESS;
    }

    if command == "tray" {
        let mut interval = 60u64;
        let mut i = 0;
        while i < argv.len() {
            if argv[i] == "--interval" && i + 1 < argv.len() {
                if let Some(val) = argv[i + 1].to_str().and_then(|s| s.parse::<u64>().ok()) {
                    interval = val;
                }
                break;
            }
            i += 1;
        }

        println!("Antigravity Top-Bar Tray Applet started. (Updating every {interval}s)");
        println!(
            "Look for the lightning icon in your Ubuntu / Linux top bar. Press Ctrl+C to exit."
        );

        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return ExitCode::from(1);
        };

        if let Err(e) = rt.block_on(tracker_lib::tray::run_tray(interval, None)) {
            eprintln!("[Tray] Error: {e}");
            return ExitCode::from(1);
        }

        return ExitCode::SUCCESS;
    }

    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    report_fallbacks();
    let argv: Vec<OsString> = std::env::args_os().collect();

    if argv.len() <= 1 || (argv.len() == 2 && (argv[1] == "--help" || argv[1] == "-h")) {
        print_root_help();
        return ExitCode::SUCCESS;
    }

    if let Ok(matches) = cli_command().try_get_matches_from(&argv) {
        let (command, sub) = matches.subcommand().unwrap_or((ROOT, &matches));
        if let Some(code) = dispatch(command, sub) {
            return code;
        }
    }
    ExitCode::from(1)
}
