use std::collections::BTreeSet;
use std::future::Future;
use std::process::ExitCode;

use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use tracker_lib::balancer::{BalancerStrategy, SwitchSurface};
use tracker_lib::daemon::{DaemonRunOptions, FailoverRestart};
use tracker_lib::shield::models::{ShieldAction, ShieldMode};

const ROOT_HELP: &str = "\
usage: agy-token [-h]
                 {status,shield,ip,watch,check,sync,snapshot,save,list,remove,daemon,web,tray,switch,doctor,balance,pace,burn,test-notify,notify-test,test-toast} ...

Antigravity Account Token & Quota Lifecycle Tracker

positional arguments:
  {status,shield,ip,watch,check,sync,snapshot,save,list,remove,daemon,web,tray,switch,doctor,balance,pace,burn,test-notify,notify-test,test-toast}
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
  -h, --help            show this help message and exit";

const STATUS_HELP: &str = "\
usage: agy-token status [-h] [--models] [--force]

options:
  -h, --help  show this help message and exit
  --models    Display all individual model quotas
  --force     Bypass local cache and query Google directly";

const SHIELD_USAGE: &str = "\
usage: agy-token shield [-h] [--mode {restricted,whitelist}]
                        [--allow COUNTRIES]
                        [--action {kill_process,kill_network,both}]
                        [{status,on,enable,off,disable,mode,allow,action,trigger}]
                        [extra_arg]";

const SHIELD_HELP: &str = "\
usage: agy-token shield [-h] [--mode {restricted,whitelist}]
                        [--allow COUNTRIES]
                        [--action {kill_process,kill_network,both}]
                        [{status,on,enable,off,disable,mode,allow,action,trigger}]
                        [extra_arg]

positional arguments:
  {status,on,enable,off,disable,mode,allow,action,trigger}
                        Shield action to perform
  extra_arg             Parameter value for mode/allow/action

options:
  -h, --help            show this help message and exit
  --mode {restricted,whitelist}
                        Policy mode
  --allow COUNTRIES     Comma-separated country codes (e.g. US,DE,GB)
  --action {kill_process,kill_network,both}
                        Action on violation";

const IP_HELP: &str = "\
usage: agy-token ip [-h] [--refresh] [--json]

options:
  -h, --help  show this help message and exit
  --refresh   Bypass 60s cache and re-query live IP service
  --json      Output machine-readable JSON";

const CHECK_HELP: &str = "\
usage: agy-token check [-h] [--json] [--force]

options:
  -h, --help  show this help message and exit
  --json      Output machine-readable JSON
  --force     Bypass local cache";

const LIST_HELP: &str = "\
usage: agy-token list [-h]

options:
  -h, --help  show this help message and exit";

const DAEMON_HELP: &str = "\
usage: agy-token daemon [-h] [--interval INTERVAL] [--auto-switch]
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
  --tray                Launch top-bar system tray indicator alongside daemon";

const WEB_HELP: &str = "\
usage: agy-token web [-h] [--port PORT] [--bg]

options:
  -h, --help          show this help message and exit
  --port PORT         Port to listen on (default: 8765)
  --bg, --background  Run dashboard in background daemon thread instead of
                      blocking";

const TRAY_HELP: &str = "\
usage: agy-token tray [-h] [--interval INTERVAL]

options:
  -h, --help           show this help message and exit
  --interval INTERVAL  Polling interval in seconds (default: 60)";

const DOCTOR_HELP: &str = "\
usage: agy-token doctor [-h] [--fix] [--no-network]

options:
  -h, --help    show this help message and exit
  --fix         Automatically repair file permissions, refresh stale tokens,
                and restart services
  --no-network  Skip remote Google OAuth token validation";

const BALANCE_HELP: &str = "\
usage: agy-token balance [-h]
                         [--strategy {watermark,expiry_first,round_robin,reactive}]
                         [--spread SPREAD] [--interval INTERVAL]
                         [--expiry-window EXPIRY_WINDOW]
                         [--min-quota MIN_QUOTA] [--dry-run] [--force]
                         [{status,on,enable,off,disable,strategy,rotate,config}]
                         [extra_arg]

positional arguments:
  {status,on,enable,off,disable,strategy,rotate,config}
                        Action to perform (default: status)
  extra_arg             Strategy name or value

options:
  -h, --help            show this help message and exit
  --strategy {watermark,expiry_first,round_robin,reactive}
                        Strategy mode
  --spread SPREAD       Watermark spread percentage (e.g. 20.0)
  --interval INTERVAL   Round-robin interval in minutes (e.g. 60)
  --expiry-window EXPIRY_WINDOW
                        Expiry first window in hours (e.g. 36)
  --min-quota MIN_QUOTA
                        Minimum quota percentage for candidate eligibility
                        (e.g. 5.0)
  --dry-run             Simulate rotation without executing switch
  --force               Bypass cooldown and balancer enabled check";

const PACE_HELP: &str = "\
usage: agy-token pace [-h] [--force]

options:
  -h, --help  show this help message and exit
  --force     Bypass cache and refresh quotas directly";

#[derive(Debug, Parser)]
#[command(
    name = "agy-token",
    about = "Antigravity Account Token & Quota Lifecycle Tracker",
    disable_version_flag = true,
    disable_help_subcommand = true,
    override_help = ROOT_HELP
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, PartialEq, Subcommand)]
enum Commands {
    /// Show token quota status and weekly refresh countdowns
    Status(StatusArgs),
    /// Manage Geo-Shield and emergency IP Killswitch
    Shield(ShieldArgs),
    /// Display public exit IP, geolocation, and Antigravity compatibility
    Ip(IpArgs),
    /// Live auto-updating dashboard
    Watch(WatchArgs),
    /// Fast exit-code / JSON check (for shell scripts/prompts)
    Check(CheckArgs),
    /// Sync active account from Antigravity IDE / app session
    Sync,
    /// Explicitly capture and save session snapshots from active IDE / App
    #[command(alias = "save")]
    Snapshot(SnapshotArgs),
    /// List all registered account profiles
    List(ListArgs),
    /// Remove an account from tracking
    Remove(RemoveArgs),
    /// Run background monitor with desktop alerts and auto-failover
    Daemon(DaemonArgs),
    /// Start interactive browser dashboard
    Web(WebArgs),
    /// Start native Linux top-bar / system tray applet
    Tray(TrayArgs),
    /// Instantly switch active account session
    Switch(SwitchArgs),
    /// Audit account tokens, system keyring, services, and 1-click switch readiness
    Doctor(DoctorArgs),
    /// Manage multi-account pool rotation & watermark balancing
    Balance(BalanceArgs),
    /// Display token burn rate velocity and Runout Clock prediction
    #[command(alias = "burn")]
    Pace(PaceArgs),
    /// Test 1-click desktop notification action buttons
    #[command(name = "test-notify", aliases = ["notify-test", "test-toast"])]
    TestNotify,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(override_help = STATUS_HELP)]
struct StatusArgs {
    /// Display all individual model quotas
    #[arg(long)]
    models: bool,
    /// Bypass local cache and query Google directly
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "snake_case")]
enum ShieldModeArg {
    Restricted,
    Whitelist,
}

impl From<ShieldModeArg> for ShieldMode {
    fn from(value: ShieldModeArg) -> Self {
        match value {
            ShieldModeArg::Restricted => Self::Restricted,
            ShieldModeArg::Whitelist => Self::Whitelist,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "snake_case")]
enum ShieldActionArg {
    KillProcess,
    KillNetwork,
    Both,
}

impl From<ShieldActionArg> for ShieldAction {
    fn from(value: ShieldActionArg) -> Self {
        match value {
            ShieldActionArg::KillProcess => Self::KillProcess,
            ShieldActionArg::KillNetwork => Self::KillNetwork,
            ShieldActionArg::Both => Self::Both,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "snake_case")]
enum ShieldActionCmd {
    Status,
    On,
    Enable,
    Off,
    Disable,
    Mode,
    Allow,
    Action,
    Trigger,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(override_help = SHIELD_HELP)]
struct ShieldArgs {
    /// Policy mode
    #[arg(long, value_enum)]
    mode: Option<ShieldModeArg>,
    /// Comma-separated country codes (e.g. US,DE,GB)
    #[arg(long, value_name = "COUNTRIES")]
    allow: Option<String>,
    /// Action on violation
    #[arg(long, value_enum)]
    action: Option<ShieldActionArg>,
    /// Shield action to perform
    #[arg(value_enum, value_name = "shield_action")]
    shield_action: Option<ShieldActionCmd>,
    /// Parameter value for mode/allow/action
    extra_arg: Option<String>,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(override_help = IP_HELP)]
struct IpArgs {
    /// Bypass 60s cache and re-query live IP service
    #[arg(long)]
    refresh: bool,
    /// Output machine-readable JSON
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
struct WatchArgs {
    /// Refresh interval in seconds
    #[arg(long, default_value_t = 5)]
    interval: u64,
    /// Bypass local cache and query Google directly
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(override_help = CHECK_HELP)]
struct CheckArgs {
    /// Output machine-readable JSON
    #[arg(long)]
    json: bool,
    /// Bypass local cache
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
struct SnapshotArgs {
    /// Account email to snapshot (defaults to active desktop/IDE sessions)
    target: Option<String>,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(override_help = LIST_HELP)]
struct ListArgs {}

#[derive(Debug, Args, PartialEq, Eq)]
struct RemoveArgs {
    /// Account email, 1-based index, or name substring to remove
    identifier: String,
}

#[derive(Debug, Args, PartialEq)]
#[command(override_help = DAEMON_HELP)]
struct DaemonArgs {
    /// Polling interval in seconds
    #[arg(long, default_value_t = 60)]
    interval: u64,
    /// Automatically switch session when active account quota runs out
    #[arg(long)]
    auto_switch: bool,
    /// Quota percentage threshold to trigger auto-failover
    #[arg(long, default_value_t = 1.0, value_parser = parse_finite_f64)]
    threshold: f64,
    /// Minimum seconds between auto-failovers
    #[arg(long, default_value_t = 120.0, value_parser = parse_finite_f64)]
    cooldown: f64,
    /// Swap session without restarting Antigravity
    #[arg(long)]
    no_restart: bool,
    /// Launch top-bar system tray indicator alongside daemon
    #[arg(long)]
    tray: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(override_help = WEB_HELP)]
struct WebArgs {
    /// Port to listen on
    #[arg(long, default_value_t = 8765)]
    port: u16,
    /// Run dashboard in background daemon process instead of blocking
    #[arg(long = "bg", alias = "background")]
    bg: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(override_help = TRAY_HELP)]
struct TrayArgs {
    /// Polling interval in seconds
    #[arg(long, default_value_t = 60)]
    interval: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "snake_case")]
enum SwitchSurfaceArg {
    Desktop,
    Ide,
    Both,
}

impl From<SwitchSurfaceArg> for SwitchSurface {
    fn from(value: SwitchSurfaceArg) -> Self {
        match value {
            SwitchSurfaceArg::Desktop => Self::Desktop,
            SwitchSurfaceArg::Ide => Self::Ide,
            SwitchSurfaceArg::Both => Self::Both,
        }
    }
}

#[derive(Debug, Args, PartialEq, Eq)]
struct SwitchArgs {
    /// Target account email (interactive selector when omitted)
    target: Option<String>,
    /// Target session surface to switch
    #[arg(long, value_enum, default_value_t = SwitchSurfaceArg::Both)]
    surface: SwitchSurfaceArg,
    /// Swap credentials without restarting Antigravity
    #[arg(long)]
    no_restart: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(override_help = DOCTOR_HELP)]
struct DoctorArgs {
    /// Automatically repair file permissions and refresh stale tokens
    #[arg(long)]
    fix: bool,
    /// Skip remote Google OAuth token validation
    #[arg(long)]
    no_network: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "snake_case")]
enum BalancerStrategyArg {
    Watermark,
    ExpiryFirst,
    RoundRobin,
    Reactive,
}

impl From<BalancerStrategyArg> for BalancerStrategy {
    fn from(value: BalancerStrategyArg) -> Self {
        match value {
            BalancerStrategyArg::Watermark => Self::Watermark,
            BalancerStrategyArg::ExpiryFirst => Self::ExpiryFirst,
            BalancerStrategyArg::RoundRobin => Self::RoundRobin,
            BalancerStrategyArg::Reactive => Self::Reactive,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "snake_case")]
enum BalanceActionCmd {
    Status,
    On,
    Enable,
    Off,
    Disable,
    Strategy,
    Rotate,
    Config,
}

#[derive(Debug, Args, PartialEq)]
#[command(override_help = BALANCE_HELP)]
struct BalanceArgs {
    /// Strategy mode
    #[arg(long, value_enum)]
    strategy: Option<BalancerStrategyArg>,
    /// Watermark spread percentage
    #[arg(long, value_parser = parse_finite_f64)]
    spread: Option<f64>,
    /// Round-robin interval in minutes
    #[arg(long)]
    interval: Option<u64>,
    /// Expiry first window in hours
    #[arg(long)]
    expiry_window: Option<u64>,
    /// Minimum quota percentage for candidate eligibility
    #[arg(long, value_parser = parse_finite_f64)]
    min_quota: Option<f64>,
    /// Simulate rotation without executing switch
    #[arg(long)]
    dry_run: bool,
    /// Bypass cooldown and balancer enabled check
    #[arg(long)]
    force: bool,
    /// Action to perform
    #[arg(value_enum)]
    action: Option<BalanceActionCmd>,
    /// Strategy name or value
    extra_arg: Option<String>,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(override_help = PACE_HELP)]
struct PaceArgs {
    /// Bypass cache and refresh quotas directly
    #[arg(long)]
    force: bool,
}

fn parse_finite_f64(raw: &str) -> Result<f64, String> {
    raw.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("invalid finite floating-point number: '{raw}'"))
}

fn run_async<F: Future<Output = ExitCode>>(fut: F) -> ExitCode {
    let Ok(rt) = tokio::runtime::Runtime::new() else {
        eprintln!("Failed to initialize async runtime.");
        return ExitCode::FAILURE;
    };
    rt.block_on(fut)
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = async {
                if let Some(ref mut term) = sigterm {
                    term.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn format_shield_choice_error(raw_choice: &str) -> String {
    let mut quoted = String::with_capacity(raw_choice.len() + 2);
    quoted.push('\'');
    for ch in raw_choice.chars() {
        match ch {
            '\'' => quoted.push_str("\\'"),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            '\u{00a0}' => quoted.push_str("\\xa0"),
            '\u{200c}' => quoted.push_str("\\u200c"),
            '\u{200d}' => quoted.push_str("\\u200d"),
            other => quoted.push(other),
        }
    }
    quoted.push('\'');

    format!(
        "{SHIELD_USAGE}\nagy-token shield: error: argument shield_action: invalid choice: {quoted} (choose from status, on, enable, off, disable, mode, allow, action, trigger)"
    )
}

fn handle_clap_error(err: &clap::Error) -> ExitCode {
    if err.kind() == ErrorKind::InvalidValue
        && let Some(ContextValue::String(arg_name)) = err.get(ContextKind::InvalidArg)
        && arg_name.contains("shield_action")
        && let Some(ContextValue::String(raw_val)) = err.get(ContextKind::InvalidValue)
    {
        eprintln!("{}", format_shield_choice_error(raw_val));
        return ExitCode::from(2);
    }

    let _ = err.print();
    let code = u8::try_from(err.exit_code()).unwrap_or(2);
    ExitCode::from(code)
}

fn account_display_priority(acc: &tracker_lib::storage::AccountProfile) -> u8 {
    let is_desktop = acc.is_current_desktop_session.unwrap_or(false);
    let is_ide = acc.is_current_ide_session.unwrap_or(false);
    match (is_desktop, is_ide) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 3,
    }
}

fn run_list() -> ExitCode {
    let storage = tracker_lib::storage::StorageManager::default();
    let all_accs = match storage.list_accounts() {
        Ok(accs) => accs,
        Err(err) => {
            eprintln!("Error reading accounts: {err}");
            return ExitCode::FAILURE;
        }
    };

    if all_accs.is_empty() {
        println!("No accounts registered yet.");
        return ExitCode::SUCCESS;
    }

    let mut sorted = all_accs;
    sorted.sort_by(|a, b| {
        account_display_priority(a)
            .cmp(&account_display_priority(b))
            .then_with(|| a.email.to_lowercase().cmp(&b.email.to_lowercase()))
    });

    println!("                       Registered Antigravity Accounts                        ");
    println!(" Email                       Name            Tier            Active Surface   ");

    for acc in sorted {
        let email = &acc.email;
        let name = acc.name.as_deref().unwrap_or("");
        let tier = acc.tier.as_deref().unwrap_or("Standard");
        let email_col = format!(" {email:<28}");
        let name_col = format!("{name:<16}");

        match (
            acc.is_current_desktop_session == Some(true),
            acc.is_current_ide_session == Some(true),
        ) {
            (true, true) => println!("{email_col}{name_col}{tier}   App + IDE    "),
            (true, false) => println!("{email_col}{name_col}{tier}   Antigravity App  "),
            (false, true) => println!("{email_col}{name_col}{tier}   Antigravity IDE  "),
            (false, false) => println!("{email_col}{name_col}{tier}  Inactive / Stored "),
        }
    }

    ExitCode::SUCCESS
}

fn parse_country_codes(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|code| code.trim().to_uppercase())
        .filter(|code| !code.is_empty())
        .collect()
}

fn print_shield_status(cfg: &tracker_lib::shield::ShieldConfig) -> ExitCode {
    run_async(async move {
        let geo_info = tracker_lib::shield::get_ip_geo(false).await;
        let (allowed, reason) = tracker_lib::shield::is_ip_allowed(&geo_info, cfg);

        let status_str = if cfg.enabled {
            "● ACTIVE (Protection ON)"
        } else {
            "○ DISABLED (Protection OFF)"
        };
        let mode_str = match cfg.mode {
            ShieldMode::Whitelist => "Whitelist",
            ShieldMode::Restricted => "Restricted",
        };
        let action_val = cfg.action.to_string();
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
        ExitCode::SUCCESS
    })
}

fn run_shield(args: ShieldArgs) -> ExitCode {
    let storage = tracker_lib::storage::StorageManager::default();
    let config_dir = storage.config_dir();
    let mut cfg = tracker_lib::shield::load_shield_config(config_dir);

    let mut updated_via_flags = false;
    if let Some(mode) = args.mode {
        cfg.mode = mode.into();
        updated_via_flags = true;
    }
    if let Some(allow_str) = args.allow.as_deref() {
        let countries = parse_country_codes(allow_str);
        if countries.is_empty() {
            println!("Please specify country codes: `agy-token shield allow US,DE,GB`.");
            return ExitCode::FAILURE;
        }
        cfg.allowed_countries = countries;
        updated_via_flags = true;
    }
    if let Some(action) = args.action {
        cfg.action = action.into();
        updated_via_flags = true;
    }
    if updated_via_flags && let Err(err) = tracker_lib::shield::save_shield_config(config_dir, &cfg)
    {
        eprintln!("Failed to save shield config: {err}");
        return ExitCode::FAILURE;
    }

    match args.shield_action {
        Some(ShieldActionCmd::On | ShieldActionCmd::Enable) => {
            let _ = tracker_lib::shield::enable_shield(config_dir);
            println!("✓ IP Killswitch Shield ENABLED.");
            println!(
                "Antigravity processes will be automatically terminated if VPN drops or restricted IP is detected.\n"
            );
            ExitCode::SUCCESS
        }
        Some(ShieldActionCmd::Off | ShieldActionCmd::Disable) => {
            let _ = tracker_lib::shield::disable_shield(config_dir);
            println!("⚠️ IP Killswitch Shield DISABLED.\n");
            ExitCode::SUCCESS
        }
        Some(ShieldActionCmd::Mode) => {
            let target = args.extra_arg.as_deref().unwrap_or("");
            let Ok(mode) = target.parse::<ShieldMode>() else {
                println!(
                    "Invalid mode. Specify: `agy-token shield mode restricted` or `agy-token shield mode whitelist`."
                );
                return ExitCode::FAILURE;
            };
            cfg.mode = mode;
            let _ = tracker_lib::shield::save_shield_config(config_dir, &cfg);
            println!("✓ Shield mode set to: {mode}\n");
            ExitCode::SUCCESS
        }
        Some(ShieldActionCmd::Allow) => {
            let list_str = args.extra_arg.as_deref().unwrap_or("");
            let countries = parse_country_codes(list_str);
            if countries.is_empty() {
                println!("Please specify country codes: `agy-token shield allow US,DE,GB`.");
                return ExitCode::FAILURE;
            }
            cfg.allowed_countries.clone_from(&countries);
            let _ = tracker_lib::shield::save_shield_config(config_dir, &cfg);
            println!(
                "✓ Allowed countries whitelist updated to: {}\n",
                countries.join(", ")
            );
            ExitCode::SUCCESS
        }
        Some(ShieldActionCmd::Action) => {
            let act = args.extra_arg.as_deref().unwrap_or("");
            let Ok(action) = act.parse::<ShieldAction>() else {
                println!("Invalid action. Choose 'kill_process', 'kill_network', or 'both'.");
                return ExitCode::FAILURE;
            };
            cfg.action = action;
            let _ = tracker_lib::shield::save_shield_config(config_dir, &cfg);
            println!("✓ Shield enforcement action set to: {action}\n");
            ExitCode::SUCCESS
        }
        Some(ShieldActionCmd::Trigger) => {
            println!("🚨 Manually triggering IP Killswitch emergency action...");
            let (cnt, pids) = tracker_lib::shield::kill_antigravity_processes(true);
            println!("✓ Terminated {cnt} Antigravity process(es) (PIDs: {pids:?}).\n");
            ExitCode::SUCCESS
        }
        Some(ShieldActionCmd::Status) | None => {
            if updated_via_flags && args.shield_action.is_none() {
                println!(
                    "✓ Shield configuration updated (Mode: {}, Action: {}, Whitelist: {}).",
                    cfg.mode,
                    cfg.action,
                    cfg.allowed_countries.join(", ")
                );
                return ExitCode::SUCCESS;
            }
            print_shield_status(&cfg)
        }
    }
}

fn run_sync() -> ExitCode {
    run_async(async {
        let storage = tracker_lib::storage::StorageManager::default();
        match tracker_lib::auth::sync_from_antigravity(&storage).await {
            Ok((Some(em), all_accs)) => {
                println!("✓ Successfully synced active session: {em}");
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
                    println!("  • {acc_email} [{tier}] {surface_str}");
                }
                ExitCode::SUCCESS
            }
            Ok((None, _)) => {
                println!("No active Antigravity session found.");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("Error syncing from Antigravity: {err}");
                ExitCode::FAILURE
            }
        }
    })
}

fn run_snapshot(args: SnapshotArgs) -> ExitCode {
    run_async(async move {
        let targets: Vec<String> = if let Some(target) = args.target {
            vec![target.trim().to_lowercase()]
        } else {
            let storage = tracker_lib::storage::StorageManager::default();
            match tracker_lib::auth::sync_from_antigravity(&storage).await {
                Ok((active_email, all_accs)) => {
                    let mut found = BTreeSet::new();
                    for (email, acc) in all_accs {
                        if acc.is_current_desktop_session == Some(true)
                            || acc.is_current_ide_session == Some(true)
                        {
                            found.insert(email.to_lowercase());
                        }
                    }
                    if let Some(em) = active_email {
                        found.insert(em.to_lowercase());
                    }
                    found.into_iter().collect()
                }
                Err(err) => {
                    eprintln!("Error syncing active Antigravity sessions: {err}");
                    return ExitCode::FAILURE;
                }
            }
        };

        if targets.is_empty() {
            println!("No active Antigravity session found to snapshot.");
            return ExitCode::FAILURE;
        }

        let mut any_saved = false;
        for email in targets {
            let res = tracker_lib::balancer::snapshot_account(&email).await;
            println!(
                "Snapshot for {email}: desktop={}, ide={}",
                if res.desktop_saved { "saved" } else { "none" },
                if res.ide_saved { "saved" } else { "none" }
            );
            if res.desktop_saved || res.ide_saved {
                any_saved = true;
            }
        }

        if any_saved {
            ExitCode::SUCCESS
        } else {
            eprintln!("No session files or tokens could be snapshotted.");
            ExitCode::FAILURE
        }
    })
}

fn run_remove(args: RemoveArgs) -> ExitCode {
    match tracker_lib::storage::remove_account(&args.identifier) {
        Ok(Some(removed)) => {
            println!("✓ Removed account '{}' from tracking.", removed.email);
            ExitCode::SUCCESS
        }
        Ok(None) => {
            eprintln!("Account '{}' not found.", args.identifier);
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!("Error removing account: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run_check(args: CheckArgs) -> ExitCode {
    run_async(async move {
        let storage = tracker_lib::storage::StorageManager::default();
        let _ = tracker_lib::auth::sync_from_antigravity(&storage).await;
        let all_quotas = tracker_lib::quota::fetch_all_accounts_quota(args.force).await;

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

        if args.json {
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
                                .and_then(serde_json::Value::as_f64)
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

        if is_any_exhausted {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        }
    })
}

fn run_pace(args: PaceArgs) -> ExitCode {
    run_async(async move {
        let all_quotas = tracker_lib::quota::fetch_all_accounts_quota(args.force).await;
        let active_email = tracker_lib::daemon::active_account(&all_quotas).map(|(e, _)| e);
        #[allow(clippy::cast_precision_loss)]
        let now_epoch = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
        let burnrates = tracker_lib::quota::calculate_pool_burnrates(
            &all_quotas,
            active_email,
            None,
            now_epoch,
        );
        for (email, velocity) in &burnrates.accounts {
            let p = &velocity.primary;
            println!(
                "{email}: {:.1}% remaining | {:.2}%/hr | {} | ETA: {}",
                p.current_quota_pct, p.velocity_pct_per_hour, p.pace_status, p.eta_human
            );
        }
        println!("Token burn rate calculation complete.");
        ExitCode::SUCCESS
    })
}

fn run_balance(args: BalanceArgs) -> ExitCode {
    let mut cfg = tracker_lib::balancer::load_balancer_config();

    if let Some(strat) = args.strategy {
        cfg.strategy = strat.into();
    }
    if let Some(spread) = args.spread {
        cfg.watermark_spread_pct = spread;
    }
    if let Some(interval) = args.interval {
        cfg.round_robin_interval_minutes = interval;
    }
    if let Some(expiry_window) = args.expiry_window {
        cfg.expiry_window_hours = expiry_window;
    }
    if let Some(min_quota) = args.min_quota {
        cfg.min_quota_pct = min_quota;
    }

    let action = args.action.unwrap_or(BalanceActionCmd::Status);
    match action {
        BalanceActionCmd::On | BalanceActionCmd::Enable => {
            cfg.enabled = true;
            let _ = tracker_lib::balancer::save_balancer_config(&cfg);
            println!("Auto-Balancer ENABLED (Strategy: {})", cfg.strategy);
            ExitCode::SUCCESS
        }
        BalanceActionCmd::Off | BalanceActionCmd::Disable => {
            cfg.enabled = false;
            let _ = tracker_lib::balancer::save_balancer_config(&cfg);
            println!("Auto-Balancer DISABLED");
            ExitCode::SUCCESS
        }
        BalanceActionCmd::Strategy => {
            let strat = if let Some(s) = args.strategy {
                s.into()
            } else {
                let raw = args.extra_arg.as_deref().unwrap_or("");
                match raw.parse::<BalancerStrategy>() {
                    Ok(parsed) => parsed,
                    Err(err) => {
                        eprintln!("{err}");
                        return ExitCode::FAILURE;
                    }
                }
            };
            cfg.strategy = strat;
            let _ = tracker_lib::balancer::save_balancer_config(&cfg);
            println!("Auto-Balancer strategy set to: {strat}");
            ExitCode::SUCCESS
        }
        BalanceActionCmd::Config => {
            if let Err(err) = tracker_lib::balancer::save_balancer_config(&cfg) {
                eprintln!("Failed to save balancer config: {err}");
                return ExitCode::FAILURE;
            }
            println!(
                "Auto-Balancer configuration saved (Strategy: {}, Spread: {:.1}%, Interval: {}m, Expiry Window: {}h, Min Quota: {:.1}%).",
                cfg.strategy,
                cfg.watermark_spread_pct,
                cfg.round_robin_interval_minutes,
                cfg.expiry_window_hours,
                cfg.min_quota_pct
            );
            ExitCode::SUCCESS
        }
        BalanceActionCmd::Rotate => run_async(async move {
            let all_quotas = tracker_lib::quota::fetch_all_accounts_quota(args.force).await;
            let decision = tracker_lib::balancer::evaluate_and_execute_balancer(
                &all_quotas,
                args.dry_run,
                args.force,
                args.strategy.map(Into::into),
            )
            .await;
            println!("{}", decision.message);
            if decision.error.is_some() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }),
        BalanceActionCmd::Status => {
            if args.strategy.is_some()
                || args.spread.is_some()
                || args.interval.is_some()
                || args.expiry_window.is_some()
                || args.min_quota.is_some()
            {
                let _ = tracker_lib::balancer::save_balancer_config(&cfg);
            }
            let state_str = if cfg.enabled { "ENABLED" } else { "DISABLED" };
            println!(
                "Auto-Balancer is currently {} (Strategy: {}, Spread: {:.1}%)",
                state_str, cfg.strategy, cfg.watermark_spread_pct
            );
            ExitCode::SUCCESS
        }
    }
}

fn run_doctor(args: DoctorArgs) -> ExitCode {
    println!("🩺 ANTIGRAVITY ACCOUNT HEALTH & TOKEN DOCTOR\n");
    run_async(async move {
        if args.fix {
            let storage = tracker_lib::storage::StorageManager::default();
            let _ = storage.ensure_config_dir();
            let _ = tracker_lib::auth::sync_from_antigravity(&storage).await;
        }
        if args.no_network {
            println!("ℹ Note: --no-network requested; running local diagnostic checks.\n");
        }

        let report = tracker_lib::daemon::run_doctor_suite().await;
        for item in &report.items {
            let icon = match item.status {
                tracker_lib::daemon::DiagnosticStatus::Pass => "✓",
                tracker_lib::daemon::DiagnosticStatus::Info => "ℹ",
                tracker_lib::daemon::DiagnosticStatus::Warn
                | tracker_lib::daemon::DiagnosticStatus::Fail => "✗",
            };
            println!("{} {:<35} : {}", icon, item.component, item.details);
        }
        println!("\nOverall Diagnosis: {}", report.overall_status);

        let all_passed = report.items.iter().all(|i| {
            matches!(
                i.status,
                tracker_lib::daemon::DiagnosticStatus::Pass
                    | tracker_lib::daemon::DiagnosticStatus::Info
            )
        });
        if all_passed {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    })
}

fn run_daemon(args: DaemonArgs) -> ExitCode {
    println!("Antigravity Token Tracker Daemon started.");
    run_async(async move {
        let (tx, rx) = tokio::sync::watch::channel(false);
        let signal_tx = tx.clone();
        tokio::spawn(async move {
            wait_for_shutdown_signal().await;
            let _ = signal_tx.send(true);
        });

        let daemon_options = DaemonRunOptions {
            interval_secs: args.interval,
            auto_switch: args.auto_switch,
            threshold: args.threshold,
            cooldown_seconds: args.cooldown,
            restart: if args.no_restart {
                FailoverRestart::NoRestart
            } else {
                FailoverRestart::Restart
            },
        };

        let ok = if args.tray {
            let (snap_tx, snap_rx) = tokio::sync::watch::channel(None);
            let daemon_tx = tx.clone();
            let daemon_rx = rx.clone();
            let tray_tx = tx;
            let tray_rx = rx;
            let (daemon_res, tray_res) = tokio::join!(
                async move {
                    let res = tracker_lib::daemon::run_daemon_with_snapshot(
                        daemon_options,
                        Some(daemon_rx),
                        Some(snap_tx),
                    )
                    .await;
                    let _ = daemon_tx.send(true);
                    res
                },
                async move {
                    let res = tracker_lib::tray::run_tray_with_feed(
                        args.interval,
                        None,
                        Some(tray_rx),
                        Some(snap_rx),
                    )
                    .await;
                    let _ = tray_tx.send(true);
                    res
                }
            );
            if let Err(err) = &daemon_res {
                eprintln!("[Daemon] Error: {err}");
            }
            if let Err(err) = &tray_res {
                eprintln!("[Tray] Error: {err}");
            }
            daemon_res.is_ok() && tray_res.is_ok()
        } else {
            let res =
                tracker_lib::daemon::run_daemon_with_snapshot(daemon_options, Some(rx), None).await;
            let _ = tx.send(true);
            if let Err(err) = &res {
                eprintln!("[Daemon] Error: {err}");
            }
            res.is_ok()
        };

        if ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    })
}

fn spawn_detached_web(port: u16) -> ExitCode {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("Failed to resolve executable path: {err}");
            return ExitCode::FAILURE;
        }
    };

    let mut cmd = std::process::Command::new(exe);
    cmd.args(["web", "--port", &port.to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    match cmd.spawn() {
        Ok(child) => {
            println!(
                "Antigravity Web Dashboard started in background (PID: {}, http://localhost:{port}).",
                child.id()
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Error starting background web dashboard: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run_web(args: WebArgs) -> ExitCode {
    if args.bg {
        return spawn_detached_web(args.port);
    }

    println!(
        "Starting Antigravity Web Dashboard on http://localhost:{} ...",
        args.port
    );
    run_async(async move {
        if let Err(err) = tracker_lib::web::start_web_server(args.port).await {
            eprintln!("Error running web server: {err}");
            return ExitCode::FAILURE;
        }
        ExitCode::SUCCESS
    })
}

fn run_status(args: StatusArgs) -> ExitCode {
    run_async(async move {
        let storage = tracker_lib::storage::StorageManager::default();
        let _ = tracker_lib::auth::sync_from_antigravity(&storage).await;
        let all_quotas = tracker_lib::quota::fetch_all_accounts_quota(args.force).await;

        if all_quotas.is_empty() {
            println!("No accounts found. Run `agy-token sync` to import from Antigravity IDE.");
            return ExitCode::SUCCESS;
        }

        let geo_info = tracker_lib::shield::get_ip_geo(args.force).await;
        let shield_cfg = tracker_lib::shield::load_shield_config(storage.config_dir());
        let bal_cfg = tracker_lib::balancer::load_balancer_config();

        let dashboard = tracker_lib::cli::render_status_dashboard(
            &all_quotas,
            &geo_info,
            &shield_cfg,
            &bal_cfg,
        );
        println!("{dashboard}");

        if args.models {
            let mut emails: Vec<&String> = all_quotas.keys().collect();
            emails.sort();
            for email in emails {
                let q = &all_quotas[email];
                if q.models.is_empty() {
                    continue;
                }
                println!("Models for {email}:");
                for model in &q.models {
                    let pct = model
                        .remaining_percent
                        .map_or_else(|| "N/A".to_string(), |p| format!("{p:.1}%"));
                    let reset = model.reset_time.as_deref().unwrap_or("-");
                    println!("  • {:<32} {:>7} (reset: {reset})", model.model_id, pct);
                }
            }
        }

        ExitCode::SUCCESS
    })
}

fn run_watch(args: WatchArgs) -> ExitCode {
    run_async(async move {
        let storage = tracker_lib::storage::StorageManager::default();
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(args.interval.max(1)));
        let shutdown = wait_for_shutdown_signal();
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                () = &mut shutdown => break,
                _ = interval.tick() => {
                    let _ = tracker_lib::auth::sync_from_antigravity(&storage).await;
                    let all_quotas = tracker_lib::quota::fetch_all_accounts_quota(args.force).await;
                    let geo_info = tracker_lib::shield::get_ip_geo(args.force).await;
                    let shield_cfg = tracker_lib::shield::load_shield_config(storage.config_dir());
                    let bal_cfg = tracker_lib::balancer::load_balancer_config();
                    let dashboard = tracker_lib::cli::render_status_dashboard(
                        &all_quotas,
                        &geo_info,
                        &shield_cfg,
                        &bal_cfg,
                    );
                    print!("\x1B[2J\x1B[H{dashboard}");
                }
            }
        }
        ExitCode::SUCCESS
    })
}

fn run_ip(args: IpArgs) -> ExitCode {
    run_async(async move {
        let geo = tracker_lib::shield::get_ip_geo(args.refresh).await;
        if args.json {
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
        ExitCode::SUCCESS
    })
}

fn run_tray(args: TrayArgs) -> ExitCode {
    println!(
        "Antigravity Top-Bar Tray Applet started. (Updating every {}s)",
        args.interval
    );
    println!("Look for the lightning icon in your Ubuntu / Linux top bar. Press Ctrl+C to exit.");

    run_async(async move {
        if let Err(err) = tracker_lib::tray::run_tray(args.interval, None, None).await {
            eprintln!("[Tray] Error: {err}");
            return ExitCode::FAILURE;
        }
        ExitCode::SUCCESS
    })
}

fn run_switch(args: SwitchArgs) -> ExitCode {
    run_async(async move {
        if let Some(target) = args.target {
            match tracker_lib::balancer::switch_to_account(
                &target,
                !args.no_restart,
                args.surface.into(),
            )
            .await
            {
                Ok(outcome) => {
                    println!("✓ {outcome}");
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("Error switching account: {err}");
                    ExitCode::FAILURE
                }
            }
        } else {
            let accounts = match tracker_lib::storage::list_accounts() {
                Ok(accs) => accs,
                Err(err) => {
                    eprintln!("Error reading accounts: {err}");
                    return ExitCode::FAILURE;
                }
            };
            match tracker_lib::cli::run_interactive_switcher(&accounts).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("{err}");
                    ExitCode::FAILURE
                }
            }
        }
    })
}

fn run_test_notify() -> ExitCode {
    run_async(async {
        let actions = [
            ("switch_best", "Switch to Best Account"),
            ("open_dashboard", "Open Dashboard"),
        ];
        match tracker_lib::daemon::send_desktop_notification_with_actions(
            "Antigravity Token Tracker",
            "Test notification: 1-click failover actions ready.",
            "normal",
            &actions,
        )
        .await
        {
            Some(notification_id) => {
                println!("✓ Desktop notification sent (id: {notification_id}).");
                ExitCode::SUCCESS
            }
            None => {
                eprintln!("Failed to send interactive desktop notification via D-Bus.");
                ExitCode::FAILURE
            }
        }
    })
}

fn dispatch(command: Commands) -> ExitCode {
    match command {
        Commands::Status(args) => run_status(args),
        Commands::Shield(args) => run_shield(args),
        Commands::Ip(args) => run_ip(args),
        Commands::Watch(args) => run_watch(args),
        Commands::Check(args) => run_check(args),
        Commands::Sync => run_sync(),
        Commands::Snapshot(args) => run_snapshot(args),
        Commands::List(_) => run_list(),
        Commands::Remove(args) => run_remove(args),
        Commands::Daemon(args) => run_daemon(args),
        Commands::Web(args) => run_web(args),
        Commands::Tray(args) => run_tray(args),
        Commands::Switch(args) => run_switch(args),
        Commands::Doctor(args) => run_doctor(args),
        Commands::Balance(args) => run_balance(args),
        Commands::Pace(args) => run_pace(args),
        Commands::TestNotify => run_test_notify(),
    }
}

fn main() -> ExitCode {
    match Cli::try_parse() {
        Ok(cli) => match cli.command {
            Some(cmd) => dispatch(cmd),
            None => {
                let _ = Cli::command().print_help();
                ExitCode::SUCCESS
            }
        },
        Err(err) => handle_clap_error(&err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_daemon_with_tray_and_failover_flags() {
        let cli = Cli::try_parse_from([
            "agy-token",
            "daemon",
            "--tray",
            "--threshold",
            "2.5",
            "--cooldown",
            "60",
            "--no-restart",
        ])
        .expect("daemon args should parse");

        assert!(matches!(
            cli.command,
            Some(Commands::Daemon(DaemonArgs {
                interval: 60,
                auto_switch: false,
                threshold,
                cooldown,
                no_restart: true,
                tray: true,
            })) if (threshold - 2.5).abs() < f64::EPSILON && (cooldown - 60.0).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn test_parse_daemon_equals_and_defaults() {
        let defaults = Cli::try_parse_from(["agy-token", "daemon"]).expect("daemon defaults");
        assert!(matches!(
            defaults.command,
            Some(Commands::Daemon(DaemonArgs {
                interval: 60,
                auto_switch: false,
                threshold,
                cooldown,
                no_restart: false,
                tray: false,
            })) if (threshold - 1.0).abs() < f64::EPSILON && (cooldown - 120.0).abs() < f64::EPSILON
        ));

        let eq = Cli::try_parse_from([
            "agy-token",
            "daemon",
            "--interval=180",
            "--threshold=2.25",
            "--cooldown=90.5",
            "--auto-switch",
            "--no-restart",
            "--tray",
        ])
        .expect("daemon equals syntax");
        assert!(matches!(
            eq.command,
            Some(Commands::Daemon(DaemonArgs {
                interval: 180,
                auto_switch: true,
                threshold,
                cooldown,
                no_restart: true,
                tray: true,
            })) if (threshold - 2.25).abs() < f64::EPSILON && (cooldown - 90.5).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn test_parse_tray_interval_equals_and_space() {
        let eq = Cli::try_parse_from(["agy-token", "tray", "--interval=30"])
            .expect("tray --interval=30 should parse");
        assert_eq!(eq.command, Some(Commands::Tray(TrayArgs { interval: 30 })));

        let sp = Cli::try_parse_from(["agy-token", "tray", "--interval", "45"])
            .expect("tray --interval 45 should parse");
        assert_eq!(sp.command, Some(Commands::Tray(TrayArgs { interval: 45 })));
    }

    #[test]
    fn test_parse_web_port_and_background_aliases() {
        let bg = Cli::try_parse_from(["agy-token", "web", "--port", "9000", "--bg"])
            .expect("web --port 9000 --bg should parse");
        assert_eq!(
            bg.command,
            Some(Commands::Web(WebArgs {
                port: 9000,
                bg: true
            }))
        );

        let background = Cli::try_parse_from(["agy-token", "web", "--background"])
            .expect("web --background should parse");
        assert_eq!(
            background.command,
            Some(Commands::Web(WebArgs {
                port: 8765,
                bg: true
            }))
        );
    }

    #[test]
    fn test_parse_test_notify_and_aliases() {
        for cmd in ["test-notify", "notify-test", "test-toast"] {
            let parsed = Cli::try_parse_from(["agy-token", cmd]).expect("test-notify alias");
            assert!(matches!(parsed.command, Some(Commands::TestNotify)));
        }
    }

    #[test]
    fn test_parse_switch_subcommand() {
        let interactive =
            Cli::try_parse_from(["agy-token", "switch"]).expect("switch without target");
        assert_eq!(
            interactive.command,
            Some(Commands::Switch(SwitchArgs {
                target: None,
                surface: SwitchSurfaceArg::Both,
                no_restart: false,
            }))
        );

        let targeted = Cli::try_parse_from([
            "agy-token",
            "switch",
            "user@example.com",
            "--surface",
            "ide",
            "--no-restart",
        ])
        .expect("switch with target and flags");
        assert_eq!(
            targeted.command,
            Some(Commands::Switch(SwitchArgs {
                target: Some("user@example.com".to_string()),
                surface: SwitchSurfaceArg::Ide,
                no_restart: true,
            }))
        );

        let desktop = Cli::try_parse_from(["agy-token", "switch", "1", "--surface=desktop"])
            .expect("switch desktop");
        assert_eq!(
            desktop.command,
            Some(Commands::Switch(SwitchArgs {
                target: Some("1".to_string()),
                surface: SwitchSurfaceArg::Desktop,
                no_restart: false,
            }))
        );
    }

    #[test]
    fn test_parse_remove_and_snapshot_subcommands() {
        let rem = Cli::try_parse_from(["agy-token", "remove", "user@example.com"])
            .expect("remove with identifier");
        assert_eq!(
            rem.command,
            Some(Commands::Remove(RemoveArgs {
                identifier: "user@example.com".to_string()
            }))
        );

        assert!(
            Cli::try_parse_from(["agy-token", "remove"]).is_err(),
            "remove without identifier must fail"
        );

        let snap = Cli::try_parse_from(["agy-token", "snapshot"]).expect("snapshot without target");
        assert_eq!(
            snap.command,
            Some(Commands::Snapshot(SnapshotArgs { target: None }))
        );

        let save = Cli::try_parse_from(["agy-token", "save", "user@example.com"])
            .expect("save alias with target");
        assert_eq!(
            save.command,
            Some(Commands::Snapshot(SnapshotArgs {
                target: Some("user@example.com".to_string())
            }))
        );
    }

    #[test]
    fn test_reject_unknown_flags_and_invalid_numbers() {
        for bad_args in [
            vec!["agy-token", "--unknown-flag"],
            vec!["agy-token", "daemon", "--bogus"],
            vec!["agy-token", "daemon", "--threshold", "NaN"],
            vec!["agy-token", "daemon", "--cooldown=inf"],
            vec!["agy-token", "tray", "--unknown"],
            vec!["agy-token", "web", "--unknown"],
            vec!["agy-token", "switch", "--unknown"],
            vec!["agy-token", "test-notify", "--unknown"],
            vec!["agy-token", "snapshot", "--unknown"],
            vec!["agy-token", "remove", "1", "--unknown"],
            vec!["agy-token", "shield", "--unknown"],
            vec!["agy-token", "list", "--unknown"],
        ] {
            assert!(
                Cli::try_parse_from(&bad_args).is_err(),
                "expected parse failure for {bad_args:?}"
            );
        }
    }

    #[test]
    fn test_shield_invalid_choice_formats_expected_stderr() {
        let err = Cli::try_parse_from(["agy-token", "shield", "a\u{00a0}b"])
            .expect_err("invalid shield choice should error");
        assert_eq!(err.kind(), ErrorKind::InvalidValue);
        let raw_val = match err.get(ContextKind::InvalidValue) {
            Some(ContextValue::String(v)) => v.as_str(),
            other => panic!("expected InvalidValue string, got {other:?}"),
        };
        let formatted = format_shield_choice_error(raw_val);
        assert!(formatted.contains("invalid choice: 'a\\xa0b'"));
    }
}
