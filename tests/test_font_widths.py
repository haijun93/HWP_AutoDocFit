"""글꼴 폭 표와 폭 계산(INDENTW1). 설치 글꼴 없이 저장소의 표 파일과 합성 자료만 쓴다."""
import json
import struct
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

from defusedxml import ElementTree as ET

from docfit_core import font_widths as fw
from docfit_core.xml_hanging_indent import ITEM_PATTERNS, OFFICIAL_SYMBOLS
from scripts import make_font_width_table as maker
from scripts import measure_indent_hangul as tool

DEFAULT_FACES = ("휴먼명조", "함초롬바탕", "함초롬돋움", "HY헤드라인M", "맑은 고딕", "한컴돋움", "HY견고딕")
TOLERANCE = 50  # HWPUNIT = 0.5pt


def em(text, face, **options):
    """글자 크기 10pt 로 재서 글자 높이 배수로 돌려준다."""
    result = fw.measure_text(text, fw.CharStyle.uniform(face, 10, **options))
    return None if result is None else round(result.width / 1000, 2)


class TableDataTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.data = json.loads(fw.DATA_PATH.read_text(encoding="utf-8"))

    def test_default_fonts_and_measured_hft_have_tables(self):
        for face in DEFAULT_FACES:
            self.assertTrue(fw.has_table(face), face)
        hft = {x["face"]: x for x in self.data["fonts"] if x["type"] == "HFT"}
        self.assertEqual(set(maker.HFT_FONTS), set(hft))
        for face, entry in hft.items():
            self.assertEqual("measured", entry["status"], face)
            self.assertTrue(fw.has_table(face, "HFT"))
            self.assertEqual((), fw.measure_text("가□ㅇ-1.", fw.CharStyle.uniform(face, font_type="HFT")).substituted)

    def test_basic_characters_are_in_every_table(self):
        basic = [chr(c) for c in range(0x20, 0x7F)] + list("가나다라마바사아자차카타파하□ㅁㅇ○·※①「」『』（）：")
        self.assertTrue(ITEM_PATTERNS)
        for face in DEFAULT_FACES:
            result = fw.measure_text("".join(basic), fw.CharStyle.uniform(face))
            self.assertEqual((), result.substituted, face)

    def test_missing_official_symbols_are_reported_not_hidden(self):
        symbols = "".join(sorted(OFFICIAL_SYMBOLS))
        for face in DEFAULT_FACES:
            result = fw.measure_text(symbols, fw.CharStyle.uniform(face))
            for ch in result.substituted:
                self.assertTrue(all(table.em(ch) is None for table in fw._tables()[(face, "TTF")] if table), ch)

    def test_english_alias(self):
        self.assertEqual(em("가 a", "함초롬바탕"), em("가 a", "HCR Batang"))


class MeasureTest(unittest.TestCase):
    def test_font_space_matches_font_file_measurement(self):
        # PM 실측(글꼴 파일 기준) = 글꼴에 어울리는 빈칸
        for text, human, hcr in (("□ ", 1.50, 1.27), ("  - ", 2.00, 1.45), ("   · ", 3.00, 1.52)):
            self.assertEqual(human, em(text, "휴먼명조", use_font_space=True))
            self.assertEqual(hcr, em(text, "함초롬바탕", use_font_space=True))

    def test_fixed_space_is_half_em_regardless_of_font(self):
        for face in DEFAULT_FACES:
            self.assertEqual(1.5, em("   ", face), face)
        self.assertEqual(1.47, em("□ ", "함초롬바탕"))

    def test_values_saved_by_hangul_within_half_point(self):
        # resources/서식예시 의 한/글 저장 내어쓰기 값
        for text, face, size, saved in (("  - ", "휴먼명조", 14, 2800), ("◈ ", "HY헤드라인M", 13, 1948)):
            result = fw.measure_text(text, fw.CharStyle.uniform(face, size))
            self.assertTrue(result.exact)
            self.assertLessEqual(abs(result.hwpunit - saved), TOLERANCE, text)

    def test_ratio_spacing_and_relative_size(self):
        base = lambda **o: fw.measure_text("가", fw.CharStyle.uniform("휴먼명조", 10, **o)).hwpunit
        self.assertEqual(1000, base())
        self.assertEqual(900, base(ratio=90))
        self.assertEqual(948, base(spacing=-5))     # 한/글 4 HWPUNIT 격자
        self.assertEqual(800, base(rel_size=80))
        self.assertEqual(684, base(rel_size=80, ratio=90, spacing=-5))

    def test_language_slots(self):
        for ch, language in (("가", "hangul"), ("ㅇ", "hangul"), ("a", "latin"), ("1", "latin"), (" ", "latin"),
                             ("□", "symbol"), ("※", "symbol"), ("「", "symbol"), ("·", "latin"), ("±", "latin"),
                             ("“", "latin"), ("•", "symbol"),
                             ("漢", "hanja"), ("か", "japanese"), ("α", "symbol"), ("", "user")):
            self.assertEqual(language, fw.language_of(ch), ch)

    def test_each_slot_uses_its_own_font_and_ratio(self):
        fonts = {"hangul": fw.LangFont("휴먼명조"), "latin": fw.LangFont("맑은 고딕", ratio=50),
                 "symbol": fw.LangFont("함초롬바탕")}
        result = fw.measure_text("가1□", fw.CharStyle(1000, fonts))
        digit = fw.measure_text("1", fw.CharStyle.uniform("맑은 고딕", 10, ratio=50)).width
        square = fw.measure_text("□", fw.CharStyle.uniform("함초롬바탕", 10)).width
        self.assertEqual(1000 + digit + square, result.width)

    def test_unknown_font_is_none_only_when_used(self):
        self.assertIsNone(em("가", "없는 글꼴"))
        self.assertIsNone(em("가", "신명 중명조"))            # HFT 이름을 TTF 로 쓰면 모름
        self.assertIsNone(em("가", "없는 글꼴", font_type="HFT"))
        fonts = {"hangul": fw.LangFont("휴먼명조"), "latin": fw.LangFont("없는 글꼴")}
        self.assertIsNotNone(fw.measure_text("가", fw.CharStyle(1000, fonts)))
        self.assertIsNone(fw.measure_text("가a", fw.CharStyle(1000, fonts)))
        self.assertIsNone(fw.measure_text("가□", fw.CharStyle(1000, fonts)))  # 칸 자체가 없음
        self.assertIsNone(em("가\t", "휴먼명조"))

    def test_missing_glyph_uses_measured_fallback_else_marked_estimate(self):
        bullet = fw.measure_text("•", fw.CharStyle.uniform("휴먼명조", 20))
        self.assertTrue(bullet.exact)               # 글꼴에 없지만 한/글 대체 폭을 실측해 둠
        self.assertLessEqual(abs(bullet.width - 924), 4)  # 20pt 실측 924
        unknown = fw.measure_text("԰가԰", fw.CharStyle.uniform("휴먼명조", 10))
        self.assertEqual(("԰",), unknown.substituted)
        self.assertFalse(unknown.exact)
        self.assertEqual(2000, unknown.hwpunit)

    def test_offset_and_runs(self):
        small = fw.CharStyle.uniform("휴먼명조", 10)
        large = fw.CharStyle.uniform("휴먼명조", 20)
        self.assertEqual(500 + 1000 + 2000, fw.measure_runs([(" 가", small), ("나다", large)], 3).hwpunit)
        self.assertEqual(0, fw.measure_runs([("가", small)], 0).hwpunit)
        # offset 뒤의 모르는 글꼴은 결과에 영향이 없다.
        self.assertEqual(1000, fw.measure_runs([("가", small), ("나", fw.CharStyle.uniform("x"))], 1).hwpunit)

    def test_bold_does_not_change_width(self):
        self.assertEqual(em("*a1", "맑은 고딕"), em("*a1", "맑은 고딕", bold=True))

    def test_values_measured_with_indent_at_caret(self):
        # (접두, 글꼴, pt, 선택, 한/글 실측 HWPUNIT) — INDENTW2 실측값
        cases = (
            ("  - ", "휴먼명조", 14, {}, 2800), ("□ ", "함초롬바탕", 15, {}, 2204),
            ("□ ", "함초롬바탕", 15, {"use_font_space": True}, 1908), ("☞ ", "함초롬바탕", 15, {}, 2268),
            ("a", "휴먼명조", 20, {"ratio": 90, "spacing": -5}, 868),
        )
        for text, face, size, options, measured in cases:
            result = fw.measure_text(text, fw.CharStyle.uniform(face, size, **options))
            self.assertLessEqual(abs(result.width - measured), 4, (text, face, size, options))

    def test_special_spaces_and_slots(self):
        def width(text, **slots):
            fonts = {lang: fw.LangFont("함초롬바탕") for lang in fw.LANGUAGES}
            fonts.update(slots)
            return fw.measure_text(text, fw.CharStyle(2000, fonts)).hwpunit

        narrow = fw.LangFont("함초롬바탕", ratio=60)
        tight = fw.LangFont("함초롬바탕", spacing=-20)
        for text, base in ((" ", 1000), (fw.FW_SPACE, 500), (fw.NB_SPACE, 1000)):
            self.assertEqual(base, width(text))
            self.assertEqual(base * 6 // 10, width(text, hangul=narrow))   # 장평은 한글 칸
            self.assertEqual(base, width(text, latin=narrow))
        self.assertEqual(800, width(" ", latin=tight))                      # 자간은 영문 칸
        self.assertEqual(400, width(fw.FW_SPACE, latin=tight))
        self.assertEqual(1000, width(fw.NB_SPACE, latin=tight))             # 묶음 빈칸은 자간 없음

    def test_style_from_hwpx_char_pr(self):
        header = ET.fromstring(
            '<head><fontfaces>'
            '<fontface lang="HANGUL"><font id="3" face="휴먼명조" type="TTF"/></fontface>'
            '<fontface lang="LATIN"><font id="1" face="신명 중명조" type="HFT"/></fontface>'
            '</fontfaces><charPr id="0" height="1400" useFontSpace="1">'
            '<fontRef hangul="3" latin="1" symbol="9"/><ratio hangul="95" latin="90"/>'
            '<spacing hangul="-3" latin="0"/><relSz hangul="110" latin="100"/><bold/></charPr></head>')
        style = fw.style_from_char_pr(header.find("charPr"), fw.fontfaces_from_header(header))
        self.assertEqual((1400, True, True), (style.height, style.use_font_space, style.bold))
        self.assertEqual(fw.LangFont("휴먼명조", "TTF", 95, -3, 110), style.fonts["hangul"])
        self.assertEqual(fw.LangFont("신명 중명조", "HFT", 90), style.fonts["latin"])
        self.assertNotIn("symbol", style.fonts)


class HangulRuleTest(unittest.TestCase):
    """INDENTW2 에서 한/글 실측으로 정한 규칙."""

    def width(self, text, face="휴먼명조", size=20, **options):
        return fw.measure_text(text, fw.CharStyle.uniform(face, size, **options)).width

    def test_ratio_floors_without_intermediate_rounding(self):
        # 20pt = 500 격자, 'a' 260/512: 장평 100 은 반올림(254), 90 은 floor(500×em×0.9)=228
        self.assertEqual(254 * 4, self.width("a"))
        self.assertEqual(228 * 4, self.width("a", ratio=90))

    def test_hft_bold_widens_and_hft_font_space(self):
        plain = self.width("1", "신명 중명조", font_type="HFT")
        bold = self.width("1", "신명 중명조", font_type="HFT", bold=True)
        self.assertAlmostEqual(0.05, (bold - plain) / 2000, delta=0.003)
        self.assertEqual(800, self.width(" ", "신명 중명조", font_type="HFT", use_font_space=True))

    def test_ttf_bold_changes_only_some_font_spaces(self):
        self.assertEqual(self.width("a1 "), self.width("a1 ", bold=True))
        self.assertEqual(1000, self.width(" ", use_font_space=True))
        self.assertEqual(984, self.width(" ", use_font_space=True, bold=True))

    def test_one_line_paragraph_is_not_stretched(self):
        style = fw.CharStyle.uniform("휴먼명조", 15)
        result = fw.justified_indent([("  - 내용", style)], 4, 42520)
        self.assertEqual(fw.measure_text("  - ", style).width, result.width)

    def test_justified_slack_goes_to_spaces_after_leading_ones(self):
        style = fw.CharStyle.uniform("휴먼명조", 10)        # 한글 1000, 빈칸 500
        text = "  - 가가 나나 다다다다"
        prefix = fw.measure_text("  - ", style).width
        # 글자 나눔: '  - 가가 나나 다' 까지가 첫 줄, 늘릴 빈칸은 문두 2칸을 뺀 3곳 중 접두 안 1곳
        natural = fw.measure_text("  - 가가 나나 다", style).width
        result = fw.justified_indent([(text, style)], 4, natural + 300)
        self.assertAlmostEqual(prefix + 300 / 3, result.width)
        # 어절 나눔: '  - 가가 나나' 에서 끊고 빈칸 2곳이 남는 폭을 나눈다
        word_line = fw.measure_text("  - 가가 나나", style).width
        by_word = fw.justified_indent([(text, style)], 4, natural + 300, korean_by_word=True)
        self.assertAlmostEqual(prefix + (natural + 300 - word_line) / 2, by_word.width)

    def test_justified_ignores_last_character_spacing(self):
        style = fw.CharStyle.uniform("휴먼명조", 10, spacing=-10)     # 한글 900(자간 -100)
        text = "- 가가 나나나나나나"
        line = fw.measure_text("- 가가 나", style).width
        result = fw.justified_indent([(text, style)], 2, line + 100)
        # 줄 끝 글자의 자간(-100)은 줄 폭에 넣지 않으므로 남는 폭은 100 - 100 = 0
        self.assertAlmostEqual(fw.measure_text("- ", style).width, result.width)

    def test_justified_unknown_cases_are_none(self):
        style = fw.CharStyle.uniform("휴먼명조", 10)
        self.assertIsNone(fw.justified_indent([("-가가가가가가가가", style)], 1, 3000))   # 늘릴 빈칸 없음
        self.assertIsNone(fw.justified_indent([("- 가 나나나나", style)], 8, 5000))      # 첫 줄 밖
        self.assertIsNone(fw.justified_indent([("- 가", fw.CharStyle.uniform("없는 글꼴"))], 2, 5000))


class MeasureToolTest(unittest.TestCase):
    """실측 도구의 한/글 없이 확인할 수 있는 부분."""

    def test_synthetic_hwpx_and_random_cases(self):
        self.assertEqual(tool.random_cases(50), tool.random_cases(50))
        cases = [case for _, case in tool.random_cases(30)]
        cases.append(tool.simple("ㅇ" + tool.FW_SPACE + tool.NB_SPACE, tool.Style("휴먼명조")))
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "synthetic.hwpx"
            tool.build_hwpx(cases, path)
            with zipfile.ZipFile(path) as archive:
                header = ET.fromstring(archive.read("Contents/header.xml"))
                section = ET.fromstring(archive.read("Contents/section0.xml"))
        paragraphs = [x for x in section if x.tag.endswith("}p")]
        self.assertEqual(len(cases) + 1, len(paragraphs))
        ids = {x.get("id") for x in header.iter() if x.tag.endswith("}paraPr")}
        self.assertTrue({p.get("paraPrIDRef") for p in paragraphs} <= ids)
        names = {x.tag.rsplit("}", 1)[-1] for x in paragraphs[-1].iter()}
        self.assertTrue({"fwSpace", "nbSpace"} <= names)


def synthetic_font(path: Path):
    """cmap 형식 4 하나를 가진 최소 sfnt: 'A'→600, 'B'→600, 'D'→700(마지막 긴 hmtx 항목 재사용)."""
    head = bytearray(54)
    struct.pack_into(">H", head, 18, 1000)
    hhea = bytearray(36)
    struct.pack_into(">H", hhea, 34, 3)
    hmtx = struct.pack(">HhHhHh", 500, 0, 600, 0, 700, 0)
    segments = ((0x41, 0x42, 1), (0x44, 0x44, 3), (0xFFFF, 0xFFFF, 1))  # (첫, 끝, 첫 글리프)
    ends = struct.pack(">3H", *(s[1] for s in segments))
    starts = struct.pack(">3H", *(s[0] for s in segments))
    deltas = struct.pack(">3H", 0xFFC0, 0xFFBF, 1)  # A→1, B→2, D→3
    body = ends + b"\0\0" + starts + deltas + b"\0" * 6
    subtable = struct.pack(">7H", 4, 14 + len(body), 0, 6, 0, 0, 0) + body
    cmap = struct.pack(">HHHHI", 0, 1, 3, 1, 12) + subtable
    tables = {b"cmap": cmap, b"head": bytes(head), b"hhea": bytes(hhea), b"hmtx": hmtx}
    offset = 12 + 16 * len(tables)
    directory, payload = b"", b""
    for name, value in tables.items():
        directory += struct.pack(">4sIII", name, 0, offset + len(payload), len(value))
        payload += value
    path.write_bytes(struct.pack(">IHHHH", 0x00010000, len(tables), 0, 0, 0) + directory + payload)


class GeneratorTest(unittest.TestCase):
    def test_reads_advances_from_font_tables(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "synthetic.ttf"
            synthetic_font(path)
            units, advances = maker.read_advances(path)
        self.assertEqual(1000, units)
        self.assertEqual({0x41: 600, 0x42: 700, 0x44: 700}, advances)

    def test_compress_keeps_gaps(self):
        self.assertEqual([[65, 66, 6], [68, 68, 6], [69, 69, 7]],
                         maker.compress({65: 6, 66: 6, 68: 6, 69: 7}))

    def test_rebuild_keeps_ttf_hangul_measurements(self):
        old = {"face": "휴먼명조", "type": "TTF", "aliases": [], "status": "ok",
               "measured": {"units_per_em": 1250, "ranges": [[8226, 8226, 579]]},
               "bold": {"units_per_em": 1250, "ranges": [[32, 32, 615]]}}
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "synthetic.ttf"
            synthetic_font(path)
            installed = {face: path for face, _ in maker.TTF_FONTS}
            with mock.patch.object(maker, "_installed", return_value=installed), \
                    mock.patch.object(maker.hangul_fonts, "_font_names", return_value=[]):
                table = maker.build({"fonts": [old]})
        entry = next(x for x in table["fonts"] if (x["face"], x["type"]) == ("휴먼명조", "TTF"))
        self.assertEqual((old["measured"], old["bold"]), (entry["measured"], entry["bold"]))

    def test_rebuild_keeps_measured_hft_entries(self):
        measured = {"face": "신명 중명조", "type": "HFT", "aliases": [], "status": "measured",
                    "units_per_em": 1000, "ranges": [[65, 65, 500]]}
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "synthetic.ttf"
            synthetic_font(path)
            catalog = [{"name": face, "path": str(path)} for face, _ in maker.TTF_FONTS]
            with mock.patch.object(maker.hangul_fonts, "catalog", return_value=catalog), \
                    mock.patch.object(maker.hangul_fonts, "_font_names", return_value=[]):
                table = maker.build({"fonts": [measured]})
        self.assertIn(measured, table["fonts"])
        self.assertEqual(1, sum(x["face"] == "신명 중명조" for x in table["fonts"]))

    def test_committed_file_is_in_generator_format(self):
        text = fw.DATA_PATH.read_text(encoding="utf-8")
        self.assertEqual(maker.dumps(json.loads(text)), text)
        faces = [(x["face"], x["type"]) for x in json.loads(text)["fonts"]]
        self.assertEqual([(f, "TTF") for f, _ in maker.TTF_FONTS] + [(f, "HFT") for f in maker.HFT_FONTS], faces)


if __name__ == "__main__":
    unittest.main()
