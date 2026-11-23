import copy
import json
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

from defusedxml import ElementTree as ET

from docfit_core import conversion_fidelity as cf
from docfit_core import conversion_fixtures as fx

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "compare_conversion_fidelity.py"
COMMITTED = ROOT / "tests" / "fixtures" / "conversion_fidelity"


def expected_struct(blocks, form="markdown"):
    return dict(fx.expected_from_blocks(blocks), form=form)


class FixtureGeneratorTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.dir = Path(cls.tmp.name)
        cls.manifest = fx.generate_fixtures(cls.dir)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def test_manifest_covers_all_formats(self):
        formats = {e["format"] for e in self.manifest["fixtures"]}
        self.assertEqual(formats, {"md", "html", "docx", "xlsx", "pptx"})
        for e in self.manifest["fixtures"]:
            self.assertEqual(fx.sha256(self.dir / e["file"]), e["sha256"])

    def test_generation_is_deterministic(self):
        with tempfile.TemporaryDirectory() as other:
            again = fx.generate_fixtures(other)
        self.assertEqual(again, self.manifest)

    def test_committed_manifest_matches_generator(self):
        committed = json.loads((COMMITTED / "manifest.json").read_text(encoding="utf-8"))
        self.assertEqual(committed, self.manifest)

    def test_office_parts_are_wellformed_xml_zips(self):
        for e in self.manifest["fixtures"]:
            if e["format"] not in ("docx", "xlsx", "pptx"):
                continue
            with zipfile.ZipFile(self.dir / e["file"]) as z:
                self.assertIsNone(z.testzip())
                self.assertIn("[Content_Types].xml", z.namelist())
                for name in z.namelist():
                    if name.endswith((".xml", ".rels")):
                        ET.fromstring(z.read(name))  # 파싱 실패면 예외

    def test_docx_contains_headings_lists_table_image(self):
        with zipfile.ZipFile(self.dir / "doc_basic.docx") as z:
            doc = z.read("word/document.xml").decode("utf-8")
            self.assertIn("media/image1.png", " ".join(z.namelist()))
        self.assertEqual(doc.count('w:val="Heading'), 4)  # h1, h2, h2, h3
        self.assertEqual(doc.count("<w:numPr>"), 8)
        self.assertEqual(doc.count("<w:tr>"), 3)
        self.assertEqual(doc.count("<w:drawing>"), 1)

    def test_xlsx_and_pptx_content(self):
        with zipfile.ZipFile(self.dir / "sheet_basic.xlsx") as z:
            sheets = [n for n in z.namelist() if n.startswith("xl/worksheets/")]
            self.assertEqual(len(sheets), 2)
            self.assertIn("인건비", z.read("xl/worksheets/sheet1.xml").decode("utf-8"))
        with zipfile.ZipFile(self.dir / "slides_basic.pptx") as z:
            slides = [n for n in z.namelist() if n.startswith("ppt/slides/slide")]
            self.assertEqual(len(slides), 2)
            self.assertIn("<a:tbl>", z.read("ppt/slides/slide2.xml").decode("utf-8"))
            self.assertIn("ppt/media/image1.png", z.namelist())

    def test_expected_structure_values(self):
        entry = next(e for e in self.manifest["fixtures"] if e["id"] == "doc_basic__md")
        exp = entry["expected"]
        self.assertEqual([h[0] for h in exp["headings"]], [1, 2, 2, 3])
        self.assertEqual(len(exp["lists"]), 8)
        self.assertEqual(exp["tables"][0][0], ["구분", "건수", "비고"])
        self.assertEqual(exp["images"], 1)

    def test_png_is_valid(self):
        data = fx.png_bytes()
        self.assertTrue(data.startswith(b"\x89PNG\r\n\x1a\n"))
        self.assertTrue(data.endswith(b"IEND\xaeB`\x82"))

    def test_html_nested_lists(self):
        text = (self.dir / "doc_basic.html").read_text(encoding="utf-8")
        self.assertIn("<ul><li>주민 편의 개선<ul><li>창구 대기시간 단축</li>", text)


class ExtractorTest(unittest.TestCase):
    def test_markdown_roundtrip_matches_expected_for_all_md_fixtures(self):
        for name, blocks in fx.DOC_SPECS.items():
            got = cf.extract_markdown(fx.render_markdown(blocks))
            exp = expected_struct(blocks)
            self.assertEqual(got["headings"], exp["headings"], name)
            self.assertEqual([list(x) for x in got["lists"]], [list(x) for x in exp["lists"]], name)
            self.assertEqual(got["tables"], exp["tables"], name)
            self.assertEqual(got["images"], exp["images"], name)
            self.assertEqual(cf.normalize(got["text"]), cf.normalize(exp["text"]), name)

    def test_markdown_inline_markup_and_pipes(self):
        s = cf.extract_markdown("## **굵은** 제목\n\n| a | b \\| c |\n|---|---|\n| [링크](http://x) | `코드` |\n")
        self.assertEqual(s["headings"], [[2, "굵은 제목"]])
        self.assertEqual(s["tables"], [[["a", "b | c"], ["링크", "코드"]]])

    def test_hwpx_extraction_with_paraPr_lists(self):
        for name, blocks in fx.DOC_SPECS.items():
            with tempfile.TemporaryDirectory() as d:
                path = Path(d) / "x.hwpx"
                fx.build_hwpx_from_blocks(blocks, path)
                got = cf.extract_hwpx(path)
            exp = expected_struct(blocks)
            self.assertEqual(got["headings"], exp["headings"], name)
            self.assertEqual([list(x) for x in got["lists"]], [list(x) for x in exp["lists"]], name)
            self.assertEqual(got["tables"], exp["tables"], name)
            self.assertEqual(got["images"], exp["images"], name)
            self.assertEqual(cf.normalize(got["text"]), cf.normalize(exp["text"]), name)

    def test_hwpx_marker_lists_detected_from_text(self):
        blocks = fx.DOC_SPECS["doc_basic"]
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "x.hwpx"
            fx.build_hwpx_from_blocks(blocks, path, list_style="marker")
            got = cf.extract_hwpx(path)
        self.assertEqual([x[2] for x in got["lists"]], [x[2] for x in expected_struct(blocks)["lists"]])
        self.assertEqual([x[0] for x in got["lists"]], [x[0] for x in expected_struct(blocks)["lists"]])

    def test_extract_file_dispatch_and_unknown(self):
        with tempfile.TemporaryDirectory() as d:
            t = Path(d) / "a.txt"
            t.write_text("가 나", encoding="utf-8")
            self.assertEqual(cf.extract_file(t)["form"], "text")
            with self.assertRaises(ValueError):
                cf.extract_file(Path(d) / "a.pdf")


class ComparatorTest(unittest.TestCase):
    def setUp(self):
        self.blocks = fx.DOC_SPECS["doc_basic"]
        self.ref = cf.extract_markdown(fx.render_markdown(self.blocks))

    def cand(self):
        return copy.deepcopy(self.ref)

    def test_identical_output_passes_everything(self):
        r = cf.compare(self.ref, self.cand())
        self.assertTrue(r["verdict"]["pass"])
        for key in cf.STRUCTURE_METRICS:
            self.assertEqual(r["metrics"][key], 1.0, key)
        self.assertEqual((r["metrics"]["recall"], r["metrics"]["precision"]), (1.0, 1.0))

    def test_dropped_heading_fails(self):
        c = self.cand()
        removed = c["headings"].pop(1)
        c["text"] = c["text"].replace(removed[1], "")
        r = cf.compare(self.ref, c)
        self.assertFalse(r["verdict"]["pass"])
        self.assertFalse(r["verdict"]["heading"])
        self.assertAlmostEqual(r["metrics"]["heading"], 3 / 4)

    def test_wrong_heading_level_fails(self):
        c = self.cand()
        c["headings"][1][0] = 3
        self.assertFalse(cf.compare(self.ref, c)["verdict"]["heading"])

    def test_broken_table_missing_row_fails(self):
        c = self.cand()
        c["tables"][0].pop()
        r = cf.compare(self.ref, c)
        self.assertFalse(r["verdict"]["table_shape"])
        self.assertFalse(r["verdict"]["table_cells"])
        self.assertFalse(r["verdict"]["pass"])

    def test_broken_table_changed_cell_fails_only_cells(self):
        c = self.cand()
        c["tables"][0][1][2] = "이상"
        r = cf.compare(self.ref, c)
        self.assertTrue(r["verdict"]["table_shape"])
        self.assertFalse(r["verdict"]["table_cells"])

    def test_table_flattened_to_text_fails(self):
        c = self.cand()
        c["tables"] = []
        r = cf.compare(self.ref, c)
        self.assertFalse(r["verdict"]["table_count"])
        self.assertFalse(r["verdict"]["pass"])

    def test_list_depth_flattened_fails_shape_not_text(self):
        c = self.cand()
        for item in c["lists"]:
            item[0] = 0
        r = cf.compare(self.ref, c)
        self.assertTrue(r["verdict"]["list_text"])
        self.assertFalse(r["verdict"]["list_shape"])

    def test_list_type_changed_fails_shape(self):
        c = self.cand()
        c["lists"][0][1] = True
        self.assertFalse(cf.compare(self.ref, c)["verdict"]["list_shape"])

    def test_missing_image_fails(self):
        c = self.cand()
        c["images"] = 0
        r = cf.compare(self.ref, c)
        self.assertFalse(r["verdict"]["images"])
        c["images"] = 2
        self.assertFalse(cf.compare(self.ref, c)["verdict"]["images"])

    def test_whitespace_differences_are_ignored(self):
        c = self.cand()
        c["text"] = c["text"].replace(" ", "  ").replace("\n", "\n\n")
        r = cf.compare(self.ref, c)
        self.assertEqual(r["metrics"]["recall"], 1.0)
        self.assertTrue(r["verdict"]["pass"])

    def test_text_recall_threshold(self):
        ref = {"form": "text", "headings": None, "lists": None, "tables": None, "images": None, "text": "가" * 1000}
        ok = dict(ref, text="가" * 996)      # 99.6% ≥ 99.5%
        bad = dict(ref, text="가" * 990)     # 99.0% < 99.5%
        self.assertTrue(cf.compare(ref, ok)["verdict"]["recall"])
        r = cf.compare(ref, bad)
        self.assertFalse(r["verdict"]["recall"])
        self.assertTrue(r["verdict"]["precision"])  # 가진 글자는 모두 맞음

    def test_text_precision_threshold(self):
        ref = {"form": "text", "headings": None, "lists": None, "tables": None, "images": None, "text": "가" * 1000}
        noisy = dict(ref, text="가" * 1000 + "x" * 20)  # 정밀도 98.04% < 99%
        r = cf.compare(ref, noisy)
        self.assertTrue(r["verdict"]["recall"])
        self.assertFalse(r["verdict"]["precision"])

    def test_text_scores_are_order_sensitive_for_short_text(self):
        s = cf.text_scores("가나다라마바사아자차", "차자아사바마라다나가")
        self.assertLess(s["recall"], 0.5)

    def test_long_text_uses_multiset_fallback(self):
        s = cf.text_scores("가" * 40000, "가" * 40000)
        self.assertEqual(s["method"], "char-multiset")
        self.assertEqual(s["recall"], 1.0)

    def test_text_form_candidate_skips_structure(self):
        r = cf.compare(self.ref, cf.extract_text(self.ref["text"]))
        self.assertTrue(all(r["metrics"][k] is None for k in cf.STRUCTURE_METRICS))
        self.assertTrue(r["verdict"]["pass"])  # 구조는 판정 안 함, 텍스트는 동일

    def test_no_structure_in_reference_is_not_applicable(self):
        ref = cf.extract_markdown(fx.render_markdown(fx.DOC_SPECS["doc_text_only"]))
        r = cf.compare(ref, copy.deepcopy(ref))
        self.assertIsNone(r["metrics"]["heading"])
        self.assertIsNone(r["metrics"]["table_shape"])
        self.assertTrue(r["verdict"]["pass"])

    def test_extra_table_in_candidate_fails_when_reference_has_none(self):
        ref = cf.extract_markdown(fx.render_markdown(fx.DOC_SPECS["doc_text_only"]))
        c = copy.deepcopy(ref)
        c["tables"] = [[["x"]]]
        r = cf.compare(ref, c)
        self.assertFalse(r["verdict"]["table_count"])

    def test_custom_thresholds(self):
        ref = {"form": "text", "headings": None, "lists": None, "tables": None, "images": None, "text": "가" * 1000}
        bad = dict(ref, text="가" * 990)
        self.assertTrue(cf.compare(ref, bad, {"recall": 0.98})["verdict"]["recall"])

    def test_hwpx_candidate_vs_markdown_expected_cross_check(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "c.hwpx"
            fx.build_hwpx_from_blocks(self.blocks, path)
            cand = cf.extract_hwpx(path)
        r = cf.compare(dict(self.ref, form="hwpx"), cand)
        self.assertTrue(r["verdict"]["pass"], r["metrics"])

    def test_hwpx_candidate_with_broken_table_detected(self):
        broken = [b if b[0] != "table" else ("table", b[1][:-1]) for b in self.blocks]
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "c.hwpx"
            fx.build_hwpx_from_blocks(broken, path)
            cand = cf.extract_hwpx(path)
        r = cf.compare(dict(self.ref, form="hwpx"), cand)
        self.assertFalse(r["verdict"]["table_shape"])
        self.assertFalse(r["verdict"]["pass"])

    def test_hwpx_candidate_without_headings_detected(self):
        flat = [("p", b[2]) if b[0] == "h" else b for b in self.blocks]
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "c.hwpx"
            fx.build_hwpx_from_blocks(flat, path)
            cand = cf.extract_hwpx(path)
        r = cf.compare(dict(self.ref, form="hwpx"), cand)
        self.assertEqual(r["metrics"]["heading"], 0.0)
        self.assertFalse(r["verdict"]["pass"])
        self.assertEqual(r["metrics"]["recall"], 1.0)  # 글은 그대로 — 구조만 잃음


class SetsAndReportTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        self.manifest = fx.generate_fixtures(self.dir / "fx")
        self.md_ids = [e["id"] for e in self.manifest["fixtures"] if e["format"] == "md"]

    def tearDown(self):
        self.tmp.cleanup()

    def _write_outputs(self, folder, mutate=None):
        folder.mkdir()
        for name, blocks in fx.DOC_SPECS.items():
            blocks = mutate(name, blocks) if mutate else blocks
            (folder / f"{name}__md.md").write_text(fx.render_markdown(blocks), encoding="utf-8")
            fx.build_hwpx_from_blocks(blocks, folder / f"{name}__md.hwpx")
        return folder

    def test_golden_json_roundtrip_and_compare_without_kordoc(self):
        ref_dir = self._write_outputs(self.dir / "ref")
        golden = self.dir / "golden.json"
        cf.save_golden(golden, "kordoc", "test", cf.collect_outputs(ref_dir))
        ref, label = cf.load_side(golden)
        self.assertIn("kordoc", label)
        cand = cf.collect_outputs(self._write_outputs(self.dir / "cand"))
        rows = cf.compare_sets(ref, cand, groups=cf.groups_from_manifest(self.manifest))
        self.assertEqual(len(rows), 6)
        self.assertTrue(all(r["verdict"]["pass"] for r in rows))

    def test_broken_candidate_set_is_caught_and_reported(self):
        ref = cf.collect_outputs(self._write_outputs(self.dir / "ref"))

        def damage(name, blocks):
            if name != "doc_tables":
                return blocks
            return [b if b[0] != "h" else ("p", b[2]) for b in blocks]  # 제목 제거

        cand = cf.collect_outputs(self._write_outputs(self.dir / "cand", damage))
        rows = cf.compare_sets(ref, cand, groups=cf.groups_from_manifest(self.manifest))
        failed = {(r["fixture"], r["form"]) for r in rows if not r["verdict"]["pass"]}
        self.assertEqual(failed, {("doc_tables__md", "markdown"), ("doc_tables__md", "hwpx")})
        report = cf.render_report(rows, ref_label="ref", cand_label="cand")
        self.assertIn("| doc_tables__md |", report)
        self.assertIn("**불합격**", report)
        self.assertIn("전체: 4/6 합격", report)
        self.assertIn("0.00% ❌", report)
        self.assertIn("100.00% ✅", report)

    def test_missing_candidate_counts_as_failure(self):
        ref = cf.collect_outputs(self._write_outputs(self.dir / "ref"))
        cand = {k: v for k, v in ref.items() if k != "doc_basic__md"}
        rows = cf.compare_sets(ref, cand)
        missing = [r for r in rows if r.get("missing")]
        self.assertEqual(len(missing), 2)
        self.assertFalse(any(r["verdict"]["pass"] for r in missing))
        self.assertIn("누락", cf.render_report(rows, ref_label="a", cand_label="b"))

    def test_expected_reference_mode(self):
        cand = cf.collect_outputs(self._write_outputs(self.dir / "cand"))
        ref, label = cf.load_side("expected", self.manifest, cand)
        self.assertIn("정답", label)
        rows = cf.compare_sets(ref, cand)
        self.assertEqual(len(rows), 6)
        self.assertTrue(all(r["verdict"]["pass"] for r in rows), [r for r in rows if not r["verdict"]["pass"]])

    def test_golden_without_results_rejected(self):
        bad = self.dir / "bad.json"
        bad.write_text("{}", encoding="utf-8")
        with self.assertRaises(ValueError):
            cf.load_golden(bad)

    def test_cli_compare_exit_codes(self):
        ref_dir = self._write_outputs(self.dir / "ref")
        bad_dir = self._write_outputs(self.dir / "bad", lambda n, b: [x for x in b if x[0] != "table"])
        manifest = self.dir / "fx" / "manifest.json"
        report = self.dir / "r.md"

        def run(cand):
            return subprocess.run([sys.executable, "-I", str(SCRIPT), "compare", "--reference", str(ref_dir),
                                   "--candidate", str(cand), "--manifest", str(manifest), "--report", str(report)],
                                  capture_output=True, text=True, encoding="utf-8")

        self.assertEqual(run(ref_dir).returncode, 0)
        self.assertIn("전체: 6/6 합격", report.read_text(encoding="utf-8"))
        self.assertEqual(run(bad_dir).returncode, 1)
        self.assertIn("**불합격**", report.read_text(encoding="utf-8"))

    def test_cli_generate_and_extract(self):
        out = self.dir / "gen"
        r = subprocess.run([sys.executable, "-I", str(SCRIPT), "generate", "--out", str(out)], capture_output=True)
        self.assertEqual(r.returncode, 0)
        self.assertTrue((out / "manifest.json").is_file())
        ref_dir = self._write_outputs(self.dir / "ref")
        golden = self.dir / "g.json"
        r = subprocess.run([sys.executable, "-I", str(SCRIPT), "extract", str(ref_dir), "--golden", str(golden)], capture_output=True)
        self.assertEqual(r.returncode, 0)
        self.assertIn("doc_basic__md", json.loads(golden.read_text(encoding="utf-8"))["results"])


if __name__ == "__main__":
    unittest.main()
