"""KFS1 문장 위치를 기존 화면 행으로 바꾼다. 원문은 SQLite/호출자에서만 읽는다."""
from __future__ import annotations

import html
import os
import re
import unicodedata
from pathlib import Path

from . import hierarchy_path
from .content_search import LEVEL_LABELS, LEVEL_ORDER


def _fold_positions(text):
    folded, positions = [], []
    # 원문 코드포인트 위치를 유지한다(NFC 합성·casefold 확장 포함).
    for match in re.finditer(r".[\u0300-\u036f\u1160-\u11ff]*", text, re.S):
        for char in unicodedata.normalize("NFC", match.group()).casefold():
            if char.isalnum():
                folded.append(char)
                positions.append((match.start(), match.end()))
    return "".join(folded), positions


def _location(paragraph, sentence, words, crumbs):
    body = paragraph["body"] or ""
    start = sentence.get("start")
    length = sentence.get("len")
    if type(start) is not int or type(length) is not int or not 0 <= start <= start + length <= len(body):
        raise ValueError("kfind 문장 위치와 저장 원문이 다릅니다")
    folded, positions = _fold_positions(body[start:start + length])
    spans = set()
    for word in words:
        for term in word.get("pieces") or [word["text"]]:
            needle = _fold_positions(term)[0]
            if not needle:
                continue
            offset = 0
            while (found := folded.find(needle, offset)) >= 0:
                spans.add((start + positions[found][0], start + positions[found + len(needle) - 1][1]))
                offset = found + 1
    first = min((a for a, _ in spans), default=start)
    left, right = max(0, first - 70), min(len(body), first + 180)
    label = paragraph.get("location_label") or f"문단 {paragraph['paragraph_index'] + 1}"
    position = {"page": None, "sheet": None, "row": None, "label": label}
    page = re.search(r"(\d+)쪽", label)
    row = re.fullmatch(r"(?:(.+?)\s+)?(\d+)행", label)
    if page:
        position["page"] = int(page[1])
    if row:
        position.update(sheet=row[1], row=int(row[2]))
    trail = hierarchy_path.breadcrumb(crumbs, paragraph["paragraph_index"])
    return {"paragraph_index": paragraph["paragraph_index"], "snippet": body[left:right],
            "highlights": [[a - left, min(b, right) - left] for a, b in sorted(spans) if left <= a < right],
            "position": position, "breadcrumb": [step["text"] for step in trail],
            "breadcrumb_text": hierarchy_path.breadcrumb_text(trail)}


def _documents(response, index, documents):
    """documents는 외부 파일·메모용 doc_key → metadata + paragraphs + scopes 사전."""
    supplied = documents or {}
    paths = {hit["path"] for hit in response["hits"] if hit.get("path") and hit.get("doc_key") not in supplied}
    loaded = {}
    if paths and index is not None:
        with index._lock:
            for row in index._select_for_paths("SELECT * FROM content_meta WHERE path IN ({paths})", paths):
                loaded[row["path"]] = dict(row, paragraphs=[])
            for row in index._select_paragraphs_for_paths(
                    "path, paragraph_index, body, location_label, level, level_rank", paths):
                if row["path"] in loaded:
                    loaded[row["path"]]["paragraphs"].append(dict(row))
            for path, doc in loaded.items():
                doc["crumbs"] = list(index._breadcrumb_rows([path]))
    for hit in response["hits"]:
        doc = supplied.get(hit.get("doc_key"), loaded.get(hit.get("path")))
        if doc is None:
            raise ValueError("kfind 결과의 저장 원문이 없습니다")
        yield hit, doc


def to_keyword_rows(response, index=None, *, documents=None, offset=0):
    rows = []
    for hit, doc in _documents(response, index, documents):
        path = hit.get("path") or doc.get("path", "")
        paragraphs = {int(p["paragraph_index"]): dict(p) for p in doc["paragraphs"]}
        crumbs = doc.get("crumbs", list(paragraphs.values()))
        locations = [_location(paragraphs[s["para"]], s, response["words"], crumbs)
                     for s in hit["sentences"][:3]]
        primary = locations[0] if locations else {"snippet": "", "highlights": [], "breadcrumb": [],
                    "breadcrumb_text": "", "position": {"page": None, "sheet": None, "row": None}}
        count = hit["count"]
        level = hit["level"]
        rows.append({"path": path, "name": doc.get("name") or os.path.basename(path),
                     "extension": doc.get("extension", Path(path).suffix.lstrip(".")),
                     "mtime": doc.get("mtime"), "folder_path": os.path.dirname(path),
                     "folder_name": os.path.basename(os.path.dirname(path)),
                     **{k: primary[k] for k in ("snippet", "highlights", "position", "breadcrumb", "breadcrumb_text")},
                     "locations": locations, "sections": hierarchy_path.group_by_parent(crumbs, [s["para"] for s in hit["sentences"]]),
                     "score": count, "total_matches": count,
                     "match_counts": {w["text"]: count for w in response["words"]},
                     "best_level": LEVEL_LABELS[LEVEL_ORDER[level] if 0 <= level < len(LEVEL_ORDER) else "body"],
                     "context_hits": 0, "relevance_rank": offset + len(rows) + 1,
                     "adjacent": hit["adjacent"], "dropped": response.get("dropped", []),
                     "engine": "kfind", "gen": response["generation"]})
    return rows


def to_hierarchy_rows(response, index=None, *, documents=None):
    return to_keyword_rows(response, index, documents=documents)


def _highlighted(location):
    text = location["snippet"]
    merged = []
    for a, b in sorted(location["highlights"]):
        if merged and a <= merged[-1][1]:
            merged[-1][1] = max(b, merged[-1][1])
        else:
            merged.append([a, b])
    parts, end = [], 0
    for a, b in merged:
        parts.extend((html.escape(text[end:a]), "<b>", html.escape(text[a:b]), "</b>"))
        end = b
    parts.append(html.escape(text[end:]))
    return "".join(parts)


def to_library_rows(response, index=None, *, documents=None):
    docs = list(_documents(response, index, documents))
    rows = to_keyword_rows(response, index, documents=documents)
    for row, (hit, doc) in zip(rows, docs):
        row.update(title=doc.get("title", row["name"]), paragraph_count=hit["count"],
                   snippets=[loc["snippet"] for loc in row["locations"]],
                   highlighted=[_highlighted(loc) for loc in row["locations"]],
                   page=row["position"]["page"], hit_pages=[loc["position"]["page"] for loc in row["locations"]],
                   box_ids=[scope[4:] for scope in doc.get("scopes", []) if scope.startswith("box:")])
    return rows


def to_notebook_rows(response, index=None, *, documents=None):
    rows = []
    for hit, doc in _documents(response, index, documents):
        sources = [s.split(":")[3] for s in doc.get("scopes", [])
                   if re.fullmatch(r"nb:[^:]+:src:[^:]+", s)]
        paragraphs = {int(p["paragraph_index"]): p for p in doc["paragraphs"]}
        for sentence in hit["sentences"]:
            paragraph = paragraphs[sentence["para"]]
            location = _location(paragraph, sentence, response["words"], doc.get("crumbs", list(paragraphs.values())))
            for source in sources:
                rows.append({"source_id": int(source), "ordinal": sentence["para"],
                             "display_name": doc.get("display_name", doc.get("name", "")),
                             "location": location["position"]["label"], "body": paragraph["body"],
                             "score": hit["count"], "snippet": location["snippet"],
                             "highlights": location["highlights"], "adjacent": hit["adjacent"],
                             "dropped": response.get("dropped", []), "engine": "kfind", "gen": response["generation"]})
    # 같은 문단의 여러 일치 문장은 업무자료함의 한 문단 행으로 표시한다.
    return list({(r["source_id"], r["ordinal"]): r for r in rows}.values())
