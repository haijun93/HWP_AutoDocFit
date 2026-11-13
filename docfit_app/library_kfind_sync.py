"""자료함 범위별 kfind 동기화. NBIDX1의 공개 파이프만 읽으며 DB를 공유하지 않는다.

pending → seeding → synced, 오류 → failed, 변경 → pending.
문서 키를 자료/출처에 귀속시켜 tm 및 다른 상자의 삭제와 충돌하지 않는다.
"""
from __future__ import annotations

import hashlib
import itertools
import json
import os
from pathlib import Path
import threading
from datetime import datetime
from types import SimpleNamespace

from docfit_core.timemachine import content_search as cs
from docfit_core.timemachine import kfind_results
from docfit_core.timemachine.kfind_records import doc_from_records


def _checked(request, payload):
    result = request(payload)
    if not isinstance(result, dict) or not result.get("ok"):
        raise ValueError(str(result.get("error", "자료함 원문 읽기 실패")) if isinstance(result, dict) else "자료함 원문 읽기 실패")
    return result


class LibraryKfindSync:
    def __init__(self, request, client):
        self.request, self.client = request, client
        self.documents, self._fingerprints, self._files = {}, {}, {}
        self._scope_keys = {}
        self._lock = threading.RLock()
        self._ids = itertools.count(1)
        self.state = dict(corpus="pending", engine="legacy", engine_reason="kfind_pending")

    def invalidate(self):
        with self._lock:
            self.state.update(corpus="pending", engine="legacy", engine_reason="kfind_pending")
            self._files.clear()

    def _records(self, item):
        path = item.get("managed_path") or item.get("source_path") or ""
        kind = item.get("kind")
        if kind in {"folder", "link", "capture", "image"}:
            return None
        if path:
            if Path(path).suffix.lower().lstrip(".") not in cs.SUPPORTED:
                return None
            stat = os.stat(path)
            signature = (path, stat.st_size, stat.st_mtime_ns)
            cached = self._files.get(item["id"])
            if cached and cached[0] == signature:
                return path, cached[1], stat
            records = cs.extract_paragraphs(path)
            self._files[item["id"]] = (signature, records)
            return path, records, stat
        text = _checked(self.request, {"cmd": "item_text_get", "id": item["id"]})["content"]
        from .notebook_service import memo_markdown_to_paragraphs
        paragraphs = memo_markdown_to_paragraphs(text)
        records = [cs.ParagraphRecord(i, p["text"], "body", 8, False)
                   for i, p in enumerate(paragraphs)]
        records = cs.classify_paragraph_levels("memo.md", records)
        stamp = float(item.get("created_at") or 0) / 1000
        return "", records, SimpleNamespace(st_size=len(text.encode("utf-8")), st_mtime=stamp, st_ctime=stamp)

    def _collect(self, scope):
        box_id = int(scope.split(":")[1])
        items = _checked(self.request, {"cmd": "box_get", "id": box_id})["items"]
        result = {}
        for item in items:
            key = f"memo:{box_id}:{item['id']}"
            try:
                extracted = self._records(item)
            except Exception:
                self.client.remove([key])
                self._fingerprints.pop(key, None)
                self.documents.pop(key, None)
                raise
            if extracted is None:
                continue
            path, records, stat = extracted
            ref_date = datetime.fromtimestamp(max(stat.st_ctime, stat.st_mtime)).strftime("%Y-%m-%d")
            ingest = doc_from_records(path or key, records, stat, ref_date, [scope])
            ingest.update(doc_key=key, path=path or key)
            doc = self._metadata(ingest, records, item.get("display_name") or Path(path).name)
            doc["item"] = item
            result[key] = (ingest, doc)
        return result

    @staticmethod
    def _metadata(ingest, records, name):
        return dict(path=ingest["path"], name=name, display_name=name,
                    mtime=ingest["mtime"] / 1000, scopes=ingest["scopes"],
                    paragraphs=[dict(paragraph_index=p.paragraph_index, body=p.text,
                                     level=p.level, level_rank=p.level_rank,
                                     location_label=f"문단 {p.paragraph_index + 1}") for p in records])

    def sync(self, scope):
        with self._lock:
            self.state.update(corpus="seeding", engine="legacy", engine_reason="kfind_seeding")
            try:
                status = self.client.status()
                generation = status["generation"]
                if generation != self.state.get("generation"):
                    self._fingerprints.clear()
                collected = self._collect(scope)
                old = set(self._scope_keys.get(scope, set()))
                # 서비스가 앱보다 오래 살아 있는 경우도 삭제를 반영한다.
                after = None
                while True:
                    inventory = self.client.stale(scope, 0xffffffff, after=after)
                    keys = inventory["doc_keys"]
                    old.update(keys)
                    if not inventory.get("more"):
                        break
                    if not keys or keys[-1] == after:
                        raise ValueError("kfind 범위 목록이 진행하지 않습니다")
                    after = keys[-1]
                removed = old - collected.keys()
                if removed:
                    self.client.remove(sorted(removed))
                for key, (ingest, doc) in collected.items():
                    digest = hashlib.sha256(json.dumps(ingest, sort_keys=True, ensure_ascii=False).encode()).hexdigest()
                    if self._fingerprints.get(key) != digest:
                        self.client.ingest(ingest)
                        self._fingerprints[key] = digest
                    self.documents[key] = doc
                for key in removed:
                    self.documents.pop(key, None)
                    self._fingerprints.pop(key, None)
                self._scope_keys[scope] = set(collected)
                self.state.update(corpus="synced", engine="kfind", engine_reason=None,
                                  generation=self.client.state()["generation"])
                return collected
            except Exception as exc:
                self.state.update(corpus="failed", engine="legacy", engine_reason=str(exc))
                raise

    def search(self, payload):
        with self._lock:
            scope = f"box:{int(payload['box_id'])}"
            collected = self.sync(scope)
            query = payload["query"]
            parsed = cs.ContentSearchIndex.parse_query(query)
            if parsed["near"] is not None:
                raise ValueError("legacy_near_filter")
            filters = {k: parsed[k] for k in ("ext", "path", "after", "before", "exclude", "phrases")}
            args = dict(filters=filters, request_id=next(self._ids))
            if payload.get("count_only"):
                response = self.client.count(" ".join(parsed["terms"]), scope, **args)
                return {"ok": True, **response, **self.state}
            limit = max(1, min(200, int(payload.get("limit", 20))))
            response = self.client.query(" ".join(parsed["terms"]), scope, top=limit, refs=3, **args)
            # 앱 재시작 전 삭제된 자료가 서비스에 남아도 현재 자료함 밖 결과를 표시하지 않는다.
            response = dict(response, hits=[h for h in response["hits"] if h["doc_key"] in collected])
            rows = kfind_results.to_library_rows(response, documents=self.documents)
            selected = rows[:limit]
            if payload.get("cmd") == "item_search":
                selected = [dict(self.documents[h["doc_key"]]["item"], snippet=row["snippet"])
                            for h, row in zip(response["hits"][:limit], selected)]
            else:
                for hit, row in zip(response["hits"][:limit], selected):
                    row.update(hits=row["paragraph_count"], pages=row["hit_pages"],
                               item_ids=[self.documents[hit["doc_key"]]["item"]["id"]])
            return dict(ok=True, **{"items" if payload.get("cmd") == "item_search" else "results": selected},
                        total_docs=response["total_docs"], total_sentences=response["total_sentences"],
                        generation=response["generation"], **{k: v for k, v in self.state.items() if k != "generation"})


class NotebookKfindSync(LibraryKfindSync):
    """NBIDX1 출처/문단 API에서만 읽는다. 원본 경로를 다시 열지 않는다."""
    def _collect(self, scope):
        notebook_id = int(scope.split(":")[1])
        sources = _checked(self.request, {"cmd": "nb_source_list", "notebook_id": notebook_id})["sources"]
        result = {}
        for source in sources:
            paragraphs = _checked(self.request, {"cmd": "nb_source_paragraphs", "notebook_id": notebook_id,
                                                "source_id": source["id"]})["paragraphs"]
            records = [cs.ParagraphRecord(int(p["ordinal"]), p["body"], "body", 8, p.get("kind") == "table")
                       for p in paragraphs]
            records = cs.classify_paragraph_levels("source.txt", records)
            key = f"nbmemo:{notebook_id}:{source['id']}"
            stamp = float(source.get("original_mtime") or 0) / 1000
            stat = SimpleNamespace(st_size=int(source.get("original_size") or 0), st_mtime=stamp, st_ctime=stamp)
            scopes = [scope, f"{scope}:src:{source['id']}"]
            ref_date = datetime.fromtimestamp(stamp).strftime("%Y-%m-%d") if stamp else None
            ingest = doc_from_records(source.get("origin") or key, records, stat, ref_date, scopes)
            ingest.update(doc_key=key)
            doc = self._metadata(ingest, records, source["display_name"])
            locations = {int(p["ordinal"]): p.get("location", "") for p in paragraphs}
            for p in doc["paragraphs"]:
                p["location_label"] = locations[p["paragraph_index"]] or p["location_label"]
            result[key] = (ingest, doc)
        return result

    def search(self, payload):
        with self._lock:
            scope = f"nb:{int(payload['notebook_id'])}"
            collected = self.sync(scope)
            parsed = cs.ContentSearchIndex.parse_query(payload["query"])
            if parsed["near"] is not None:
                raise ValueError("legacy_near_filter")
            filters = {k: parsed[k] for k in ("ext", "path", "after", "before", "exclude", "phrases")}
            response = self.client.query(" ".join(parsed["terms"]), scope, top=200, refs=100,
                filters=filters, sources=payload.get("source_ids"), request_id=next(self._ids))
            response = dict(response, hits=[h for h in response["hits"] if h["doc_key"] in collected])
            rows = kfind_results.to_notebook_rows(response, documents=self.documents)
            limit = max(1, min(200, int(payload.get("limit", 20))))
            return dict(ok=True, terms=[w["text"] for w in response.get("words", [])],
                        results=rows[:limit], total_docs=response["total_docs"],
                        total_sentences=response["total_sentences"], **self.state)
