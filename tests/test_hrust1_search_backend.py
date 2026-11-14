"""HRUST1: 검색 위계 판정의 기본 경로는 Rust DLL, 설정으로 Python 규칙에 되돌린다."""
import unittest
from unittest.mock import patch

from docfit_app import hierarchy_bridge as hb
from docfit_app.settings_schema import DEFAULTS
from docfit_core.timemachine.content_search import ParagraphRecord, classify_paragraph_levels


def _records(texts):
    return [ParagraphRecord(i, t, "body", 8, False) for i, t in enumerate(texts)]


class SearchBackendSettingTest(unittest.TestCase):
    def test_default_is_rust_and_setting_reverts_to_python(self):
        self.assertTrue(DEFAULTS["rust_search_hierarchy"])
        self.assertEqual(hb.search_backend({}), "rust")
        self.assertEqual(hb.search_backend({"rust_search_hierarchy": False}), "python")

    def test_python_setting_never_loads_library(self):
        with patch.object(hb, "find_library") as find, patch.object(hb, "run_search_library") as run:
            self.assertIsNone(hb.search_levels([{"text": "a"}], {"rust_search_hierarchy": False}))
        find.assert_not_called()
        run.assert_not_called()

    def test_missing_library_returns_none(self):
        with patch.object(hb, "find_library", return_value=None):
            self.assertIsNone(hb.search_levels([{"text": "a"}], {}))
        self.assertIsNone(hb.search_levels([], {}))


class ClassifyUsesRustTest(unittest.TestCase):
    def test_rust_levels_are_used_when_engine_off(self):
        records = _records(["□ 상위", "본문"])
        rust = {"search_levels": ["circle", "dash"]}  # 일부러 Python과 다른 값: 경로 확인용
        with patch.object(hb, "analyze_levels", return_value=None), \
                patch.object(hb, "search_levels", return_value=rust) as call:
            levels = [p.level for p in classify_paragraph_levels("x.txt", records)]
        self.assertEqual(levels, ["circle", "dash"])
        payload = call.call_args.args[0]
        self.assertEqual([p["text"] for p in payload], ["□ 상위", "본문"])
        self.assertEqual(payload[0]["source_path"], "x.txt")

    def test_backend_python_skips_both_engines(self):
        records = _records(["□ 상위", "ㅇ 하위"])
        with patch.object(hb, "analyze_levels") as fmt, patch.object(hb, "search_levels") as rust:
            levels = [p.level for p in classify_paragraph_levels("x.txt", records, backend="python")]
        fmt.assert_not_called()
        rust.assert_not_called()
        self.assertEqual(levels, ["square", "circle"])

    def test_invalid_rust_reply_falls_back_to_python_rules(self):
        records = _records(["□ 상위", "ㅇ 하위"])
        for bad in ({"search_levels": ["square"]}, {"search_levels": ["nope", "body"]}, None):
            with self.subTest(bad=bad), patch.object(hb, "analyze_levels", return_value=None), \
                    patch.object(hb, "search_levels", return_value=bad):
                levels = [p.level for p in classify_paragraph_levels("x.txt", records)]
            self.assertEqual(levels, ["square", "circle"])

    def test_format_engine_reply_keeps_priority(self):
        records = _records(["□ 상위", "ㅇ 하위"])
        with patch.object(hb, "analyze_levels", return_value={"search_levels": ["chapter", "body"]}), \
                patch.object(hb, "search_levels") as rust:
            levels = [p.level for p in classify_paragraph_levels("x.txt", records)]
        rust.assert_not_called()
        self.assertEqual(levels, ["chapter", "body"])


class RunSearchLibraryTest(unittest.TestCase):
    def test_bad_library_and_wrong_length_return_none(self):
        self.assertIsNone(hb.run_search_library("no-such-hierarchy.dll", [{"text": "a"}]))
        with patch.object(hb, "_load_library"), patch.object(hb, "_call_json",
                                                              return_value={"search_levels": ["body"]}):
            self.assertIsNone(hb.run_search_library("x.dll", [{"text": "a"}, {"text": "b"}]))
            self.assertEqual(hb.run_search_library("x.dll", [{"text": "a"}]), {"search_levels": ["body"]})

    def test_built_library_matches_python_rules(self):
        library = hb.find_library()
        if library is None:
            self.skipTest("hierarchy DLL이 빌드되지 않음")
        texts = ["# 제목", "## 장", "□ 상위", "ㅇ 하위", "- 내용", "* 주석", "본문", "|", "||"]
        records = _records(texts)
        payload = [{"text": t, "source_path": "x.md", "is_table_cell": False} for t in texts]
        expected = [p.level for p in classify_paragraph_levels("x.md", records, backend="python")]
        self.assertEqual(hb.run_search_library(library, payload), {"search_levels": expected})


if __name__ == "__main__":
    unittest.main()
