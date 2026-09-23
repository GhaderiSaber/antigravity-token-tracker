import unittest
import os
import json
import tempfile
import shutil
import time
from unittest.mock import patch, MagicMock

from antigravity_tracker import notifier


class TestNotifier(unittest.TestCase):

    def setUp(self):
        self.temp_dir = tempfile.mkdtemp(prefix="agy_notify_test_")
        self.orig_state_file = notifier.STATE_FILE
        notifier.STATE_FILE = os.path.join(self.temp_dir, "notification_state.json")

    def tearDown(self):
        notifier.STATE_FILE = self.orig_state_file
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_load_and_save_notification_state(self):
        state = notifier.load_notification_state()
        self.assertEqual(state, {})

        sample_state = {
            "dev@example.com": {
                "Gemini Models-weekly": {
                    "status": "OK",
                    "pct": 85.0
                }
            }
        }
        notifier.save_notification_state(sample_state)
        reloaded = notifier.load_notification_state()
        self.assertIn("dev@example.com", reloaded)
        self.assertEqual(reloaded["dev@example.com"]["Gemini Models-weekly"]["pct"], 85.0)

    def test_dbus_notification_client_action_execution(self):
        client = notifier.DBusNotificationClient()
        # Mock the DBus interface
        mock_iface = MagicMock()
        mock_iface.Notify.return_value = 42
        client._iface = mock_iface
        client._initialized = True

        called = []

        def my_action():
            called.append("success")

        actions = [
            ("switch_test", "⚡ Switch Test", my_action),
            ("dashboard", "🌐 Dashboard", None)
        ]

        nid = client.notify(
            title="Test Title",
            message="Test Message",
            urgency="critical",
            actions=actions
        )

        self.assertEqual(nid, 42)
        self.assertIn(42, client._active_actions)
        self.assertIn("switch_test", client._active_actions[42])

        # Simulate ActionInvoked signal
        client._on_action_invoked(42, "switch_test")

        # Allow worker thread to run
        time.sleep(0.1)

        self.assertIn("success", called)
        # Verify action was removed after invocation
        self.assertNotIn(42, client._active_actions)

    def test_dbus_notification_client_closed_cleanup(self):
        client = notifier.DBusNotificationClient()
        mock_iface = MagicMock()
        mock_iface.Notify.return_value = 99
        client._iface = mock_iface
        client._initialized = True

        actions = [("test_key", "Test Label", lambda: None)]
        nid = client.notify("Title", "Message", actions=actions)
        self.assertEqual(nid, 99)
        self.assertIn(99, client._active_actions)

        # Simulate NotificationClosed signal
        client._on_notification_closed(99, 1)
        self.assertNotIn(99, client._active_actions)

    @patch("antigravity_tracker.notifier.subprocess.run")
    def test_send_desktop_notification_fallback(self, mock_run):
        # Force DBus failure
        with patch.object(notifier.DBusNotificationClient, "notify", return_value=None):
            with patch("antigravity_tracker.notifier.is_notify_available", return_value=True):
                nid = notifier.send_desktop_notification("Fallback Title", "Fallback Message", urgency="normal")
                self.assertIsNone(nid)
                mock_run.assert_called_once()
                args, kwargs = mock_run.call_args
                cmd = args[0]
                self.assertIn("notify-send", cmd)
                self.assertIn("Fallback Title", cmd)
                self.assertIn("Fallback Message", cmd)

    @patch("antigravity_tracker.failover.find_best_failover_candidate", return_value=("backup@example.com", 80.0))
    @patch("antigravity_tracker.notifier.send_desktop_notification")
    def test_check_and_notify_lifecycle_events_exhausted(self, mock_notify, mock_find_best):
        # Initial OK state in memory
        init_state = {
            "primary@example.com": {
                "Gemini Models-weekly": {"status": "OK", "pct": 40.0}
            }
        }
        notifier.save_notification_state(init_state)

        analyzed_quotas = {
            "primary@example.com": {
                "email": "primary@example.com",
                "is_current_desktop_session": True,
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "weekly": {"remainingPercent": 0.5, "countdown": "2h"}
                    }
                ]
            },
            "backup@example.com": {
                "email": "backup@example.com",
                "is_current_desktop_session": False,
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "weekly": {"remainingPercent": 80.0, "countdown": "5d"}
                    }
                ]
            }
        }

        notifier.check_and_notify_lifecycle_events(analyzed_quotas)
        self.assertTrue(mock_notify.called)

        # Inspect the call arguments
        call_args = mock_notify.call_args
        title = call_args[0][0]
        msg = call_args[0][1]
        kwargs = call_args[1]

        self.assertIn("Depleted", title)
        self.assertEqual(kwargs.get("urgency"), "critical")
        self.assertIn("actions", kwargs)
        actions = kwargs["actions"]
        # Verify 1-click switch action button was generated with backup account
        action_keys = [a[0] for a in actions]
        action_labels = [a[1] for a in actions]
        self.assertTrue(any("switch:backup@example.com" in k for k in action_keys))
        self.assertTrue(any("backup@example.com" in l for l in action_labels))
        self.assertTrue(any("Dashboard" in l for l in action_labels))


if __name__ == "__main__":
    unittest.main()
