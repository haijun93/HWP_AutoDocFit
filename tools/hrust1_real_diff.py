"""HRUST1 실제 문서 위계 대조: Python 규칙(기준) ↔ Rust(DLL·klib) 문단별 비교.

본문 색인 DB **사본**의 문서 목록을 읽어 두 가지 입력으로 대조한다.
- file: 원본 파일을 읽기만 해서 다시 추출한 문단(스타일·outline·표 정보 포함, 색인과 같은 입력)
- db:   사본 DB에 저장된 문단 본문(표 칸 여부·표 칸 번호 포함)
요약(--out)에는 건수·유형만 남긴다. 문단 본문·경로가 담긴 상세(--details)는 작업 폴더 밖에 둔다.
"""
import argparse
import collections
import hashlib
import json
import os
import sqlite3
import subprocess
import sys
import time
from dataclasses import asdict
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from docfit_core.timemachine import content_search as cs  # noqa: E402
from docfit_core.timemachine.content_search import LEVEL_RANKS, ParagraphRecord  # noqa: E402


def _python(path, records):
    return [p.level for p in cs.classify_paragraph_levels(path, records, backend="python")]


def _payload(path, records):
    return [dict(asdict(p), source_path=os.fspath(path)) for p in records]


def _file_records(path):
    with patch.object(cs, "classify_paragraph_levels", side_effect=lambda _p, records: records):
        return cs.extract_paragraphs(path)


def _db_records(db, path):
    cells = dict(db.execute("SELECT paragraph_index, cell_id FROM content_table_cells WHERE path=?", (path,)))
    rows = db.execute(
        "SELECT m.paragraph_index, f.body, f.level FROM content_paragraph_map m"
        " JOIN content_paragraph_fts f ON f.rowid = m.para_rowid WHERE m.path=? ORDER BY m.paragraph_index",
        (path,)).fetchall()
    return [ParagraphRecord(int(i), cs._nfc(body), "body", LEVEL_RANKS["body"], level == "table_cell",
                            table_cell_id=cells.get(int(i))) for i, body, level in rows]


def compare(db_path, dll, klib=None, limit=None, details=None):
    from docfit_app.hierarchy_bridge import run_search_library
    db = sqlite3.connect(f"file:{Path(db_path).as_posix()}?mode=ro", uri=True)
    paths = [r[0] for r in db.execute("SELECT path FROM content_meta ORDER BY path")][:limit]
    summary = {"documents": len(paths), "by_source": {}, "kinds": collections.Counter(),
               "errors": collections.Counter()}
    rows_out = []
    for source in ("file", "db"):
        stat = summary["by_source"][source] = collections.Counter()
        for path in paths:
            ext = Path(path).suffix.lower().lstrip(".")
            try:
                records = _file_records(path) if source == "file" else _db_records(db, path)
            except Exception as exc:  # 추출 실패는 건수만 센다.
                summary["errors"][f"{source}:{ext}:{type(exc).__name__}"] += 1
                continue
            if not records:
                continue
            expected = _python(path, records)
            payload = _payload(path, records)
            native = run_search_library(dll, payload) or {}
            backends = {"dll": native.get("search_levels")}
            with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=None),                     patch("docfit_app.hierarchy_bridge.search_levels", return_value=native or None):
                backends["engine_path"] = [p.level for p in cs.classify_paragraph_levels(path, records)]
            if klib:
                done = subprocess.run([str(klib), "classify-levels", "7"],
                                      input=json.dumps(payload, ensure_ascii=False).encode("utf-8"),
                                      capture_output=True, check=False)
                try:
                    backends["klib"] = json.loads(done.stdout)
                except ValueError:
                    backends["klib"] = None
            stat["documents"] += 1
            stat[f"documents_{ext}"] += 1
            stat["paragraphs"] += len(records)
            for backend, actual in backends.items():
                if not isinstance(actual, list) or len(actual) != len(expected):
                    stat[f"{backend}_bad_response_docs"] += 1
                    continue
                diff = [i for i, (a, b) in enumerate(zip(expected, actual)) if a != b]
                stat[f"{backend}_diff_paragraphs"] += len(diff)
                stat[f"{backend}_diff_docs"] += bool(diff)
                for i in diff:
                    summary["kinds"][f"{source}:{backend}:{ext}:{expected[i]}->{actual[i]}"] += 1
                    rows_out.append({"source": source, "backend": backend, "path": path, "paragraph": i,
                                     "python": expected[i], "rust": actual[i], "text": records[i].text[:120],
                                     "style": records[i].style_name, "outline": records[i].outline_level,
                                     "cell": records[i].is_table_cell})
    if details:
        Path(details).write_text(json.dumps(rows_out, ensure_ascii=False, indent=1), encoding="utf-8")
    summary["by_source"] = {k: dict(v) for k, v in summary["by_source"].items()}
    summary["kinds"] = dict(summary["kinds"].most_common())
    summary["errors"] = dict(summary["errors"])
    summary["corpus_id"] = hashlib.sha256("\n".join(paths).encode("utf-8")).hexdigest()[:12]
    return summary


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", required=True, type=Path, help="본문 색인 DB 사본(원본 아님)")
    parser.add_argument("--dll", required=True, type=Path)
    parser.add_argument("--klib", type=Path)
    parser.add_argument("--limit", type=int)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--details", type=Path, help="본문이 담긴 상세 파일(작업 폴더 밖)")
    args = parser.parse_args()
    started = time.perf_counter()
    result = compare(args.db, args.dll, args.klib, args.limit, args.details)
    result["seconds"] = round(time.perf_counter() - started, 1)
    text = json.dumps(result, ensure_ascii=False, indent=2)
    if args.out:
        args.out.write_text(text, encoding="utf-8")
    print(text)
