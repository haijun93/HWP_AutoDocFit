"""RLEV: Rust 본문 색인(klib index-content) 경로도 위계는 Python 규칙과 같아야 한다.

klib 실행 파일이 있으면(KLIB_EXE 또는 klib/target/{release,debug}/klib.exe) 실제로 두 경로를 돌려
level·level_rank·table_cell_id를 문단마다 견준다. 없으면 그 시험만 건너뛴다(CI는 klib 빌드 뒤 KLIB_EXE로 돌린다).
"""
import os
import shutil
import tempfile
import threading
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_app.timemachine_service import TimemachineService
from docfit_core.timemachine.content_search import CURRENT_HIERARCHY_SCHEMA_VERSION

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests" / "fixtures"
SAMPLE_MD = "# 제목1\n\n## 장\n\n### 절\n\n본문 고유검색어 문장\n"
RUST_EXTENSIONS = {"txt", "md", "hwpx", "docx"}


def find_klib():
    env = os.environ.get("KLIB_EXE")
    if env and os.path.isfile(env):
        return env
    for build in ("release", "debug"):
        candidate = ROOT / "klib" / "target" / build / "klib.exe"
        if candidate.is_file():
            return str(candidate)
    return None


class RustLevelsBase(unittest.TestCase):
    def setUp(self):
        self.base = Path(tempfile.mkdtemp(prefix="rlev-test-"))
        self.root = self.base / "docs"
        self.root.mkdir()
        self.services = []

    def tearDown(self):
        for service in self.services:
            service.close()
        shutil.rmtree(self.base, ignore_errors=True)

    def service(self, name, *, rust):
        service = TimemachineService(self.base / f"settings-{name}", db_path=self.base / f"{name}.db", agent=False)
        service._settings = {**service._settings, "roots": [str(self.root)],
                             "extensions": sorted(RUST_EXTENSIONS), "rust_content_index": rust}
        self.services.append(service)
        return service

    @staticmethod
    def index(service):
        service._content_index_worker([str(service._settings["roots"][0])],
                                      frozenset(service._settings["extensions"]), threading.Event())

    @staticmethod
    def levels(service):
        db = service.content_index.db
        cells = {(row[0], int(row[1])): row[2] for row in db.execute(
            "SELECT path, paragraph_index, cell_id FROM content_table_cells")}
        result = {}
        for row in db.execute("SELECT path, paragraph_index, level, level_rank, body FROM content_paragraph_fts"
                              " ORDER BY path, CAST(paragraph_index AS INTEGER)"):
            index = int(row[1])
            result.setdefault(row[0], []).append(
                (index, row[2], int(row[3]), cells.get((row[0], index)), row[4]))
        return result

    @staticmethod
    def breadcrumb(service, query):
        items = service.command("content_search", {"query": query})["items"]
        return [item.get("breadcrumb_text") for item in items]

    @staticmethod
    def versions(service):
        return {row[0] for row in service.content_index.db.execute(
            "SELECT hierarchy_schema_version FROM content_meta")}


class RustWrittenRowsAreRelevelledTest(RustLevelsBase):
    """klib 없이도 도는 시험: Rust가 쓴 것처럼 위계 없는(body·옛 버전) 행을 두고 색인 작업자가 다시 매기는지."""

    def fake_rust(self, service):
        def run(paths, cancel_event, seen, indexed, errors):
            index = service.content_index
            for path in paths:
                index.index_file(path)
            with index._lock, index.db:
                # Rust index-content가 남기는 상태: 위계 없음, 표 칸 번호 없음, 위계 버전 1
                index.db.execute("UPDATE content_paragraph_fts SET level='body', level_rank=8")
                index.db.execute("DELETE FROM content_table_cells")
                index.db.execute("UPDATE content_meta SET hierarchy_schema_version=1")
            return [], seen + len(paths), indexed + len(paths), errors
        return run

    def test_markdown_breadcrumb_after_rust_rows(self):
        (self.root / "sample.md").write_text(SAMPLE_MD, encoding="utf-8")
        service = self.service("fake", rust=True)
        with patch.object(service, "_run_klib_index", side_effect=self.fake_rust(service)):
            self.index(service)
        self.assertEqual(self.breadcrumb(service, "고유검색어"), ["제목1 > 장 > 절"])
        self.assertEqual(self.versions(service), {CURRENT_HIERARCHY_SCHEMA_VERSION})
        self.assertEqual(service.content_index.needs_reindex_count(roots=[str(self.root)]), 0)

    def test_same_levels_as_python_path_for_fixtures(self):
        for path in FIXTURES.rglob("*"):
            if path.is_file() and path.suffix.lower().lstrip(".") in RUST_EXTENSIONS:
                shutil.copy2(path, self.root / path.name)
        (self.root / "sample.md").write_text(SAMPLE_MD, encoding="utf-8")
        python = self.service("python", rust=False)
        self.index(python)
        rust = self.service("fake", rust=True)
        with patch.object(rust, "_run_klib_index", side_effect=self.fake_rust(rust)):
            self.index(rust)
        self.assertEqual(self.levels(rust), self.levels(python))

    def test_extraction_failure_keeps_rust_rows(self):
        (self.root / "sample.md").write_text(SAMPLE_MD, encoding="utf-8")
        service = self.service("fake", rust=True)
        rules_fail = patch("docfit_core.timemachine.content_search.classify_paragraph_levels",
                           side_effect=RuntimeError("rules failed"))
        rust_then_fail = self.fake_rust(service)
        def run(*args):
            result = rust_then_fail(*args)
            rules_fail.start()  # Rust 단계가 끝난 뒤 위계 재판정만 실패시킨다
            return result
        try:
            with patch.object(service, "_run_klib_index", side_effect=run):
                self.index(service)
        finally:
            rules_fail.stop()
        rows = self.levels(service)
        self.assertEqual(len(next(iter(rows.values()))), 4)
        self.assertEqual(self.versions(service), {1})


@unittest.skipUnless(find_klib(), "klib 실행 파일이 없음(KLIB_EXE 또는 cargo build --release로 빌드)")
class RealKlibLevelsTest(RustLevelsBase):
    def setUp(self):
        super().setUp()
        self._env = patch.dict(os.environ, {"KLIB_EXE": find_klib()})
        self._env.start()

    def tearDown(self):
        self._env.stop()
        super().tearDown()

    def test_rust_path_markdown_breadcrumb(self):
        (self.root / "sample.md").write_text(SAMPLE_MD, encoding="utf-8")
        service = self.service("rust", rust=True)
        self.index(service)
        self.assertEqual(self.breadcrumb(service, "고유검색어"), ["제목1 > 장 > 절"])
        self.assertEqual(service.content_index.needs_reindex_count(roots=[str(self.root)]), 0)
        status = service.content_status()
        self.assertEqual((status["seen"], status["indexed"], status["errors"]), (1, 1, 0))  # 재판정은 두 번 세지 않는다

    def test_rust_and_python_paths_store_same_levels(self):
        for path in FIXTURES.rglob("*"):
            if path.is_file() and path.suffix.lower().lstrip(".") in RUST_EXTENSIONS:
                shutil.copy2(path, self.root / path.name)
        (self.root / "sample.md").write_text(SAMPLE_MD, encoding="utf-8")
        python = self.service("python", rust=False)
        self.index(python)
        rust = self.service("rust", rust=True)
        self.index(rust)
        expected, actual = self.levels(python), self.levels(rust)
        self.assertEqual(sorted(actual), sorted(expected))
        for path in expected:
            strip = lambda rows: [row[:4] for row in rows]
            self.assertEqual(strip(actual[path]), strip(expected[path]), path)

    def test_old_rust_index_is_relevelled_on_next_run(self):
        """오늘 넣은 자동 재색인: 옛 Rust 색인(위계 버전 1)이 다음 색인 때 현재 규칙으로 고쳐지는지."""
        (self.root / "sample.md").write_text(SAMPLE_MD, encoding="utf-8")
        service = self.service("rust", rust=True)
        index = service.content_index
        import subprocess
        subprocess.run([find_klib(), "index-content", index.path, str(self.root / "sample.md")],
                       check=True, capture_output=True)
        self.assertEqual(index.needs_reindex_count(roots=[str(self.root)]), 1)
        # RHIER1(2026-11-13) 뒤로는 klib 단독 색인도 Python과 같은 위계를 내므로 옛 경로가 틀린 위계를 낸다고 가정하지 않는다.
        # 핵심은 위계 판 표시가 옛 값이면 다음 색인 때 다시 판정되는 것.
        self.index(service)
        self.assertEqual(self.breadcrumb(service, "고유검색어"), ["제목1 > 장 > 절"])
        self.assertEqual(index.needs_reindex_count(roots=[str(self.root)]), 0)


if __name__ == "__main__":
    unittest.main()
