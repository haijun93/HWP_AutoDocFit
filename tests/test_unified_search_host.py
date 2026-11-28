"""앱 안 몽땅검색 통합 보기의 파이썬 연결. 합성 자료와 가짜 호스트만 쓴다.

``UNIFIED_BENCH_EXE``(ktm 예제 unified_bench)와 ``KTM_HOST_EXE``(ktm.exe)를 지정하면
실제 호스트를 합성 색인·격리 파이프로 왕복하는 시험도 돈다. 설치 서비스는 쓰지 않는다.
"""
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import time
import unittest

from docfit_app.timemachine_service import TimemachineService
from docfit_app.unified_search_host import UnifiedSearchHost, supports_search_host
from tests.test_timemachine_service import FakeAgent, stub_scanner


class FakeProcess:
    """표준 입력에 적힌 명령을 기록하고, 시험이 넣은 응답을 표준 출력으로 낸다."""
    def __init__(self):
        self.sent, self.lines = [], queue.Queue()
        self.stdin, self.stdout = self, iter(self.lines.get, None)
        self.closed = self.killed = False
        self.on_send = None

    def write(self, data):
        message = json.loads(data.decode("utf-8"))
        self.sent.append(message)
        if self.on_send:
            self.on_send(message)

    def flush(self):
        pass

    def close(self):
        self.closed = True
        self.lines.put(None)

    def emit(self, **event):
        self.lines.put((json.dumps(event, ensure_ascii=False) + "\n").encode("utf-8"))

    def poll(self):
        return 0 if self.closed else None

    def wait(self, timeout=None):
        return 0

    def kill(self):
        self.killed = True


def until(check, timeout=3.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(0.005)
    raise AssertionError("시간 초과")


class HostTests(unittest.TestCase):
    def setUp(self):
        self.processes, self.now = [], [0.0]

        def popen(command, **options):
            process = FakeProcess()
            process.command = command
            self.processes.append(process)
            return process

        self.host = UnifiedSearchHost(executable=lambda: Path("ktm.exe"), popen=popen,
                                      supported=lambda path: True, clock=lambda: self.now[0])
        self.addCleanup(self.host.close)

    def test_streams_latest_part_then_page_and_drops_old_generations(self):
        first = self.host.start({"query": "예산"})
        process = self.processes[0]
        self.assertEqual(process.command[1:], ["--search-host"])
        self.assertEqual(process.sent[-1], {"query": "예산", "command": "start", "generation": first})
        self.assertIsNone(self.host.poll(first))
        process.emit(event="part", generation=first, completed="파일명")
        self.assertEqual(until(lambda: self.host.poll(first))["event"], "part")
        second = self.host.start({"query": "계약"})
        self.assertEqual(len(self.processes), 1)
        self.assertEqual(self.host.poll(first)["event"], "stale")
        process.emit(event="page", generation=first)
        process.emit(event="page", generation=second, cursor=0)
        self.assertEqual(until(lambda: self.host.poll(second))["generation"], second)

    def test_page_waits_for_the_answer_of_the_same_session(self):
        generation = self.host.start({"query": "예산"})
        process = self.processes[0]
        process.emit(event="page", generation=generation, cursor=0)
        until(lambda: self.host.poll(generation))
        process.on_send = lambda m: process.emit(event="page", generation=m["generation"], cursor=m["cursor"])
        self.assertEqual(self.host.page(generation, 1)["cursor"], 1)
        self.assertEqual(self.host.page(generation + 1, 1)["event"], "error")

    def test_cancel_discards_late_answers(self):
        generation = self.host.start({"query": "예산"})
        process = self.processes[0]
        self.host.cancel(generation)
        self.assertEqual(process.sent[-1], {"command": "cancel", "generation": generation})
        process.emit(event="page", generation=generation)
        time.sleep(0.05)
        self.assertEqual(self.host.poll(generation)["event"], "stale")
        # 취소 뒤 새 검색은 늘어난 세대로 보낸다.
        self.assertGreater(self.host.start({"query": "계약"}), generation + 1)

    def test_failures_are_not_reported_as_zero_results(self):
        generation = self.host.start({"query": "예산"})
        self.processes[0].close()
        failed = until(lambda: self.host.poll(generation))
        self.assertEqual((failed["summary"], failed["notice"]["title"]), ("검색 못함", "검색을 완료하지 못했습니다"))
        self.assertIn("0건은 '파일이 없다'는 뜻이 아닙니다.", failed["notice"]["detail"])
        generation = self.host.start({"query": "예산"})
        self.assertEqual(len(self.processes), 2)
        self.now[0] = 1000.0
        self.assertEqual(self.host.poll(generation)["summary"], "검색 못함")

    def test_missing_or_old_executable_is_never_launched(self):
        for host in (UnifiedSearchHost(executable=lambda: None, popen=self.fail),
                     UnifiedSearchHost(executable=lambda: Path("ktm.exe"), popen=self.fail, supported=lambda p: False)):
            self.assertEqual(host.poll(host.start({"query": "예산"}))["summary"], "검색 못함")
        with tempfile.TemporaryDirectory() as folder:
            old, new = Path(folder) / "old.exe", Path(folder) / "new.exe"
            old.write_bytes(b"MZ --status --install")
            new.write_bytes(b"MZ --search-host")
            self.assertFalse(supports_search_host(old))
            self.assertTrue(supports_search_host(new))
            self.assertFalse(supports_search_host(Path(folder) / "none.exe"))

    def test_release_keeps_the_process_for_the_next_input(self):
        generation = self.host.start({"query": "예산"})
        process = self.processes[0]
        self.host.release()
        self.assertEqual(process.sent[-1], {"command": "close", "generation": generation})
        self.assertFalse(process.closed)
        self.assertEqual(self.host.poll(generation)["event"], "stale")
        self.host.start({"query": "계약"})
        self.assertEqual(len(self.processes), 1)

    def test_close_ends_the_process(self):
        self.host.start({"query": "예산"})
        self.host.close()
        self.assertTrue(self.processes[0].closed)
        self.assertFalse(self.processes[0].killed)


class FakeHost:
    def __init__(self):
        self.requests, self.closed = [], 0

    def start(self, request):
        self.requests.append(request)
        return len(self.requests)

    def poll(self, generation):
        return {"event": "page", "generation": generation}

    def page(self, generation, cursor):
        return {"event": "page", "generation": generation, "cursor": cursor}

    def cancel(self, generation):
        pass

    def close(self):
        self.closed += 1


class ServiceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        base = Path(self.temp.name)
        self.root = base / "문서"
        self.root.mkdir()
        self.settings = base / "설정"
        self.host = FakeHost()
        self.opened, self.revealed = [], []
        self.service = self.make(base)
        self.addCleanup(self.service.close)
        self.service.command("settings_set", {"roots": [str(self.root)], "time_mode": "max", "enabled": False,
                                              "extensions": ["hwpx", "txt", "pdf"]})

    def make(self, base):
        return TimemachineService(self.settings, db_path=base / "timeline.db", agent=FakeAgent(),
                                  opener=self.opened.append, revealer=self.revealed.append, scanner=stub_scanner,
                                  thumb_dir=base / "thumbs", unified_host=self.host)

    def view(self):
        return self.service.command("settings_get")["settings"]["unified_view"]

    def test_view_is_off_by_default_persists_and_survives_other_saves(self):
        self.assertFalse(self.view())
        self.assertEqual(self.service.command("unified_view_set", {"enabled": True}),
                         {"ok": True, "unified_view": True})
        settings = self.service.command("settings_get")["settings"]
        self.service.command("settings_set", {key: settings[key] for key in ("roots", "time_mode", "enabled")})
        self.assertTrue(self.view())
        self.assertTrue(self.service._load_settings()["unified_view"])
        self.assertFalse(self.service.command("unified_view_set", {"enabled": "yes"})["ok"])
        self.service.command("unified_view_set", {"enabled": False})
        self.assertFalse(self.view())
        self.assertGreaterEqual(self.host.closed, 1)

    def test_start_scopes_roots_kinds_dates_and_related_query(self):
        start = time.mktime((2026, 3, 1, 0, 0, 0, 0, 0, -1))
        end = time.mktime((2026, 4, 1, 0, 0, 0, 0, 0, -1))
        result = self.service.command("unified_start", {"query": "2026; 공사 계약", "kinds": ["hwp"],
                                                        "start": start, "end": end})
        self.assertEqual(result, {"ok": True, "generation": 1})
        request = self.host.requests[-1]
        self.assertEqual(request["roots"], [str(self.root)])
        self.assertEqual(request["extensions"], ["hwpx"])
        self.assertEqual(request["dates"], [20260301, 20260331])
        self.assertTrue(request["related_queries"])
        self.assertIn("appdata", request["exclude_dirs"])
        self.service.command("unified_start", {"query": "예산"})
        self.assertIsNone(self.host.requests[-1]["dates"])
        self.assertFalse(self.host.requests[-1]["related_queries"])

    def test_empty_scope_never_widens_to_everything(self):
        for payload in ({"query": ""}, {"query": "예산", "kinds": ["image"]}, {"query": "예산", "kinds": ["x"]},
                        {"query": '"미완료'}, {"query": "예산", "start": 2, "end": 1}):
            self.assertFalse(self.service.command("unified_start", payload)["ok"], payload)
        self.service.command("settings_set", {"roots": [], "time_mode": "max", "enabled": False})
        self.assertIn("조사 폴더", self.service.command("unified_start", {"query": "예산"})["message"])
        self.assertEqual(self.host.requests, [])

    def test_poll_page_and_path_open_keep_existing_checks(self):
        self.assertEqual(self.service.command("unified_poll", {"generation": 1})["result"]["generation"], 1)
        self.assertEqual(self.service.command("unified_page", {"generation": 1, "cursor": 2})["result"]["cursor"], 2)
        self.assertFalse(self.service.command("unified_page", {"generation": 1, "cursor": -1})["ok"])
        self.assertFalse(self.service.command("unified_poll", {})["ok"])
        document = self.root / "합성.txt"
        document.write_text("합성", encoding="utf-8")
        self.assertTrue(self.service.command("open", {"path": str(document)})["ok"])
        self.assertTrue(self.service.command("reveal", {"path": str(document)})["ok"])
        self.assertEqual((self.opened, self.revealed), ([str(document)], [str(document)]))


    def test_folder_results_are_searched_without_kind_filter_and_open_by_path_only_inside_roots(self):
        self.service.command("unified_start", {"query": "공사"})
        self.assertFalse(self.host.requests[-1]["files_only"])
        self.service.command("unified_start", {"query": "공사", "kinds": ["hwp"]})
        self.assertTrue(self.host.requests[-1]["files_only"])
        folder = self.root / "합성 공사계약 v1.2"
        folder.mkdir()
        self.assertTrue(self.service.command("open", {"path": str(folder)})["ok"])
        self.assertTrue(self.service.command("reveal", {"path": str(folder)})["ok"])
        self.assertEqual((self.opened, self.revealed), ([str(folder)], [str(folder)]))
        outside = Path(self.temp.name) / "밖"
        outside.mkdir()
        excluded = self.root / "appdata"
        excluded.mkdir()
        hidden = self.root / ".숨김"
        hidden.mkdir()
        for path in (outside, excluded, hidden):
            self.assertFalse(self.service.command("open", {"path": str(path)})["ok"], path)
        self.assertEqual(self.opened, [str(folder)])


@unittest.skipUnless(os.environ.get("UNIFIED_BENCH_EXE") and os.environ.get("KTM_HOST_EXE"),
                     "실제 통합 검색 호스트 실행 파일을 지정하지 않음")
class RealHostTests(unittest.TestCase):
    def test_synthetic_round_trip(self):
        with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as folder:
            corpus = Path(folder) / "corpus"
            pipe = rf"\\.\pipe\unified-app-py-{os.getpid()}"
            fixture = subprocess.Popen([os.environ["UNIFIED_BENCH_EXE"], str(corpus), pipe],
                                       stdout=subprocess.DEVNULL)
            try:
                until(lambda: (corpus / "server-ready").exists(), 60)
                host = UnifiedSearchHost(executable=lambda: Path(os.environ["KTM_HOST_EXE"]),
                                         overrides={"pipe": pipe, "index_root": str(corpus)})
                try:
                    generation = host.start({"query": "예산집행", "roots": [str(corpus / "documents")],
                                             "extensions": ["txt"],
                                             "paragraphs": str(corpus / "timeline-content.sqlite")})
                    page = until(lambda: (host.poll(generation) or {}).get("event") != "part"
                                 and host.poll(generation), 30)
                    self.assertEqual(page["event"], "page", page)
                    self.assertFalse(page["incomplete"], page)
                    paths, total = set(), sum(page["totals"])
                    while True:
                        paths.update(row["path"] for row in page["answer"]["rows"])
                        if page["next_cursor"] is None:
                            break
                        page = host.page(generation, page["next_cursor"])
                    self.assertEqual(len(paths), total)
                    self.assertGreater(total, 80)
                finally:
                    host.close()
            finally:
                fixture.kill()
                fixture.wait()


if __name__ == "__main__":
    unittest.main()
