import unittest
import os
from unittest.mock import patch, MagicMock
from antigravity_tracker import tray


class TestTrayApplet(unittest.TestCase):

    def test_generate_tray_icon(self):
        icon_name = tray.generate_tray_icon(85.0)
        self.assertEqual(icon_name, "quota_85")
        icon_path = os.path.join(tray.ICON_CACHE_DIR, f"{icon_name}.png")
        self.assertTrue(os.path.isfile(icon_path))

        icon_exh = tray.generate_tray_icon(0.2, is_exhausted=True)
        self.assertEqual(icon_exh, "quota_exhausted")
        icon_exh_path = os.path.join(tray.ICON_CACHE_DIR, f"{icon_exh}.png")
        self.assertTrue(os.path.isfile(icon_exh_path))

    @patch("antigravity_tracker.accounts.list_accounts")
    def test_build_menu_layout(self, mock_list_accs):
        mock_list_accs.return_value = [
            {"email": "active@example.com"},
            {"email": "backup@example.com"}
        ]

        applet = tray.TrayApplet()
        applet.analyzed_quotas = {
            "active@example.com": {
                "email": "active@example.com",
                "is_current_desktop_session": True,
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "weekly": {"remainingPercent": 55.0, "countdown": "2d 4h"}
                    }
                ]
            },
            "backup@example.com": {
                "email": "backup@example.com",
                "is_current_desktop_session": False,
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "weekly": {"remainingPercent": 95.0, "countdown": "5d 10h"}
                    }
                ]
            }
        }

        root_struct = applet.build_menu_layout()
        self.assertIsNotNone(root_struct)
        # root_struct is (id, props, children)
        self.assertEqual(root_struct[0], 0)
        children = root_struct[2]
        self.assertGreater(len(children), 3)

        # Check action map
        has_switch = any(action == "switch" and target == "backup@example.com" for action, target in applet.menu_action_map.values())
        self.assertTrue(has_switch)

    def test_singleton_lock_acquisition_and_duplicate_prevention(self):
        import tempfile
        import shutil

        temp_dir = tempfile.mkdtemp(prefix="agy_lock_test_")
        test_lock_file = os.path.join(temp_dir, "test_tray.lock")

        try:
            # 1. First process/caller acquires lock successfully
            fd1, existing_pid = tray.acquire_tray_lock(lock_path=test_lock_file)
            self.assertIsNotNone(fd1)
            self.assertIsNone(existing_pid)
            self.assertTrue(os.path.isfile(test_lock_file))

            with open(test_lock_file, "r") as f:
                content = f.read().strip()
                self.assertEqual(content, str(os.getpid()))

            # 2. Second attempt on the same lockfile must be rejected
            fd2, existing_pid2 = tray.acquire_tray_lock(lock_path=test_lock_file)
            self.assertIsNone(fd2)
            self.assertEqual(existing_pid2, os.getpid())

            # 3. Release first lock
            tray.release_tray_lock(fd1, lock_path=test_lock_file)

            # 4. Third attempt should now succeed
            fd3, existing_pid3 = tray.acquire_tray_lock(lock_path=test_lock_file)
            self.assertIsNotNone(fd3)
            self.assertIsNone(existing_pid3)
            tray.release_tray_lock(fd3, lock_path=test_lock_file)

        finally:
            shutil.rmtree(temp_dir, ignore_errors=True)

    def test_tray_applet_lock_lifecycle(self):
        import tempfile
        import shutil

        temp_dir = tempfile.mkdtemp(prefix="agy_applet_lock_test_")
        test_lock_file = os.path.join(temp_dir, "applet.lock")

        try:
            applet1 = tray.TrayApplet(lock_path=test_lock_file)
            self.assertTrue(applet1.acquire_lock())
            self.assertIsNotNone(applet1.lock_fd)

            # Attempting to acquire again on same applet returns True (already held)
            self.assertTrue(applet1.acquire_lock())

            # Second applet instance fails to acquire
            applet2 = tray.TrayApplet(lock_path=test_lock_file)
            self.assertFalse(applet2.acquire_lock())
            self.assertIsNone(applet2.lock_fd)

            # Releasing applet1 allows applet2 to acquire
            applet1.release_lock()
            self.assertIsNone(applet1.lock_fd)

            self.assertTrue(applet2.acquire_lock())
            self.assertIsNotNone(applet2.lock_fd)
            applet2.release_lock()

        finally:
            shutil.rmtree(temp_dir, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
