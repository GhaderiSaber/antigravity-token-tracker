import unittest
from unittest.mock import patch, MagicMock
from antigravity_tracker import doctor


class TestDoctorDiagnostic(unittest.TestCase):

    def test_check_system_environment(self):
        sys_res = doctor.check_system_environment()
        self.assertIsInstance(sys_res, list)
        self.assertGreater(len(sys_res), 0)
        for item in sys_res:
            self.assertIn("component", item)
            self.assertIn("status", item)
            self.assertIn("details", item)

    def test_check_antigravity_runtime(self):
        app_res = doctor.check_antigravity_runtime()
        self.assertIsInstance(app_res, list)
        self.assertGreater(len(app_res), 0)

    @patch("antigravity_tracker.accounts.load_accounts")
    @patch("antigravity_tracker.switcher.get_account_session_dir")
    @patch("os.path.isdir")
    @patch("os.path.isfile")
    def test_check_accounts_health(self, mock_isfile, mock_isdir, mock_get_dir, mock_load_accs):
        mock_load_accs.return_value = {
            "ready@example.com": {
                "email": "ready@example.com",
                "tier": "Pro",
                "refresh_token": "valid_rt"
            },
            "partial@example.com": {
                "email": "partial@example.com",
                "tier": "Standard"
            }
        }
        mock_get_dir.return_value = "/fake/dir"
        mock_isdir.return_value = True
        mock_isfile.return_value = True

        acc_res = doctor.check_accounts_health(validate_google_oauth=False)
        self.assertEqual(len(acc_res), 2)
        ready_item = next(a for a in acc_res if a["email"] == "ready@example.com")
        self.assertEqual(ready_item["switch_readiness"], "READY")

    def test_run_full_diagnostic(self):
        diag = doctor.run_full_diagnostic(validate_remote=False)
        self.assertIn("overall_health", diag)
        self.assertIn("system_checks", diag)
        self.assertIn("app_checks", diag)
        self.assertIn("account_checks", diag)


if __name__ == "__main__":
    unittest.main()
