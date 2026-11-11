"""docfit_core.com_perf — 한/글 COM 호출 계측(개발용) 시험. 한/글 없이 가짜 객체로 확인한다."""
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_core import com_perf


class FakeCom:
    """win32com 동적 객체 흉내: ``_oleobj_`` 속성이 있으면 COM 객체로 본다."""

    def __init__(self, name="hwp"):
        self._oleobj_ = object()
        self.name = name
        self.PageCount = 3
        self.received = []
        self.child = None

    def Run(self, action):
        self.received.append(("Run", action))
        return True

    def Execute(self, action, parameter_set):
        self.received.append(("Execute", action, parameter_set))
        return True

    def Item(self, index):
        return FakeCom(f"{self.name}.Item({index})")

    def GetText(self):
        return (2, "글")


class FakeClock:
    def __init__(self):
        self.now = 0.0

    def __call__(self):
        self.now += 1.0
        return self.now


class ComPerfTest(unittest.TestCase):
    def setUp(self):
        com_perf.counter().reset()

    def test_disabled_returns_same_object_and_records_nothing(self):
        target = FakeCom()
        with patch.dict(os.environ, {com_perf.ENV_ENABLE: ""}):
            self.assertIs(com_perf.wrap_if_enabled(target), target)
            com_perf.set_stage("자간")
            self.assertIsNone(com_perf.finish_document("a.hwpx", {"전체": 1.0, "합": {}, "순서": []}))
        self.assertEqual(0, com_perf.counter().total_calls)

    def test_proxy_forwards_calls_properties_and_assignment(self):
        target = FakeCom()
        target.child = FakeCom("HAction")
        with patch.dict(os.environ, {com_perf.ENV_ENABLE: "1"}):
            hwp = com_perf.wrap_if_enabled(target)
        self.assertIsInstance(hwp, com_perf.ComProxy)
        self.assertEqual(3, hwp.PageCount)
        self.assertTrue(hwp.child.Run("MoveDocBegin"))
        hwp.PageCount = 7
        self.assertEqual(7, target.PageCount)
        self.assertEqual([("Run", "MoveDocBegin")], target.child.received)
        self.assertEqual((2, "글"), hwp.GetText())
        calls = com_perf.counter().by_call
        self.assertEqual(1, calls["child.Run()"][0])
        self.assertEqual(1, calls["PageCount="][0])

    def test_com_results_are_wrapped_and_arguments_unwrapped(self):
        target = FakeCom()
        target.child = FakeCom("HParameterSet")
        with patch.dict(os.environ, {com_perf.ENV_ENABLE: "1"}):
            hwp = com_perf.wrap_if_enabled(target)
        item = hwp.Item(0)
        self.assertIsInstance(item, com_perf.ComProxy)
        hwp.Execute("CharShape", hwp.child)
        # 한/글에는 대리 객체가 아니라 원래 COM 객체가 넘어가야 한다.
        self.assertIs(target.child, target.received[-1][2])
        self.assertIs(target.child, com_perf.unwrap(hwp.child))

    def test_stage_attribution_and_report(self):
        clock = FakeClock()
        counter = com_perf.ComCounter(clock=clock)
        with patch.object(com_perf, "_counter", counter), \
                patch.dict(os.environ, {com_perf.ENV_ENABLE: "1"}):
            hwp = com_perf.wrap_if_enabled(FakeCom())
            com_perf.set_stage("열기")
            hwp.Run("FileOpen")
            com_perf.set_stage("자간 조정")
            hwp.Run("A")
            hwp.Run("B")
            table = {"전체": 20.0, "합": {"열기": [5.0, 1], "자간 조정": [15.0, 1]}, "순서": ["열기", "자간 조정"]}
            report = com_perf.build_report("문서.hwpx", table)
        stages = {item["name"]: item for item in report["stages"]}
        # 호출 한 번은 속성 읽기 1회 + 실행 1회로 센다(둘 다 COM 왕복이다).
        self.assertEqual(2, stages["열기"]["com_calls"])
        self.assertEqual(4, stages["자간 조정"]["com_calls"])
        self.assertEqual(6, report["com_calls"])
        self.assertAlmostEqual(report["com_seconds"] + report["python_seconds"], 20.0, places=3)
        self.assertEqual({"Run", "Run()"}, {item["name"] for item in report["top_calls"]})
        lines = com_perf.summary_lines(report)
        self.assertEqual(3, len(lines))
        self.assertIn("COM 호출 6회", lines[0])

    def test_nested_calls_count_time_once(self):
        clock = FakeClock()
        counter = com_perf.ComCounter(clock=clock)

        def outer():
            return counter.timed("inner", lambda: 1)

        counter.timed("outer", outer)
        self.assertEqual(2, counter.total_calls)
        self.assertEqual(0.0, counter.by_call["inner"][1])
        self.assertGreater(counter.by_call["outer"][1], 0.0)
        self.assertEqual(counter.by_call["outer"][1], counter.total_seconds)

    def test_finish_document_writes_json_when_directory_is_set(self):
        with tempfile.TemporaryDirectory() as folder:
            env = {com_perf.ENV_ENABLE: "1", com_perf.ENV_OUTPUT_DIR: folder}
            with patch.dict(os.environ, env):
                com_perf.start_document()
                hwp = com_perf.wrap_if_enabled(FakeCom())
                hwp.Run("X")
                logged = []
                report = com_perf.finish_document(
                    "보고서.hwpx", {"전체": 2.0, "합": {"준비": [2.0, 1]}, "순서": ["준비"]},
                    log=logged.append, source_path=Path(folder) / "보고서.hwpx")
            saved = json.loads((Path(folder) / "보고서.comperf.json").read_text(encoding="utf-8"))
        self.assertEqual(report, saved)
        self.assertEqual(2, saved["com_calls"])
        self.assertEqual(3, len(logged))


if __name__ == "__main__":
    unittest.main()
