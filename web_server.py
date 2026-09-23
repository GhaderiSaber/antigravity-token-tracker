import os
import json
import mimetypes
from datetime import datetime, timezone
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
import urllib.parse
from antigravity_tracker import accounts, quota, lifecycle

FRONTEND_DIST_DIR = os.path.join(os.path.dirname(__file__), "frontend", "dist")
HTML_TEMPLATE_PATH = os.path.join(os.path.dirname(__file__), "templates", "dashboard.html")


class DashboardHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)

        if parsed.path == "/" or parsed.path == "/index.html":
            dist_index = os.path.join(FRONTEND_DIST_DIR, "index.html")
            if os.path.isfile(dist_index):
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Cache-Control", "no-cache")
                self.end_headers()
                with open(dist_index, "rb") as f:
                    self.wfile.write(f.read())
            elif os.path.isfile(HTML_TEMPLATE_PATH):
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Cache-Control", "no-cache")
                self.end_headers()
                with open(HTML_TEMPLATE_PATH, "rb") as f:
                    self.wfile.write(f.read())
            else:
                self.send_error(404, "Dashboard template not found")

        elif parsed.path.startswith("/assets/") or os.path.isfile(os.path.join(FRONTEND_DIST_DIR, parsed.path.lstrip("/"))):
            clean_rel = parsed.path.lstrip("/")
            file_path = os.path.abspath(os.path.join(FRONTEND_DIST_DIR, clean_rel))
            if file_path.startswith(os.path.abspath(FRONTEND_DIST_DIR)) and os.path.isfile(file_path):
                ctype, _ = mimetypes.guess_type(file_path)
                if not ctype:
                    if file_path.endswith(".js"): ctype = "application/javascript"
                    elif file_path.endswith(".css"): ctype = "text/css"
                    else: ctype = "application/octet-stream"

                self.send_response(200)
                self.send_header("Content-Type", f"{ctype}; charset=utf-8" if "text" in ctype or "javascript" in ctype else ctype)
                if parsed.path.startswith("/assets/"):
                    self.send_header("Cache-Control", "public, max-age=31536000, immutable")
                else:
                    self.send_header("Cache-Control", "no-cache")
                self.end_headers()
                with open(file_path, "rb") as f:
                    self.wfile.write(f.read())
            else:
                self.send_error(404, "Asset not found")

        elif parsed.path == "/api/quota":
            query = urllib.parse.parse_qs(parsed.query)
            force = query.get("force", ["false"])[0].lower() == "true"
            accounts.sync_from_antigravity()
            all_quotas = quota.fetch_all_accounts_quota(force_refresh=force)
            analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}
            rec = lifecycle.compute_switching_recommendation(analyzed)
            from antigravity_tracker import failover, geo, shield, balancer, burnrate
            failover_info = failover.get_failover_status(analyzed)
            active_sessions = failover.get_active_sessions(analyzed)
            geo_info = geo.get_ip_geo(force_refresh=force)
            shield_cfg = shield.load_shield_config()
            bal_cfg = balancer.load_balancer_config()
            bal_state = balancer.load_balancer_state()
            try:
                burnrate_info = burnrate.calculate_pool_burnrates(analyzed)
            except Exception as be:
                burnrate_info = {"error": str(be)}

            now_utc = datetime.now(timezone.utc)
            now_local = datetime.now()

            active_sessions_dict = {}
            for surf, info in active_sessions.items():
                if info and info[0]:
                    email, acc_data = info
                    rem_pct = 0.0
                    if acc_data and acc_data.get("groups"):
                        for g in acc_data["groups"]:
                            if "gemini" in g.get("displayName", "").lower() and g.get("weekly"):
                                rem_pct = g["weekly"].get("remainingPercent", 0.0)
                                break
                    active_sessions_dict[surf] = {
                        "email": email,
                        "quota_pct": rem_pct,
                        "name": acc_data.get("name", "") if acc_data else ""
                    }

            payload = {
                "accounts": analyzed,
                "recommendation": rec,
                "failover": failover_info,
                "active_sessions": active_sessions_dict,
                "geo": geo_info,
                "shield": shield_cfg,
                "balancer": bal_cfg,
                "balancer_state": bal_state,
                "burnrate": burnrate_info,
                "server_time_utc": now_utc.isoformat(),
                "server_time_local": now_local.strftime("%Y-%m-%d %H:%M:%S")
            }

            data = json.dumps(payload).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(data)

        elif parsed.path == "/api/history":
            from antigravity_tracker import burnrate
            query = urllib.parse.parse_qs(parsed.query)
            try:
                hours = float(query.get("hours", ["24"])[0])
            except ValueError:
                hours = 24.0
            max_age = hours * 3600.0
            snapshots = burnrate.load_snapshots(max_age_seconds=max_age)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(json.dumps({"snapshots": snapshots, "count": len(snapshots)}).encode("utf-8"))

        elif parsed.path == "/api/sync-surfaces":
            from antigravity_tracker import switcher
            query = urllib.parse.parse_qs(parsed.query)
            target = query.get("target", ["ide"])[0].lower()

            registered = accounts.load_accounts()
            desktop_acc = None
            ide_acc = None
            for em, acc in registered.items():
                if acc.get("is_current_desktop_session"):
                    desktop_acc = em
                if acc.get("is_current_ide_session"):
                    ide_acc = em

            if target in ("ide", "align_ide") and desktop_acc:
                success, msg = switcher.switch_to_account(desktop_acc, restart=False, surface="ide")
            elif target in ("desktop", "app", "align_desktop", "align_app") and ide_acc:
                success, msg = switcher.switch_to_account(ide_acc, restart=True, surface="desktop")
            else:
                success, msg = False, "Source session not detected for alignment or invalid target."

            self.send_response(200 if success else 400)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(json.dumps({"success": success, "message": msg}).encode("utf-8"))

        elif parsed.path == "/api/burnrate":
            from antigravity_tracker import burnrate
            all_quotas = quota.fetch_all_accounts_quota(force_refresh=False)
            analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}
            burn_info = burnrate.calculate_pool_burnrates(analyzed)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(json.dumps(burn_info).encode("utf-8"))

        elif parsed.path == "/api/shield":
            from antigravity_tracker import shield, geo
            if self.command == "POST":
                try:
                    length = int(self.headers.get("Content-Length", 0))
                    if length > 0:
                        body = json.loads(self.rfile.read(length).decode("utf-8"))
                        if "enabled" in body:
                            shield.set_shield_enabled(body["enabled"])
                        if "mode" in body or "allowed_countries" in body or "action" in body:
                            cfg = shield.load_shield_config()
                            if "mode" in body: cfg["mode"] = body["mode"]
                            if "allowed_countries" in body: cfg["allowed_countries"] = body["allowed_countries"]
                            if "action" in body: cfg["action"] = body["action"]
                            shield.save_shield_config(cfg)
                    else:
                        cfg = shield.load_shield_config()
                        shield.set_shield_enabled(not cfg.get("enabled", True))
                except Exception:
                    pass

            cfg = shield.load_shield_config()
            g = geo.get_ip_geo(force_refresh=False)
            allowed, reason = shield.is_ip_allowed(g, cfg)
            resp_payload = {
                "config": cfg,
                "allowed": allowed,
                "reason": reason,
                "geo": g
            }
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(json.dumps(resp_payload).encode("utf-8"))

        elif parsed.path == "/api/ip":
            from antigravity_tracker import geo
            query = urllib.parse.parse_qs(parsed.query)
            force = query.get("refresh", ["false"])[0].lower() == "true"
            geo_info = geo.get_ip_geo(force_refresh=force)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(json.dumps(geo_info).encode("utf-8"))

        elif parsed.path == "/api/balancer":
            from antigravity_tracker import balancer
            if self.command == "POST":
                try:
                    length = int(self.headers.get("Content-Length", 0))
                    if length > 0:
                        body = json.loads(self.rfile.read(length).decode("utf-8"))
                        cfg = balancer.load_balancer_config()
                        if "enabled" in body:
                            cfg["enabled"] = bool(body["enabled"])
                        if "strategy" in body and body["strategy"] in balancer.SUPPORTED_STRATEGIES:
                            cfg["strategy"] = body["strategy"]
                        if "watermark_spread_pct" in body:
                            cfg["watermark_spread_pct"] = float(body["watermark_spread_pct"])
                        if "round_robin_interval_minutes" in body:
                            cfg["round_robin_interval_minutes"] = int(body["round_robin_interval_minutes"])
                        if "expiry_window_hours" in body:
                            cfg["expiry_window_hours"] = int(body["expiry_window_hours"])
                        balancer.save_balancer_config(cfg)
                except Exception:
                    pass

            cfg = balancer.load_balancer_config()
            state = balancer.load_balancer_state()
            all_quotas = quota.fetch_all_accounts_quota(force_refresh=False)
            analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}
            pool = balancer.get_eligible_pool_accounts(analyzed, min_quota_pct=cfg.get("min_quota_pct", 5.0))

            resp_payload = {
                "config": cfg,
                "state": state,
                "pool": pool
            }
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(json.dumps(resp_payload).encode("utf-8"))

        elif parsed.path == "/api/balancer/rotate":
            from antigravity_tracker import balancer
            all_quotas = quota.fetch_all_accounts_quota(force_refresh=False)
            analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}
            res = balancer.evaluate_and_execute_balancer(analyzed, force=True)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(json.dumps(res).encode("utf-8"))

        elif parsed.path == "/api/switch-best":
            accounts.sync_from_antigravity()
            all_quotas = quota.fetch_all_accounts_quota(force_refresh=False)
            analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}
            from antigravity_tracker import failover, switcher
            failover_info = failover.get_failover_status(analyzed)
            best_target = failover_info.get("backup_candidate_email")

            if not best_target:
                self.send_response(400)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps({"success": False, "error": "No backup candidate with available quota."}).encode("utf-8"))
                return

            success, msg = switcher.switch_to_account(best_target, restart=True)
            self.send_response(200 if success else 500)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(json.dumps({"success": success, "message": msg, "target": best_target}).encode("utf-8"))

        elif parsed.path == "/api/switch":
            query = urllib.parse.parse_qs(parsed.query)
            target_email = query.get("email", [None])[0]
            surface = query.get("surface", ["both"])[0]
            if not target_email:
                try:
                    length = int(self.headers.get("Content-Length", 0))
                    if length > 0:
                        body = json.loads(self.rfile.read(length).decode("utf-8"))
                        target_email = body.get("email")
                        surface = body.get("surface", surface)
                except Exception:
                    pass

            if not target_email:
                self.send_response(400)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps({"success": False, "error": "Missing email parameter"}).encode("utf-8"))
                return

            from antigravity_tracker import switcher
            success, msg = switcher.switch_to_account(target_email, restart=True, surface=surface)
            self.send_response(200 if success else 400)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(json.dumps({"success": success, "message": msg}).encode("utf-8"))

        elif parsed.path == "/api/doctor":
            from antigravity_tracker import doctor
            query = urllib.parse.parse_qs(parsed.query)
            validate = query.get("validate", ["true"])[0].lower() == "true"
            diag = doctor.run_full_diagnostic(validate_remote=validate)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(json.dumps(diag).encode("utf-8"))

        else:
            self.send_error(404, "Not Found")

    def do_POST(self):
        return self.do_GET()

    def do_HEAD(self):
        return self.do_GET()

    def log_message(self, format, *args):
        # Silence default request spam in console
        return


_background_server = None
_background_thread = None


def start_background_server(port: int = 8765):
    """Starts the web dashboard in a background daemon thread if not already running."""
    global _background_server, _background_thread
    if _background_server is not None:
        return _background_server

    import socket
    # Check if port is already listening
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        if s.connect_ex(("127.0.0.1", port)) == 0:
            # Already running
            return None

    try:
        import threading
        server_address = ("127.0.0.1", port)
        _background_server = ThreadingHTTPServer(server_address, DashboardHandler)
        _background_thread = threading.Thread(target=_background_server.serve_forever, daemon=True)
        _background_thread.start()
        print(f"[Web] Dashboard active at http://localhost:{port}/")
        return _background_server
    except Exception as e:
        print(f"[Web] Notice: Could not bind dashboard port {port}: {e}")
        return None


def start_server(port: int = 8765):
    server_address = ("127.0.0.1", port)
    httpd = ThreadingHTTPServer(server_address, DashboardHandler)
    print(f"\n[Antigravity Token Dashboard] Serving at http://localhost:{port}/")
    print("Press Ctrl+C to stop the web server.\n")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping web server...")
        httpd.server_close()


if __name__ == "__main__":
    start_server()
