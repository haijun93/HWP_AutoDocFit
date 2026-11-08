import json
import tempfile
import unittest
from pathlib import Path

from tools.usage_dash.collect import claude_usage, latest_codex_event, scan_logs


class UsageParsingTests(unittest.TestCase):
    def test_latest_codex_rate_limit_event(self):
        with tempfile.TemporaryDirectory() as td:
            path = Path(td) / "session.jsonl"
            rows = [
                {"timestamp": "2026-11-08T01:00:00Z", "type": "event_msg", "payload": {"type": "token_count", "info": {"total_token_usage": {"total_tokens": 100}}, "rate_limits": {"primary": {"used_percent": 21, "resets_at": 123}}}},
                {"timestamp": "2026-11-08T02:00:00Z", "type": "event_msg", "payload": {"type": "token_count", "info": {"total_token_usage": {"total_tokens": 200}}, "rate_limits": {"primary": {"used_percent": 44, "resets_at": 456}, "secondary": {"used_percent": 8}}}},
                {"timestamp": "2026-11-08T03:00:00Z", "type": "response_item", "payload": {"type": "message"}},
            ]
            path.write_text("\n".join(json.dumps(x) for x in rows), encoding="utf-8")
            result = latest_codex_event([path])
            self.assertEqual(result["primary"]["used_percent"], 44)
            self.assertEqual(result["secondary"]["used_percent"], 8)

    def test_claude_recent_token_totals_and_limit_alert(self):
        with tempfile.TemporaryDirectory() as td:
            path = Path(td) / "claude.jsonl"
            rows = [
                {"timestamp": "2026-11-08T10:00:00+00:00", "message": {"usage": {"input_tokens": 120, "output_tokens": 30, "cache_read_input_tokens": 50}}},
                {"timestamp": "2026-11-08T10:01:00+00:00", "message": {"content": [{"text": "rate limit reached"}]}},
            ]
            path.write_text("\n".join(json.dumps(x) for x in rows), encoding="utf-8")
            data = claude_usage(Path(td), 1794135000)
            self.assertEqual(data["tokens_5h"], 200)
            self.assertTrue(data["alert"])
            self.assertTrue(data["estimate"])

    def test_gemini_quota_log_and_worker_count(self):
        with tempfile.TemporaryDirectory() as td:
            path = Path(td) / "VERIFY-agy.log"
            path.write_text("2026-11-08T10:00:00Z starting agy\n2026-11-08T10:02:00Z HTTP 429 quota exceeded\n", encoding="utf-8")
            data = scan_logs(Path(td), "test", 1794135000)
            self.assertTrue(data["alert"])
            self.assertEqual(data["runs"], 1)
            self.assertEqual(data["limit_log"], path.name)


if __name__ == "__main__":
    unittest.main()
