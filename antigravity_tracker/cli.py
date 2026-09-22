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


def render_account_dashboard(analyzed_account: dict, show_models: bool = False):
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

    # Header
    console.print()
    console.print("[bold cyan]═══════════════════════════════════════════════════════════════════════════════[/bold cyan]")
    console.print("[bold white]            ⚡ ANTIGRAVITY TOKEN & WEEKLY QUOTA LIFECYCLE TRACKER             [/bold white]")
    console.print(f"[dim]                Current Local Time: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')} (UTC: {datetime.now(timezone.utc).strftime('%H:%M:%S')})[/dim]")
    console.print("[bold cyan]═══════════════════════════════════════════════════════════════════════════════[/bold cyan]")
    console.print()

    for email, a_q in analyzed.items():
        render_account_dashboard(a_q, show_models=args.models)

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
            console.print(f"  • [cyan]{em}[/cyan] [{acc.get('tier', 'Standard')}] {status_str}")
    else:
        console.print("[yellow]No active Antigravity session found.[/yellow]")
    return 0


def cmd_list(args):
    all_accs = accounts.list_accounts()
    if not all_accs:
        console.print("[dim]No accounts registered yet.[/dim]")
        return 0

    table = Table(title="Registered Antigravity Accounts", box=None)
    table.add_column("Email", style="bold")
    table.add_column("Name")
    table.add_column("Tier", style="cyan")
    table.add_column("Active Surface", justify="center")

    for acc in all_accs:
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
                    r, _, _ = select.select([sys.stdin], [], [], 0.05)
                    if r:
                        ch2 = sys.stdin.read(1)
                        if ch2 == '[':
                            r, _, _ = select.select([sys.stdin], [], [], 0.05)
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
        console.print("[bold green]🖥️ Top-Bar System Tray Applet ACTIVE[/bold green]")
        from . import tray
        applet = tray.TrayApplet(update_interval=interval)

        def daemon_cycle():
            try:
                accounts.sync_from_antigravity()
                all_quotas = quota.fetch_all_accounts_quota(force_refresh=True)
                analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}
                lifecycle.log_lifecycle_snapshot(analyzed)
                notifier.check_and_notify_lifecycle_events(analyzed)

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

    console.print(f"[cyan]Switching session to:[/cyan] [bold]{target_email}[/bold]...")
    success, msg = switcher.switch_to_account(target_email, restart=not args.no_restart)
    if success:
        console.print(f"[green]{msg}[/green]")
        return 0
    else:
        console.print(f"[red]{msg}[/red]")
        return 1


def cmd_web(args):
    from web_server import start_server
    start_server(port=args.port)
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
    t_acc.add_column("Keyring Token", width=14, justify="center")
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
    p_switch.add_argument("--no-restart", action="store_true", help="Swap session without restarting Antigravity")

    # doctor
    p_doctor = subparsers.add_parser("doctor", help="Audit account tokens, system keyring, services, and 1-click switch readiness")
    p_doctor.add_argument("--fix", action="store_true", help="Automatically repair file permissions, refresh stale tokens, and restart services")
    p_doctor.add_argument("--no-network", action="store_true", help="Skip remote Google OAuth token validation")

    args = parser.parse_args()

    # Default to status if no command given
    if not args.command:
        class Args:
            models = False
            force = False
        return cmd_status(Args())

    cmd_map = {
        "status": cmd_status,
        "watch": cmd_watch,
        "check": cmd_check,
        "sync": cmd_sync,
        "list": cmd_list,
        "remove": cmd_remove,
        "daemon": cmd_daemon,
        "web": cmd_web,
        "switch": cmd_switch,
        "tray": cmd_tray,
        "doctor": cmd_doctor
    }

    func = cmd_map.get(args.command)
    if func:
        return func(args)
    else:
        parser.print_help()
        return 0


if __name__ == "__main__":
    sys.exit(main() or 0)
