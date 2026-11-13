import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from docfit_app.library_bridge import LibraryBridge
from docfit_app.library_kfind_sync import LibraryKfindSync, NotebookKfindSync
from docfit_core.timemachine.content_search import ParagraphRecord


class FakeKfind:
    def __init__(self):
        self.docs, self.calls = {}, []
        self.generation = 1

    def state(self):
        return dict(generation=self.generation)

    def status(self):
        self.calls.append(("status",))
        return self.state()

    def stale(self, scope, level_rule, **kw):
        return dict(doc_keys=sorted(k for k, d in self.docs.items() if scope in d["scopes"]), more=False)

    def ingest(self, doc):
        self.calls.append(("ingest", copy.deepcopy(doc)))
        self.docs[doc["doc_key"]] = copy.deepcopy(doc)
        return self.state()

    def remove(self, keys):
        self.calls.append(("remove", keys))
        for key in keys:
            self.docs.pop(key, None)
        return self.state()

    def query(self, q, scope, **kwargs):
        if not 1 <= kwargs["top"] <= 1000 or not 0 <= kwargs["refs"] <= 100:
            raise ValueError("KFS1 query bound")
        self.calls.append(("query", q, scope, kwargs))
        hits = []
        for key, doc in self.docs.items():
            if scope not in doc["scopes"]:
                continue
            sources = kwargs.get("sources")
            if sources is not None and not any(f"{scope}:src:{s}" in doc["scopes"] for s in sources):
                continue
            sentences = [dict(para=p["p"], ord=i, start=0, len=len(p["t"]))
                         for i, p in enumerate(doc["paras"]) if q in p["t"]]
            if sentences:
                hits.append(dict(doc_key=key, path=doc["path"], count=len(sentences),
                                 level=8, adjacent=True, sentences=sentences[:kwargs["refs"]]))
        return dict(generation=self.generation, words=[dict(text=q)], dropped=[],
                    total_docs=len(hits), total_sentences=sum(h["count"] for h in hits),
                    hits=hits[:kwargs["top"]])

    def count(self, q, scope, **kwargs):
        self.calls.append(("count", q, scope))
        response = self.query(q, scope, top=1000, refs=0, **kwargs)
        return {k: response[k] for k in ("generation", "total_docs", "total_sentences")}


class LibrarySyncTests(unittest.TestCase):
    def setUp(self):
        self.client = FakeKfind()
        self.items = [dict(id=3, box_id=2, kind="text", source_path="", display_name="메모", created_at=1)]
        self.text = "예산 집행"
        self.requests = []

    def request(self, payload):
        self.requests.append(payload)
        if payload["cmd"] == "box_get":
            return dict(ok=True, items=copy.deepcopy(self.items))
        if payload["cmd"] == "item_text_get":
            return dict(ok=True, content=self.text)
        raise AssertionError(payload)

    def test_memo_outside_roots_indexed_and_identity_preserved(self):
        sync = LibraryKfindSync(self.request, self.client)
        result = sync.search(dict(cmd="search", box_id=2, query="예산"))
        self.assertEqual(result["engine"], "kfind")
        self.assertEqual(result["results"][0]["item_ids"], [3])
        self.assertEqual(result["total_docs"], 1)
        self.assertEqual(self.client.docs["memo:2:3"]["scopes"], ["box:2"])
        self.assertEqual(sync.state["corpus"], "synced")

    def test_item_search_retains_item_metadata_and_snippet(self):
        result = LibraryKfindSync(self.request, self.client).search(dict(cmd="item_search", box_id=2, query="예산"))
        self.assertEqual(result["items"][0]["id"], 3)
        self.assertIn("예산", result["items"][0]["snippet"])

    def test_changed_memo_ingested_once_unchanged_is_cached(self):
        sync = LibraryKfindSync(self.request, self.client)
        payload = dict(cmd="search", box_id=2, query="예산")
        sync.search(payload)
        sync.search(payload)
        self.assertEqual(sum(c[0] == "ingest" for c in self.client.calls), 1)
        self.text = "예산 수정"
        sync.search(payload)
        self.assertEqual(sum(c[0] == "ingest" for c in self.client.calls), 2)

    def test_removed_item_and_restart_orphan_removed_without_other_scope(self):
        sync = LibraryKfindSync(self.request, self.client)
        sync.sync("box:2")
        self.client.docs["memo:9:1"] = dict(scopes=["box:9"])
        self.items.clear()
        restarted = LibraryKfindSync(self.request, self.client)
        self.assertEqual(restarted.search(dict(cmd="search", box_id=2, query="예산"))["total_docs"], 0)
        self.assertEqual(set(self.client.docs), {"memo:9:1"})

    def test_service_generation_change_reseeds(self):
        sync = LibraryKfindSync(self.request, self.client)
        sync.sync("box:2")
        self.client.docs.clear()
        self.client.generation += 1
        sync.sync("box:2")
        self.assertIn("memo:2:3", self.client.docs)

    def test_count_uses_server_box_scope(self):
        result = LibraryKfindSync(self.request, self.client).search(dict(cmd="search", box_id=2, query="예산", count_only=True))
        self.assertEqual(result["total_sentences"], 1)
        self.assertIn(("count", "예산", "box:2"), self.client.calls)

    def test_external_file_uses_stored_path_and_python_levels(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / "outside.txt"
            path.write_text("예산 집행", encoding="utf-8")
            self.items[0].update(kind="file", source_path=str(path))
            records = [ParagraphRecord(0, "예산 집행", "section", 2, False)]
            with patch("docfit_app.library_kfind_sync.cs.extract_paragraphs", return_value=records) as extract:
                sync = LibraryKfindSync(self.request, self.client)
                sync.sync("box:2")
                sync.sync("box:2")
            self.assertEqual(extract.call_count, 1)
            self.assertEqual(self.client.docs["memo:2:3"]["paras"][0]["lv"], 2)
            self.assertEqual(self.client.docs["memo:2:3"]["path"], str(path))

    def test_sync_error_is_failed_and_retried(self):
        sync = LibraryKfindSync(lambda p: dict(ok=False, error="missing"), self.client)
        with self.assertRaises(ValueError):
            sync.sync("box:2")
        self.assertEqual(sync.state["corpus"], "failed")
        sync.request = self.request
        sync.sync("box:2")
        self.assertEqual(sync.state["corpus"], "synced")


class NotebookSyncTests(unittest.TestCase):
    def test_nbidx1_uses_source_paragraph_api_not_library_or_original(self):
        calls = []
        def request(p):
            calls.append(p)
            if p["cmd"] == "nb_source_list":
                return dict(ok=True, sources=[dict(id=7, display_name="출처", origin="missing.txt")])
            if p["cmd"] == "nb_source_paragraphs":
                return dict(ok=True, paragraphs=[dict(ordinal=4, body="예산 집행", location="9쪽")])
            raise AssertionError(p)
        client = FakeKfind()
        sync = NotebookKfindSync(request, client)
        result = sync.search(dict(notebook_id=2, query="예산", source_ids=[7]))
        self.assertEqual(result["results"][0]["source_id"], 7)
        self.assertEqual(result["results"][0]["ordinal"], 4)
        self.assertEqual(result["results"][0]["location"], "9쪽")
        self.assertEqual({p["cmd"] for p in calls}, {"nb_source_list", "nb_source_paragraphs"})
        self.assertEqual(client.docs["nbmemo:2:7"]["scopes"], ["nb:2", "nb:2:src:7"])
        self.assertEqual(sync.search(dict(notebook_id=2, query="예산", source_ids=[]))["results"], [])


class BridgeKfindTests(unittest.TestCase):
    def bridge(self, settings=None):
        return LibraryBridge(pipe_name="test", settings_provider=lambda: settings or {}, kfind=FakeKfind())

    def test_legacy_setting_never_calls_new_engine(self):
        bridge = self.bridge(dict(search_engine="legacy"))
        with patch.object(bridge, "_legacy_request", return_value=dict(ok=True, results=[1])):
            result = bridge.request(dict(cmd="search", box_id=2, query="예산"))
        self.assertEqual(result["results"], [1])
        self.assertEqual(bridge._kfind.calls, [])
        self.assertEqual(result["engine_reason"], "configured_legacy")

    def test_default_kfind_failure_falls_back_with_reason(self):
        bridge = self.bridge()
        with patch.object(bridge, "_search_sync", side_effect=OSError("service down")), \
             patch.object(bridge, "_legacy_request", return_value=dict(ok=True, results=[1])):
            result = bridge.request(dict(cmd="search", box_id=2, query="예산"))
        self.assertEqual((result["engine"], result["engine_reason"]), ("legacy", "service down"))

    def test_empty_item_search_does_not_connect(self):
        bridge = self.bridge()
        with patch.object(bridge, "_legacy_request", return_value=dict(ok=True, items=[])):
            self.assertEqual(bridge.request(dict(cmd="item_search", box_id=2, query="")), dict(ok=True, items=[]))
        self.assertEqual(bridge._kfind.calls, [])
