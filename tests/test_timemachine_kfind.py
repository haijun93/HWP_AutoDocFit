"""APP1-F: 임시 문서와 실제 바이트 스트림 가짜 서버만 사용한다."""
import json
import tempfile
import threading
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_app.kfind_client import KfindClient
from docfit_app.timemachine_service import TimemachineService
from tests.test_kfind_client import FakeServer
from tests.test_timemachine_service import FakeAgent, stub_scanner


class KfindServiceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.path = self.root / "sample.txt"
        self.path.write_text("예산 집행 계획", encoding="utf-8")
        self.server = FakeServer()
        self.addCleanup(self.server.finish)
        self.client = KfindClient(opener=self.server.open)
        self.service = TimemachineService(self.root / "settings", db_path=self.root / "timeline.db",
            agent=FakeAgent(), scanner=stub_scanner, thumb_dir=self.root / "thumbs", kfind_client=self.client)
        self.addCleanup(self.service.close)
        self.service._settings.update(roots=[str(self.root)], extensions=["txt"], rust_content_index=False)
        self.service.content_index.index_file(self.path)

        def handle(request, result):
            if request["command"] == "ingest":
                self.server.status["generation"] += 1
            if request["command"] == "query":
                return dict(id=request["id"], generation=self.server.status["generation"], dropped=[],
                    words=[dict(text="예산", pieces=[])], hits=[dict(path=str(self.path), doc_key=str(self.path),
                        count=2, level=8, adjacent=True, sentences=[dict(para=0, start=0, len=8)])])
            return dict(result, generation=self.server.status["generation"])
        self.server.handler = handle

    def seed(self):
        self.service._seed_kfind(threading.Event())
        self.assertTrue(self.service._kfind_synced, self.service._engine_status())

    def test_default_and_explicit_legacy_never_connects(self):
        self.assertEqual(self.service._search_engine(), "kfind")
        self.service._settings["search_engine"] = "legacy"
        self.service._kfind_sync(str(self.path), {"indexed": True})
        self.service._seed_kfind(threading.Event())
        result = self.service.command("content_search", {"query": "예산"})
        self.assertTrue(result["items"])
        self.assertEqual(result["engine"], "legacy")
        self.assertEqual(self.server.requests, [])

    def test_missing_service_and_query_error_fallback_status(self):
        self.server.error = FileNotFoundError("missing service")
        result = self.service.command("content_search", {"query": "예산"})
        self.assertTrue(result["items"])
        self.assertEqual(result["engine"], "legacy")
        self.assertEqual(result["connection"]["reason"], "not_installed")
        self.assertTrue(result["engine_reason"])
        self.client.close()
        self.server.error = None
        self.seed()
        self.server.handler = lambda r, v: dict(ok=False, error="broken index") if r["command"] == "query" else v
        result = self.service.command("related_search", {"query": "예산"})
        self.assertTrue(result["items"])
        self.assertEqual(result["engine_reason"], "broken index")

    def test_seed_metadata_and_keyword_hierarchy_routes(self):
        self.seed()
        ingest = next(r for r in self.server.requests if r["command"] == "ingest")
        self.assertEqual(ingest["scopes"], ["tm"])
        self.assertEqual(ingest["paras"][0]["t"], "예산 집행 계획")
        with self.service.content_index._lock:
            meta = self.service.content_index.db.execute("SELECT kfind_gen,kfind_level_rule FROM content_meta").fetchone()
        self.assertEqual(meta["kfind_gen"], self.server.status["generation"])
        for action in ("content_search", "related_search"):
            result = self.service.command(action, {"query": "예산 ext:txt -제외", "limit": 1})
            self.assertTrue(result["ok"], result)
            self.assertEqual(result["engine"], "kfind", result)
            self.assertEqual(result["items"][0]["total_matches"], 2)
            request = self.server.requests[-1]
            self.assertEqual(request["scope"], "tm")
            self.assertEqual(request["text"], "예산")
            self.assertEqual(request["filters"]["exclude"], ["제외"])

    def test_lazy_seeding_phase_and_cancel(self):
        entered, release = threading.Event(), threading.Event()
        original = self.server.handler
        def handler(request, result):
            if request["command"] == "ingest":
                entered.set()
                release.wait(2)
            return original(request, result)
        self.server.handler = handler
        try:
            result = self.service.command("content_search", {"query": "예산"})
            self.assertEqual(result["engine"], "legacy")
            self.assertTrue(entered.wait(2))
            self.assertEqual(self.service.content_status()["phase"], "kfind_seeding")
            self.service.cancel_content_index()
        finally:
            release.set()
            self.service._content_thread.join(3)
        self.assertFalse(self.service._kfind_synced)
        self.assertEqual(self.service.content_status()["phase"], "cancelled")

    def test_sync_remove_error_and_empty_roots(self):
        self.service._kfind_sync(str(self.path), {"error": "bad"})
        self.assertEqual(self.service._kfind_pending, {})
        self.service.content_index.remove_file(self.path)
        self.service._kfind_sync(str(self.path), {"removed": True})
        self.seed()
        self.assertIn("remove", [r["command"] for r in self.server.requests])
        self.assertEqual(self.service._kfind_pending, {})
        self.service._settings["roots"] = []
        self.assertEqual(self.service.command("content_search", {"query": "예산"})["items"], [])

    def test_shadow_keeps_legacy_and_hash_only(self):
        self.service._settings["search_engine"] = "shadow"
        self.seed()
        result = self.service.command("content_search", {"query": "예산"})
        self.assertEqual(result["engine_reason"], "shadow")
        record = (self.service.settings_root / "logs" / "search-shadow.jsonl").read_text()
        self.assertNotIn("예산", record)
        self.assertEqual(len(json.loads(record)["query_hash"]), 64)

    def test_worker_hooks_and_legacy_preserves_search(self):
        self.service._settings["search_engine"] = "legacy"
        with patch.object(self.service, "_kfind_sync", wraps=self.service._kfind_sync) as sync:
            self.service._content_index_worker([str(self.root)], ["txt"], use_rust=False)
        self.assertTrue(sync.called)
        self.assertEqual(self.server.requests, [])

    def test_restarted_service_and_sync_generation_reseed(self):
        self.seed()
        self.server.status["generation"] = 0
        result = self.service.command("content_search", {"query": "예산"})
        self.assertEqual(result["engine"], "legacy")
        self.service._content_thread.join(3)
        self.assertTrue(self.service._kfind_synced)
        self.assertEqual(self.service.command("content_search", {"query": "예산"})["engine"], "kfind")

    def test_settings_engine_roundtrip_and_status(self):
        payload = dict(roots=[str(self.root)], time_mode="max", enabled=False, search_engine="legacy")
        self.assertTrue(self.service.command("settings_set", payload)["ok"])
        self.assertEqual(self.service._load_settings()["search_engine"], "legacy")
        self.assertEqual(self.service.content_status()["engine_reason"], "configured_legacy")
        payload["search_engine"] = "invalid"
        self.assertFalse(self.service.command("settings_set", payload)["ok"])

    def test_filters_pagination_and_invalid_input(self):
        self.seed()
        result = self.service.command("content_search", {"query": "예산 ext:hwp", "offset": 1})
        self.assertEqual(result["items"], [])
        request = self.server.requests[-1]
        self.assertEqual(set(request["filters"]["ext"]), {"hwp", "hwpx"})
        self.assertEqual(request["top"], 0)
        for payload in ({"query": "ext:hwp"}, {"query": "예산", "limit": True}, {"query": "예산", "offset": -1}):
            self.assertFalse(self.service.command("content_search", payload)["ok"])

    def test_sync_delta_does_not_resend_unchanged_documents(self):
        other = self.root / "other.txt"
        other.write_text("예산", encoding="utf-8")
        self.service.content_index.index_file(other)
        self.seed()
        self.server.requests.clear()
        self.service._kfind_sync(str(self.path), {"indexed": True})
        self.seed()
        ingested = [r["path"] for r in self.server.requests if r["command"] == "ingest"]
        self.assertEqual(ingested, [str(self.path)])
