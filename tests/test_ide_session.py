import unittest
import os
import json
import sqlite3
import tempfile
import base64
from unittest.mock import patch, MagicMock

from antigravity_tracker import auth, switcher, accounts, doctor, cli


class TestIDESession(unittest.TestCase):

    def setUp(self):
        self.tmp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp_dir.cleanup)

    def test_extract_user_email_from_state_db(self):
        db_path = os.path.join(self.tmp_dir.name, "state.vscdb")
        conn = sqlite3.connect(db_path)
        c = conn.cursor()
        c.execute("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)")

        # Create a mock userStatus payload with base64 encoded email
        inner = base64.b64encode(b"\x1a\x05Duzen:\x12duzenapp@gmail.com\x8a\x02").decode()
        outer = base64.b64encode(inner.encode()).decode()
        c.execute("INSERT INTO ItemTable VALUES (?, ?)", ("antigravityUnifiedStateSync.userStatus", outer))
        conn.commit()
        conn.close()

        email = auth.extract_user_email_from_state_db(db_path)
        self.assertEqual(email, "duzenapp@gmail.com")

    @patch("antigravity_tracker.auth.extract_tokens_from_state_db")
    @patch("antigravity_tracker.auth.extract_user_email_from_state_db")
    @patch("antigravity_tracker.accounts.load_accounts")
    @patch("antigravity_tracker.accounts.save_accounts")
    def test_snapshot_ide_session_success(self, mock_save_accs, mock_load_accs, mock_email, mock_tokens):
        db_path = os.path.join(self.tmp_dir.name, "state.vscdb")
        conn = sqlite3.connect(db_path)
        c = conn.cursor()
        c.execute("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)")
        c.execute("INSERT INTO ItemTable VALUES (?, ?)", ("antigravityUnifiedStateSync.oauthToken", "tok_val"))
        c.execute("INSERT INTO ItemTable VALUES (?, ?)", ("antigravityUnifiedStateSync.userStatus", "status_val"))
        c.execute("INSERT INTO ItemTable VALUES (?, ?)", ("antigravity.profileUrl", "https://pic.url"))
        conn.commit()
        conn.close()

        mock_tokens.return_value = {
            "access_token": "ya29.ide_test",
            "refresh_token": "1//ide_refresh_test"
        }
        mock_email.return_value = "duzenapp@gmail.com"
        mock_load_accs.return_value = {
            "duzenapp@gmail.com": {"email": "duzenapp@gmail.com", "refresh_token": ""}
        }

        sessions_dir = os.path.join(self.tmp_dir.name, "sessions")
        with patch("antigravity_tracker.switcher.IDE_STATE_DB", db_path), \
             patch("antigravity_tracker.switcher.SESSIONS_DIR", sessions_dir):

            ok = switcher.snapshot_ide_session("duzenapp@gmail.com")
            self.assertTrue(ok)

            # Verify ide_tokens.json was saved
            ide_tokens_file = os.path.join(sessions_dir, "duzenapp@gmail.com", "ide_tokens.json")
            self.assertTrue(os.path.isfile(ide_tokens_file))
            with open(ide_tokens_file) as f:
                saved = json.load(f)
            self.assertEqual(saved["antigravityUnifiedStateSync.oauthToken"], "tok_val")
            self.assertEqual(saved["antigravityUnifiedStateSync.userStatus"], "status_val")
            self.assertEqual(saved["antigravity.profileUrl"], "https://pic.url")

            # Verify keyring_token.json was synthesized
            kr_file = os.path.join(sessions_dir, "duzenapp@gmail.com", "keyring_token.json")
            self.assertTrue(os.path.isfile(kr_file))
            with open(kr_file) as f:
                kr_data = json.load(f)
            self.assertEqual(kr_data["token"]["refresh_token"], "1//ide_refresh_test")

            # Verify snapshot_meta.json has has_ide_session = True
            meta_file = os.path.join(sessions_dir, "duzenapp@gmail.com", "snapshot_meta.json")
            self.assertTrue(os.path.isfile(meta_file))
            with open(meta_file) as f:
                meta = json.load(f)
            self.assertTrue(meta["has_ide_session"])

            # Verify accounts.json was updated with tokens
            mock_save_accs.assert_called()

    def test_restore_ide_session(self):
        db_path = os.path.join(self.tmp_dir.name, "state.vscdb")
        sessions_dir = os.path.join(self.tmp_dir.name, "sessions")
        acc_dir = os.path.join(sessions_dir, "duzenapp@gmail.com")
        os.makedirs(acc_dir, exist_ok=True)

        ide_tokens_file = os.path.join(acc_dir, "ide_tokens.json")
        with open(ide_tokens_file, "w") as f:
            json.dump({
                "antigravityUnifiedStateSync.oauthToken": "restored_token",
                "antigravityUnifiedStateSync.userStatus": "restored_status"
            }, f)

        with patch("antigravity_tracker.switcher.IDE_STATE_DB", db_path), \
             patch("antigravity_tracker.switcher.SESSIONS_DIR", sessions_dir):

            ok = switcher.restore_ide_session("duzenapp@gmail.com")
            self.assertTrue(ok)

            # Check SQLite table
            conn = sqlite3.connect(db_path)
            c = conn.cursor()
            c.execute("SELECT key, value FROM ItemTable WHERE key = 'antigravityUnifiedStateSync.oauthToken'")
            row = c.fetchone()
            self.assertEqual(row[1], "restored_token")
            conn.close()

    @patch("antigravity_tracker.auth.discover_antigravity_desktop_app", return_value=None)
    @patch("antigravity_tracker.auth.discover_antigravity_ide")
    @patch("antigravity_tracker.auth.extract_tokens_from_state_db")
    @patch("antigravity_tracker.auth.extract_user_email_from_state_db")
    @patch("antigravity_tracker.switcher.snapshot_ide_session")
    @patch("antigravity_tracker.accounts.load_accounts")
    @patch("antigravity_tracker.accounts.save_accounts")
    def test_sync_from_antigravity_autosnapshots_ide(self, mock_save, mock_load, mock_snap_ide,
                                                      mock_email, mock_tokens, mock_ide, mock_app):
        mock_load.return_value = {
            "duzenapp@gmail.com": {"email": "duzenapp@gmail.com", "tier": "Google AI Pro"}
        }
        mock_ide.return_value = {
            "email": "duzenapp@gmail.com",
            "name": "Duzen",
            "tier": "Google AI Pro",
            "port": 38473,
            "csrf_token": "csrf-test"
        }
        mock_tokens.return_value = {"access_token": "acc_tok", "refresh_token": "ref_tok"}
        mock_email.return_value = "duzenapp@gmail.com"

        active_email, all_accs = accounts.sync_from_antigravity()
        self.assertEqual(active_email, "duzenapp@gmail.com")
        self.assertTrue(all_accs["duzenapp@gmail.com"]["is_current_ide_session"])
        self.assertEqual(all_accs["duzenapp@gmail.com"]["refresh_token"], "ref_tok")
        # Verify snapshot_ide_session was automatically invoked!
        mock_snap_ide.assert_called_with("duzenapp@gmail.com")

    @patch("antigravity_tracker.accounts.load_accounts")
    @patch("antigravity_tracker.switcher.get_account_session_dir")
    def test_doctor_ide_session_health(self, mock_session_dir, mock_load):
        mock_load.return_value = {
            "ide_only@example.com": {
                "email": "ide_only@example.com",
                "tier": "Pro",
                "refresh_token": "rt"
            }
        }
        acc_dir = os.path.join(self.tmp_dir.name, "ide_only@example.com")
        os.makedirs(acc_dir, exist_ok=True)
        # Create only ide_tokens.json
        with open(os.path.join(acc_dir, "ide_tokens.json"), "w") as f:
            f.write("{}")

        mock_session_dir.return_value = acc_dir
        res = doctor.check_accounts_health(validate_google_oauth=False)
        self.assertEqual(len(res), 1)
        self.assertTrue(res[0]["has_ide_snapshot"])
        self.assertEqual(res[0]["switch_readiness"], "READY")


if __name__ == "__main__":
    unittest.main()
