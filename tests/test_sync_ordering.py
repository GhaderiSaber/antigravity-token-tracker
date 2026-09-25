import unittest
from unittest.mock import patch, MagicMock
from antigravity_tracker import auth, accounts, cli, switcher


class TestSyncAndOrdering(unittest.TestCase):

    def test_account_display_priority(self):
        both_active = {"is_current_desktop_session": True, "is_current_ide_session": True}
        desktop_only = {"is_current_desktop_session": True, "is_current_ide_session": False}
        ide_only = {"is_current_desktop_session": False, "is_current_ide_session": True}
        inactive = {"is_current_desktop_session": False, "is_current_ide_session": False}

        self.assertEqual(cli.get_account_display_priority(both_active), 0)
        self.assertEqual(cli.get_account_display_priority(desktop_only), 1)
        self.assertEqual(cli.get_account_display_priority(ide_only), 2)
        self.assertEqual(cli.get_account_display_priority(inactive), 3)

    def test_sort_analyzed_accounts(self):
        analyzed = {
            "duzen@example.com": {
                "is_current_desktop_session": False,
                "is_current_ide_session": False,
                "groups": [{"displayName": "Gemini Models", "weekly": {"remainingPercent": 0.2}}]
            },
            "saber@example.com": {
                "is_current_desktop_session": True,
                "is_current_ide_session": True,
                "groups": [{"displayName": "Gemini Models", "weekly": {"remainingPercent": 15.0}}]
            },
            "other@example.com": {
                "is_current_desktop_session": False,
                "is_current_ide_session": False,
                "groups": [{"displayName": "Gemini Models", "weekly": {"remainingPercent": 85.0}}]
            }
        }

        sorted_accs = cli.sort_analyzed_accounts(analyzed)
        self.assertEqual(len(sorted_accs), 3)
        # Priority 0 (saber@example.com) MUST be first!
        self.assertEqual(sorted_accs[0][0], "saber@example.com")
        # Inactive accounts: higher quota (other@example.com, 85%) before duzen (0.2%)
        self.assertEqual(sorted_accs[1][0], "other@example.com")
        self.assertEqual(sorted_accs[2][0], "duzen@example.com")

    @patch("antigravity_tracker.switcher.snapshot_desktop_session")
    @patch("antigravity_tracker.switcher.snapshot_ide_session")
    @patch("antigravity_tracker.auth.discover_antigravity_ide")
    @patch("antigravity_tracker.auth.discover_antigravity_desktop_app")
    @patch("antigravity_tracker.auth.get_desktop_keyring_token")
    @patch("antigravity_tracker.accounts.load_accounts")
    @patch("antigravity_tracker.accounts.save_accounts")
    def test_sync_from_antigravity_precedence(self, mock_save, mock_load, mock_kr, mock_desk, mock_ide, mock_snap_ide, mock_snap_desk):
        mock_load.return_value = {
            "duzen@example.com": {"email": "duzen@example.com", "tier": "Standard"},
            "saber@example.com": {"email": "saber@example.com", "tier": "Google AI Pro"}
        }
        mock_desk.return_value = {
            "email": "saber@example.com",
            "tier": "Google AI Pro",
            "port": 37027,
            "csrf_token": "csrf-123"
        }
        mock_ide.return_value = {
            "email": "saber@example.com",
            "tier": "Google AI Pro",
            "port": 37271,
            "csrf_token": "csrf-456"
        }
        mock_kr.return_value = (
            "saber@example.com",
            {"token": {"access_token": "ya29.saber", "refresh_token": "1//saber-ref"}},
            "{}"
        )

        active_email, synced_accs = accounts.sync_from_antigravity()
        self.assertEqual(active_email, "saber@example.com")

        # Verify saber has both active flags set
        saber_acc = synced_accs["saber@example.com"]
        self.assertTrue(saber_acc["is_current_desktop_session"])
        self.assertTrue(saber_acc["is_current_ide_session"])
        self.assertEqual(saber_acc["access_token"], "ya29.saber")
        self.assertEqual(saber_acc["refresh_token"], "1//saber-ref")

        # Verify duzen is marked inactive across all surfaces
        duzen_acc = synced_accs["duzen@example.com"]
        self.assertFalse(duzen_acc["is_current_desktop_session"])
        self.assertFalse(duzen_acc["is_current_ide_session"])

    @patch("antigravity_tracker.auth.extract_tokens_from_state_db")
    @patch("antigravity_tracker.accounts.load_accounts")
    def test_snapshot_ide_session_pollution_guard(self, mock_load, mock_extract):
        # state.vscdb contains duzen's token
        mock_extract.return_value = {
            "access_token": "ya29.duzen",
            "refresh_token": "1//duzen-ref"
        }
        mock_load.return_value = {
            "duzen@example.com": {"refresh_token": "1//duzen-ref"},
            "saber@example.com": {"refresh_token": "1//saber-ref"}
        }

        with patch("os.path.isfile", return_value=True):
            # Attempting to snapshot saber when state.vscdb belongs to duzen must fail
            result = switcher.snapshot_ide_session("saber@example.com")
            self.assertFalse(result)

    @patch("antigravity_tracker.switcher._record_switch_state")
    @patch("antigravity_tracker.switcher.restore_desktop_session")
    @patch("antigravity_tracker.switcher.restore_ide_session")
    @patch("antigravity_tracker.switcher.snapshot_desktop_session")
    @patch("antigravity_tracker.switcher.snapshot_ide_session")
    @patch("antigravity_tracker.switcher.is_antigravity_running", return_value=False)
    @patch("antigravity_tracker.accounts.sync_from_antigravity")
    @patch("antigravity_tracker.accounts.load_accounts")
    @patch("antigravity_tracker.auth.discover_antigravity_desktop_app")
    @patch("antigravity_tracker.auth.discover_antigravity_ide")
    def test_switch_surface_targeting(self, mock_ide, mock_desk, mock_accs, mock_sync, mock_run,
                                       mock_snap_ide, mock_snap_desk, mock_rest_ide, mock_rest_desk, mock_record):
        mock_accs.return_value = {
            "saber@example.com": {"email": "saber@example.com", "refresh_token": "tok1"},
            "duzen@example.com": {"email": "duzen@example.com", "refresh_token": "tok2"}
        }
        mock_desk.return_value = {"email": "saber@example.com"}
        mock_ide.return_value = {"email": "saber@example.com"}
        mock_rest_desk.return_value = True
        mock_rest_ide.return_value = True

        # Test desktop-only switch to duzen
        with patch("os.path.isdir", return_value=True), patch("os.path.isfile", return_value=True):
            ok, msg = switcher.switch_to_account("duzen@example.com", restart=False, surface="desktop")
            self.assertTrue(ok)
            self.assertIn("Desktop App session", msg)
            self.assertIn("IDE remains untouched", msg)
            mock_rest_desk.assert_called_with("duzen@example.com")
            mock_rest_ide.assert_not_called()
            mock_record.assert_called_with("saber@example.com", "duzen@example.com", "desktop")

        mock_rest_desk.reset_mock()
        mock_rest_ide.reset_mock()
        mock_record.reset_mock()

        # Test ide-only switch to duzen
        with patch("os.path.isdir", return_value=True), patch("os.path.isfile", return_value=True):
            ok, msg = switcher.switch_to_account("duzen@example.com", restart=False, surface="ide")
            self.assertTrue(ok)
            self.assertIn("IDE session", msg)
            self.assertIn("Desktop App remains untouched", msg)
            mock_rest_ide.assert_called_with("duzen@example.com")
            mock_rest_desk.assert_not_called()
            mock_record.assert_called_with("saber@example.com", "duzen@example.com", "ide")


if __name__ == "__main__":
    unittest.main()
