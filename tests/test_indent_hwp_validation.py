"""INDENTW4 실측 도구가 글꼴·단위·본문 변화를 숨기지 않는지 확인한다(COM 없음)."""
import copy
import runpy
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from docfit_core import indent_table as it
from docfit_core.xml_hanging_indent import hanging_indent_offset
from scripts import measure_indent_hangul as maker
from scripts import validate_indent_hwp_pc as probe


class ValidationInventoryTest(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.path = Path(folder.name) / "synthetic.hwpx"
        self.items = [maker.simple("ㅇ ", maker.Style("휴먼명조", 15), "내용을 보존한다"),
                      maker.simple("- ", maker.Style("휴먼명조", 15, "HFT"), "다른 글꼴")]
        maker.build_hwpx(self.items, self.path)

    def test_same_face_with_different_font_type_is_distinguished(self):
        rows = probe.inventory(self.path)
        self.assertNotEqual(rows[1]["styles"][0], rows[2]["styles"][0])
        self.assertEqual(self.items[0].text, rows[1]["text"])
        self.assertEqual(len(rows[1]["text"]), len(rows[1]["styles"]))

    def test_comparison_detects_character_changes(self):
        before = probe.inventory(self.path)
        after = copy.deepcopy(before)
        after[1]["styles"][0] = after[2]["styles"][0]
        after[2]["text"] += "변경"
        result = probe.compare(before, after)
        self.assertEqual([1], result["styles"])
        self.assertEqual([2], result["text"])

    def test_comparison_detects_missing_paragraph(self):
        before = probe.inventory(self.path)
        self.assertFalse(probe.compare(before, before[:-1])["same_count"])

    def test_margin_error_keeps_half_point_boundary(self):
        before = probe.inventory(self.path)
        after = copy.deepcopy(before)
        after[1]["margins"][0]["intent"] = (-50, "HWPUNIT")
        self.assertEqual(0, probe.compare(before, after)["over_half_pt"])
        after[1]["margins"][0]["intent"] = (-51, "HWPUNIT")
        self.assertEqual(1, probe.compare(before, after)["over_half_pt"])

    def test_xml_application_preserves_all_character_shapes(self):
        from unittest.mock import patch
        before = probe.inventory(self.path)
        with patch.object(it, "font_verified", return_value=True):
            self.assertEqual(2, it.apply_in_place(self.path, hanging_indent_offset))
        after = probe.inventory(self.path)
        result = probe.compare(before, after)
        self.assertEqual([], result["text"])
        self.assertEqual([], result["styles"])
        self.assertEqual([], result["paragraph_other"])

    def test_preflight_covers_every_font_and_justification(self):
        cases = probe.cases()
        self.assertEqual(set(maker.ALIGN_NAMES), {case.align for case in cases})
        self.assertTrue(any(len(case.runs) > 1 and {s.font_type for _, s in case.runs} == {"TTF", "HFT"}
                            for case in cases))
        self.assertTrue(all(case.offset < len(case.text) for case in cases))

    def test_hwp_needs_a_real_converted_package(self):
        hwp = mock.Mock(Version=it.environment_fingerprint()["hangul"])
        original = self.path.with_suffix(".hwp")
        original.write_bytes(it._OLE_MAGIC + b"synthetic")
        self.assertFalse(it.enabled(hwp, lambda: {}, original))
        self.assertTrue(it.enabled(hwp, lambda: {}, original, converted_hwpx=self.path))
        self.assertFalse(it.enabled(hwp, lambda: {}, original, converted_hwpx=original))
        self.assertFalse(it.enabled(hwp, lambda: {}, original, converted_hwpx=self.path.with_name("missing.hwpx")))
        self.path.write_bytes(b"not a package")
        self.assertFalse(it.enabled(hwp, lambda: {}, original, converted_hwpx=self.path))

    def test_renamed_binary_uses_the_same_conversion_gate(self):
        hwp = mock.Mock(Version=it.environment_fingerprint()["hangul"])
        original = self.path.with_name("renamed.hwpx")
        original.write_bytes(it._OLE_MAGIC + b"synthetic")
        self.assertFalse(it.enabled(hwp, lambda: {}, original))
        self.assertTrue(it.enabled(hwp, lambda: {}, original, converted_hwpx=self.path))

    def test_conversion_does_not_bypass_settings_or_environment(self):
        hwp = mock.Mock(Version=it.environment_fingerprint()["hangul"])
        original = self.path.with_suffix(".hwp")
        self.assertFalse(it.enabled(hwp, lambda: {it.SETTING_KEY: False}, original, converted_hwpx=self.path))
        hwp.Version = "unmeasured"
        self.assertFalse(it.enabled(hwp, lambda: {}, original, converted_hwpx=self.path))

    def test_hwp_stage_uses_its_saved_snapshot(self):
        ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / "hwp-auto-docfit.py"))
        fn = ns["_폭표_내어쓰기_검사"]
        hwp = mock.Mock(Version=it.environment_fingerprint()["hangul"], GetPos=lambda: (0, 1, 0))
        save = mock.Mock(return_value=self.path)
        with mock.patch.object(it, "font_verified", return_value=True), mock.patch.dict(fn.__globals__, {
            "hwp": hwp, "설정_불러오기": lambda: {}, "현재_처리파일": self.path.with_suffix(".hwp"),
            "_제목_임시hwpx_저장": save, "_제목_임시폴더_정리": lambda folder: folder.rmdir(),
            "폭표_내어쓰기_오프셋": hanging_indent_offset,
        }):
            check = fn()
        self.assertEqual(2, len(check.plans))
        save.assert_called_once()
        hwp.SetPos.assert_called_once_with(0, 1, 0)


if __name__ == "__main__":
    unittest.main()
