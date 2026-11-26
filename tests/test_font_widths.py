"""글꼴 폭 표와 폭 계산(INDENTW1). 설치 글꼴 없이 저장소의 표 파일과 합성 자료만 쓴다."""
import json
import struct
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from defusedxml import ElementTree as ET

from docfit_core import font_widths as fw
from docfit_core.xml_hanging_indent import ITEM_PATTERNS, OFFICIAL_SYMBOLS
from scripts import make_font_width_table as maker

DEFAULT_FACES = ("휴먼명조", "함초롬바탕", "함초롬돋움", "HY헤드라인M", "맑은 고딕")
TOLERANCE = 50  # HWPUNIT = 0.5pt


def em(text, face, **options):
    """글자 크기 10pt 로 재서 글자 높이 배수로 돌려준다."""
    result = fw.measure_text(text, fw.CharStyle.uniform(face, 10, **options))
    return None if result is None else round(result.width / 1000, 2)


class TableDataTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.data = json.loads(fw.DATA_PATH.read_text(encoding="utf-8"))

    def test_default_fonts_have_tables_and_hft_needs_measurement(self):
        for face in DEFAULT_FACES:
            self.assertTrue(fw.has_table(face), face)
        hft = {x["face"]: x for x in self.data["fonts"] if x["type"] == "HFT"}
        for face in ("휴먼명조", "한양신명조", "신명 중명조", "신명 태명조"):
            self.assertIn(face, hft)
        for entry in hft.values():
            if entry["status"] == "needs_measurement":
                self.assertNotIn("ranges", entry)
                self.assertFalse(fw.has_table(entry["face"], "HFT"))

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
                self.assertIsNone(fw._tables()[(face, "TTF")][0].em(ch))

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
        self.assertEqual(950, base(spacing=-5))
        self.assertEqual(800, base(rel_size=80))
        self.assertEqual(680, base(rel_size=80, ratio=90, spacing=-5))

    def test_language_slots(self):
        for ch, language in (("가", "hangul"), ("ㅇ", "hangul"), ("a", "latin"), ("1", "latin"), (" ", "latin"),
                             ("□", "symbol"), ("※", "symbol"), ("「", "symbol"), ("·", "symbol"),
                             ("漢", "hanja"), ("か", "japanese"), ("α", "other"), ("", "user")):
            self.assertEqual(language, fw.language_of(ch), ch)

    def test_each_slot_uses_its_own_font_and_ratio(self):
        fonts = {"hangul": fw.LangFont("휴먼명조"), "latin": fw.LangFont("맑은 고딕", ratio=50),
                 "symbol": fw.LangFont("함초롬바탕")}
        result = fw.measure_text("가1□", fw.CharStyle(1000, fonts))
        digit = fw.measure_text("1", fw.CharStyle.uniform("맑은 고딕", 10, ratio=50)).width
        self.assertAlmostEqual(1000 + digit + 970, result.width)

    def test_unknown_font_is_none_only_when_used(self):
        self.assertIsNone(em("가", "없는 글꼴"))
        self.assertIsNone(em("가", "신명 중명조", font_type="HFT"))
        self.assertIsNone(em("가", "휴먼명조", font_type="HFT"))
        fonts = {"hangul": fw.LangFont("휴먼명조"), "latin": fw.LangFont("없는 글꼴")}
        self.assertIsNotNone(fw.measure_text("가", fw.CharStyle(1000, fonts)))
        self.assertIsNone(fw.measure_text("가a", fw.CharStyle(1000, fonts)))
        self.assertIsNone(fw.measure_text("가□", fw.CharStyle(1000, fonts)))  # 칸 자체가 없음
        self.assertIsNone(em("가\t", "휴먼명조"))

    def test_missing_glyph_is_estimated_and_marked(self):
        result = fw.measure_text("•가•", fw.CharStyle.uniform("휴먼명조", 10))
        self.assertEqual(("•",), result.substituted)
        self.assertFalse(result.exact)
        self.assertEqual(3000, result.hwpunit)  # • 는 East Asian Width A → 1em

    def test_offset_and_runs(self):
        small = fw.CharStyle.uniform("휴먼명조", 10)
        large = fw.CharStyle.uniform("휴먼명조", 20)
        self.assertEqual(500 + 1000 + 2000, fw.measure_runs([(" 가", small), ("나다", large)], 3).hwpunit)
        self.assertEqual(0, fw.measure_runs([("가", small)], 0).hwpunit)
        # offset 뒤의 모르는 글꼴은 결과에 영향이 없다.
        self.assertEqual(1000, fw.measure_runs([("가", small), ("나", fw.CharStyle.uniform("x"))], 1).hwpunit)

    def test_bold_uses_bold_face_widths(self):
        self.assertEqual(0.42, em("*", "맑은 고딕"))
        self.assertEqual(0.46, em("*", "맑은 고딕", bold=True))
        self.assertEqual(em("*", "휴먼명조"), em("*", "휴먼명조", bold=True))

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
