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

        # Check action map (supports both single string or tuple target)
        has_switch = any(
            action == "switch" and (target == "backup@example.com" or (isinstance(target, tuple) and target[0] == "backup@example.com"))
            for action, target in applet.menu_action_map.values()
        )
        self.assertTrue(has_switch)

    def test_get_active_summary_split_sessions(self):
        applet = tray.TrayApplet()
        applet.geo_info = {"country_code": "US", "country_name": "United States", "flag": "🇺🇸", "is_restricted": False}
        applet.analyzed_quotas = {
            "app@example.com": {
                "email": "app@example.com",
                "is_current_desktop_session": True,
                "is_current_ide_session": False,
                "groups": [{"displayName": "Gemini Models", "weekly": {"remainingPercent": 16.6}}]
            },
            "ide@example.com": {
                "email": "ide@example.com",
                "is_current_desktop_session": False,
                "is_current_ide_session": True,
                "groups": [{"displayName": "Gemini Models", "weekly": {"remainingPercent": 98.9}}]
            }
        }
        summary = applet.get_active_summary()
        self.assertIn("📱 App: app@example.com (16.6%)", summary)
        self.assertIn("💻 IDE: ide@example.com (98.9%)", summary)
        self.assertIn("🇺🇸 US", summary)

    def test_get_active_summary_unified_session(self):
        applet = tray.TrayApplet()
        applet.geo_info = {"country_name": "United States", "flag": "🇺🇸", "is_restricted": False}
        applet.analyzed_quotas = {
            "unified@example.com": {
                "email": "unified@example.com",
                "is_current_desktop_session": True,
                "is_current_ide_session": True,
                "groups": [{"displayName": "Gemini Models", "weekly": {"remainingPercent": 80.0}}]
            }
        }
        summary = applet.get_active_summary()
        self.assertIn("unified@example.com (App + IDE): 80.0% remaining", summary)

    @patch("antigravity_tracker.accounts.list_accounts")
    def test_build_menu_layout_split_sessions(self, mock_list_accs):
        mock_list_accs.return_value = [
            {"email": "app@example.com"},
            {"email": "ide@example.com"},
            {"email": "standby@example.com"}
        ]
        applet = tray.TrayApplet()
        applet.analyzed_quotas = {
            "app@example.com": {
                "email": "app@example.com",
                "is_current_desktop_session": True,
                "is_current_ide_session": False,
                "groups": [{"displayName": "Gemini Models", "weekly": {"remainingPercent": 20.0, "countdown": "1d"}}]
            },
            "ide@example.com": {
                "email": "ide@example.com",
                "is_current_desktop_session": False,
                "is_current_ide_session": True,
                "groups": [{"displayName": "Gemini Models", "weekly": {"remainingPercent": 90.0, "countdown": "4d"}}]
            },
            "standby@example.com": {
                "email": "standby@example.com",
                "is_current_desktop_session": False,
                "is_current_ide_session": False,
                "groups": [{"displayName": "Gemini Models", "weekly": {"remainingPercent": 95.0, "countdown": "6d"}}]
            }
        }

        root_struct = applet.build_menu_layout()
        children = root_struct[2]

        labels = []
        for child in children:
            props = child[1]
            if "label" in props:
                labels.append(str(props["label"]))

        # Assert App and IDE separate sections
        self.assertTrue(any("📱 Desktop App: app@example.com" in l for l in labels))
        self.assertTrue(any("💻 Antigravity IDE: ide@example.com" in l for l in labels))

        # Assert quick sync surface actions
        self.assertTrue(any("Sync Surfaces: Align IDE to App" in l for l in labels))
        self.assertTrue(any("Sync Surfaces: Align App to IDE" in l for l in labels))

        # Assert action map contains targeted sync actions and dashboard actions
        actions = list(applet.menu_action_map.values())
        self.assertIn(("switch", ("app@example.com", "ide")), actions)
        self.assertIn(("switch", ("ide@example.com", "desktop")), actions)
        self.assertIn(("switch", ("standby@example.com", "both")), actions)
        self.assertIn(("web", None), actions)

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

    @patch("antigravity_tracker.switcher.switch_to_account")
    @patch("antigravity_tracker.notifier.send_desktop_notification")
    def test_handle_menu_click_surface_switch(self, mock_notify, mock_switch):
        mock_switch.return_value = (True, "Switched successfully")
        applet = tray.TrayApplet()
        applet.menu_action_map = {
            1: ("switch", ("target@example.com", "desktop")),
            2: ("switch", ("target@example.com", "ide")),
            3: ("switch", "target@example.com")
        }

        # 1. Desktop switch
        applet.handle_menu_click(1)
        import time
        time.sleep(0.1)  # allow thread to run
        mock_switch.assert_called_with("target@example.com", restart=True, surface="desktop")

        # 2. IDE switch
        applet.handle_menu_click(2)
        time.sleep(0.1)
        mock_switch.assert_called_with("target@example.com", restart=True, surface="ide")

        # 3. Default / both switch
        applet.handle_menu_click(3)
        time.sleep(0.1)
        mock_switch.assert_called_with("target@example.com", restart=True, surface="both")

    @patch("antigravity_tracker.accounts.sync_from_antigravity")
    @patch("antigravity_tracker.quota.fetch_all_accounts_quota")
    @patch("antigravity_tracker.geo.get_ip_geo")
    @patch("antigravity_tracker.shield.evaluate_and_enforce_shield")
    def test_refresh_data_lowest_quota_icon(self, mock_shield, mock_geo, mock_quota, mock_sync):
        mock_geo.return_value = {"country_code": "US", "is_restricted": False}
        mock_quota.return_value = {
            "app@example.com": {
                "email": "app@example.com",
                "is_current_desktop_session": True,
                "is_current_ide_session": False,
                "groups": [{"displayName": "Gemini Models", "weekly": {"remainingPercent": 16.6}}]
            },
            "ide@example.com": {
                "email": "ide@example.com",
                "is_current_desktop_session": False,
                "is_current_ide_session": True,
                "groups": [{"displayName": "Gemini Models", "weekly": {"remainingPercent": 95.0}}]
            }
        }
        applet = tray.TrayApplet()
        applet.refresh_data(force=False)
        # 16.6 rounds to 17, representing the lower quota between Desktop (16.6%) and IDE (95.0%)
        self.assertEqual(applet.current_icon_name, "quota_17")


if __name__ == "__main__":
    unittest.main()
