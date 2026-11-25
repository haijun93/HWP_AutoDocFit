"""SQLite 연관 검색. 기존 순위·문맥 확장 경로와 독립된 opt-in 구현."""
import os
import re
import unicodedata
from pathlib import Path

from ..path_utils import path_is_within
from .related_query import matches, normalized, parse_related_query
from .rules import excluded_name


def evidence(text, paragraph_index, level, level_rank, group):
    # 응답의 강조 좌표는 Python 문자 인덱스(UTF-8 바이트 인덱스가 아님).
    text = unicodedata.normalize("NFC", text or "")
    folded = normalized(text)
    mapping = [i for i, char in enumerate(text) for _ in char.lower()]
    spans = sorted({(mapping[m.start()], mapping[m.end()-1]+1) for term in group.terms
                    for m in re.finditer(re.escape(normalized(term)), folded)})
    first = spans[0][0] if spans else 0
    start = max(0, first - 70)
    end = min(len(text), first + 180)
    return {"paragraph_index": paragraph_index, "level": level, "level_rank": level_rank,
            "snippet": text[start:end],
            "highlights": [[a-start, b-start] for a, b in spans if a >= start and b <= end]}


def search_related(index, query, *, roots=None, limit=100, offset=0,
                   start=None, end=None, filenames=False):
    from .content_search import LEVEL_LABELS
    groups = parse_related_query(query)
    if not groups:
        return []
    offset = max(0, int(offset))
    limit = max(1, min(int(limit), 500))
    allowed = [os.path.normcase(os.path.abspath(r)) for r in roots] if roots else None
    with index._lock:
        index._ensure_paragraph_map()
        metadata = index.db.execute("SELECT path,name,extension,mtime,size FROM content_meta").fetchall()
        valid = {}
        for meta in metadata:
            path = os.path.abspath(meta["path"])
            if excluded_name(meta["name"] or Path(path).name) or excluded_name(Path(path).name):
                continue
            if any(part.lower() == "$recycle.bin" for part in Path(path).parts):
                continue
            if start is not None and (meta["mtime"] is None or meta["mtime"] < start):
                continue
            if end is not None and (meta["mtime"] is None or meta["mtime"] >= end):
                continue
            if allowed is not None and not any(path_is_within(os.path.normcase(path), r) for r in allowed):
                continue
            valid[meta["path"]] = meta

        group_maps = []
        for group in groups:
            found = {}
            if filenames:
                rows = ((path, None, meta["name"] or Path(path).name, "filename", -1)
                        for path, meta in valid.items())
            else:
                # 원문을 열지 않는다. 긴 검색어 하나로 FTS 후보를 줄인 뒤
                # 실제 부분 문자열 AND를 검증한다(구문·기호도 정확히 판정).
                anchor = max(group.terms, key=len)
                if len(anchor) >= 3:
                    # Prefix 확장으로 토크나이저가 붙여 쓴 조사를 포함해 후보를 놓치지 않는다.
                    # 최종 결과는 아래 문장 단위 부분 문자열 AND로 다시 판정한다.
                    expression = '"' + anchor.replace('"', '""') + '"*'
                    rows = index.db.execute(
                        "SELECT path,paragraph_index,body,level,level_rank FROM content_paragraph_fts "
                        "WHERE content_paragraph_fts MATCH ?", (expression,)).fetchall()
                else:
                    rows = index.db.execute(
                        "SELECT path,paragraph_index,body,level,level_rank FROM content_paragraph_fts").fetchall()
            for path, para, body, level, rank in rows:
                if path not in valid or not matches(body, group, single_unit=filenames):
                    continue
                rank = int(rank) if rank is not None else 8
                found.setdefault(path, []).append(evidence(body, int(para) if para is not None else None,
                                                          level or "body", rank, group))
            group_maps.append(found)
        doc_set = set(group_maps[0])
        for found in group_maps[1:]:
            doc_set.intersection_update(found)
        ranked = sorted(doc_set, key=lambda path: (
            min(m["level_rank"] for found in group_maps for m in found[path]),
            -sum(len(found[path]) for found in group_maps), normalized(os.path.abspath(path))))
        results = []
        for relevance, path in enumerate(ranked[offset:offset+limit], offset+1):
            meta = valid[path]
            info = [{"group_index": i, "query": group.query, "terms": list(group.terms),
                     "match_count": len(found[path]),
                     "matches": sorted(found[path], key=lambda m: (m["level_rank"], m["paragraph_index"] or 0))}
                    for i, (group, found) in enumerate(zip(groups, group_maps))]
            all_matches = [m for g in info for m in g["matches"]]
            primary = min(all_matches, key=lambda m: (m["level_rank"], m["paragraph_index"] or 0))
            total = sum(g["match_count"] for g in info)
            row = {"path": os.path.abspath(path), "name": meta["name"] or Path(path).name,
                   "snippet": primary["snippet"], "highlights": primary["highlights"],
                   "score": total, "total_matches": total,
                   "match_counts": {g.query: len(found[path]) for g, found in zip(groups, group_maps)},
                   "best_level": LEVEL_LABELS.get(primary["level"], "파일명" if filenames else "본문"),
                   "position": {"page": None, "sheet": None, "row": None},
                   "breadcrumb": [], "breadcrumb_text": "", "context_hits": 0,
                   "relevance_rank": relevance, "group_matches": info,
                   "extension": meta["extension"], "mtime": meta["mtime"],
                   "folder_path": os.path.dirname(path), "folder_name": Path(path).parent.name,
                   "locations": [dict(m, position={"page": None, "sheet": None, "row": None,
                                  "label": f"문단 {m['paragraph_index'] + 1}" if m["paragraph_index"] is not None else "파일명"},
                                  breadcrumb=[], breadcrumb_text="") for m in all_matches], "sections": []}
            if filenames:
                row["size"] = meta["size"]
            results.append(row)
        return results
