import json
import tempfile
import unittest
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

from scripts.allqa_compare import qa_checks, render, xml_hashes
from scripts.allqa_run import enabled_stage_check, read_docset


def make_hwpx(path: Path, text: str = "alpha", bold: str = "0") -> None:
    with ZipFile(path, "w", ZIP_DEFLATED) as archive:
        archive.writestr("Contents/header.xml", '<h><charPr id="0" bold="' + bold + '"/></h>')
        archive.writestr("Contents/section0.xml", '<s><p><run charPrIDRef="0"><t>' + text + '</t></run></p></s>')


def make_para_hwpx(path: Path, definitions: str, para_ref: str, table_id: str,
                   table_width: str = "100") -> None:
    with ZipFile(path, "w", ZIP_DEFLATED) as archive:
        archive.writestr(
            "Contents/header.xml",
            "<header><paraProperties>" + definitions + "</paraProperties></header>",
        )
        archive.writestr(
            "Contents/section0.xml",
            '<section><p paraPrIDRef="' + para_ref + '"><run><t>alpha</t>'
            '<tbl id="' + table_id + '" width="' + table_width + '"/></run></p></section>',
        )


def make_border_fill_hwpx(path: Path, definitions: str, para_border_ref: str,
                          cell_border_ref: str) -> None:
    with ZipFile(path, "w", ZIP_DEFLATED) as archive:
        archive.writestr(
            "Contents/header.xml",
            "<header><borderFills>" + definitions + "</borderFills><paraProperties>"
            '<paraPr id="0"><border borderFillIDRef="' + para_border_ref
            + '"/></paraPr></paraProperties></header>',
        )
        archive.writestr(
            "Contents/section0.xml",
            '<section><pageBorderFill borderFillIDRef="' + para_border_ref
            + '"/><tbl id="100"><tc borderFillIDRef="' + cell_border_ref
            + '"><p><run><t>alpha</t></run></p></tc></tbl></section>',
        )


class AllqaToolsTests(unittest.TestCase):
    def test_docset_reads_only_marked_data(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "set.md"
            path.write_text("<!-- ALLQA-DATA -->\n| id | source | hash | kind |\n|---|---|---|---|\n| a | tests/a.txt | deadbeef | TXT |\n<!-- /ALLQA-DATA -->", encoding="utf-8")
            self.assertEqual(read_docset(path), [{"id": "a", "source": "tests/a.txt", "sha256": "deadbeef", "kind": "TXT"}])

    def test_xml_hashes_separate_text_and_format(self):
        with tempfile.TemporaryDirectory() as tmp:
            a, b, c = (Path(tmp) / name for name in ("a.hwpx", "b.hwpx", "c.hwpx"))
            make_hwpx(a, "alpha", "0"); make_hwpx(b, "alpha", "1"); make_hwpx(c, "beta", "0")
            self.assertEqual(xml_hashes(a)["text_xml_sha256"], xml_hashes(b)["text_xml_sha256"])
            self.assertNotEqual(xml_hashes(a)["format_xml_sha256"], xml_hashes(b)["format_xml_sha256"])
            self.assertNotEqual(xml_hashes(a)["text_xml_sha256"], xml_hashes(c)["text_xml_sha256"])

    def test_format_hash_normalizes_paragraph_definition_ids_and_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            a, b = Path(tmp) / "a.hwpx", Path(tmp) / "b.hwpx"
            make_para_hwpx(
                a,
                '<paraPr id="0" margin="0"/><paraPr id="1" margin="10"/>',
                "1", "1001",
            )
            make_para_hwpx(
                b,
                '<paraPr id="77" margin="10"/><paraPr id="0" margin="0"/>',
                "77", "9999",
            )
            hashes_a, hashes_b = xml_hashes(a), xml_hashes(b)
            self.assertNotEqual(hashes_a["raw_format_xml_sha256"], hashes_b["raw_format_xml_sha256"])
            self.assertEqual(hashes_a["format_xml_sha256"], hashes_b["format_xml_sha256"])

    def test_format_hash_normalizes_border_fill_ids_and_definition_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            a, b, changed = (Path(tmp) / name for name in ("a.hwpx", "b.hwpx", "changed.hwpx"))
            none = '<borderFill id="__BORDER_ID__"><leftBorder type="NONE" width="0.1 mm"/></borderFill>'
            solid = '<borderFill id="__BORDER_ID__"><leftBorder type="SOLID" width="0.3 mm"/></borderFill>'
            make_border_fill_hwpx(
                a, solid.replace("__BORDER_ID__", "4") + none.replace("__BORDER_ID__", "7"), "7", "4",
            )
            make_border_fill_hwpx(
                b, none.replace("__BORDER_ID__", "2") + solid.replace("__BORDER_ID__", "8"), "2", "8",
            )
            make_border_fill_hwpx(
                changed, none.replace("__BORDER_ID__", "2") + solid.replace("__BORDER_ID__", "8"), "2", "2",
            )
            hashes_a, hashes_b = xml_hashes(a), xml_hashes(b)
            self.assertNotEqual(hashes_a["raw_format_xml_sha256"], hashes_b["raw_format_xml_sha256"])
            self.assertEqual(hashes_a["format_xml_sha256"], hashes_b["format_xml_sha256"])
            self.assertNotEqual(hashes_a["format_xml_sha256"],
                                xml_hashes(changed)["format_xml_sha256"])

    def test_format_hash_keeps_real_paragraph_and_table_changes(self):
        with tempfile.TemporaryDirectory() as tmp:
            a, para_changed, table_changed = (
                Path(tmp) / name for name in ("a.hwpx", "para_changed.hwpx", "table_changed.hwpx")
            )
            definitions = '<paraPr id="0" margin="0"/><paraPr id="1" margin="10"/>'
            make_para_hwpx(a, definitions, "1", "1001")
            make_para_hwpx(
                para_changed,
                '<paraPr id="0" margin="0"/><paraPr id="1" margin="11"/>',
                "1", "1001",
            )
            make_para_hwpx(table_changed, definitions, "1", "1001", table_width="101")
            expected = xml_hashes(a)["format_xml_sha256"]
            self.assertNotEqual(expected, xml_hashes(para_changed)["format_xml_sha256"])
            self.assertNotEqual(expected, xml_hashes(table_changed)["format_xml_sha256"])

    def test_qa_records_missing_page_count_as_unmeasured(self):
        row = {"source_sha256_before": "x", "source_sha256_after": "x", "copy_sha256": "y", "copy_sha256_after": "y", "returncode": 0,
               "errors": [], "saved": 1, "text_preserved_ignoring_whitespace": [True],
               "paragraph_counts": [1], "page_count": None, "stages_selected": {}, "result_files": []}
        checks = dict(qa_checks(row, Path("made-up.hwpx")))
        self.assertEqual(checks["원본 해시 불변"], "PASS")
        self.assertEqual(checks["쪽 수 기록"], "UNMEASURED")

    def test_enabled_stages_are_checked_against_measured_stages(self):
        result = enabled_stage_check({"body_spacing": True, "style_unify": False},
                                     [{"name": "본문 자간·단어 분리 조정"}])
        self.assertTrue(result["pass"])
        self.assertEqual(result["executed"], ["body_spacing"])

    def test_render_reports_paragraph_differences_and_defects(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); before_dir = root / "before"; after_dir = root / "after"
            a = before_dir / "x" / "result01.hwpx"; b = after_dir / "x" / "result01.hwpx"
            a.parent.mkdir(parents=True); b.parent.mkdir(parents=True)
            make_hwpx(a, "alpha"); make_hwpx(b, "beta")
            row_a = {"id": "x", "com": {"total_seconds": 2, "com_calls": 5, "stages": []},
                     "source_sha256_before": "a", "source_sha256_after": "a", "returncode": 0,
                     "copy_sha256": "a", "copy_sha256_after": "a",
                     "errors": [], "saved": 1, "text_preserved_ignoring_whitespace": [True],
                     "paragraph_counts": [1], "page_count": 1, "stages_selected": {}, "log_file": "",
                     "result_files": ["Z:/moved/before/x/result01.hwpx"]}
            row_b = dict(row_a, com={"total_seconds": 1, "com_calls": 4, "stages": []},
                         result_files=["Z:/moved/after/x/result01.hwpx"])
            report = render({"label": "before", "results": [row_a]}, {"label": "after", "results": [row_b]},
                            before_dir, after_dir)
            self.assertIn("XML 해시", report)
            self.assertIn("결함 목록", report)
            self.assertIn("x: 결과 HWPX 차이", report)
            self.assertIn("결과 저장 | PASS", report)


if __name__ == "__main__":
    unittest.main()
