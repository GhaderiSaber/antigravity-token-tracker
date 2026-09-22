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


if __name__ == "__main__":
    unittest.main()
