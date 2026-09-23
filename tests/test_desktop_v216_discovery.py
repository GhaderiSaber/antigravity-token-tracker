import unittest
from unittest.mock import patch, MagicMock
import io
import json

from antigravity_tracker import auth, quota


class TestDesktopV216Discovery(unittest.TestCase):

    @patch("subprocess.check_output")
    @patch("glob.glob")
    @patch("urllib.request.urlopen")
    def test_discover_desktop_app_proc_success(self, mock_urlopen, mock_glob, mock_ss):
        """Tests that discover_antigravity_desktop_app locates the v2.16.0 process via /proc."""
        mock_ss.return_value = b"tcp LISTEN 0 4096 127.0.0.1:38465 0.0.0.0:* users:((\"language_server\",pid=8430,fd=18))\n"
        mock_glob.return_value = ["/proc/8430/cmdline"]

        cmdline_bytes = (
            b"/snap/antigravity/35/opt/antigravity/resources/bin/language_server\x00"
            b"--subclient_type\x00hub\x00"
            b"--csrf_token\x00test-csrf-token-12345\x00"
            b"--app_data_dir\x00antigravity\x00"
        )

        mock_resp = MagicMock()
        mock_resp.read.return_value = json.dumps({
            "userStatus": {
                "email": "test.user@example.com",
                "name": "Test User",
                "userTier": {"name": "Google AI Pro"}
            }
        }).encode("utf-8")
        mock_resp.__enter__.return_value = mock_resp
        mock_resp.__exit__.return_value = None
        mock_urlopen.return_value = mock_resp

        with patch("builtins.open", unittest.mock.mock_open(read_data=cmdline_bytes)):
            res = auth.discover_antigravity_desktop_app()

        self.assertIsNotNone(res)
        self.assertEqual(res["email"], "test.user@example.com")
        self.assertEqual(res["port"], 38465)
        self.assertEqual(res["csrf_token"], "test-csrf-token-12345")
        self.assertEqual(res["tier"], "Google AI Pro")
        self.assertEqual(res["surface"], "Antigravity Desktop App")
        self.assertEqual(res["pid"], 8430)

    @patch("subprocess.check_output", side_effect=Exception("ss failed"))
    @patch("glob.glob", return_value=[])
    @patch("os.path.isfile")
    @patch("urllib.request.urlopen")
    def test_discover_desktop_app_fallback_to_main_log(self, mock_urlopen, mock_isfile, mock_glob, mock_ss):
        """Tests that discover_antigravity_desktop_app falls back to main.log when /proc scan yields no match."""
        def isfile_side_effect(path):
            return "main.log" in path

        mock_isfile.side_effect = isfile_side_effect

        log_content = (
            "2026-09-20T10:00:00.000Z [info] Spawning: ... --csrf_token fallback-token-999 ...\n"
            "2026-09-20T10:00:01.000Z [info] Listening on 127.0.0.1:39000\n"
        )

        mock_resp = MagicMock()
        mock_resp.read.return_value = json.dumps({
            "userStatus": {
                "email": "fallback@example.com",
                "name": "Fallback User",
                "userTier": {"name": "Standard"}
            }
        }).encode("utf-8")
        mock_resp.__enter__.return_value = mock_resp
        mock_resp.__exit__.return_value = None
        mock_urlopen.return_value = mock_resp

        with patch("builtins.open", unittest.mock.mock_open(read_data=log_content)):
            with patch("antigravity_tracker.auth.get_desktop_keyring_token", return_value=(None, None, None)):
                res = auth.discover_antigravity_desktop_app()

        self.assertIsNotNone(res)
        self.assertEqual(res["email"], "fallback@example.com")
        self.assertEqual(res["port"], 39000)
        self.assertEqual(res["csrf_token"], "fallback-token-999")

    @patch("antigravity_tracker.accounts.upsert_account")
    @patch("antigravity_tracker.auth.discover_antigravity_desktop_app")
    @patch("antigravity_tracker.auth.discover_antigravity_ide")
    @patch("antigravity_tracker.auth.fetch_quota_from_desktop_app")
    def test_fetch_account_quota_ide_fallback(self, mock_fetch_ls, mock_ide, mock_desk, mock_upsert):
        """Tests that fetch_account_quota uses the IDE language server when Desktop session doesn't match."""
        mock_desk.return_value = {"email": "other@example.com", "port": 38465, "csrf_token": "tok1"}
        mock_ide.return_value = {"email": "ide.user@example.com", "port": 43997, "csrf_token": "tok2"}
        mock_fetch_ls.return_value = {
            "groups": [
                {
                    "displayName": "Gemini Models",
                    "buckets": [
                        {
                            "bucketId": "gemini-weekly",
                            "window": "weekly",
                            "remainingFraction": 0.85,
                            "resetTime": "2026-09-30T10:00:00Z"
                        }
                    ]
                }
            ]
        }

        account = {"email": "ide.user@example.com", "name": "IDE User", "tier": "Google AI Pro"}
        res = quota.fetch_account_quota(account, force_refresh=True)

        self.assertIsNotNone(res)
        self.assertEqual(res["email"], "ide.user@example.com")
        self.assertEqual(len(res["groups"]), 1)
        self.assertEqual(res["groups"][0]["weekly"]["remainingPercent"], 85.0)
        mock_fetch_ls.assert_called_once_with(43997, "tok2")


if __name__ == "__main__":
    unittest.main()
