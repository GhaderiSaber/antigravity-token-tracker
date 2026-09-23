import unittest
from unittest.mock import patch, MagicMock
import time
from antigravity_tracker import failover


class TestFailoverEngine(unittest.TestCase):

    def setUp(self):
        self.mock_quotas = {
            "active@example.com": {
                "email": "active@example.com",
                "is_current_desktop_session": True,
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "weekly": {"remainingPercent": 0.5, "isExhausted": True}
                    }
                ]
            },
            "backup_high@example.com": {
                "email": "backup_high@example.com",
                "is_current_desktop_session": False,
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "weekly": {"remainingPercent": 85.0, "isExhausted": False}
                    }
                ]
            },
            "backup_low@example.com": {
                "email": "backup_low@example.com",
                "is_current_desktop_session": False,
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "weekly": {"remainingPercent": 15.0, "isExhausted": False}
                    }
                ]
            }
        }

    def test_get_account_primary_quota(self):
        pct, is_exh = failover.get_account_primary_quota(self.mock_quotas["active@example.com"])
        self.assertEqual(pct, 0.5)
        self.assertTrue(is_exh)

        pct_high, is_exh_high = failover.get_account_primary_quota(self.mock_quotas["backup_high@example.com"])
        self.assertEqual(pct_high, 85.0)
        self.assertFalse(is_exh_high)

    def test_get_active_account(self):
        active_pair = failover.get_active_account(self.mock_quotas)
        self.assertIsNotNone(active_pair)
        self.assertEqual(active_pair[0], "active@example.com")

    @patch("antigravity_tracker.accounts.load_accounts")
    @patch("antigravity_tracker.switcher.get_account_session_dir")
    @patch("os.path.isdir")
    @patch("os.path.isfile")
    def test_find_best_failover_candidate(self, mock_isfile, mock_isdir, mock_get_dir, mock_load_accs):
        mock_load_accs.return_value = {
            "backup_high@example.com": {"email": "backup_high@example.com"},
            "backup_low@example.com": {"email": "backup_low@example.com"}
        }
        mock_get_dir.return_value = "/fake/session/dir"
        mock_isdir.return_value = True
        mock_isfile.return_value = True

        candidate = failover.find_best_failover_candidate(self.mock_quotas, current_email="active@example.com")
        self.assertIsNotNone(candidate)
        self.assertEqual(candidate[0], "backup_high@example.com")
        self.assertEqual(candidate[1], 85.0)

    @patch("antigravity_tracker.accounts.load_accounts")
    @patch("antigravity_tracker.switcher.get_account_session_dir")
    @patch("os.path.isdir")
    @patch("os.path.isfile")
    @patch("antigravity_tracker.failover.load_failover_state")
    def test_evaluate_failover_dry_run(self, mock_load_state, mock_isfile, mock_isdir, mock_get_dir, mock_load_accs):
        mock_load_state.return_value = {
            "last_failover_timestamp": 0.0,
            "last_from_email": None,
            "last_to_email": None
        }
        mock_load_accs.return_value = {
            "backup_high@example.com": {"email": "backup_high@example.com"}
        }
        mock_get_dir.return_value = "/fake/session/dir"
        mock_isdir.return_value = True
        mock_isfile.return_value = True

        res = failover.evaluate_and_execute_failover(
            self.mock_quotas,
            threshold=1.0,
            dry_run=True
        )
        self.assertTrue(res["triggered"])
        self.assertTrue(res.get("dry_run"))
        self.assertEqual(res["from_account"], "active@example.com")
        self.assertEqual(res["to_account"], "backup_high@example.com")
        self.assertEqual(res["to_quota_pct"], 85.0)

    @patch("antigravity_tracker.failover.load_failover_state")
    def test_evaluate_failover_healthy(self, mock_load_state):
        healthy_quotas = {
            "active@example.com": {
                "email": "active@example.com",
                "is_current_desktop_session": True,
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "weekly": {"remainingPercent": 50.0, "isExhausted": False}
                    }
                ]
            }
        }
        res = failover.evaluate_and_execute_failover(healthy_quotas, threshold=1.0)
        self.assertFalse(res["triggered"])
        self.assertEqual(res["reason"], "HEALTHY")

    @patch("antigravity_tracker.failover.load_failover_state")
    def test_evaluate_failover_cooldown(self, mock_load_state):
        mock_load_state.return_value = {
            "last_failover_timestamp": time.time() - 30.0  # 30 seconds ago
        }
        res = failover.evaluate_and_execute_failover(self.mock_quotas, threshold=1.0, cooldown_seconds=120)
        self.assertFalse(res["triggered"])
        self.assertEqual(res["reason"], "COOLDOWN")

    @patch("antigravity_tracker.accounts.load_accounts")
    @patch("antigravity_tracker.switcher.get_account_session_dir")
    @patch("os.path.isdir")
    @patch("os.path.isfile")
    @patch("antigravity_tracker.failover.load_failover_state")
    def test_evaluate_failover_split_sessions(self, mock_load_state, mock_isfile, mock_isdir, mock_get_dir, mock_load_accs):
        mock_load_state.return_value = {
            "last_failover_timestamp": 0.0,
            "last_from_email": None,
            "last_to_email": None
        }
        mock_load_accs.return_value = {
            "backup_high@example.com": {"email": "backup_high@example.com"}
        }
        mock_get_dir.return_value = "/fake/session/dir"
        mock_isdir.return_value = True
        mock_isfile.return_value = True

        split_quotas = {
            "desktop_acc@example.com": {
                "email": "desktop_acc@example.com",
                "is_current_desktop_session": True,
                "is_current_ide_session": False,
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "weekly": {"remainingPercent": 0.2, "isExhausted": True}
                    }
                ]
            },
            "ide_acc@example.com": {
                "email": "ide_acc@example.com",
                "is_current_desktop_session": False,
                "is_current_ide_session": True,
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "weekly": {"remainingPercent": 92.0, "isExhausted": False}
                    }
                ]
            },
            "backup_high@example.com": {
                "email": "backup_high@example.com",
                "is_current_desktop_session": False,
                "is_current_ide_session": False,
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "weekly": {"remainingPercent": 85.0, "isExhausted": False}
                    }
                ]
            }
        }

        res = failover.evaluate_and_execute_failover(
            split_quotas,
            threshold=1.0,
            dry_run=True
        )
        self.assertTrue(res["triggered"])
        self.assertEqual(res["from_account"], "desktop_acc@example.com")
        self.assertEqual(res["surface"], "desktop")
        self.assertEqual(res["to_account"], "backup_high@example.com")


if __name__ == "__main__":
    unittest.main()
