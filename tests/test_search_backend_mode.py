import json
import threading
import unittest
from pathlib import Path
from unittest.mock import Mock

from docfit_app.configuration_service import EXCLUDED, fields
from docfit_app.settings_schema import (
    ALL_DEFAULTS,
    SEARCH_BACKEND_MODES,
    normalize_search_backend_mode,
    validate_search_backend_mode,
)
from docfit_app.timemachine_service import TimemachineService


class SearchBackendModeTests(unittest.TestCase):
    def test_modes_parse_and_unknown_values_fall_back_to_sqlite(self):
        self.assertEqual(SEARCH_BACKEND_MODES, ("sqlite", "rust", "shadow_compare"))
        for mode in SEARCH_BACKEND_MODES:
            with self.subTest(mode=mode):
                self.assertEqual(normalize_search_backend_mode(mode), mode)
                self.assertEqual(validate_search_backend_mode(mode), mode)
        for value in (None, "", "typo", 4):
            with self.subTest(value=value):
                self.assertEqual(normalize_search_backend_mode(value), "sqlite")
                with self.assertRaises(ValueError):
                    validate_search_backend_mode(value)

    def test_unavailable_rust_modes_log_reason_and_select_sqlite_fallback(self):
        for mode in ("rust", "shadow_compare"):
            service = TimemachineService.__new__(TimemachineService)
            service.search_backend_mode_provider = lambda mode=mode: mode
            service._settings = {}
            service._unsupported_search_backend_logged = set()
            with self.subTest(mode=mode), self.assertLogs("docfit_app.timemachine_service", level="WARNING") as captured:
                self.assertEqual(service._search_backend_mode(), mode)
            self.assertIn("SQLite로 대체", captured.output[0])
            self.assertIn("아직 연결되지 않았습니다", captured.output[0])

    def test_rust_mode_search_returns_sqlite_adapter_rows(self):
        service = TimemachineService.__new__(TimemachineService)
        service.search_backend_mode_provider = lambda: "rust"
        service._unsupported_search_backend_logged = set()
        service.search_engine_provider = None
        service.content_index = Mock()
        service.content_index.parse_query.return_value = {"terms": ["query"], "phrases": [], "near": None}
        service._settings = {"roots": ["synthetic-root"], "search_engine": "kfind"}
        service._content_lock = threading.RLock()
        service._engine_state = {"engine": "legacy", "engine_reason": "not_connected"}
        expected = [{"path": "synthetic.txt", "name": "synthetic.txt"}]
        service.search_backend = Mock()
        service.search_backend.search.return_value = expected
        with self.assertLogs("docfit_app.timemachine_service", level="WARNING"):
            actual = service._search_rows("query")
        self.assertIs(actual, expected)
        service.search_backend.search.assert_called_once_with(
            "query", roots=["synthetic-root"], hierarchy=False, limit=100, offset=0,
            sort="hierarchy_relevance", start=None, end=None)

    def test_mode_is_hidden_from_settings_form_and_golden_matches_schema(self):
        self.assertIn("search_backend_mode", EXCLUDED)
        self.assertNotIn("search_backend_mode", {field["key"] for field in fields()})
        root = Path(__file__).resolve().parents[1]
        golden = json.loads((root / "docs" / "settings_defaults.json").read_text(encoding="utf-8"))
        self.assertEqual(golden, ALL_DEFAULTS)


if __name__ == "__main__":
    unittest.main()
