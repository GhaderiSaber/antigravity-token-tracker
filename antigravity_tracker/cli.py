import sys
import os
import time
import argparse
import json
from datetime import datetime, timezone

from rich.console import Console
from rich.table import Table
from rich.panel import Panel
from rich.progress import BarColumn, Progress, TextColumn
from rich.text import Text
from rich.layout import Layout
from rich.live import Live
from rich.prompt import Prompt

from . import accounts
from . import quota
from . import lifecycle
from . import notifier
from . import auth
from . import failover
from . import geo
from . import shield
from . import balancer
from . import burnrate
from . import switcher

console = Console()


def create_progress_bar(pct: float, width: int = 24) -> str:
    filled = int(round((pct / 100.0) * width))
    empty = width - filled
    if pct <= 1.0:
        color = "red"
    elif pct <= 10.0:
        color = "bright_red"
    elif pct <= 25.0:
        color = "yellow"
    elif pct <= 50.0:
        color = "cyan"
    else:
        color = "green"
    return f"[{color}]{'█' * filled}{'░' * empty}[/{color}] {pct:>5.1f}%"


def get_account_display_priority(account_data: dict) -> int:
    """Returns sort priority integer for an account:
    0: Active in both Desktop App and IDE
    1: Active in Desktop App
    2: Active in IDE
    3: Inactive / Stored
    """
    is_desktop = bool(account_data.get("is_current_desktop_session"))
    is_ide = bool(account_data.get("is_current_ide_session"))
    if is_desktop and is_ide:
        return 0
    elif is_desktop:
        return 1
    elif is_ide:
        return 2
    return 3


def sort_analyzed_accounts(analyzed: Dict[str, Dict[str, Any]]) -> List[Tuple[str, Dict[str, Any]]]:
    """Sorts analyzed accounts so active sessions appear first, followed by healthy inactive accounts."""
    def _sort_key(item):
        email, a_q = item
        prio = get_account_display_priority(a_q)
        remaining = -1.0
        for g in a_q.get("groups", []):
            if "gemini" in g.get("displayName", "").lower() and g.get("weekly"):
                remaining = g["weekly"].get("remainingPercent", 0.0)
                break
        return (prio, -remaining, email.lower())

    return sorted(analyzed.items(), key=_sort_key)


def render_account_dashboard(analyzed_account: dict, show_models: bool = False, burnrate_info: Optional[dict] = None):
    email = analyzed_account.get("email", "Unknown")
    name = analyzed_account.get("name", "")
    tier = analyzed_account.get("tier", "Standard")
    is_ide_active = analyzed_account.get("is_current_ide_session", False)
    is_desktop_active = analyzed_account.get("is_current_desktop_session", False)

    header_title = f"[bold white]{email}[/bold white]"
    if name:
        header_title += f" ({name})"
    header_title += f"  •  [bold cyan]{tier}[/bold cyan]"
    if is_desktop_active and is_ide_active:
        header_title += "  [bold green]● Active (App + IDE)[/bold green]"
    elif is_desktop_active:
        header_title += "  [bold green]● Active in Antigravity App[/bold green]"
    elif is_ide_active:
        header_title += "  [bold green]● Active in IDE[/bold green]"
    elif analyzed_account.get("is_cached"):
        header_title += "  [dim yellow]● Inactive (Last Seen Quota)[/dim yellow]"
    else:
        header_title += "  [dim]● Inactive[/dim]"

    # If burn rate information is available, append velocity badge
    if burnrate_info:
        badge_text = burnrate.format_burnrate_badge(burnrate_info)
        header_title += f"  {badge_text}"

    table = Table(box=None, expand=True)
    table.add_column("Model Group", style="bold", ratio=2)
    table.add_column("Weekly Quota", ratio=3)
    table.add_column("Weekly Reset In", ratio=2)
    table.add_column("5-Hour Quota", ratio=3)
    table.add_column("Status", justify="center", ratio=2)

    groups = analyzed_account.get("groups", [])
    if groups:
        for g in groups:
            g_name = g.get("displayName", "")
            w = g.get("weekly")
            f = g.get("fiveHour")

            w_bar = create_progress_bar(w["remainingPercent"]) if w else "N/A"
            w_reset = f"[bold]{w['countdown']}[/bold]\n[dim]{w['resetTime'][:16].replace('T', ' ')} UTC[/dim]" if w else "N/A"

            f_bar = create_progress_bar(f["remainingPercent"]) if f else "N/A"
            badge = f"[{w['statusStyle']}]{w['statusBadge']}[/{w['statusStyle']}]" if w else "N/A"

            table.add_row(g_name, w_bar, w_reset, f_bar, badge)
    else:
        table.add_row("[dim italic]No quota data recorded yet. Log into Antigravity to snapshot session.[/dim italic]", "-", "-", "-", "[dim]NO DATA[/dim]")

    console.print(Panel(table, title=header_title, border_style="blue" if (is_desktop_active or is_ide_active) else "dim"))

    if show_models and analyzed_account.get("models"):
        m_table = Table(title="Individual Model Remaining Quota", box=None, expand=True)
        m_table.add_column("Model ID", style="cyan")
        m_table.add_column("Remaining Quota", justify="right")
        m_table.add_column("Reset Time", justify="right", style="dim")

        for m in sorted(analyzed_account["models"], key=lambda x: x["modelId"]):
            m_pct = m.get("remainingPercent", 0.0)
            m_table.add_row(
                m["modelId"],
                f"{m_pct:.2f}%",
                m.get("resetTime", "N/A")
            )
        console.print(Panel(m_table, border_style="dim"))


def cmd_status(args):
    # Auto sync active IDE session if present
    accounts.sync_from_antigravity()
    all_quotas = quota.fetch_all_accounts_quota(force_refresh=args.force)

    if not all_quotas:
        console.print("[yellow]No accounts found. Run `agy-token sync` to import from Antigravity IDE.[/yellow]")
        return 0

    analyzed = {}
    for email, q in all_quotas.items():
        analyzed[email] = lifecycle.analyze_account_lifecycle(q)

    # IP & Geolocation
    geo_info = geo.get_ip_geo(force_refresh=getattr(args, "force", False))
    ip_str = geo_info.get("ip", "Unknown")
    flag = geo_info.get("flag", "🌐")
    loc = geo_info.get("country_name", "Unknown")
    if geo_info.get("city"):
        loc = f"{geo_info.get('city')}, {loc}"

    if geo_info.get("is_restricted"):
        ip_status_badge = f"[bold white on red] ⚠️ RESTRICTED REGION ({geo_info.get('country_code')}) - VPN REQUIRED [/bold white on red]"
    else:
        ip_status_badge = "[bold green]✓ Safe Egress[/bold green]"

    # Shield Status Badge
    shield_cfg = shield.load_shield_config()
    if shield_cfg.get("enabled", True):
        shield_badge = "[bold green]🛡️ Shield: ACTIVE[/bold green]"
    else:
        shield_badge = "[dim]🛡️ Shield: OFF[/dim]"

    # Balancer Status Badge
    bal_cfg = balancer.load_balancer_config()
    if bal_cfg.get("enabled", False):
        strat_name = bal_cfg.get("strategy", "watermark").replace("_", " ").title()
        bal_badge = f"[bold cyan]🔄 Balancer: {strat_name}[/bold cyan]"
    else:
        bal_badge = "[dim]🔄 Balancer: OFF[/dim]"

    # Header
    console.print()
    console.print("[bold cyan]═══════════════════════════════════════════════════════════════════════════════[/bold cyan]")
    console.print("[bold white]            ⚡ ANTIGRAVITY TOKEN & WEEKLY QUOTA LIFECYCLE TRACKER             [/bold white]")
    console.print(f"[dim]                Current Local Time: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')} (UTC: {datetime.now(timezone.utc).strftime('%H:%M:%S')})[/dim]")
    console.print(f"[dim]                🌐 Exit IP: [/dim][bold]{flag} {ip_str}[/bold] [dim]({loc})[/dim]  {ip_status_badge}  {shield_badge}  {bal_badge}")
    console.print("[bold cyan]═══════════════════════════════════════════════════════════════════════════════[/bold cyan]")
    console.print()

    # Compute Burn Rate Velocity & Runout Clocks
    pool_burn = burnrate.calculate_pool_burnrates(analyzed)
    active_burn = pool_burn.get("active_metrics")

    for email, a_q in sort_analyzed_accounts(analyzed):
        acc_burn = pool_burn.get("accounts", {}).get(email)
        render_account_dashboard(a_q, show_models=args.models, burnrate_info=acc_burn)

    # Active Runout Clock Warning if rapidly depleting
    if active_burn:
        pri = active_burn.get("primary", {})
        if pri.get("is_depleting"):
            console.print(Panel(
                f"[bold red]⏱️ Runout Clock Warning:[/bold red] Active session is burning [bold]{pri.get('velocity_pct_per_hour', 0.0):.1f}% quota/hr[/bold].\n"
                f"Estimated depletion in [bold yellow]{pri.get('eta_human')}[/bold yellow] (around [bold white]{pri.get('depletion_time_str')}[/bold white]).",
                border_style="yellow"
            ))

    # Recommendation
    rec = lifecycle.compute_switching_recommendation(analyzed)
    if rec:
        console.print(Panel(f"[bold yellow]{rec}[/bold yellow]", title="💡 Smart Recommendation", border_style="yellow"))

    # Failover standby status
    failover_status = failover.get_failover_status(analyzed)
    if failover_status.get("backup_candidate_email"):
        console.print(f"[dim]⚡ [bold cyan]Auto-Failover Standby:[/bold cyan] If active session depletes, next in line is [bold]{failover_status['backup_candidate_email']}[/bold] ({failover_status['backup_candidate_pct']}% available)[/dim]\n")

    # Log to history
    lifecycle.log_lifecycle_snapshot(analyzed)

    # Check for notification events
    notifier.check_and_notify_lifecycle_events(analyzed)
    return 0


def cmd_watch(args):
    console.print("[green]Starting live tracker watch mode (press Ctrl+C to exit)...[/green]")
    if getattr(args, "auto_switch", False):
        console.print(f"[bold green]⚡ Auto-Failover ACTIVE[/bold green] (triggers when active quota <= {getattr(args, 'threshold', 1.0)}%, cooldown: {getattr(args, 'cooldown', 120)}s)")
    try:
        while True:
            console.clear()
            cmd_status(args)

            if getattr(args, "auto_switch", False):
                all_quotas = quota.fetch_all_accounts_quota(force_refresh=False)
                analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}
                res = failover.evaluate_and_execute_failover(
                    analyzed,
                    threshold=getattr(args, "threshold", 1.0),
                    restart=not getattr(args, "no_restart", False),
                    cooldown_seconds=getattr(args, "cooldown", 120)
                )
                if res.get("triggered"):
                    console.print(f"\n[bold green]⚡ Auto-Failover: {res.get('message')}[/bold green]")
                    time.sleep(3)

            time.sleep(args.interval)
    except KeyboardInterrupt:
        console.print("\n[dim]Exited watch mode.[/dim]")
        return 0


def cmd_check(args):
    accounts.sync_from_antigravity()
    all_quotas = quota.fetch_all_accounts_quota(force_refresh=args.force)
    analyzed = {}
    is_any_exhausted = False

    for email, q in all_quotas.items():
        a = lifecycle.analyze_account_lifecycle(q)
        analyzed[email] = a
        if a.get("is_gemini_exhausted"):
            is_any_exhausted = True

    if args.json:
        output_data = {
            "timestamp": time.time(),
            "accounts": analyzed,
            "has_exhausted_account": is_any_exhausted
        }
        print(json.dumps(output_data, indent=2))
    else:
        for email, a in analyzed.items():
            for g in a.get("groups", []):
                w = g.get("weekly")
                if w:
                    print(f"{email} | {g['displayName']}: {w['remainingPercent']}% | Reset: {w['countdown']} | {w['statusBadge']}")

    return 1 if is_any_exhausted else 0


def cmd_sync(args):
    active_email, all_accs = accounts.sync_from_antigravity()
    if active_email:
        console.print(f"[green]✓ Successfully synced active session:[/green] [bold]{active_email}[/bold]")
        console.print(f"[dim]Total registered accounts: {len(all_accs)}[/dim]")
        for em, acc in all_accs.items():
            surface = []
            if acc.get("is_current_desktop_session"):
                surface.append("Antigravity App")
            if acc.get("is_current_ide_session"):
                surface.append("Antigravity IDE")
            status_str = f"({', '.join(surface)})" if surface else "(Inactive / Stored)"

            # Check snapshot coverage
            snaps = []
            s_dir = switcher.get_account_session_dir(em)
            if os.path.isdir(os.path.join(s_dir, "desktop")):
                snaps.append("App Snapshot ✓")
            if os.path.isfile(os.path.join(s_dir, "ide_tokens.json")):
                snaps.append("IDE Snapshot ✓")
            snap_str = f"  [dim][{', '.join(snaps)}][/dim]" if snaps else ""

            console.print(f"  • [cyan]{em}[/cyan] [{acc.get('tier', 'Standard')}] {status_str}{snap_str}")
    else:
        console.print("[yellow]No active Antigravity session found.[/yellow]")
    return 0


def cmd_snapshot(args):
    """Explicitly snapshots current active Desktop and/or IDE sessions, or a specified account."""
    target_email = getattr(args, "email", None)
    if target_email:
        clean_email = target_email.strip().lower()
        res = switcher.snapshot_account(clean_email)
        saved = [k for k, v in res.items() if v]
        if saved:
            console.print(f"[green]✓ Saved session snapshot for {clean_email}: {', '.join(saved)}[/green]")
            return 0
        else:
            console.print(f"[yellow]No active credentials found to snapshot for {clean_email}.[/yellow]")
            return 1

    accounts.sync_from_antigravity()
    cur_app = auth.discover_antigravity_desktop_app()
    cur_ide = auth.discover_antigravity_ide()
    if not cur_ide:
        db_email = getattr(auth, "extract_user_email_from_state_db", lambda: None)()
        if db_email:
            cur_ide = {"email": db_email}

    snapshotted_any = False
    if cur_app and cur_app.get("email"):
        em = cur_app["email"].strip().lower()
        ok = switcher.snapshot_desktop_session(em)
        if ok:
            console.print(f"[green]✓ Saved Antigravity Desktop App session:[/green] [bold]{em}[/bold]")
            snapshotted_any = True

    if cur_ide and cur_ide.get("email"):
        em = cur_ide["email"].strip().lower()
        ok = switcher.snapshot_ide_session(em)
        if ok:
            console.print(f"[green]✓ Saved Antigravity IDE session:[/green] [bold]{em}[/bold]")
            snapshotted_any = True

    if not snapshotted_any:
        console.print("[yellow]No active Desktop App or IDE sessions detected to snapshot.[/yellow]")
        return 1

    return 0


def cmd_list(args):
    all_accs = accounts.list_accounts()
    if not all_accs:
        console.print("[dim]No accounts registered yet.[/dim]")
        return 0

    sorted_accs = sorted(
        all_accs,
        key=lambda a: (get_account_display_priority(a), a.get("email", "").lower())
    )

    table = Table(title="Registered Antigravity Accounts", box=None)
    table.add_column("Email", style="bold")
    table.add_column("Name")
    table.add_column("Tier", style="cyan")
    table.add_column("Active Surface", justify="center")

    for acc in sorted_accs:
        surface = "[dim]Inactive / Stored[/dim]"
        if acc.get("is_current_desktop_session") and acc.get("is_current_ide_session"):
            surface = "[bold green]App + IDE[/bold green]"
        elif acc.get("is_current_desktop_session"):
            surface = "[green]Antigravity App[/green]"
        elif acc.get("is_current_ide_session"):
            surface = "[blue]Antigravity IDE[/blue]"

        table.add_row(
            acc.get("email", ""),
            acc.get("name", ""),
            acc.get("tier", "Standard"),
            surface
        )
    console.print(table)
    return 0


def resolve_target_account(query: str, all_accounts: list) -> Tuple[Optional[str], Optional[str]]:
    """Resolves an input query (number index 1-N, exact email, or substring match) to an account email.
    Returns (email, error_message)."""
    if not all_accounts:
        return None, "No accounts registered in tracker."

    q = query.strip()
    # 1. Number index (e.g. "1", "2")
    if q.isdigit():
        idx = int(q) - 1
        if 0 <= idx < len(all_accounts):
            return all_accounts[idx].get("email"), None
        else:
            return None, f"Invalid account number '{q}'. Please choose between 1 and {len(all_accounts)}."

    q_lower = q.lower()
    # 2. Exact match
    for acc in all_accounts:
        if acc.get("email", "").lower() == q_lower:
            return acc.get("email"), None

    # 3. Substring match (e.g. "duzen", "saber")
    matches = []
    for acc in all_accounts:
        email = acc.get("email", "").lower()
        name = acc.get("name", "").lower()
        if q_lower in email or (name and q_lower in name):
            matches.append(acc.get("email"))

    if len(matches) == 1:
        return matches[0], None
    elif len(matches) > 1:
        return None, f"Ambiguous input '{query}' matches multiple accounts ({', '.join(matches)}). Please enter full email or account number."

    return None, f"No registered account matches '{query}'."


def prepare_accounts_menu_data() -> Tuple[list, Optional[str]]:
    """Builds structured menu options for all registered accounts."""
    all_accs = accounts.list_accounts()
    all_quotas = quota.fetch_all_accounts_quota(force_refresh=False)
    analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}

    current_app = auth.discover_antigravity_desktop_app()
    current_active = current_app.get("email").lower() if current_app else None

    best_candidate_email = None
    best_pct = -1.0

    items = []
    for acc in all_accs:
        email = acc.get("email", "")
        tier = acc.get("tier", "Standard")
        q_data = analyzed.get(email, {})

        gemini_pct = 0.0
        countdown = ""
        is_exhausted = False
        for g in q_data.get("groups", []):
            if "gemini" in g.get("displayName", "").lower() and g.get("weekly"):
                gemini_pct = g["weekly"].get("remainingPercent", 0.0)
                countdown = g["weekly"].get("countdown", "")
                is_exhausted = g["weekly"].get("isExhausted", False)

        is_active_desktop = acc.get("is_current_desktop_session", False)
        is_active_ide = acc.get("is_current_ide_session", False)

        if (not current_active or email.lower() != current_active) and gemini_pct > best_pct:
            best_pct = gemini_pct
            best_candidate_email = email

        status_flags = []
        if is_active_desktop and is_active_ide:
            status_flags.append("Active in App+IDE")
        elif is_active_desktop:
            status_flags.append("Active in App")
        elif is_active_ide:
            status_flags.append("Active in IDE")

        if is_exhausted:
            status_flags.append("Exhausted")

        flag_str = f" [{', '.join(status_flags)}]" if status_flags else ""

        items.append({
            "email": email,
            "tier": tier,
            "gemini_pct": gemini_pct,
            "countdown": countdown,
            "is_active_desktop": is_active_desktop,
            "display_text": f"{email} ({tier}) - Gemini: {gemini_pct:.1f}%{flag_str}"
        })

    if best_candidate_email:
        for it in items:
            if it["email"].lower() == best_candidate_email.lower():
                it["display_text"] += " ★ Recommended"

    return items, best_candidate_email


def choose_account_interactive(accounts_data: list, default_email: Optional[str] = None) -> Optional[str]:
    """Displays an interactive selector for accounts.
    Allows selection via Up/Down arrow keys + Enter, or pressing a number [1-N].
    Falls back cleanly to rich Prompt in non-TTY environments."""
    if not accounts_data:
        return None

    default_idx = 0
    if default_email:
        for i, a in enumerate(accounts_data):
            if a["email"].lower() == default_email.lower():
                default_idx = i
                break

    # If stdin is not a TTY, use rich Prompt
    if not sys.stdin.isatty():
        console.print("\n[bold cyan]Registered Antigravity Accounts:[/bold cyan]")
        for i, acc in enumerate(accounts_data, 1):
            console.print(f"  [[bold]{i}[/bold]] {acc['display_text']}")
        try:
            choice = Prompt.ask(
                "Select account number",
                choices=[str(i) for i in range(1, len(accounts_data) + 1)],
                default=str(default_idx + 1)
            )
            return accounts_data[int(choice) - 1]["email"]
        except (KeyboardInterrupt, EOFError):
            if default_email:
                return default_email
            return None

    try:
        import termios
        import tty
        import select

        selected = default_idx
        num_items = len(accounts_data)

        def draw_menu(first=False):
            if not first:
                sys.stdout.write(f"\033[{num_items}A\r")
            for idx, acc in enumerate(accounts_data):
                is_sel = (idx == selected)
                prefix = " ❯ " if is_sel else "   "
                num_tag = f"[{idx + 1}]"
                if is_sel:
                    line = f"\033[1;36m{prefix}{num_tag} {acc['display_text']}\033[0m"
                else:
                    line = f"\033[0;37m{prefix}{num_tag} {acc['display_text']}\033[0m"
                sys.stdout.write(f"\033[K{line}\r\n")
            sys.stdout.flush()

        console.print("\n[bold]Select an account to switch to:[/bold] [dim](Use ↑/↓ arrows or number, Enter to select, 'q' to cancel)[/dim]")
        draw_menu(first=True)

        fd = sys.stdin.fileno()
        old_settings = termios.tcgetattr(fd)
        try:
            tty.setraw(fd)
            while True:
                ch = sys.stdin.read(1)
                if ch == '\x03':  # Ctrl+C
                    raise KeyboardInterrupt
                elif ch in ('q', 'Q'):
                    return None
                elif ch == '\x1b':
                    r, _, _ = select.select([sys.stdin], [], [], 0.2)
                    if r:
                        ch2 = sys.stdin.read(1)
                        if ch2 == '[':
                            r, _, _ = select.select([sys.stdin], [], [], 0.2)
                            if r:
                                ch3 = sys.stdin.read(1)
                                if ch3 == 'A':  # UP
                                    selected = (selected - 1) % num_items
                                    draw_menu()
                                    continue
                                elif ch3 == 'B':  # DOWN
                                    selected = (selected + 1) % num_items
                                    draw_menu()
                                    continue
                    return None  # Standalone Esc
                elif ch in ('\r', '\n'):
                    return accounts_data[selected]["email"]
                elif ch.isdigit():
                    digit = int(ch)
                    if 1 <= digit <= num_items:
                        selected = digit - 1
                        draw_menu()
        finally:
            termios.tcsetattr(fd, termios.TCSADRAIN, old_settings)
            sys.stdout.write("\r\n")
            sys.stdout.flush()
    except Exception:
        # Fallback to rich prompt
        console.print("\n[bold cyan]Registered Antigravity Accounts:[/bold cyan]")
        for i, acc in enumerate(accounts_data, 1):
            console.print(f"  [[bold]{i}[/bold]] {acc['display_text']}")
        try:
            choice = Prompt.ask(
                "Select account number",
                choices=[str(i) for i in range(1, len(accounts_data) + 1)],
                default=str(default_idx + 1)
            )
            return accounts_data[int(choice) - 1]["email"]
        except (KeyboardInterrupt, EOFError):
            if default_email:
                return default_email
            return None


def cmd_remove(args):
    all_accs = accounts.list_accounts()
    if not all_accs:
        console.print("[dim]No accounts registered yet.[/dim]")
        return 0

    target_query = getattr(args, "target", None) or getattr(args, "email", None)
    if not target_query:
        console.print("\n[bold cyan]Registered Antigravity Accounts:[/bold cyan]")
        for i, acc in enumerate(all_accs, 1):
            console.print(f"  [[bold]{i}[/bold]] {acc.get('email')} ({acc.get('tier', 'Standard')})")
        try:
            choice = Prompt.ask(
                "Select account number to remove",
                choices=[str(i) for i in range(1, len(all_accs) + 1)]
            )
            target_query = choice
        except (KeyboardInterrupt, EOFError):
            console.print("\n[dim]Cancelled.[/dim]")
            return 0

    email, err = resolve_target_account(target_query, all_accs)
    if err:
        console.print(f"[red]{err}[/red]")
        return 1

    if accounts.remove_account(email):
        console.print(f"[green]✓ Removed account profile:[/green] [bold]{email}[/bold]")
        return 0
    else:
        console.print(f"[red]Account {email} not found in registry.[/red]")
        return 1


def cmd_daemon(args):
    interval = getattr(args, "interval", 300)
    auto_switch = getattr(args, "auto_switch", False)
    threshold = getattr(args, "threshold", 1.0)
    cooldown = getattr(args, "cooldown", 120)
    no_restart = getattr(args, "no_restart", False)
    use_tray = getattr(args, "tray", False)

    console.print(f"[bold cyan]Antigravity Token Lifecycle Daemon started.[/bold cyan] Polling every {interval}s...")
    if auto_switch:
        console.print(f"[bold green]⚡ Auto-Failover: ENABLED[/bold green] (triggers when active quota <= {threshold}%, cooldown: {cooldown}s)")
    else:
        console.print("[dim]Auto-failover is disabled (monitoring mode). Use --auto-switch to enable automated account switching.[/dim]")

    # Start background web dashboard
    try:
        import web_server
        web_server.start_background_server(port=8765)
    except Exception:
        pass

    if use_tray:
        from . import tray
        applet = tray.TrayApplet(update_interval=interval)
        if not applet.acquire_lock():
            console.print("[yellow]⚠️ Another tray indicator instance is already active. Continuing daemon in background mode without duplicate tray.[/yellow]")
            use_tray = False
        else:
            console.print("[bold green]🖥️ Top-Bar System Tray Applet ACTIVE[/bold green]")

            def daemon_cycle():
                try:
                    accounts.sync_from_antigravity()
                    all_quotas = quota.fetch_all_accounts_quota(force_refresh=True)
                    analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}
                    lifecycle.log_lifecycle_snapshot(analyzed)
                    notifier.check_and_notify_lifecycle_events(analyzed)

                    # Enforce IP Killswitch Shield
                    shield.evaluate_and_enforce_shield()

                    if auto_switch:
                        res = failover.evaluate_and_execute_failover(
                            analyzed,
                            threshold=threshold,
                            restart=not no_restart,
                            cooldown_seconds=cooldown
                        )
                        if res.get("triggered"):
                            console.print(f"[bold green]⚡ Auto-Failover: {res.get('message')}[/bold green]")
                        elif res.get("reason") in ("COOLDOWN", "ALL_ACCOUNTS_DEPLETED", "SWITCH_FAILED"):
                            console.print(f"[dim]Failover notice: {res.get('message')}[/dim]")
                        else:
                            # Evaluate proactive pool balancing
                            bal_res = balancer.evaluate_and_execute_balancer(analyzed)
                            if bal_res.get("triggered"):
                                console.print(f"[bold cyan]🔄 Auto-Balancer: {bal_res.get('message')}[/bold cyan]")
                    else:
                        # Even if auto-failover is off, check if balancer is explicitly enabled
                        bal_res = balancer.evaluate_and_execute_balancer(analyzed)
                        if bal_res.get("triggered"):
                            console.print(f"[bold cyan]🔄 Auto-Balancer: {bal_res.get('message')}[/bold cyan]")
                    applet.refresh_data(force=False)
                except Exception as e:
                    console.print(f"[red]Daemon error during cycle: {e}[/red]")
                return True

            from gi.repository import GLib
            GLib.timeout_add_seconds(interval, daemon_cycle)
            applet.start()
            return 0

    try:
        while True:
            try:
                accounts.sync_from_antigravity()
                all_quotas = quota.fetch_all_accounts_quota(force_refresh=True)
                analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}
                lifecycle.log_lifecycle_snapshot(analyzed)
                notifier.check_and_notify_lifecycle_events(analyzed)

                # Enforce IP Killswitch Shield
                shield.evaluate_and_enforce_shield()

                if auto_switch:
                    res = failover.evaluate_and_execute_failover(
                        analyzed,
                        threshold=threshold,
                        restart=not no_restart,
                        cooldown_seconds=cooldown
                    )
                    if res.get("triggered"):
                        console.print(f"[bold green]⚡ Auto-Failover: {res.get('message')}[/bold green]")
                    elif res.get("reason") in ("COOLDOWN", "ALL_ACCOUNTS_DEPLETED", "SWITCH_FAILED"):
                        console.print(f"[dim]Failover notice: {res.get('message')}[/dim]")
                    else:
                        bal_res = balancer.evaluate_and_execute_balancer(analyzed)
                        if bal_res.get("triggered"):
                            console.print(f"[bold cyan]🔄 Auto-Balancer: {bal_res.get('message')}[/bold cyan]")
                else:
                    bal_res = balancer.evaluate_and_execute_balancer(analyzed)
                    if bal_res.get("triggered"):
                        console.print(f"[bold cyan]🔄 Auto-Balancer: {bal_res.get('message')}[/bold cyan]")
            except Exception as e:
                console.print(f"[red]Daemon error during cycle: {e}[/red]")
            time.sleep(interval)
    except KeyboardInterrupt:
        console.print("\n[dim]Daemon stopped.[/dim]")
        return 0


def cmd_switch(args):
    from . import switcher
    target_query = getattr(args, "target", None) or getattr(args, "email", None)

    all_accs = accounts.list_accounts()
    if not all_accs:
        console.print("[yellow]No accounts registered yet in tracker.[/yellow]")
        return 1

    menu_items, best_candidate_email = prepare_accounts_menu_data()

    target_email = None

    if target_query and target_query.lower() == "auto":
        args.auto = True
        target_query = None

    if getattr(args, "auto", False):
        all_quotas = quota.fetch_all_accounts_quota(force_refresh=False)
        analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}
        failover_info = failover.get_failover_status(analyzed)
        target_email = failover_info.get("backup_candidate_email") or best_candidate_email
        target_pct = failover_info.get("backup_candidate_pct", 0.0)

        if not target_email:
            console.print("[yellow]No alternative account found with available quota.[/yellow]")
            return 1
        console.print(f"[cyan]Auto-selected best available account:[/cyan] [bold]{target_email}[/bold] ([green]{target_pct:.1f}% quota[/green])")
    elif target_query:
        target_email, err = resolve_target_account(target_query, all_accs)
        if err:
            console.print(f"[red]{err}[/red]")
            return 1
    else:
        # Interactive selection
        target_email = choose_account_interactive(menu_items, default_email=best_candidate_email)
        if not target_email:
            console.print("[dim]Switch cancelled.[/dim]")
            return 0

    surface = "both"
    if getattr(args, "surface_app", False) and getattr(args, "surface_ide", False):
        surface = "both"
    elif getattr(args, "surface_app", False):
        surface = "desktop"
    elif getattr(args, "surface_ide", False):
        surface = "ide"

    surface_tag = ""
    if surface == "desktop":
        surface_tag = " [dim](Desktop App only)[/dim]"
    elif surface == "ide":
        surface_tag = " [dim](IDE only)[/dim]"

    console.print(f"[cyan]Switching session to:[/cyan] [bold]{target_email}[/bold]{surface_tag}...")
    success, msg = switcher.switch_to_account(target_email, restart=not args.no_restart, surface=surface)
    if success:
        console.print(f"[green]{msg}[/green]")
        return 0
    else:
        console.print(f"[red]{msg}[/red]")
        return 1


def cmd_web(args):
    port = getattr(args, "port", 8765)
    import socket
    from .tray import open_browser

    # Check if dashboard server is already listening
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        is_running = (s.connect_ex(("127.0.0.1", port)) == 0)

    url = f"http://localhost:{port}/"
    if is_running:
        console.print(f"[bold green]✓ Web Dashboard is active at {url}[/bold green]")
        console.print("[dim]Opening in your default browser...[/dim]")
        open_browser(url)
        return 0
    else:
        console.print(f"[bold cyan]Starting Web Dashboard at {url}...[/bold cyan]")
        import web_server
        web_server.start_background_server(port=port)
        open_browser(url)
        from web_server import start_server
        start_server(port=port)
        return 0


def cmd_tray(args):
    from . import tray
    interval = getattr(args, "interval", 60)
    console.print(f"[bold cyan]Antigravity Top-Bar Tray Applet started.[/bold cyan] (Updating every {interval}s)")
    console.print("[dim]Look for the lightning icon in your Ubuntu / Linux top bar. Press Ctrl+C to exit.[/dim]")
    tray.run_tray(interval=interval)
    return 0


def cmd_doctor(args):
    from . import doctor

    console.print("\n[bold cyan]═══════════════════════════════════════════════════════════════════════════════[/bold cyan]")
    console.print("[bold white]            🩺 ANTIGRAVITY ACCOUNT HEALTH & TOKEN DOCTOR                     [/bold white]")
    console.print("[bold cyan]═══════════════════════════════════════════════════════════════════════════════[/bold cyan]\n")

    if getattr(args, "fix", False):
        console.print("[yellow]Running auto-repairs...[/yellow]")
        actions = doctor.run_auto_repair()
        if actions:
            for act in actions:
                console.print(f"  [green]✓ {act}[/green]")
        else:
            console.print("  [dim]No repairs needed.[/dim]")
        console.print()

    console.print("[dim]Auditing system services, processes, and accounts...[/dim]\n")
    validate_remote = not getattr(args, "no_network", False)
    diag = doctor.run_full_diagnostic(validate_remote=validate_remote)

    # 1. System & Environment Table
    t_sys = Table(title="1. System & Service Infrastructure", border_style="cyan", show_lines=True)
    t_sys.add_column("Component", style="bold white", width=28)
    t_sys.add_column("Status", width=12, justify="center")
    t_sys.add_column("Diagnostic Details", style="dim")

    for c in diag["system_checks"]:
        badge = "[bold green]PASS[/bold green]" if c["status"] == "PASS" else f"[bold yellow]{c['status']}[/bold yellow]"
        t_sys.add_row(c["component"], badge, c["details"])
    console.print(t_sys)
    console.print()

    # 2. Antigravity Desktop Runtime Table
    t_app = Table(title="2. Antigravity Desktop Runtime & Keyring Sync", border_style="cyan", show_lines=True)
    t_app.add_column("Component", style="bold white", width=28)
    t_app.add_column("Status", width=12, justify="center")
    t_app.add_column("Diagnostic Details", style="dim")

    for c in diag["app_checks"]:
        badge = "[bold green]PASS[/bold green]" if c["status"] == "PASS" else (f"[bold yellow]{c['status']}[/bold yellow]" if c["status"] != "INFO" else "[bold blue]INFO[/bold blue]")
        t_app.add_row(c["component"], badge, c["details"])
    console.print(t_app)
    console.print()

    # 3. Per-Account Health Table
    t_acc = Table(title="3. Registered Accounts & 1-Click Switch Readiness", border_style="cyan", show_lines=True)
    t_acc.add_column("Account Email", style="bold cyan", width=28)
    t_acc.add_column("OAuth Status", width=14, justify="center")
    t_acc.add_column("App / Keyring", width=14, justify="center")
    t_acc.add_column("IDE Session", width=14, justify="center")
    t_acc.add_column("1-Click Switch", width=16, justify="center")
    t_acc.add_column("Action / Recommendation")

    for acc in diag["account_checks"]:
        oauth_st = acc["oauth_status"]
        if oauth_st == "VALID":
            o_badge = "[green]VALID[/green]"
        elif oauth_st == "REVOKED":
            o_badge = "[bold red]REVOKED[/bold red]"
        elif oauth_st == "NO_REFRESH_TOKEN":
            o_badge = "[dim]COOKIES ONLY[/dim]"
        else:
            o_badge = f"[yellow]{oauth_st}[/yellow]"

        keyring_badge = "[green]✓ Saved[/green]" if acc["has_keyring_snapshot"] else "[yellow]Missing[/yellow]"
        ide_badge = "[green]✓ Saved[/green]" if acc.get("has_ide_snapshot") else "[dim]Missing[/dim]"

        readiness = acc["switch_readiness"]
        if readiness == "READY":
            r_badge = "[bold green]● READY[/bold green]"
        elif readiness == "PARTIAL":
            r_badge = "[bold yellow]▲ PARTIAL[/bold yellow]"
        else:
            r_badge = "[bold red]✖ NEEDS LOGIN[/bold red]"

        t_acc.add_row(
            acc["email"],
            o_badge,
            keyring_badge,
            ide_badge,
            r_badge,
            acc["recommendation"]
        )

    console.print(t_acc)
    console.print()

    # Summary
    if diag["overall_health"] == "HEALTHY":
        console.print("[bold green]✓ Antigravity environment is fully healthy and operational.[/bold green]\n")
    else:
        console.print(f"[bold yellow]⚠️ System Status: {diag['overall_health']}[/bold yellow] - Run `agy-token doctor --fix` or see recommendations above.\n")

    return 0


def cmd_ip(args):
    """Audits public egress IP, geolocation, ISP, and Antigravity compatibility."""
    force = getattr(args, "refresh", False)
    geo_info = geo.get_ip_geo(force_refresh=force)

    if getattr(args, "json", False):
        print(json.dumps(geo_info, indent=2))
        return 0

    table = Table(title="🌐 Egress IP & Geolocation Audit", box=None)
    table.add_column("Property", style="bold cyan", ratio=1)
    table.add_column("Value", ratio=2)

    table.add_row("Public IP", geo_info.get("ip", "Unknown"))
    table.add_row("Country", f"{geo_info.get('flag', '🌐')} {geo_info.get('country_name', 'Unknown')} ({geo_info.get('country_code', '')})")
    table.add_row("Region / State", geo_info.get("region", "N/A") or "N/A")
    table.add_row("City", geo_info.get("city", "N/A") or "N/A")
    table.add_row("ISP / Organization", geo_info.get("isp") or geo_info.get("org") or "N/A")

    if geo_info.get("is_restricted"):
        status_msg = "[bold red]⚠️ RESTRICTED REGION (Google APIs Blocked) - VPN REQUIRED![/bold red]"
    else:
        status_msg = "[bold green]✓ Compatible (Google Antigravity APIs Accessible)[/bold green]"
    table.add_row("Antigravity Status", status_msg)

    console.print()
    console.print(Panel(table, border_style="red" if geo_info.get("is_restricted") else "green"))
    return 0


def cmd_shield(args):
    """Manages the Antigravity Geo-Shield and Emergency IP Killswitch."""
    action = getattr(args, "shield_action", None) or "status"
    cfg = shield.load_shield_config()

    if action in ("on", "enable"):
        shield.set_shield_enabled(True)
        console.print("[bold green]✓ IP Killswitch Shield ENABLED.[/bold green]")
        console.print("[dim]Antigravity processes will be automatically terminated if VPN drops or restricted IP is detected.[/dim]\n")
        return 0

    elif action in ("off", "disable"):
        shield.set_shield_enabled(False)
        console.print("[bold yellow]⚠️ IP Killswitch Shield DISABLED.[/bold yellow]\n")
        return 0

    elif action == "mode":
        target_mode = getattr(args, "mode_val", None) or getattr(args, "extra_arg", None)
        if target_mode not in ("restricted", "whitelist"):
            console.print("[red]Invalid mode. Specify: `agy-token shield mode restricted` or `agy-token shield mode whitelist`.[/red]")
            return 1
        cfg["mode"] = target_mode
        shield.save_shield_config(cfg)
        console.print(f"[green]✓ Shield mode set to: [bold]{target_mode}[/bold][/green]\n")
        return 0

    elif action == "allow":
        countries_str = getattr(args, "countries", None) or getattr(args, "extra_arg", None)
        if not countries_str:
            console.print("[red]Please specify country codes: `agy-token shield allow US,DE,GB`.[/red]")
            return 1
        countries = [c.strip().upper() for c in countries_str.split(",") if c.strip()]
        cfg["allowed_countries"] = countries
        shield.save_shield_config(cfg)
        console.print(f"[green]✓ Allowed countries whitelist updated to: [bold]{', '.join(countries)}[/bold][/green]\n")
        return 0

    elif action == "action":
        act_val = getattr(args, "action_val", None) or getattr(args, "extra_arg", None)
        if act_val not in ("kill_process", "kill_network", "both"):
            console.print("[red]Invalid action. Choose 'kill_process', 'kill_network', or 'both'.[/red]")
            return 1
        cfg["action"] = act_val
        shield.save_shield_config(cfg)
        console.print(f"[green]✓ Shield enforcement action set to: [bold]{act_val}[/bold][/green]\n")
        return 0

    elif action == "trigger":
        console.print("[bold red]🚨 Manually triggering IP Killswitch emergency action...[/bold red]")
        killed_cnt, pids = shield.kill_antigravity_processes(force=True)
        console.print(f"[green]✓ Terminated {killed_cnt} Antigravity process(es) (PIDs: {pids}).[/green]\n")
        return 0

    # Default: Show Status Table
    geo_info = geo.get_ip_geo(force_refresh=False)
    allowed, reason = shield.is_ip_allowed(geo_info, cfg)

    table = Table(title="🛡️ Antigravity Geo-Shield & Killswitch Configuration", box=None)
    table.add_column("Property", style="bold cyan", ratio=1)
    table.add_column("Value", ratio=2)

    status_str = "[bold green]● ACTIVE (Protection ON)[/bold green]" if cfg.get("enabled", True) else "[bold red]○ DISABLED (Protection OFF)[/bold red]"
    table.add_row("Shield Status", status_str)
    table.add_row("Mode", cfg.get("mode", "restricted").title())
    table.add_row("Enforcement Action", cfg.get("action", "kill_process"))
    table.add_row("Allowed Whitelist", ", ".join(cfg.get("allowed_countries", [])))
    table.add_row("Auto-Relaunch on Reconnect", "Yes" if cfg.get("auto_relaunch", False) else "No")

    current_ip = geo_info.get("ip", "Unknown")
    flag = geo_info.get("flag", "🌐")
    cc = geo_info.get("country_code", "")
    country = geo_info.get("country_name", "Unknown")
    table.add_row("Current Egress IP", f"{flag} {current_ip} ({country}, {cc})")

    policy_str = "[bold green]✓ COMPLIANT (Safe to run Antigravity)[/bold green]" if allowed else f"[bold red]⚠️ VIOLATION: {reason}[/bold red]"
    table.add_row("Policy Check", policy_str)

    console.print()
    console.print(Panel(table, border_style="green" if allowed else "red"))
    console.print("[dim]Commands: `agy-token shield on|off`, `agy-token shield mode restricted|whitelist`, `agy-token shield allow US,DE,GB`[/dim]\n")
    return 0


def cmd_balance(args):
    cfg = balancer.load_balancer_config()
    state = balancer.load_balancer_state()
    action = getattr(args, "balance_action", "status") or "status"

    if action in ("on", "enable"):
        cfg["enabled"] = True
        balancer.save_balancer_config(cfg)
        strat = cfg.get("strategy", "watermark").replace("_", " ").title()
        console.print(f"\n[bold green]✓ Auto-Balancer ENABLED[/bold green] [dim](Strategy: {strat})[/dim]\n")
        return 0

    elif action in ("off", "disable"):
        cfg["enabled"] = False
        balancer.save_balancer_config(cfg)
        console.print("\n[bold yellow]✓ Auto-Balancer DISABLED[/bold yellow]\n")
        return 0

    elif action == "strategy":
        strat_val = getattr(args, "strategy_val", None) or getattr(args, "extra_arg", None)
        if not strat_val or strat_val not in balancer.SUPPORTED_STRATEGIES:
            console.print(f"[red]Invalid strategy '{strat_val}'. Choose from: {', '.join(balancer.SUPPORTED_STRATEGIES)}[/red]")
            return 1
        cfg["strategy"] = strat_val
        balancer.save_balancer_config(cfg)
        console.print(f"\n[green]✓ Balancer strategy set to: [bold]{strat_val}[/bold][/green]\n")
        return 0

    elif action == "config":
        updated = False
        if getattr(args, "spread", None) is not None:
            cfg["watermark_spread_pct"] = float(args.spread)
            updated = True
        if getattr(args, "interval", None) is not None:
            cfg["round_robin_interval_minutes"] = int(args.interval)
            updated = True
        if getattr(args, "expiry_window", None) is not None:
            cfg["expiry_window_hours"] = int(args.expiry_window)
            updated = True
        if getattr(args, "min_quota", None) is not None:
            cfg["min_quota_pct"] = float(args.min_quota)
            updated = True

        if updated:
            balancer.save_balancer_config(cfg)
            console.print("[green]✓ Balancer configuration updated.[/green]\n")
        else:
            console.print("[dim]No configuration parameters specified. Use --spread, --interval, --expiry-window, or --min-quota.[/dim]\n")
        return 0

    elif action == "rotate":
        strat_override = getattr(args, "strategy_val", None) or getattr(args, "extra_arg", None)
        dry_run = getattr(args, "dry_run", False)
        force = getattr(args, "force", False)

        console.print(f"[cyan]Evaluating pool rotation (Strategy: {strat_override or cfg.get('strategy', 'watermark')})...[/cyan]")
        all_quotas = quota.fetch_all_accounts_quota(force_refresh=False)
        analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}

        res = balancer.evaluate_and_execute_balancer(
            analyzed,
            dry_run=dry_run,
            force=force or True,
            strategy_override=strat_override
        )

        if res.get("triggered"):
            mode_tag = "[bold yellow][DRY RUN][/bold yellow] " if dry_run else "[bold green]✓[/bold green] "
            console.print(f"\n{mode_tag}{res.get('message')}\n")
        else:
            console.print(f"\n[yellow]No rotation executed: {res.get('message')}[/yellow]\n")
        return 0

    # Default: Show Balancer Status & Pool Overview
    all_quotas = quota.fetch_all_accounts_quota(force_refresh=False)
    analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}
    pool = balancer.get_eligible_pool_accounts(analyzed, min_quota_pct=cfg.get("min_quota_pct", 5.0))

    table = Table(title="🔄 Antigravity Auto-Balancer & Pool Rotation", box=None)
    table.add_column("Setting", style="bold cyan", ratio=1)
    table.add_column("Value", ratio=2)

    status_str = "[bold green]● ACTIVE[/bold green]" if cfg.get("enabled", False) else "[bold dim]○ DISABLED[/bold dim]"
    table.add_row("Balancer Status", status_str)
    strat_key = cfg.get("strategy", "watermark")
    strat_desc = {
        "watermark": f"Watermark Leveling (Min spread: {cfg.get('watermark_spread_pct', 20.0)}%)",
        "expiry_first": f"Zero-Waste Reset Optimizer (Window: {cfg.get('expiry_window_hours', 36)}h)",
        "round_robin": f"Round-Robin Interval ({cfg.get('round_robin_interval_minutes', 60)}m)",
        "reactive": "Reactive Fallback (depletion-only)"
    }.get(strat_key, strat_key)
    table.add_row("Active Strategy", f"[bold]{strat_key.title()}[/bold] - [dim]{strat_desc}[/dim]")

    now = time.time()
    last_ts = state.get("last_rotation_timestamp", 0.0)
    cooldown = cfg.get("cooldown_seconds", 300)
    time_since = now - last_ts
    if time_since < cooldown:
        cd_str = f"[yellow]{int(cooldown - time_since)}s remaining[/yellow]"
    else:
        cd_str = "[green]Ready (cooldown elapsed)[/green]"
    table.add_row("Anti-Flap Cooldown", cd_str)

    if state.get("last_to_email"):
        ago_min = int((now - last_ts) // 60) if last_ts > 0 else 0
        table.add_row("Last Rotation", f"{state.get('last_from_email')} ➔ {state.get('last_to_email')} ({ago_min}m ago via {state.get('last_strategy_used')})")

    console.print()
    console.print(Panel(table, border_style="cyan"))

    # Pool Ranking Table
    p_table = Table(title="👥 Switchable Account Pool", box=None)
    p_table.add_column("Account Email", style="bold")
    p_table.add_column("Quota", justify="right")
    p_table.add_column("Weekly Reset In", justify="right", style="dim")
    p_table.add_column("Pool Role", justify="center")
    p_table.add_column("Status", justify="center")

    for acc in sorted(pool, key=lambda x: x["quota_pct"], reverse=True):
        email_str = acc["email"]
        q_pct = f"{acc['quota_pct']:.1f}%"
        hrs = acc["reset_seconds"] / 3600.0
        reset_str = f"{hrs:.1f}h" if hrs < 48 else f"{hrs/24:.1f}d"

        if acc["is_active"]:
            role = "[bold cyan]● CURRENT ACTIVE[/bold cyan]"
        elif acc["is_eligible"]:
            role = "[green]Ready Candidate[/green]"
        else:
            role = "[dim]Depleted / Low[/dim]"

        status_flag = "[bold red]EXHAUSTED[/bold red]" if acc["is_exhausted"] else "[bold green]HEALTHY[/bold green]"

        p_table.add_row(email_str, q_pct, reset_str, role, status_flag)

    console.print(Panel(p_table, border_style="blue"))
    console.print("[dim]Commands: `agy-token balance on|off`, `agy-token balance strategy watermark|expiry_first|round_robin`, `agy-token balance rotate`[/dim]\n")
    return 0


def cmd_pace(args):
    # Auto sync active IDE session if present
    accounts.sync_from_antigravity()
    all_quotas = quota.fetch_all_accounts_quota(force_refresh=getattr(args, "force", False))

    if not all_quotas:
        console.print("[yellow]No accounts found. Run `agy-token sync` to import from Antigravity IDE.[/yellow]")
        return 0

    analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}
    burn_data = burnrate.calculate_pool_burnrates(analyzed)

    active_email = burn_data.get("active_email")
    active_m = burn_data.get("active_metrics")

    # Header Panel
    console.print()
    console.print("[bold cyan]═══════════════════════════════════════════════════════════════════════════════[/bold cyan]")
    console.print("[bold white]            ⏱️ ANTIGRAVITY TOKEN BURN RATE & RUNOUT CLOCK                     [/bold white]")
    console.print(f"[dim]                Current Local Time: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')} (UTC: {datetime.now(timezone.utc).strftime('%H:%M:%S')})[/dim]")
    console.print("[bold cyan]═══════════════════════════════════════════════════════════════════════════════[/bold cyan]\n")

    if active_email and active_m:
        pri = active_m.get("primary", {})
        win = active_m.get("windows", {})
        v_15m = win.get("15m", {})
        v_1h = win.get("1h", {})
        v_6h = win.get("6h", {})

        h_table = Table(box=None, expand=True)
        h_table.add_column("Metric", style="bold cyan", ratio=1)
        h_table.add_column("Value", ratio=2)

        h_table.add_row("Active Session Account", f"[bold white]{active_email}[/bold white]")
        h_table.add_row("Current Remaining Quota", f"[bold]{pri.get('current_quota_pct', 0.0):.1f}%[/bold]")

        pace_badge = f"[{pri.get('pace_style')}]{pri.get('pace_icon')} {pri.get('pace_status')}[/{pri.get('pace_style')}]"
        h_table.add_row("Consumption Pace", f"{pace_badge}  ({pri.get('velocity_pct_per_hour', 0.0):.1f}% / hr)")

        if pri.get("is_depleting"):
            eta_str = f"[bold yellow]{pri.get('eta_human')}[/bold yellow] (estimated empty at [bold white]{pri.get('depletion_time_str')}[/bold white])"
        elif pri.get("current_quota_pct", 0.0) <= 1.0:
            eta_str = "[bold red]Exhausted (0% remaining)[/bold red]"
        else:
            eta_str = "[green]Stable (minimal or no recent consumption)[/green]"
        h_table.add_row("⏱️ Runout Clock (ETA)", eta_str)

        # Multi-window velocity breakdown
        pace_15m = f"{v_15m.get('velocity_pct_per_hour', 0.0):.1f}%/hr"
        pace_1h = f"{v_1h.get('velocity_pct_per_hour', 0.0):.1f}%/hr"
        pace_6h = f"{v_6h.get('velocity_pct_per_hour', 0.0):.1f}%/hr"
        h_table.add_row("Multi-Window Velocity", f"Burst (15m): [bold]{pace_15m}[/bold]  |  Hourly (1h): [bold]{pace_1h}[/bold]  |  Daily (6h): [bold]{pace_6h}[/bold]")

        console.print(Panel(h_table, title="⚡ Active Session Burn Rate & Runout Clock", border_style="cyan"))
    else:
        console.print("[dim]No active Antigravity session detected to track velocity.[/dim]\n")

    # Pool Velocity Comparison Table
    p_table = Table(title="📊 Account Pool Consumption Velocity & Projections", box=None)
    p_table.add_column("Account Email", style="bold")
    p_table.add_column("Quota", justify="right")
    p_table.add_column("Hourly Burn", justify="right")
    p_table.add_column("Pace Status", justify="center")
    p_table.add_column("Runout Clock", justify="center")
    p_table.add_column("Weekly Reset In", justify="right", style="dim")

    for email, multi in burn_data.get("accounts", {}).items():
        pri = multi.get("primary", {})
        q_data = analyzed.get(email, {})
        rem_pct, _ = failover.get_account_primary_quota(q_data)

        # Weekly reset
        reset_str = "N/A"
        for g in q_data.get("groups", []):
            w = g.get("weekly")
            if w and "countdown" in w:
                reset_str = w.get("countdown")
                break

        email_label = email
        if active_email and email.lower() == active_email.lower():
            email_label = f"● {email} [cyan](Active)[/cyan]"

        vel_str = f"{pri.get('velocity_pct_per_hour', 0.0):.1f}%/h"
        pace_lbl = f"[{pri.get('pace_style')}]{pri.get('pace_icon')} {pri.get('pace_status')}[/{pri.get('pace_style')}]"

        if pri.get("is_depleting"):
            eta_lbl = f"[yellow]{pri.get('eta_human')}[/yellow]"
        elif rem_pct <= 1.0:
            eta_lbl = "[red]Exhausted[/red]"
        else:
            eta_lbl = "[dim]Stable[/dim]"

        p_table.add_row(email_label, f"{rem_pct:.1f}%", vel_str, pace_lbl, eta_lbl, reset_str)

    console.print(Panel(p_table, border_style="blue"))
    console.print("[dim]Tip: Burn rates are continuously calculated from rolling daemon snapshots. Run `agy-token status` for full quotas.[/dim]\n")
    return 0


def cmd_test_notify(args):
    """Sends a sample interactive desktop notification with 1-click action buttons."""
    console.print("\n[bold cyan]⚡ Dispatching Interactive Toast with 1-Click Action Buttons...[/bold cyan]")

    all_q = quota.fetch_all_accounts_quota(force_refresh=False)
    active_pair = failover.get_active_account(all_q)
    active_email = active_pair[0] if active_pair else "current.session@gmail.com"

    # Find a backup candidate to test switching
    all_registered = accounts.list_accounts()
    backup_email = None
    for acc in all_registered:
        e = acc.get("email", "")
        if e.lower() != active_email.lower():
            backup_email = e
            break

    target_email = backup_email or "backup.account@gmail.com"

    actions = [
        (
            f"switch:{target_email}",
            f"⚡ Switch to {target_email}",
            lambda: notifier.trigger_action_switch(target_email)
        ),
        (
            "dashboard",
            "🌐 Open Dashboard",
            notifier.trigger_action_dashboard
        )
    ]

    nid = notifier.send_desktop_notification(
        "⚡ Antigravity 1-Click Action Toast",
        f"Active Session: {active_email}\nClick an action button below to test 1-click desktop control!",
        urgency="normal",
        actions=actions
    )

    if nid:
        console.print(f"[bold green]✓ Interactive notification dispatched (ID: {nid}) via DBus![/bold green]")
        console.print(f"  [dim]Look for the toast popup in your desktop top bar or notification center.[/dim]")
        console.print(f"  [dim]Clicking '[bold cyan]⚡ Switch to {target_email}[/bold cyan]' will execute the account switch.[/dim]")
        console.print("  [dim]Waiting 10 seconds for user action (Press Ctrl+C to exit)...[/dim]\n")
        try:
            time.sleep(10)
        except KeyboardInterrupt:
            pass
    else:
        console.print("[yellow]Notification sent via non-interactive fallback.[/yellow]\n")
    return 0


def main():
    parser = argparse.ArgumentParser(
        prog="agy-token",
        description="Antigravity Account Token & Quota Lifecycle Tracker"
    )
    subparsers = parser.add_subparsers(dest="command", help="Available subcommands")

    # status
    p_status = subparsers.add_parser("status", help="Show token quota status and weekly refresh countdowns")
    p_status.add_argument("--models", action="store_true", help="Display all individual model quotas")
    p_status.add_argument("--force", action="store_true", help="Bypass local cache and query Google directly")

    # shield
    p_shield = subparsers.add_parser("shield", help="Manage Geo-Shield and emergency IP Killswitch")
    p_shield.add_argument("shield_action", nargs="?", default="status", choices=["status", "on", "enable", "off", "disable", "mode", "allow", "action", "trigger"], help="Shield action to perform")
    p_shield.add_argument("extra_arg", nargs="?", default=None, help="Parameter value for mode/allow/action")
    p_shield.add_argument("--mode", dest="mode_val", choices=["restricted", "whitelist"], help="Policy mode")
    p_shield.add_argument("--allow", dest="countries", help="Comma-separated country codes (e.g. US,DE,GB)")
    p_shield.add_argument("--action", dest="action_val", choices=["kill_process", "kill_network", "both"], help="Action on violation")

    # ip
    p_ip = subparsers.add_parser("ip", help="Display public exit IP, geolocation, and Antigravity compatibility")
    p_ip.add_argument("--refresh", action="store_true", help="Bypass 60s cache and re-query live IP service")
    p_ip.add_argument("--json", action="store_true", help="Output machine-readable JSON")

    # watch
    p_watch = subparsers.add_parser("watch", help="Live auto-updating dashboard")
    p_watch.add_argument("--interval", type=int, default=30, help="Refresh interval in seconds (default: 30)")
    p_watch.add_argument("--models", action="store_true", help="Display all individual model quotas")
    p_watch.add_argument("--force", action="store_true", help="Bypass local cache")
    p_watch.add_argument("--auto-switch", action="store_true", help="Automatically switch session when active account quota runs out")
    p_watch.add_argument("--threshold", type=float, default=1.0, help="Quota percentage threshold to trigger auto-failover (default: 1.0%%)")
    p_watch.add_argument("--cooldown", type=int, default=120, help="Minimum seconds between auto-failovers (default: 120)")
    p_watch.add_argument("--no-restart", action="store_true", help="Swap session without restarting Antigravity")

    # check
    p_check = subparsers.add_parser("check", help="Fast exit-code / JSON check (for shell scripts/prompts)")
    p_check.add_argument("--json", action="store_true", help="Output machine-readable JSON")
    p_check.add_argument("--force", action="store_true", help="Bypass local cache")

    # sync
    subparsers.add_parser("sync", help="Sync active account from Antigravity IDE / app session")

    # snapshot / save
    p_snap = subparsers.add_parser("snapshot", aliases=["save"], help="Explicitly capture and save session snapshots from active IDE / App")
    p_snap.add_argument("email", nargs="?", default=None, help="Optional email to snapshot (defaults to all currently active sessions)")

    # list
    subparsers.add_parser("list", help="List all registered account profiles")

    # remove
    p_remove = subparsers.add_parser("remove", help="Remove an account from tracking")
    p_remove.add_argument("target", nargs="?", default=None, help="Email, number [1-N], or name of account to remove")

    # daemon
    p_daemon = subparsers.add_parser("daemon", help="Run background monitor with desktop alerts and auto-failover")
    p_daemon.add_argument("--interval", type=int, default=300, help="Polling interval in seconds (default: 300)")
    p_daemon.add_argument("--auto-switch", action="store_true", help="Automatically switch session when active account quota runs out")
    p_daemon.add_argument("--threshold", type=float, default=1.0, help="Quota percentage threshold to trigger auto-failover (default: 1.0%%)")
    p_daemon.add_argument("--cooldown", type=int, default=120, help="Minimum seconds between auto-failovers (default: 120)")
    p_daemon.add_argument("--no-restart", action="store_true", help="Swap session without restarting Antigravity")
    p_daemon.add_argument("--tray", action="store_true", help="Launch top-bar system tray indicator alongside daemon")

    # web
    p_web = subparsers.add_parser("web", help="Start interactive browser dashboard")
    p_web.add_argument("--port", type=int, default=8765, help="Port to listen on (default: 8765)")

    # tray
    p_tray = subparsers.add_parser("tray", help="Start native Linux top-bar / system tray applet")
    p_tray.add_argument("--interval", type=int, default=60, help="Polling interval in seconds (default: 60)")

    # switch
    p_switch = subparsers.add_parser("switch", help="Instantly switch active account session")
    p_switch.add_argument("target", nargs="?", default=None, help="Email, number [1-N], or partial name (opens interactive menu if omitted)")
    p_switch.add_argument("--auto", action="store_true", help="Auto-switch to account with highest available quota without prompting")
    p_switch.add_argument("--desktop", "--app", dest="surface_app", action="store_true", help="Switch only the Antigravity Desktop App session (leave IDE untouched)")
    p_switch.add_argument("--ide", dest="surface_ide", action="store_true", help="Switch only the Antigravity IDE session (leave Desktop App untouched)")
    p_switch.add_argument("--no-restart", action="store_true", help="Swap session without restarting Antigravity")

    # doctor
    p_doctor = subparsers.add_parser("doctor", help="Audit account tokens, system keyring, services, and 1-click switch readiness")
    p_doctor.add_argument("--fix", action="store_true", help="Automatically repair file permissions, refresh stale tokens, and restart services")
    p_doctor.add_argument("--no-network", action="store_true", help="Skip remote Google OAuth token validation")

    # balance
    p_balance = subparsers.add_parser("balance", help="Manage multi-account pool rotation & watermark balancing")
    p_balance.add_argument("balance_action", nargs="?", default="status",
                           choices=["status", "on", "enable", "off", "disable", "strategy", "rotate", "config"],
                           help="Action to perform (default: status)")
    p_balance.add_argument("extra_arg", nargs="?", default=None, help="Strategy name or value")
    p_balance.add_argument("--strategy", dest="strategy_val", choices=["watermark", "expiry_first", "round_robin", "reactive"], help="Strategy mode")
    p_balance.add_argument("--spread", type=float, help="Watermark spread percentage (e.g. 20.0)")
    p_balance.add_argument("--interval", type=int, help="Round-robin interval in minutes (e.g. 60)")
    p_balance.add_argument("--expiry-window", type=int, help="Expiry first window in hours (e.g. 36)")
    p_balance.add_argument("--min-quota", type=float, help="Minimum quota percentage for candidate eligibility (e.g. 5.0)")
    p_balance.add_argument("--dry-run", action="store_true", help="Simulate rotation without executing switch")
    p_balance.add_argument("--force", action="store_true", help="Bypass cooldown and balancer enabled check")

    # pace / burn
    p_pace = subparsers.add_parser("pace", aliases=["burn"], help="Display token burn rate velocity and Runout Clock prediction")
    p_pace.add_argument("--force", action="store_true", help="Bypass cache and refresh quotas directly")

    # test-notify
    subparsers.add_parser("test-notify", aliases=["notify-test", "test-toast"], help="Test 1-click desktop notification action buttons")

    args = parser.parse_args()

    # Default to status if no command given
    if not args.command:
        class Args:
            models = False
            force = False
        return cmd_status(Args())

    cmd_map = {
        "status": cmd_status,
        "shield": cmd_shield,
        "ip": cmd_ip,
        "watch": cmd_watch,
        "check": cmd_check,
        "sync": cmd_sync,
        "snapshot": cmd_snapshot,
        "save": cmd_snapshot,
        "list": cmd_list,
        "remove": cmd_remove,
        "daemon": cmd_daemon,
        "web": cmd_web,
        "switch": cmd_switch,
        "tray": cmd_tray,
        "doctor": cmd_doctor,
        "balance": cmd_balance,
        "pace": cmd_pace,
        "burn": cmd_pace,
        "test-notify": cmd_test_notify,
        "notify-test": cmd_test_notify,
        "test-toast": cmd_test_notify
    }

    func = cmd_map.get(args.command)
    if func:
        return func(args)
    else:
        parser.print_help()
        return 0


if __name__ == "__main__":
    sys.exit(main() or 0)
