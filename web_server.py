import os
import json
from http.server import HTTPServer, BaseHTTPRequestHandler
import urllib.parse
from antigravity_tracker import accounts, quota, lifecycle

HTML_TEMPLATE_PATH = os.path.join(os.path.dirname(__file__), "templates", "dashboard.html")


class DashboardHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)

        if parsed.path == "/" or parsed.path == "/index.html":
            if os.path.isfile(HTML_TEMPLATE_PATH):
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.end_headers()
                with open(HTML_TEMPLATE_PATH, "rb") as f:
                    self.wfile.write(f.read())
            else:
                self.send_error(404, "Template not found")

        elif parsed.path == "/api/quota":
            accounts.sync_from_antigravity()
            all_quotas = quota.fetch_all_accounts_quota(force_refresh=False)
            analyzed = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_quotas.items()}
            rec = lifecycle.compute_switching_recommendation(analyzed)
            from antigravity_tracker import failover, geo
            failover_info = failover.get_failover_status(analyzed)
            geo_info = geo.get_ip_geo(force_refresh=False)

            payload = {
                "accounts": analyzed,
                "recommendation": rec,
                "failover": failover_info,
                "geo": geo_info
            }

            data = json.dumps(payload).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(data)

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
            if not target_email:
                try:
                    length = int(self.headers.get("Content-Length", 0))
                    if length > 0:
                        body = json.loads(self.rfile.read(length).decode("utf-8"))
                        target_email = body.get("email")
                except Exception:
                    pass

            if not target_email:
                self.send_response(400)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps({"success": False, "error": "Missing email parameter"}).encode("utf-8"))
                return

            from antigravity_tracker import switcher
            success, msg = switcher.switch_to_account(target_email, restart=True)
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
        _background_server = HTTPServer(server_address, DashboardHandler)
        _background_thread = threading.Thread(target=_background_server.serve_forever, daemon=True)
        _background_thread.start()
        print(f"[Web] Dashboard active at http://localhost:{port}/")
        return _background_server
    except Exception as e:
        print(f"[Web] Notice: Could not bind dashboard port {port}: {e}")
        return None


def start_server(port: int = 8765):
    server_address = ("127.0.0.1", port)
    httpd = HTTPServer(server_address, DashboardHandler)
    print(f"\n[Antigravity Token Dashboard] Serving at http://localhost:{port}/")
    print("Press Ctrl+C to stop the web server.\n")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping web server...")
        httpd.server_close()


if __name__ == "__main__":
    start_server()
