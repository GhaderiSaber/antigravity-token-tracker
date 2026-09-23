import unittest
import os
import json
import time
import tempfile
import shutil
from datetime import datetime, timedelta

from antigravity_tracker import burnrate


class TestBurnRate(unittest.TestCase):

    def setUp(self):
        self.temp_dir = tempfile.mkdtemp(prefix="agy_burn_test_")
        self.history_file = os.path.join(self.temp_dir, "token_history.jsonl")

    def tearDown(self):
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_load_snapshots_empty_and_populated(self):
        # 1. Non-existent file
        self.assertEqual(burnrate.load_snapshots(history_file="/nonexistent/file.jsonl"), [])

        # 2. Populated file
        now = time.time()
        entries = [
            {"timestamp": now - 7200, "accounts": {"acc1@example.com": {"groups": {"Gemini Models": {"weekly_pct": 90.0}}}}},
            {"timestamp": now - 3600, "accounts": {"acc1@example.com": {"groups": {"Gemini Models": {"weekly_pct": 75.0}}}}},
            {"timestamp": now - 1800, "accounts": {"acc1@example.com": {"groups": {"Gemini Models": {"weekly_pct": 65.0}}}}},
        ]
        with open(self.history_file, "w", encoding="utf-8") as f:
            for entry in entries:
                f.write(json.dumps(entry) + "\n")

        # Load with 2h max age
        loaded = burnrate.load_snapshots(history_file=self.history_file, max_age_seconds=8000)
        self.assertEqual(len(loaded), 3)

        # Load with 45m max age -> only last entry
        loaded_recent = burnrate.load_snapshots(history_file=self.history_file, max_age_seconds=2700)
        self.assertEqual(len(loaded_recent), 1)

    def test_extract_account_quota_pct(self):
        snapshot = {
            "accounts": {
                "User@Example.com": {
                    "groups": {
                        "Gemini Models": {"weekly_pct": 82.5},
                        "Claude Models": {"weekly_pct": 40.0}
                    }
                }
            }
        }
        # Case insensitive match
        pct = burnrate.extract_account_quota_pct(snapshot, "user@example.com")
        self.assertEqual(pct, 82.5)

        # Other group
        claude_pct = burnrate.extract_account_quota_pct(snapshot, "user@example.com", group_name="Claude Models")
        self.assertEqual(claude_pct, 40.0)

        # Missing user
        self.assertIsNone(burnrate.extract_account_quota_pct(snapshot, "missing@example.com"))

    def test_format_eta_countdown(self):
        self.assertEqual(burnrate.format_eta_countdown(0), "Now")
        self.assertEqual(burnrate.format_eta_countdown(-10), "Now")
        self.assertEqual(burnrate.format_eta_countdown(45 * 60), "~45m")
        self.assertEqual(burnrate.format_eta_countdown(3600), "~1h")
        self.assertEqual(burnrate.format_eta_countdown(3600 + 15 * 60), "~1h 15m")
        self.assertEqual(burnrate.format_eta_countdown(86400 * 2 + 3600 * 5), "~2d 5h")

    def test_format_depletion_clock(self):
        now_dt = datetime.now()
        # Today
        today_target = now_dt.replace(hour=15, minute=30, second=0).timestamp()
        today_str = burnrate.format_depletion_clock(today_target)
        self.assertIn(":", today_str)

        # Tomorrow
        tomorrow_target = (now_dt + timedelta(days=1)).replace(hour=10, minute=0, second=0).timestamp()
        tomorrow_str = burnrate.format_depletion_clock(tomorrow_target)
        self.assertTrue(tomorrow_str.startswith("Tomorrow at"))

    def test_calculate_account_velocity_linear(self):
        now = 1700000000.0
        # 1 hour window: dropped from 70% to 40% (30% consumed in 1 hour)
        snapshots = [
            {"timestamp": now - 3600, "accounts": {"dev@example.com": {"groups": {"Gemini Models": {"weekly_pct": 70.0}}}}},
            {"timestamp": now - 1800, "accounts": {"dev@example.com": {"groups": {"Gemini Models": {"weekly_pct": 55.0}}}}},
            {"timestamp": now, "accounts": {"dev@example.com": {"groups": {"Gemini Models": {"weekly_pct": 40.0}}}}},
        ]

        metrics = burnrate.calculate_account_velocity(
            snapshots,
            email="dev@example.com",
            window_seconds=3600.0,
            now=now
        )

        self.assertEqual(metrics["velocity_pct_per_hour"], 30.0)
        self.assertEqual(metrics["pace_status"], "HIGH BURN")
        self.assertTrue(metrics["is_depleting"])
        self.assertEqual(metrics["current_quota_pct"], 40.0)
        # ETA = 40% / 30% * 3600s = 4800s (~1h 20m)
        self.assertEqual(metrics["eta_seconds"], 4800.0)
        self.assertEqual(metrics["eta_human"], "~1h 20m")

    def test_calculate_account_velocity_idle(self):
        now = 1700000000.0
        # 1 hour window: stable at 95.0%
        snapshots = [
            {"timestamp": now - 3600, "accounts": {"idle@example.com": {"groups": {"Gemini Models": {"weekly_pct": 95.0}}}}},
            {"timestamp": now, "accounts": {"idle@example.com": {"groups": {"Gemini Models": {"weekly_pct": 95.0}}}}},
        ]

        metrics = burnrate.calculate_account_velocity(
            snapshots,
            email="idle@example.com",
            window_seconds=3600.0,
            now=now
        )

        self.assertEqual(metrics["velocity_pct_per_hour"], 0.0)
        self.assertEqual(metrics["pace_status"], "IDLE")
        self.assertFalse(metrics["is_depleting"])
        self.assertEqual(metrics["eta_human"], "Stable / Idle")

    def test_calculate_account_velocity_reset_boundary(self):
        now = 1700000000.0
        # Account was at 5%, weekly reset occurred to 100%, then consumed down to 90%
        snapshots = [
            {"timestamp": now - 3600, "accounts": {"reset@example.com": {"groups": {"Gemini Models": {"weekly_pct": 5.0}}}}},
            {"timestamp": now - 1800, "accounts": {"reset@example.com": {"groups": {"Gemini Models": {"weekly_pct": 100.0}}}}},
            {"timestamp": now, "accounts": {"reset@example.com": {"groups": {"Gemini Models": {"weekly_pct": 90.0}}}}},
        ]

        metrics = burnrate.calculate_account_velocity(
            snapshots,
            email="reset@example.com",
            window_seconds=3600.0,
            now=now
        )

        # Total true consumption should only be (100 - 90) = 10%, NOT counting reset jump
        self.assertEqual(metrics["quota_consumed_in_window"], 10.0)
        self.assertEqual(metrics["velocity_pct_per_hour"], 10.0)
        self.assertEqual(metrics["pace_status"], "ACTIVE")

    def test_calculate_multiwindow_velocity_and_pool(self):
        now = time.time()
        snapshots = [
            {"timestamp": now - 1800, "accounts": {"pool@example.com": {"groups": {"Gemini Models": {"weekly_pct": 80.0}}}}},
            {"timestamp": now - 600, "accounts": {"pool@example.com": {"groups": {"Gemini Models": {"weekly_pct": 76.0}}}}},
            {"timestamp": now, "accounts": {"pool@example.com": {"groups": {"Gemini Models": {"weekly_pct": 74.0}}}}},
        ]
        with open(self.history_file, "w", encoding="utf-8") as f:
            for s in snapshots:
                f.write(json.dumps(s) + "\n")

        analyzed_quotas = {
            "pool@example.com": {
                "email": "pool@example.com",
                "is_current_desktop_session": True,
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "weekly": {"remainingPercent": 74.0}
                    }
                ]
            }
        }

        res = burnrate.calculate_pool_burnrates(analyzed_quotas, history_file=self.history_file)
        self.assertEqual(res["active_email"], "pool@example.com")
        self.assertIsNotNone(res["active_metrics"])
        self.assertIn("windows", res["active_metrics"])
        self.assertIn("15m", res["active_metrics"]["windows"])
        self.assertIn("1h", res["active_metrics"]["windows"])
        self.assertIn("6h", res["active_metrics"]["windows"])

    def test_badges_and_tray_summary(self):
        # Depleting metrics
        depleting = {
            "primary": {
                "velocity_pct_per_hour": 15.5,
                "pace_icon": "⚡",
                "pace_style": "bold yellow",
                "eta_human": "~2h 30m",
                "is_depleting": True,
                "current_quota_pct": 38.0
            }
        }
        badge = burnrate.format_burnrate_badge(depleting)
        self.assertIn("15.5%/hr", badge)
        self.assertIn("~2h 30m", badge)

        tray = burnrate.format_burnrate_tray_summary(depleting)
        self.assertEqual(tray, " (-15.5%/h, ~2h 30m left)")

        # Idle metrics
        idle = {
            "primary": {
                "velocity_pct_per_hour": 0.0,
                "pace_icon": "💤",
                "pace_style": "dim",
                "eta_human": "Stable / Idle",
                "is_depleting": False,
                "current_quota_pct": 80.0
            }
        }
        badge_idle = burnrate.format_burnrate_badge(idle)
        self.assertIn("Stable", badge_idle)
        self.assertEqual(burnrate.format_burnrate_tray_summary(idle), "")


if __name__ == "__main__":
    unittest.main()
