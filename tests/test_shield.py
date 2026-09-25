import unittest
from unittest.mock import patch, MagicMock
import os
import signal

import tempfile
import shutil
from antigravity_tracker import shield


class TestShield(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.mkdtemp(prefix="agy_shield_test_")
        self.orig_config_file = shield.SHIELD_CONFIG_FILE
        shield.SHIELD_CONFIG_FILE = os.path.join(self.temp_dir, "shield.json")
        # Reset timestamps
        shield._LAST_TRIGGER_TIMESTAMP = 0.0
        shield._WAS_VIOLATING = False

    def tearDown(self):
        shield.SHIELD_CONFIG_FILE = self.orig_config_file
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_shield_config_defaults_and_toggle(self):
        cfg = shield.load_shield_config()
        self.assertIn("enabled", cfg)
        self.assertIn("mode", cfg)
        self.assertIn("action", cfg)

        shield.set_shield_enabled(False)
        cfg_off = shield.load_shield_config()
        self.assertFalse(cfg_off["enabled"])

        shield.set_shield_enabled(True)
        cfg_on = shield.load_shield_config()
        self.assertTrue(cfg_on["enabled"])

    def test_is_ip_allowed_restricted_mode(self):
        cfg = {
            "enabled": True,
            "mode": "restricted",
            "allowed_countries": ["US", "DE"],
            "action": "kill_process"
        }
        # Safe country
        allowed, _ = shield.is_ip_allowed({"ip": "1.1.1.1", "country_code": "US", "country_name": "United States", "is_restricted": False}, cfg)
        self.assertTrue(allowed)

        # Restricted country (Iran)
        allowed, reason = shield.is_ip_allowed({"ip": "2.2.2.2", "country_code": "IR", "country_name": "Iran", "is_restricted": True}, cfg)
        self.assertFalse(allowed)
        self.assertIn("restricted", reason.lower())

        # Restricted country (Russia)
        allowed, reason = shield.is_ip_allowed({"ip": "3.3.3.3", "country_code": "RU", "country_name": "Russia", "is_restricted": True}, cfg)
        self.assertFalse(allowed)

    def test_is_ip_allowed_whitelist_mode(self):
        cfg = {
            "enabled": True,
            "mode": "whitelist",
            "allowed_countries": ["US", "DE", "GB"],
            "action": "kill_process"
        }
        # Allowed country in whitelist
        allowed, _ = shield.is_ip_allowed({"ip": "1.1.1.1", "country_code": "US", "country_name": "United States", "is_restricted": False}, cfg)
        self.assertTrue(allowed)

        allowed, _ = shield.is_ip_allowed({"ip": "1.1.1.2", "country_code": "DE", "country_name": "Germany", "is_restricted": False}, cfg)
        self.assertTrue(allowed)

        # Country NOT in whitelist (France)
        allowed, reason = shield.is_ip_allowed({"ip": "1.1.1.3", "country_code": "FR", "country_name": "France", "is_restricted": False}, cfg)
        self.assertFalse(allowed)
        self.assertIn("outside allowed whitelist", reason.lower())

    def test_shield_disabled_bypasses(self):
        cfg = {
            "enabled": False,
            "mode": "restricted",
            "allowed_countries": ["US"],
            "action": "kill_process"
        }
        allowed, _ = shield.is_ip_allowed({"ip": "2.2.2.2", "country_code": "IR", "country_name": "Iran", "is_restricted": True}, cfg)
        self.assertTrue(allowed)

    @patch("antigravity_tracker.shield.get_running_antigravity_pids")
    @patch("os.kill")
    @patch("os.path.exists")
    def test_kill_antigravity_processes(self, mock_exists, mock_kill, mock_get_pids):
        mock_get_pids.return_value = [10101, 10102]
        # Simulate process dies after SIGTERM
        mock_exists.return_value = False

        count, killed = shield.kill_antigravity_processes(force=True)
        self.assertEqual(count, 2)
        self.assertEqual(killed, [10101, 10102])
        self.assertEqual(mock_kill.call_count, 2)
        mock_kill.assert_any_call(10101, signal.SIGTERM)
        mock_kill.assert_any_call(10102, signal.SIGTERM)

    @patch("antigravity_tracker.shield.kill_antigravity_processes")
    @patch("antigravity_tracker.notifier.send_desktop_notification")
    def test_evaluate_and_enforce_shield_trigger(self, mock_notify, mock_kill):
        mock_kill.return_value = (2, [1234, 5678])

        geo_restricted = {
            "ip": "5.5.5.5",
            "country_code": "IR",
            "country_name": "Iran",
            "flag": "🇮🇷",
            "is_restricted": True
        }

        cfg = {
            "enabled": True,
            "mode": "restricted",
            "allowed_countries": ["US"],
            "action": "kill_process",
            "cooldown_seconds": 60
        }

        with patch("antigravity_tracker.shield.load_shield_config", return_value=cfg):
            res = shield.evaluate_and_enforce_shield(geo_restricted)
            self.assertTrue(res["violation"])
            self.assertEqual(res["processes_killed"], 2)
            mock_kill.assert_called_once_with(force=True)
            mock_notify.assert_called_once()


if __name__ == "__main__":
    unittest.main()
