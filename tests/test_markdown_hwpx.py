"""KORDOCC1: 합성 정답·서식·원본 보호·실제 kordoc 추출 회귀 시험."""
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from zipfile import ZipFile, ZIP_STORED

from defusedxml import ElementTree as ET

from docfit_core import conversion_fidelity as cf
from docfit_core.markdown_hwpx import (
    HH, HP, GenerationResult, generate_hwpx, inline_runs, parse_markdown, render_hwpx,
)
from docfit_core.markdown_hwpx_fixtures import fixtures


class MarkdownHwpxTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / "report.hwpx"

    def create(self, source):
        result = generate_hwpx(source, self.path)
        self.assertIsInstance(result, GenerationResult)
        return cf.extract_hwpx(self.path)

    def test_eleven_independent_synthetic_expectations(self):
        self.assertEqual(len(fixtures()), 11)
        for name, (source, expected) in fixtures().items():
            with self.subTest(name=name):
                target = Path(self.temp.name) / f"{name}.hwpx"
                result = generate_hwpx(source, target)
                comparison = cf.compare(dict(expected, form="hwpx"), cf.extract_hwpx(target))
                if name == "doc_basic":
                    self.assertFalse(comparison["verdict"]["pass"])
                    self.assertEqual(comparison["metrics"]["images"], 0)
                    self.assertTrue(any("그림" in w for w in result.warnings))
                else:
                    self.assertTrue(comparison["verdict"]["pass"], comparison)

    def test_title_levels_survive_without_decorative_tables(self):
        got = self.create("# 제목\n\n## 배경\n\n### 계획\n\n본문")
        self.assertEqual(got["headings"], [[1, "제목"], [2, "배경"], [3, "계획"]])
        self.assertEqual(got["tables"], [])

    def test_public_markers_one_space_no_tab(self):
        self.create("□\t  네모\n  ㅇ    원\n    -\t하위\n※   참고")
        with ZipFile(self.path) as archive:
            section = archive.read("Contents/section0.xml").decode("utf-8")
        self.assertNotIn("<hp:tab", section)
        self.assertNotIn("\t", section)
        root = ET.fromstring(section)
        texts = [cf._paragraph_text(p) for p in root.findall(f"{{{HP}}}p")]
        self.assertEqual(texts[1:], ["□ 네모", "  ㅇ 원", "    - 하위", "※ 참고"])

    def test_numbered_list_start_and_nesting(self):
        got = self.create("3. 셋\n   1. 안쪽\n4. 넷")
        self.assertEqual(got["lists"], [[0, True, "셋"], [1, True, "안쪽"], [0, True, "넷"]])

    def test_inline_emphasis_nested_and_escaped(self):
        for source, expected in (
            ("**굵게**", [("굵게", True, False)]),
            ("*기울임*", [("기울임", False, True)]),
            ("***동시***", [("동시", True, True)]),
            ("**바깥 *안쪽***", [("바깥 ", True, False), ("안쪽", True, True)]),
            ("*바깥 **안쪽***", [("바깥 ", False, True), ("안쪽", True, True)]),
            (r"\*그대로\* `**코드**`", [("*그대로* ", False, False), ("**코드**", False, False)]),
            ("file_name_value", [("file_name_value", False, False)]),
        ):
            with self.subTest(source=source):
                runs = inline_runs(source, [])
                self.assertEqual([(r.text, r.bold, r.italic) for r in runs], expected)

    def test_soft_and_hard_linebreaks_and_tail(self):
        got = self.create("가  \n나\\\n다<br/>라\n\n마")
        self.assertEqual(cf.normalize(got["text"]), "가나다라마")
        with ZipFile(self.path) as archive:
            section = archive.read("Contents/section0.xml").decode()
        self.assertEqual(section.count("<hp:lineBreak/>"), 3)

    def test_table_escaped_pipe_short_row_and_emphasis(self):
        got = self.create("| 항목 | 설명 |\n| :--- | ---: |\n| **가** | A \\| B |\n| 나 |\n")
        self.assertEqual(got["tables"], [[["항목", "설명"], ["가", "A | B"], ["나", ""]]])

    def test_wide_ragged_table_rejected_without_output(self):
        with self.assertRaises(ValueError):
            self.create("| 가 | 나 |\n| --- | --- |\n| A | B | C |")
        self.assertFalse(self.path.exists())

    def test_existing_document_never_overwritten(self):
        self.path.write_bytes(b"existing synthetic document")
        with self.assertRaises(FileExistsError):
            generate_hwpx("본문", self.path)
        self.assertEqual(self.path.read_bytes(), b"existing synthetic document")

    def test_validation_failure_never_publishes(self):
        with patch("docfit_core.markdown_hwpx.validate_hwpx", side_effect=ValueError("invalid")):
            with self.assertRaises(ValueError):
                generate_hwpx("본문", self.path)
        self.assertFalse(self.path.exists())

    def test_invalid_control_character_rejected(self):
        with self.assertRaises(ValueError):
            self.create("본문\x00제어")
        self.assertFalse(self.path.exists())

    def test_empty_document_remains_openable_package(self):
        got = self.create("")
        self.assertEqual(got["text"], "")

    def test_unsupported_features_explicitly_warn(self):
        doc = parse_markdown("#### 원문 제목\n\n![그림](image.png)\n\n[링크](https://example.invalid/)\n\n```\n**코드 원문**\n```")
        # KORDOCC2: 4~6단 제목·코드 블록은 지원되므로 남은 경고는 그림(이미지 디렉터리 없음)·링크 대상 2건
        self.assertEqual(len(doc.warnings), 2)
        self.assertTrue(any("그림" in w for w in doc.warnings))
        self.assertTrue(any("하이퍼링크" in w for w in doc.warnings))
        self.assertEqual(doc.blocks[-1].runs[0].text, "**코드 원문**")

    def test_deterministic_zip_and_hancom_package_references(self):
        first, _ = render_hwpx("# 제목\n\n본문")
        self.assertEqual(first, render_hwpx("# 제목\n\n본문")[0])
        with ZipFile(io.BytesIO(first)) as archive:
            self.assertEqual(archive.infolist()[0].filename, "mimetype")
            self.assertEqual(archive.infolist()[0].compress_type, ZIP_STORED)
            package = ET.fromstring(archive.read("Contents/content.hpf"))
            opf = "{http://www.idpf.org/2007/opf/}"
            for item in package.findall(f"{opf}manifest/{opf}item"):
                self.assertIn(item.get("href"), archive.namelist())
            self.assertEqual([n.get("idref") for n in package.findall(f"{opf}spine/{opf}itemref")], ["header", "section0"])

    def test_public_fonts_sizes_and_real_emphasis_properties(self):
        data, _ = render_hwpx("# 제목\n\n**굵게** *기울임*")
        with ZipFile(io.BytesIO(data)) as archive:
            header = ET.fromstring(archive.read("Contents/header.xml"))
        hh = f"{{{HH}}}"
        fonts = header.findall(f".//{hh}fontface")
        self.assertEqual(len(fonts), 7)
        self.assertEqual([f.get("face") for f in fonts[0]], ["휴먼명조", "한컴돋움"])
        chars = {p.get("id"): p for p in header.findall(f".//{hh}charPr")}
        self.assertEqual(chars["0"].get("height"), "1100")
        self.assertEqual(chars["4"].get("height"), "1600")
        self.assertIsNotNone(chars["1"].find(f"{hh}bold"))
        self.assertIsNotNone(chars["2"].find(f"{hh}italic"))


class FidelityRealLayoutRegressionTest(unittest.TestCase):
    def test_tail_after_linebreak_and_fixed_space_is_not_lost(self):
        p = ET.fromstring(f'<hp:p xmlns:hp="{HP}"><hp:run><hp:t>가<hp:lineBreak/>나<hp:fwSpace/>다</hp:t></hp:run></hp:p>')
        self.assertEqual(cf._paragraph_text(p), "가 나 다")

    def test_only_explicit_kordoc_title_cell_is_a_heading(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "kordoc-layout.hwpx"
            # 실제 보고서 프리셋의 표 칸 이름을 재현한다. 일반 1칸 표도 함께 둔다.
            tables = ''.join(f'<hp:tbl><hp:tr><hp:tc name="{name}"><hp:subList><hp:p><hp:run><hp:t>{text}</hp:t></hp:run></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl>'
                             for name, text in (("__kordoc_h2", "추진 배경"), ("", "데이터")))
            with ZipFile(path, "w") as archive:
                archive.writestr("Contents/section0.xml", f'<hp:sec xmlns:hp="{HP}"><hp:p><hp:run>{tables}</hp:run></hp:p></hp:sec>')
            got = cf.extract_hwpx(path)
            self.assertEqual(got["headings"], [[2, "추진 배경"]])
            self.assertEqual(got["tables"], [[["데이터"]]])
            self.assertEqual(got["text"], "추진 배경\n데이터")

    def test_kordoc_three_cell_heading_excludes_generated_ordinal(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "heading.hwpx"
            cells = ''.join(f'<hp:tc name="{name}"><hp:subList><hp:p><hp:run><hp:t>{text}</hp:t></hp:run></hp:p></hp:subList></hp:tc>'
                            for name, text in (("", "Ⅰ"), ("", ""), ("__kordoc_h2", "추진 배경")))
            with ZipFile(path, "w") as archive:
                archive.writestr("Contents/section0.xml", f'<hp:sec xmlns:hp="{HP}"><hp:p><hp:run><hp:tbl><hp:tr>{cells}</hp:tr></hp:tbl></hp:run></hp:p></hp:sec>')
            got = cf.extract_hwpx(path)
            self.assertEqual(got["headings"], [[2, "추진 배경"]])
            self.assertEqual(got["tables"], [])
            self.assertEqual(got["text"], "추진 배경")

    def test_list_margin_depth_does_not_change_ordered_type(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "margins.hwpx"
            with ZipFile(path, "w") as archive:
                archive.writestr("Contents/header.xml", f'<hh:head xmlns:hh="{HH}" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"><hh:paraPr id="0"><hh:margin><hc:left value="0"/></hh:margin></hh:paraPr><hh:paraPr id="1"><hh:margin><hc:left value="750"/></hh:margin></hh:paraPr></hh:head>')
                archive.writestr("Contents/section0.xml", f'<hp:sec xmlns:hp="{HP}"><hp:p paraPrIDRef="0"><hp:run><hp:t>□ 상위</hp:t></hp:run></hp:p><hp:p paraPrIDRef="1"><hp:run><hp:t>ㅇ 하위</hp:t></hp:run></hp:p></hp:sec>')
            self.assertEqual(cf.extract_hwpx(path)["lists"], [[0, False, "상위"], [1, False, "하위"]])


if __name__ == "__main__":
    unittest.main()
