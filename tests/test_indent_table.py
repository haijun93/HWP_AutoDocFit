"""폭 표 내어쓰기 연결(INDENTW3). 한/글·설치 글꼴 없이 합성 HWPX 로 확인한다."""
import re
import runpy
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from defusedxml import ElementTree as ET

from docfit_core import font_widths as fw
from docfit_core import indent_table as it
from docfit_core.batch_format import _serialize
from docfit_core.fidelity.package import PackageSnapshot, build_package
from docfit_core.xml_hanging_indent import hanging_indent_offset
from scripts import measure_indent_hangul as tool

BODY = "가나다 라마바 사아자"
FACE = "함초롬바탕"


def case(text="ㅇ " + BODY, style=None, **options):
    style = style or tool.Style(FACE)
    return tool.Case(((text, style),), 2, **options)


def edit(path: Path, name: str, old: str, new: str, count: int = 0) -> None:
    """합성 문서의 XML 조각을 바꾼다(count=0 이면 전부)."""
    snapshot = PackageSnapshot.load(path)
    xml = snapshot.read(name).decode("utf-8")
    assert old in xml, old
    xml = xml.replace(old, new) if not count else xml.replace(old, new, count)
    path.write_bytes(build_package(snapshot, {name: xml.encode("utf-8")}))


class Base(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.path = Path(folder.name) / "sample.hwpx"
        # 설치 글꼴과 무관하게: 표에 있는 글꼴이면 확인된 것으로 본다.
        patcher = mock.patch.object(it, "font_verified", lambda face, kind: fw.has_table(face, kind))
        patcher.start()
        self.addCleanup(patcher.stop)

    def build(self, *cases):
        tool.build_hwpx(list(cases), self.path)

    def plans(self, offset=hanging_indent_offset):
        return it.plan_document(self.path, offset).plans


class PlanTest(Base):
    def test_body_paragraph_uses_table_width(self):
        self.build(case())
        plan = self.plans()[1]           # 0번은 구역 설정 문단
        expected = fw.measure_text("ㅇ " + BODY, fw.CharStyle.uniform(FACE), 2).hwpunit
        self.assertEqual((0, -expected), (plan.left, plan.intent))

    def test_justified_multiline_uses_stretched_first_line(self):
        text = "ㅇ " + "가나다 " * 30
        self.build(case(text, align="JUSTIFY"))
        style = fw.CharStyle.uniform(FACE)
        # 폭은 문서 XML 의 쪽 설정에서 읽는다(합성 서식: 용지 59528 - 좌우 5669).
        expected = fw.justified_indent([(text, style)], 2, 59528 - 2 * 5669).hwpunit
        self.assertEqual(-expected, self.plans()[1].intent)
        self.assertGreater(expected, fw.measure_text(text, style, 2).hwpunit)

    def test_unverified_conditions_fall_back(self):
        cases = {
            "표에 없는 글꼴": case(style=tool.Style("굴림")),
            "한자": tool.Case((("韓: " + BODY, tool.Style(FACE)),), 3),
            "범위 밖 크기": case(style=tool.Style(FACE, size=30)),
            "범위 밖 자간": case(style=tool.Style(FACE, spacing=-20)),
            "범위 밖 장평": case(style=tool.Style(FACE, ratio=60)),
        }
        for name, item in cases.items():
            with self.subTest(name):
                self.build(item)
                self.assertEqual({}, self.plans(offset=lambda text: item.offset))

    def test_tab_and_special_space_fall_back(self):
        for element in ("<hp:tab/>", "<hp:fwSpace/>", "<hp:nbSpace/>"):
            with self.subTest(element):
                self.build(case())
                edit(self.path, "Contents/section0.xml", "ㅇ 가나다", "ㅇ 가나다" + element)
                self.assertEqual({}, self.plans())

    def test_paragraph_shape_conditions_fall_back(self):
        changes = (
            ('condense="0"', 'condense="20"'),
            ('fontLineHeight="0"', 'fontLineHeight="1"'),
            ('horizontal="LEFT"', 'horizontal="CENTER"'),
            ('<hh:heading type="NONE"', '<hh:heading type="BULLET"'),
            ('breakLatinWord="KEEP_WORD"', 'breakLatinWord="BREAK_WORD"'),
            ('eAsianEng="0"', 'eAsianEng="1"'),
        )
        for old, new in changes:
            with self.subTest(new):
                self.build(case())
                edit(self.path, "Contents/header.xml", old, new)
                self.assertEqual({}, self.plans())

    def test_character_effects_fall_back(self):
        changes = (
            ('useKerning="0"', 'useKerning="1"'),
            ('<hh:offset hangul="0"', '<hh:offset hangul="10"'),
            ('<hh:outline type="NONE"/>', '<hh:outline type="SOLID"/>'),
            ('<hh:underline type="NONE"', '<hh:supscript/><hh:underline type="NONE"'),
            ('<hh:underline type="NONE"', '<hh:italic/><hh:underline type="NONE"'),
            ('<hh:underline type="NONE"', '<hh:underline type="BOTTOM"'),
        )
        for old, new in changes:
            with self.subTest(new):
                self.build(case())
                edit(self.path, "Contents/header.xml", old, new)
                self.assertEqual({}, self.plans())

    def test_section_conditions_fall_back(self):
        for old, new in (('lineGrid="0"', 'lineGrid="1600"'), ('landscape="WIDELY"', 'landscape="NARROWLY"'),
                         ('gutter="0"', 'gutter="1000"')):
            with self.subTest(new):
                self.build(case())
                edit(self.path, "Contents/section0.xml", old, new)
                self.assertEqual({}, self.plans())

    def test_table_cell_and_controls_are_not_planned(self):
        self.build(case())
        inner = ('<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:tbl><hp:tr><hp:tc><hp:subList>'
                 '<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>ㅇ 칸 안 글</hp:t></hp:run></hp:p>'
                 '</hp:subList></hp:tc></hp:tr></hp:tbl></hp:run></hp:p></hs:sec>')
        edit(self.path, "Contents/section0.xml", "</hs:sec>", inner)
        self.assertEqual([1], list(self.plans()))

    def test_trailing_attachment_block_is_left_to_hangul(self):
        self.build(case("ㅇ " + BODY), case("붙임  1. 계획서 1부."), case("      2. 명단 1부.  끝."))
        self.assertEqual([1], list(self.plans(offset=lambda text: 2)))

    def test_rejected_offset_is_not_planned(self):
        self.build(case())
        self.assertEqual({}, self.plans(offset=lambda text: None))


class ApplyTest(Base):
    def test_writes_intent_and_keeps_text_and_other_paragraphs(self):
        self.build(case("ㅇ " + BODY), case("ㅇ 다른 문단", style=tool.Style("굴림")))
        before = ET.fromstring(PackageSnapshot.load(self.path).read("Contents/section0.xml"))
        self.assertEqual(1, it.apply_in_place(self.path, hanging_indent_offset))
        snapshot = PackageSnapshot.load(self.path)
        header = ET.fromstring(snapshot.read("Contents/header.xml"))
        after = ET.fromstring(snapshot.read("Contents/section0.xml"))
        self.assertEqual("".join(before.itertext()), "".join(after.itertext()))
        ids = lambda root: [p.get("paraPrIDRef") for p in root]
        self.assertNotEqual(ids(before)[1], ids(after)[1])
        self.assertEqual(ids(before)[2], ids(after)[2])
        shapes = {x.get("id"): x for x in header.iter() if x.tag.endswith("}paraPr")}
        values = [int(x.get("value")) for x in shapes[ids(after)[1]].iter() if x.tag.endswith("}intent")]
        expected = fw.measure_text("ㅇ " + BODY, fw.CharStyle.uniform(FACE), 2).hwpunit
        self.assertEqual([-expected, -expected * 2], values)          # case, default
        old = shapes[ids(before)[1]]
        self.assertEqual({"0"}, {x.get("value") for x in old.iter() if x.tag.endswith("}intent")})
        self.assertEqual(0, it.apply_in_place(self.path, hanging_indent_offset))   # 두 번 써도 그대로

    def test_only_requested_texts_are_written(self):
        self.build(case())
        self.assertEqual(0, it.apply_in_place(self.path, hanging_indent_offset, texts={"다른 글"}))

    def test_estimated_margin_pair_is_reset_like_hangul_path(self):
        self.build(case(left=2000))
        edit(self.path, "Contents/header.xml", '<hc:intent value="0" unit="HWPUNIT"/><hc:left value="2000"',
             '<hc:intent value="-2000" unit="HWPUNIT"/><hc:left value="2000"')
        self.assertEqual(0, self.plans()[1].left)


class StageCheckTest(unittest.TestCase):
    def hwp(self, left, indent, pos=(0, 1, 0)):
        values = {"LeftMargin": left, "Indentation": indent}
        return mock.Mock(GetPos=lambda: pos, ParaShape=mock.Mock(Item=values.__getitem__))

    def check(self):
        return it.StageCheck({1: it.Plan("s", None, "ㅇ 글", 0, -1500, (0, 0))})

    def test_matching_value_is_skipped(self):
        check = self.check()
        self.assertTrue(check.matches(self.hwp(0, -3000), "ㅇ 글\r\n"))
        self.assertEqual("[폭 표 내어쓰기] 표 처리 1문단 / 한/글 처리 0문단", check.summary())

    def test_changed_value_text_or_area_goes_to_hangul(self):
        check = self.check()
        for hwp, text in ((self.hwp(0, -3100), "ㅇ 글"), (self.hwp(200, -3000), "ㅇ 글"),
                          (self.hwp(0, -3000), "ㅇ 바뀐 글"), (self.hwp(0, -3000, (5, 1, 0)), "ㅇ 글"),
                          (self.hwp(0, -3000, (0, 2, 0)), "ㅇ 글")):
            self.assertFalse(check.matches(hwp, text))
        self.assertEqual(0, check.table)


class EnvironmentTest(unittest.TestCase):
    def test_only_the_measured_hangul_version(self):
        self.assertTrue(it.version_matches("9, 6, 1, 10778"))
        self.assertTrue(it.version_matches("9.6.1.10778"))
        for other in ("11, 0, 0, 1", "9, 6, 1, 99999", "", None, mock.MagicMock()):
            self.assertFalse(it.version_matches(other))

    def test_setting_turns_it_off(self):
        hwp = mock.Mock(Version="9, 6, 1, 10778")
        self.assertFalse(it.enabled(hwp, lambda: {}))
        self.assertTrue(it.enabled(hwp, lambda: {it.SETTING_KEY: True}))
        self.assertFalse(it.enabled(hwp, lambda: {it.SETTING_KEY: False}))
        self.assertFalse(it.enabled(None, lambda: {}))
        self.assertFalse(it.enabled(hwp, None))

    def test_hwp_source_stays_on_hangul(self):
        hwp = mock.Mock(Version="9, 6, 1, 10778")
        with tempfile.TemporaryDirectory() as folder:
            binary = bytes.fromhex("d0cf11e0a1b11ae1") + b"synthetic"
            for name, content, expected in (("a.hwp", b"", False), ("b.HWP", binary, False),
                                            ("renamed.hwpx", binary, False), ("c.hwpx", b"PK", True),
                                            ("d.txt", b"text", True)):
                with self.subTest(name):
                    source = Path(folder) / name
                    source.write_bytes(content)
                    self.assertEqual(expected, it.enabled(hwp, lambda: {it.SETTING_KEY: True}, str(source)))

    def test_every_ttf_in_table_has_a_fingerprint(self):
        environment = it.environment_fingerprint()
        faces = {face for face, kind in fw._tables() if kind == "TTF"}
        known = set(environment["fonts"])
        self.assertTrue(known <= faces)
        self.assertEqual(7, len(known))
        self.assertTrue(all(re.fullmatch(r"[0-9a-f]{64}", item["sha256"]) for item in environment["fonts"].values()))

    def test_different_font_file_is_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            other = Path(folder) / "other.ttf"
            other.write_bytes(b"another version")
            it.font_verified.cache_clear()
            self.addCleanup(it.font_verified.cache_clear)
            with mock.patch.object(it, "font_file", lambda name: other):
                self.assertFalse(it.font_verified("HY헤드라인M", "TTF"))
            it.font_verified.cache_clear()
            with mock.patch.object(it, "hft_fingerprint", lambda: "0" * 64):
                self.assertFalse(it.font_verified("신명 중명조", "HFT"))


class EngineWiringTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / "hwp-auto-docfit.py"))

    def run_one(self, check, attachments=None):
        function = self.ns["_내어쓰기_한문단"]
        applied = mock.Mock(return_value=True)
        hwp = mock.Mock(GetPos=lambda: (0, 1, 0))
        with mock.patch.dict(function.__globals__, {"hwp": hwp, "문단_내어쓰기_적용": applied,
                                                    "문단_내어쓰기_기준_오프셋": lambda text: 2,
                                                    "내어쓰기_기호선택_허용": lambda text: True}):
            function("ㅇ 글", attachments or {}, {}, check)
        return applied

    def test_table_paragraph_does_not_call_shift_tab(self):
        check = mock.Mock(table=0, com=0, matches=mock.Mock(return_value=True))
        self.run_one(check).assert_not_called()
        self.assertEqual(0, check.com)

    def test_other_paragraph_uses_hangul_and_is_counted(self):
        check = it.StageCheck()
        self.run_one(check).assert_called_once()
        self.assertEqual((0, 1), (check.table, check.com))

    def test_disabled_check_reads_nothing_and_saves_nothing(self):
        function = self.ns["_폭표_내어쓰기_검사"]
        save = mock.Mock()
        with mock.patch.dict(function.__globals__, {"hwp": None, "_제목_임시hwpx_저장": save}):
            self.assertEqual({}, function().plans)
        save.assert_not_called()


if __name__ == "__main__":
    unittest.main()
