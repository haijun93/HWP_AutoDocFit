"""INDENTW5: 저장본 공유는 한 시점에 한정하고 불확실한 문서는 새로 저장한다."""
import copy
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import Mock, patch

from docfit_core import indent_table as it
from docfit_core.xml_hanging_indent import hanging_indent_offset
from scripts import measure_indent_hangul as maker
from scripts import validate_indent_hwp_pc as probe


class SnapshotCostTest(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.path = Path(folder.name) / "working.hwpx"
        maker.build_hwpx([maker.simple("ㅇ ", maker.Style("휴먼명조", 15), "내용을 보존한다")], self.path)
        self.snapshot = it.PackageSnapshot.load(self.path)

    def test_conversion_gate_and_planner_share_validated_bytes(self):
        hwp = Mock(Version=it.environment_fingerprint()["hangul"])
        with patch.object(it.PackageSnapshot, "load", side_effect=AssertionError("중복 패키지 읽기")), \
                patch.object(it, "font_verified", return_value=True):
            self.assertTrue(it.enabled(hwp, lambda: {it.SETTING_KEY: True}, self.path.with_suffix(".hwp"),
                                       converted_hwpx=self.snapshot))
            document = it.plan_document(self.snapshot, hanging_indent_offset)
            self.assertEqual(1, len(document.plans))
            self.assertIs(self.snapshot, document.snapshot)

    def test_xml_write_with_shared_snapshot_preserves_text_and_styles(self):
        before = probe.inventory(self.path)
        with patch.object(it, "font_verified", return_value=True):
            self.assertEqual(1, it.apply_in_place(self.path, hanging_indent_offset, snapshot=self.snapshot))
        result = probe.compare(before, probe.inventory(self.path))
        for field in ("text", "styles", "paragraph_other"):
            self.assertEqual([], result[field])

    def test_stale_snapshot_cannot_overwrite_newer_file(self):
        self.path.write_bytes(b"newer document")
        with self.assertRaises(ValueError):
            it.apply_in_place(self.path, hanging_indent_offset, snapshot=self.snapshot)
        self.assertEqual(b"newer document", self.path.read_bytes())

    def test_snapshot_of_another_path_is_rejected(self):
        other = self.path.with_name("other.hwpx")
        other.write_bytes(self.snapshot.data)
        with self.assertRaises(ValueError):
            it.apply_in_place(other, hanging_indent_offset, snapshot=self.snapshot)

    def test_fake_conversion_snapshot_does_not_bypass_gate(self):
        bad = copy.copy(self.snapshot)
        bad.format = "hwp"
        hwp = Mock(Version=it.environment_fingerprint()["hangul"])
        self.assertFalse(it.enabled(hwp, lambda: {it.SETTING_KEY: True}, self.path.with_suffix(".hwp"),
                                    converted_hwpx=bad))

    def test_locked_cleanup_has_no_sleep_or_retry(self):
        with patch.object(it.shutil, "rmtree", side_effect=PermissionError("COM file handle")) as remove, \
                patch("time.sleep", side_effect=AssertionError("삭제 대기 금지")):
            self.assertFalse(it.cleanup_snapshot(self.path.parent))
            remove.assert_called_once_with(self.path.parent)
        self.assertTrue(self.path.exists())

    def test_stage_saves_again_without_a_document_revision_proof(self):
        ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / "hwp-auto-docfit.py"))
        fn = ns["_폭표_내어쓰기_검사"]
        hwp = Mock(Version=it.environment_fingerprint()["hangul"], GetPos=lambda: (0, 1, 0))
        save = Mock(return_value=self.path)
        with patch.object(it, "font_verified", return_value=True), patch.dict(fn.__globals__, {
            "hwp": hwp, "설정_불러오기": lambda: {it.SETTING_KEY: True},
            "현재_처리파일": self.path.with_suffix(".hwp"),
            "_폭표_성능판정": it.performance_decision(59),
            "_제목_임시hwpx_저장": save, "폭표_내어쓰기_오프셋": hanging_indent_offset,
        }):
            self.assertEqual(1, len(fn().plans))
            self.assertEqual(1, len(fn().plans))
        self.assertEqual(2, save.call_count)

    def test_performance_boundary_uses_confirmed_paragraphs(self):
        for count, state in ((None, "unmeasured"), (0, "unmeasured"), (14, "small"), (58, "small"),
                             (59, "eligible"), (299, "eligible")):
            decision = it.performance_decision(count)
            self.assertEqual(state, decision.state)
            self.assertEqual(state == "eligible", decision.use_table)

    def test_unknown_and_small_stages_do_not_save_or_read_com(self):
        ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / "hwp-auto-docfit.py"))
        fn = ns["_폭표_내어쓰기_검사"]
        for decision in (None, it.performance_decision(14)):
            with patch.dict(fn.__globals__, {"_폭표_성능판정": decision}), \
                    patch.object(it, "enabled", side_effect=AssertionError("폴백에서 COM 판 읽기 금지")):
                self.assertEqual({}, fn().plans)

    def test_new_document_resets_performance_decision_before_cancellation(self):
        ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / "hwp-auto-docfit.py"))
        fn = ns["_문서_처리_본체"]
        with patch.dict(fn.__globals__, {"_폭표_성능판정": it.performance_decision(299),
                                         "중단_요청됨": lambda: True}):
            self.assertFalse(fn("synthetic.hwpx", 1, 1))
            self.assertIsNone(fn.__globals__["_폭표_성능판정"])

    def test_default_is_off_but_explicit_setting_is_respected(self):
        from docfit_app.settings_schema import DEFAULTS
        self.assertFalse(DEFAULTS[it.SETTING_KEY])
        hwp = Mock(Version=it.environment_fingerprint()["hangul"])
        self.assertFalse(it.enabled(hwp, lambda: {}))
        self.assertTrue(it.enabled(hwp, lambda: {it.SETTING_KEY: True}))


if __name__ == "__main__":
    unittest.main()
