import unittest
import os
import tempfile
import shutil
import time
from unittest.mock import patch, MagicMock

from antigravity_tracker import balancer


class TestBalancer(unittest.TestCase):

    def setUp(self):
        self.temp_dir = tempfile.mkdtemp(prefix="agy_bal_test_")
        self.orig_cfg_file = balancer.CONFIG_FILE
        self.orig_state_file = balancer.STATE_FILE
        balancer.CONFIG_FILE = os.path.join(self.temp_dir, "balancer.json")
        balancer.STATE_FILE = os.path.join(self.temp_dir, "balancer_state.json")

    def tearDown(self):
        balancer.CONFIG_FILE = self.orig_cfg_file
        balancer.STATE_FILE = self.orig_state_file
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_load_and_save_config(self):
        cfg = balancer.load_balancer_config()
        self.assertFalse(cfg["enabled"])
        self.assertEqual(cfg["strategy"], "watermark")

        cfg["enabled"] = True
        cfg["strategy"] = "expiry_first"
        cfg["watermark_spread_pct"] = 15.0
        balancer.save_balancer_config(cfg)

        reloaded = balancer.load_balancer_config()
        self.assertTrue(reloaded["enabled"])
        self.assertEqual(reloaded["strategy"], "expiry_first")
        self.assertEqual(reloaded["watermark_spread_pct"], 15.0)

    def test_watermark_strategy_spread_detection(self):
        active = {"email": "active@example.com", "quota_pct": 50.0}
        candidates = [
            {"email": "backup1@example.com", "quota_pct": 60.0},
            {"email": "backup2@example.com", "quota_pct": 85.0}
        ]

        # Spread threshold = 20%
        # backup2 has 85% - 50% = 35% >= 20% -> should trigger on backup2
        decision = balancer.evaluate_watermark_strategy(active, candidates, spread_pct=20.0)
        self.assertIsNotNone(decision)
        target, reason = decision
        self.assertEqual(target["email"], "backup2@example.com")
        self.assertIn("Quota spread", reason)

        # Spread threshold = 40% -> 35% < 40% -> should not trigger
        no_decision = balancer.evaluate_watermark_strategy(active, candidates, spread_pct=40.0)
        self.assertIsNone(no_decision)

    def test_expiry_first_strategy_prioritization(self):
        active = {"email": "active@example.com", "quota_pct": 30.0, "reset_seconds": 259200} # 3 days
        candidates = [
            # Candidate 1 resets in 6 hours with 20% quota remaining
            {"email": "soon@example.com", "quota_pct": 20.0, "reset_seconds": 21600},
            # Candidate 2 resets in 5 days with 90% quota remaining
            {"email": "later@example.com", "quota_pct": 90.0, "reset_seconds": 432000}
        ]

        # Expiry window = 24 hours
        decision = balancer.evaluate_expiry_first_strategy(
            active, candidates, expiry_window_hours=24, min_quota_pct=5.0
        )
        self.assertIsNotNone(decision)
        target, reason = decision
        self.assertEqual(target["email"], "soon@example.com")
        self.assertIn("Quota resets in 6.0h", reason)

    def test_round_robin_strategy_rotation(self):
        now = 10000.0
        last_rotation = now - 3700.0  # 61 minutes ago

        pool = [
            {"email": "acc1@example.com", "quota_pct": 50.0, "is_eligible": True},
            {"email": "acc2@example.com", "quota_pct": 40.0, "is_eligible": True},
            {"email": "acc3@example.com", "quota_pct": 60.0, "is_eligible": True}
        ]
        active = pool[0] # acc1 is currently active

        # Target interval = 60 minutes -> should rotate to acc2
        decision = balancer.evaluate_round_robin_strategy(
            active, pool, interval_minutes=60, last_rotation_ts=last_rotation, now=now
        )
        self.assertIsNotNone(decision)
        target, reason = decision
        self.assertEqual(target["email"], "acc2@example.com")

        # If only 30 minutes elapsed -> should NOT rotate
        no_decision = balancer.evaluate_round_robin_strategy(
            active, pool, interval_minutes=60, last_rotation_ts=now - 1800.0, now=now
        )
        self.assertIsNone(no_decision)

    @patch("antigravity_tracker.balancer.get_eligible_pool_accounts")
    def test_evaluate_and_execute_balancer_dry_run(self, mock_pool):
        mock_pool.return_value = [
            {"email": "curr@example.com", "quota_pct": 10.0, "is_active": True, "is_eligible": True, "reset_seconds": 86400, "is_exhausted": False},
            {"email": "next@example.com", "quota_pct": 90.0, "is_active": False, "is_eligible": True, "reset_seconds": 86400, "is_exhausted": False}
        ]

        cfg = balancer.load_balancer_config()
        cfg["enabled"] = True
        cfg["strategy"] = "watermark"
        cfg["watermark_spread_pct"] = 20.0
        balancer.save_balancer_config(cfg)

        res = balancer.evaluate_and_execute_balancer({}, dry_run=True, force=True)
        self.assertTrue(res.get("triggered"))
        self.assertTrue(res.get("dry_run"))
        self.assertEqual(res.get("to_account"), "next@example.com")

    @patch("antigravity_tracker.balancer.get_eligible_pool_accounts")
    def test_cooldown_enforcement(self, mock_pool):
        mock_pool.return_value = [
            {"email": "curr@example.com", "quota_pct": 10.0, "is_active": True, "is_eligible": True, "reset_seconds": 86400, "is_exhausted": False},
            {"email": "next@example.com", "quota_pct": 90.0, "is_active": False, "is_eligible": True, "reset_seconds": 86400, "is_exhausted": False}
        ]

        cfg = balancer.load_balancer_config()
        cfg["enabled"] = True
        cfg["cooldown_seconds"] = 300
        balancer.save_balancer_config(cfg)

        state = balancer.load_balancer_state()
        state["last_rotation_timestamp"] = time.time() - 30.0  # only 30s ago
        balancer.save_balancer_state(state)

        # Non-forced call should be blocked by cooldown
        res = balancer.evaluate_and_execute_balancer({}, dry_run=True, force=False)
        self.assertFalse(res.get("triggered"))
        self.assertEqual(res.get("reason"), "COOLDOWN")

        # Forced call should bypass cooldown
        res_forced = balancer.evaluate_and_execute_balancer({}, dry_run=True, force=True)
        self.assertTrue(res_forced.get("triggered"))


if __name__ == "__main__":
    unittest.main()
