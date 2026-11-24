import threading
import unittest
from unittest.mock import Mock

from docfit_core.timemachine.search_backend import SearchBackend, SqliteBackend


class SearchBackendContractTests(unittest.TestCase):
    def setUp(self):
        self.index = Mock()
        self.backend = SqliteBackend(self.index)

    def test_sqlite_adapter_wraps_index_results_without_transforming_them(self):
        indexed = {"indexed": True, "error": None}
        status = {"has_index": True, "persistent_error_count": 0}
        self.index.index_file.return_value = indexed
        self.index.remove_file.return_value = None
        self.index.status_summary.return_value = status

        self.assertIs(self.backend.index_file("doc.txt"), indexed)
        self.assertIsNone(self.backend.remove_file("doc.txt"))
        self.assertIs(self.backend.status(roots=["root"]), status)
        self.index.index_file.assert_called_once_with("doc.txt")
        self.index.remove_file.assert_called_once_with("doc.txt")
        self.index.status_summary.assert_called_once_with(roots=["root"])

    def test_search_routes_to_existing_keyword_and_hierarchy_apis(self):
        keyword_rows = [{"path": "doc.txt", "locations": [{"paragraph_index": 2}]}]
        hierarchy_rows = [{"path": "doc.txt", "total_matches": 1}]
        self.index.keyword_search.return_value = keyword_rows
        self.index.search.return_value = hierarchy_rows

        self.assertIs(self.backend.search("query", roots=["root"], limit=7, offset=3), keyword_rows)
        self.assertIs(self.backend.search("query", roots=["root"], hierarchy=True,
                                          limit=5, sort="mentions", start=10, end=20), hierarchy_rows)
        self.index.keyword_search.assert_called_once_with(
            "query", roots=["root"], limit=7, offset=3, start=None, end=None)
        self.index.search.assert_called_once_with(
            "query", roots=["root"], limit=5, sort="mentions", start=10, end=20)

    def test_cancel_sets_the_service_event_and_reports_repeat_request(self):
        cancel_event = threading.Event()
        self.assertTrue(self.backend.cancel(cancel_event))
        self.assertTrue(cancel_event.is_set())
        self.assertFalse(self.backend.cancel(cancel_event))

    def test_contract_is_abstract(self):
        with self.assertRaises(TypeError):
            SearchBackend()


if __name__ == "__main__":
    unittest.main()
