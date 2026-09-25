import unittest
import os
from unittest.mock import patch
import json
import urllib.request
import urllib.error
import threading
import time
from http.server import ThreadingHTTPServer

import web_server


class TestWebServer(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Bind to a test port
        cls.port = 8789
        cls.server = ThreadingHTTPServer(("127.0.0.1", cls.port), web_server.DashboardHandler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        time.sleep(0.2)

    @classmethod
    def tearDownClass(cls):
        if cls.server:
            cls.server.shutdown()
            cls.server.server_close()

    def test_root_index_html(self):
        url = f"http://127.0.0.1:{self.port}/"
        with urllib.request.urlopen(url, timeout=5) as resp:
            self.assertEqual(resp.status, 200)
            content = resp.read().decode("utf-8")
            self.assertIn("Antigravity Token & Quota Commander", content)
            self.assertTrue('<div id="root"></div>' in content or "Upcoming Quota Resets" in content)

    def test_static_assets_serving(self):
        import glob
        css_files = glob.glob("frontend/dist/assets/*.css")
        if css_files:
            rel_asset = os.path.basename(css_files[0])
            url = f"http://127.0.0.1:{self.port}/assets/{rel_asset}"
            with urllib.request.urlopen(url, timeout=5) as resp:
                self.assertEqual(resp.status, 200)
                self.assertIn("text/css", resp.headers.get("Content-Type", ""))

    @patch("antigravity_tracker.accounts.sync_from_antigravity", return_value=(None, {}))
    @patch("antigravity_tracker.geo.get_ip_geo")
    @patch("antigravity_tracker.quota.fetch_all_accounts_quota")
    def test_api_quota_endpoint(self, mock_fetch, mock_geo, mock_sync):
        mock_geo.return_value = {
            "ip": "127.0.0.1",
            "country_code": "US",
            "country_name": "United States",
            "city": "Local",
            "flag": "🌐",
            "is_restricted": False
        }
        mock_fetch.return_value = {
            "test@example.com": {
                "email": "test@example.com",
                "name": "Test User",
                "tier": "Standard",
                "is_current_desktop_session": True,
                "is_current_ide_session": True,
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "weekly": {
                            "remainingPercent": 85.0,
                            "remainingFraction": 0.85,
                            "resetTime": "2026-09-30T10:00:00Z"
                        },
                        "fiveHour": {
                            "remainingPercent": 100.0,
                            "remainingFraction": 1.0,
                            "resetTime": "2026-09-23T15:00:00Z"
                        }
                    }
                ],
                "models": [
                    {
                        "modelId": "gemini-2.5-pro",
                        "remainingPercent": 85.0,
                        "resetTime": "2026-09-30T10:00:00Z"
                    }
                ]
            }
        }
        url = f"http://127.0.0.1:{self.port}/api/quota"
        with urllib.request.urlopen(url, timeout=5) as resp:
            self.assertEqual(resp.status, 200)
            data = json.loads(resp.read().decode("utf-8"))
            self.assertIn("accounts", data)
            self.assertIn("recommendation", data)
            self.assertIn("failover", data)
            self.assertIn("active_sessions", data)
            self.assertIn("server_time_utc", data)
            self.assertIn("server_time_local", data)

    @patch("antigravity_tracker.accounts.sync_from_antigravity", return_value=(None, {}))
    @patch("antigravity_tracker.geo.get_ip_geo")
    @patch("antigravity_tracker.quota.fetch_all_accounts_quota")
    def test_api_quota_force_refresh(self, mock_fetch, mock_geo, mock_sync):
        mock_geo.return_value = {
            "ip": "127.0.0.1",
            "country_code": "US",
            "country_name": "United States",
            "city": "Local",
            "flag": "🌐",
            "is_restricted": False
        }
        mock_fetch.return_value = {}
        url = f"http://127.0.0.1:{self.port}/api/quota?force=true"
        with urllib.request.urlopen(url, timeout=5) as resp:
            self.assertEqual(resp.status, 200)
            data = json.loads(resp.read().decode("utf-8"))
            self.assertIn("accounts", data)
        mock_fetch.assert_called_once_with(force_refresh=True)

    def test_api_history_endpoint(self):
        url = f"http://127.0.0.1:{self.port}/api/history?hours=24"
        with urllib.request.urlopen(url, timeout=5) as resp:
            self.assertEqual(resp.status, 200)
            data = json.loads(resp.read().decode("utf-8"))
            self.assertIn("snapshots", data)
            self.assertIn("count", data)

    def test_api_doctor_endpoint(self):
        url = f"http://127.0.0.1:{self.port}/api/doctor?validate=false"
        with urllib.request.urlopen(url, timeout=10) as resp:
            self.assertEqual(resp.status, 200)
            data = json.loads(resp.read().decode("utf-8"))
            self.assertIn("system_checks", data)
            self.assertIn("overall_health", data)

    def test_api_sync_surfaces_invalid_target(self):
        url = f"http://127.0.0.1:{self.port}/api/sync-surfaces?target=unknown"
        try:
            with urllib.request.urlopen(url, timeout=5) as resp:
                data = json.loads(resp.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            self.assertEqual(e.code, 400)


if __name__ == "__main__":
    unittest.main()
