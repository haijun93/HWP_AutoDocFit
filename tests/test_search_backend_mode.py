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
from docfit_core.timemachine.search_backend import ModeSearchBackend, RustBackend


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

    def test_mode_provider_returns_requested_mode_without_disabling_rust(self):
        for mode in ("rust", "shadow_compare"):
            service = TimemachineService.__new__(TimemachineService)
            service.search_backend_mode_provider = lambda mode=mode: mode
            service._settings = {}
            service._unsupported_search_backend_logged = set()
            with self.subTest(mode=mode):
                self.assertEqual(service._search_backend_mode(), mode)

    def test_rust_failure_falls_back_to_sqlite_and_logs_reason(self):
        sqlite = Mock()
        rust = Mock()
        rust.search.side_effect = RuntimeError("klib unavailable")
        sqlite.search.return_value = [{"path": "sqlite.txt"}]
        backend = ModeSearchBackend(sqlite, rust, lambda: "rust")
        with self.assertLogs("docfit_core.timemachine.search_backend", level="WARNING") as captured:
            self.assertEqual(backend.search("query"), [{"path": "sqlite.txt"}])
        self.assertIn("SQLite fallback", captured.output[0])

    def test_batch_index_uses_rust_and_falls_back_per_failed_file(self):
        sqlite = Mock()
        rust = Mock()
        paths = ["ok.txt", "unsupported.txt"]
        rust.index_files.return_value = [{"indexed": True, "error": None},
                                         {"indexed": False, "error": "unsupported"}]
        sqlite.index_file.return_value = {"indexed": True, "error": None}
        backend = ModeSearchBackend(sqlite, rust, lambda: "rust")
        rows = backend.index_files(paths, batch_size=250)
        rust.index_files.assert_called_once_with(paths, batch_size=250)
        sqlite.index_file.assert_called_once_with("unsupported.txt")
        self.assertEqual(rows, [{"indexed": True, "error": None}, {"indexed": True, "error": None}])

    def test_sqlite_batch_adapter_keeps_existing_per_file_calls(self):
        sqlite = Mock()
        rust = Mock()
        sqlite.index_files.return_value = [{"indexed": True, "error": None}]
        backend = ModeSearchBackend(sqlite, rust, lambda: "sqlite")
        self.assertEqual(backend.index_files(["one.txt"]), sqlite.index_files.return_value)
        sqlite.index_files.assert_called_once_with(["one.txt"], batch_size=250)
        rust.index_files.assert_not_called()

    def test_shadow_compare_records_diffs_and_returns_sqlite_rows(self):
        import json
        import tempfile

        sqlite = Mock()
        rust = Mock()
        sqlite.search.return_value = [{"path": "same.txt"}, {"path": "sqlite-only.txt"}]
        rust.search.return_value = [{"path": "same.txt"}, {"path": "rust-only.txt"}]
        with tempfile.TemporaryDirectory() as temp:
            log = Path(temp) / "shadow.jsonl"
            backend = ModeSearchBackend(sqlite, rust, lambda: "shadow_compare", shadow_log=log)
            result = backend.search("query", limit=20)
            record = json.loads(log.read_text(encoding="utf-8"))
        self.assertEqual(result, sqlite.search.return_value)
        self.assertEqual(record["missing_in_rust"], ["sqlite-only.txt"])
        self.assertEqual(record["extra_in_rust"], ["rust-only.txt"])
        self.assertEqual(record["match_rate"], 0.5)

    def test_rust_rows_convert_to_existing_keyword_response_shape(self):
        rust = RustBackend.__new__(RustBackend)
        rust._request = Mock(return_value={"results": [{
            "path": "folder/doc.txt", "name": "doc.txt", "snippet": "정책 문서",
            "position": {"page": None, "sheet": None, "row": None}, "score": 2,
        }]})
        rows = rust.search("정책 문서", limit=20)
        self.assertEqual(rows[0]["name"], "doc.txt")
        self.assertEqual(rows[0]["total_matches"], 2)
        self.assertEqual(rows[0]["match_counts"], {"정책": 1, "문서": 1})
        self.assertEqual(rows[0]["highlights"], [[0, 2], [3, 5]])

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
