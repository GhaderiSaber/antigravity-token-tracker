import unittest
from unittest.mock import patch, MagicMock
import time

from antigravity_tracker import geo


class TestGeo(unittest.TestCase):
    def test_get_flag_emoji(self):
        self.assertEqual(geo.get_flag_emoji("US"), "🇺🇸")
        self.assertEqual(geo.get_flag_emoji("de"), "🇩🇪")
        self.assertEqual(geo.get_flag_emoji("IR"), "🇮🇷")
        self.assertEqual(geo.get_flag_emoji(""), "🌐")
        self.assertEqual(geo.get_flag_emoji("XYZ"), "🌐")

    def test_is_restricted_country(self):
        self.assertTrue(geo.is_restricted_country("IR"))
        self.assertTrue(geo.is_restricted_country("ir"))
        self.assertTrue(geo.is_restricted_country("RU"))
        self.assertTrue(geo.is_restricted_country("CN"))
        self.assertFalse(geo.is_restricted_country("US"))
        self.assertFalse(geo.is_restricted_country("DE"))
        self.assertFalse(geo.is_restricted_country(""))

    def test_format_ip_summary(self):
        sample = {
            "ip": "1.2.3.4",
            "country_code": "DE",
            "country_name": "Germany",
            "city": "Frankfurt",
            "flag": "🇩🇪"
        }
        res = geo.format_ip_summary(sample, include_ip=True)
        self.assertIn("🇩🇪", res)
        self.assertIn("Frankfurt, DE", res)
        self.assertIn("1.2.3.4", res)

        res_no_ip = geo.format_ip_summary(sample, include_ip=False)
        self.assertNotIn("1.2.3.4", res_no_ip)
        self.assertIn("Frankfurt, DE", res_no_ip)

    @patch("antigravity_tracker.geo.query_ip_geo")
    def test_get_ip_geo_caching(self, mock_query):
        # Reset cache
        geo._GEO_CACHE = None
        geo._CACHE_TIMESTAMP = 0.0

        mock_query.return_value = {
            "ip": "8.8.8.8",
            "country_code": "US",
            "country_name": "United States",
            "city": "Mountain View",
            "region": "California",
            "isp": "Google LLC",
            "org": "Google",
            "flag": "🇺🇸",
            "is_restricted": False,
            "checked_at": time.time(),
            "summary": "🇺🇸 Mountain View, US (8.8.8.8)"
        }

        # 1st call: queries external
        res1 = geo.get_ip_geo(force_refresh=False)
        self.assertEqual(res1["ip"], "8.8.8.8")
        self.assertEqual(mock_query.call_count, 1)

        # 2nd call: hits cache without querying again
        res2 = geo.get_ip_geo(force_refresh=False)
        self.assertEqual(res2["ip"], "8.8.8.8")
        self.assertEqual(mock_query.call_count, 1)

        # Force refresh calls query again
        res3 = geo.get_ip_geo(force_refresh=True)
        self.assertEqual(res3["ip"], "8.8.8.8")
        self.assertEqual(mock_query.call_count, 2)

    @patch("antigravity_tracker.geo.query_ip_geo")
    def test_get_ip_geo_fallback(self, mock_query):
        geo._GEO_CACHE = None
        geo._CACHE_TIMESTAMP = 0.0
        mock_query.return_value = None

        res = geo.get_ip_geo(force_refresh=False)
        self.assertEqual(res["ip"], "Unknown")
        self.assertFalse(res["is_restricted"])


if __name__ == "__main__":
    unittest.main()
